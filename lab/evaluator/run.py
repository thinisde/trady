"""Concrete independent evaluator; acceptance is computed, never supplied."""
import argparse
import hashlib
import json
import math
import os
import pathlib
import stat
import sys
import time
from contextlib import contextmanager
from datetime import datetime, timezone
from fractions import Fraction
from .common import canonical, decimal_string, integer, no_gate_assertions, number, numeric_margin, pinned_json, rational_json, ratio, require, rounded_lower, sha, strict_json, timestamp, unique, valid_hash
from .observations import calendar_index, equity_series, search_accounting, shadow_coverage, validate_fits
from .predictions import score_predictions
from .statistics import deterministic_risk, objective, paired_bounds, risk_metrics


def neighbors(bundle, parameters, envelope, policy):
    rows = unique(bundle["neighbors"], "strategy_id")
    require(1 <= len(rows) <= 16, "predeclared parameter neighborhood missing")
    decimals = {"entry_allowance_fraction": (Fraction(0), number(envelope["max_entry_allowance_fraction"])),
                "stop_atr_multiple": (number(envelope["min_stop_atr_multiple"]), number(envelope["max_stop_atr_multiple"])),
                "target_net_fraction": (number(envelope["min_target_net_fraction"]), number(envelope["max_target_net_fraction"]))}
    integer_bounds = {"max_holding_sessions": (1, 5), "atr_lookback": (2, 252), "bar_seconds": (60, 86400)}
    for row in rows.values():
        p = row["parameters"]
        require(set(p) == set(parameters) and p["family"] == parameters["family"] and p["version"] == parameters["version"], "structural change masquerades as neighbor")
        changed = [key for key in p if p[key] != parameters[key]]
        require(len(changed) == 1 and changed[0] in policy["neighborhood_steps"], "neighbor was not a one-step registered perturbation")
        key = changed[0]
        delta = abs((number(p[key]) - number(parameters[key])) if key in decimals else Fraction(p[key] - parameters[key]))
        require(delta == number(policy["neighborhood_steps"][key]), "neighbor step differs from predeclared sensitivity")
        for field, (lower, upper) in decimals.items():
            require(lower <= number(p[field]) <= upper, "neighbor outside formula envelope")
        for field, (lower, upper) in integer_bounds.items():
            integer(p[field], lower, upper)
    return list(rows)


def validate_policy(policy, request, neighbor_parameters):
    no_gate_assertions(policy)
    require(policy["schema_version"] == 1 and sha(policy) == request["manifest"]["component_hashes"]["sealed_evaluator_policy"], "evaluation policy was not frozen in the run")
    require(policy["benchmark_ids"] == [request["manifest"]["champion_id"], "cash", "simple_baseline"], "champion/cash/simple baseline comparison missing")
    require(len(set(policy["benchmark_ids"])) == 3, "benchmark identities overlap")
    require(request["trial"]["candidate_id"] not in policy["benchmark_ids"], "candidate aliases a benchmark")
    benchmark_ids = set(policy["benchmark_ids"])
    require(isinstance(policy["benchmark_parameters"], dict) and set(policy["benchmark_parameters"]) == benchmark_ids,
            "benchmark parameter keys must exactly match benchmark identities")
    require(all(isinstance(value, dict) for value in policy["benchmark_parameters"].values()), "benchmark parameters must be objects")
    require(isinstance(policy["benchmark_complexities"], dict) and set(policy["benchmark_complexities"]) == benchmark_ids,
            "benchmark complexity keys must exactly match benchmark identities")
    for value in policy["benchmark_complexities"].values():
        integer(value, 0, 10000)
    scenarios = unique(policy["stress_cases"], "scenario_id")
    require("base" in scenarios and number(scenarios["base"]["cost_multiple"]) == 1
            and any(number(s["cost_multiple"]) >= 2 for s in scenarios.values()), "predeclared 2x variable-cost stress missing")
    for spec in scenarios.values():
        require(1 <= number(spec["cost_multiple"]) <= 10 and 1 <= number(spec["spread_multiple"]) <= 10, "invalid cost/spread stress")
        integer(spec["latency_ms"], 0, 60_000)
    require(1 <= len(request["plan"]["candidates"]) <= 32
            and request["trial"]["parameters"] in request["plan"]["candidates"], "candidate was not predeclared in the frozen plan")
    all_parameters = [*request["plan"]["candidates"], *neighbor_parameters]
    for parameters in all_parameters:
        require(parameters["family"] in ("TREND", "REVERSAL"), "unsupported formula family")
        integer(parameters["version"], 1, 2**32 - 1)
        integer(parameters["max_holding_sessions"], 1, 5)
    integer(policy["max_label_horizon_sessions"], 1, 5)
    integer(policy["block_sessions"], 1, 252)
    maximum_horizon = max(p["max_holding_sessions"] for p in all_parameters)
    require(policy["max_label_horizon_sessions"] >= maximum_horizon, "label horizon omits registered holding periods")
    require(policy["block_sessions"] >= max(maximum_horizon, policy["max_label_horizon_sessions"]), "resampling block shorter than overlapping forecast horizon")
    numeric_margin(policy)
    integer(policy["annual_sessions"], 1, 366)
    for field in ["minimum_calibration_samples", "minimum_payoff_samples_per_event", "maximum_forecast_age_seconds", "planned_looks"]:
        integer(policy[field], 1)
    integer(policy["minimum_calibration_sessions"], 2 * policy["block_sessions"])
    integer(policy["minimum_calibration_samples_per_event"], 1)
    for field in ["maximum_drawdown_increase", "maximum_cvar_increase", "maximum_worst_loss_increase", "maximum_turnover_increase",
                  "maximum_drawdown", "maximum_exposure", "maximum_brier_deterioration", "maximum_log_loss", "maximum_ece",
                  "maximum_class_shift", "maximum_brier_drift", "minimum_forecast_coverage", "maximum_payoff_mean_error",
                  "utility_risk_weight", "utility_uncertainty_weight", "utility_complexity_weight", "cash_hurdle",
                  "project_cost_account_currency", "neighborhood_maximum_objective_drop"]:
        require(number(policy[field]) >= 0, "negative acceptance/risk/economic setting")
    require(number(policy["maximum_exposure"]) <= 1 and number(policy["maximum_ece"]) <= 1
            and number(policy["minimum_forecast_coverage"]) <= 1, "invalid unit interval policy")
    require(all(0 <= number(v) <= 100 for v in request["plan"]["objective"].values()), "objective weight outside evaluator numeric domain")
    return scenarios


def evaluate(request, lock, bundle, search, shadow, now=None):
    """Pure computation entrypoint. Production CLI additionally checks every file pin/owner."""
    now = now or datetime.now(timezone.utc)
    require(request["schema_version"] == lock["schema_version"] == bundle["schema_version"] == search["schema_version"] == shadow["schema_version"] == 1
            and request["stage"] == "evaluate", "invalid evaluator protocol")
    for source in (bundle, search, shadow):
        no_gate_assertions(source)
        require(source["evidence_class"] == lock["evidence_class"], "TEST ONLY data cannot be reclassified as production")
    require(lock["evidence_class"] in ("PRODUCTION", "TEST_ONLY"), "unknown evidence class")
    expected = {"input_hash": request["input_hash"], "run_id": request["run_id"], "plan_hash": request["plan_hash"],
                "parameters_hash": request["parameters_hash"], "dataset_hash": request["manifest"]["dataset_hash"],
                "candidate_id": request["trial"]["candidate_id"], "account_id": request["manifest"]["account_id"], "mode": request["manifest"]["mode"]}
    require(lock["bindings"] == expected and lock["request_sha256"] == sha(request)
            and all(valid_hash(expected[k]) for k in ("input_hash", "plan_hash", "parameters_hash", "dataset_hash")), "sealed candidate/run binding changed")
    require(expected["candidate_id"] == "candidate_" + expected["parameters_hash"], "candidate parameters identity mismatch")
    require(sha(bundle) == lock["artifacts"]["dataset"]["sha256"], "frozen dataset content mismatch")
    source_manifest = bundle["source_manifest"]
    require(source_manifest["schema_version"] == 1 and source_manifest["input_ledger_hashes"]
            and len(set(source_manifest["input_ledger_hashes"])) == len(source_manifest["input_ledger_hashes"])
            and all(valid_hash(h) for h in source_manifest["input_ledger_hashes"])
            and valid_hash(source_manifest["normalization_hash"]) and sha(source_manifest) == expected["dataset_hash"],
            "underlying frozen source dataset identity changed")
    require(sha(search) == lock["artifacts"]["search"]["sha256"] and sha(shadow) == lock["artifacts"]["shadow"]["sha256"], "sealed search/shadow content mismatch")
    require(sha(bundle["calendar"]) == request["manifest"]["component_hashes"]["calendar"], "unapproved exchange calendar")
    require(sha(bundle["approved_models"]) == request["manifest"]["component_hashes"]["models"]
            and sha(bundle["approved_transforms"]) == request["manifest"]["component_hashes"]["calibrators"], "model/calibrator identities changed")
    policy = lock["policy"]
    neighborhood = neighbors(bundle, request["trial"]["parameters"], request["plan"]["envelope"], policy)
    scenarios = validate_policy(policy, request, [row["parameters"] for row in bundle["neighbors"]])
    candidate = expected["candidate_id"]
    benchmark_ids = policy["benchmark_ids"]
    require(not set(neighborhood).intersection([candidate, *benchmark_ids]), "neighbor aliases a candidate/benchmark")
    # Exact benchmark key sets and disjoint strategy namespaces are established
    # before this merge and before any observations are read. Neither a receipt
    # nor an extra policy key can replace the frozen candidate/neighbor objects.
    parameters = {candidate: request["trial"]["parameters"], **{n["strategy_id"]: n["parameters"] for n in bundle["neighbors"]},
                  **policy["benchmark_parameters"]}
    require(set(parameters) == {candidate, *neighborhood, *benchmark_ids}, "ambiguous strategy/benchmark identity")
    report_keys = ["|".join((strategy, scenario)) for strategy in parameters for scenario in scenarios]
    require(len(report_keys) == len(set(report_keys)), "ambiguous strategy/scenario diagnostic identity")
    cutoff = timestamp(request["manifest"]["cutoff_at"])
    frozen = timestamp(lock["candidate_frozen_at"])
    require(timestamp(policy["frozen_at"]) <= frozen <= now and timestamp(lock["sealed_at"]) <= frozen
            and cutoff <= now, "policy/candidate/holdout freeze ordering failed")
    sessions, outer, folds = calendar_index(bundle, cutoff)
    # A sealed outer period is separate from ALL visible development intervals.
    gap = policy["embargo_seconds"]
    from datetime import timedelta
    for development in request["plan"]["development_folds"]:
        require(timestamp(development["closes_at"]) + timedelta(seconds=gap) <= timestamp(outer[0]["opens_at"]), "outer testing overlaps development or embargo")
    fits, samples = validate_fits(bundle, sessions, outer, policy)
    require(all(f["model_hash"] in bundle["approved_models"].values() and f["transform_hash"] in bundle["approved_transforms"].values() for f in fits.values()), "unapproved fitted model/transform")
    trials, reads = search_accounting(search, request, now)
    require(reads <= policy["planned_looks"], "sequential look budget exhausted")
    series, complexities, accounting = equity_series(bundle, sessions, outer, parameters, scenarios, policy, cutoff,
                                                     request["manifest"]["component_hashes"]["engine"], expected["dataset_hash"])
    bounds, inference = paired_bounds(series, folds, request["plan"]["objective"], complexities, candidate, neighborhood,
                                     benchmark_ids, scenarios, policy, trials, str(request["manifest"]["random_seed"]))
    require(inference["sufficient"], "insufficient independent blocks or bootstrap tail resolution after search correction")
    lower = min(value["lower"] for key, value in bounds.items() if key[0] == candidate)
    statistics, exact_risks = {}, {}
    for key, returns in series.items():
        details = accounting[key]
        exact_risks[key] = deterministic_risk(details["exact_returns"], details["exposures"], details["turnovers"])
        stats = risk_metrics(details["exact_returns"], details["exposures"], details["turnovers"], policy["annual_sessions"], exact_risks[key])
        stats["deterministic_risk_exact"] = {metric: rational_json(value) for metric, value in exact_risks[key].items()}
        stats["closed_trades"] = len(details["closed_trade_ids"])
        stats["open_trades"] = len(details["open_trade_ids"])
        stats["fill_rate"] = details["fill_rate"]
        stats["objective"] = objective([[returns[i] for i in f] for f in folds], request["plan"]["objective"], complexities[key[0]])
        regimes = sorted({row["regime"] for row in outer})
        stats["regime_breakdown"] = {regime: {"sessions": sum(row["regime"] == regime for row in outer),
                                              "mean_log_return": math.fsum(math.log1p(returns[i]) for i, row in enumerate(outer) if row["regime"] == regime)
                                              / sum(row["regime"] == regime for row in outer)} for regime in regimes}
        stats["sector_gross_attribution"] = details["sector_attribution"]
        stats["unallocated_execution_costs"] = details["unallocated_execution_costs"]
        statistics["|".join(key)] = stats
    risk_ok = True
    for scenario in scenarios:
        own, champion = exact_risks[candidate, scenario], exact_risks[request["manifest"]["champion_id"], scenario]
        for metric, allowance in [("max_drawdown", "maximum_drawdown_increase"), ("cvar_95_loss", "maximum_cvar_increase"),
                                  ("worst_session_loss", "maximum_worst_loss_increase"), ("mean_turnover", "maximum_turnover_increase")]:
            risk_ok = risk_ok and own[metric] <= champion[metric] + number(policy[allowance])
        risk_ok = risk_ok and own["max_drawdown"] <= number(policy["maximum_drawdown"]) and own["maximum_exposure"] <= number(policy["maximum_exposure"])
    neighborhood_ok = all(value["lower"] > 0 and Fraction(bounds[candidate, key[1], key[2]]["point"]) - Fraction(value["point"]) + numeric_margin(policy)
                          <= number(policy["neighborhood_maximum_objective_drop"])
                          for key, value in bounds.items() if key[0] in neighborhood)
    holding_limit = request["trial"]["parameters"]["max_holding_sessions"]
    predictions = score_predictions(bundle, sessions, fits, samples, candidate, policy, cutoff, trials, str(request["manifest"]["random_seed"]), holding_limit)
    shadow_metrics = shadow_coverage(shadow, sessions, bundle["calendar"]["weeks"], candidate, frozen, cutoff, holding_limit)
    capital = accounting[candidate, "base"]["opening_equity"]
    project_hurdle = ratio(number(policy["project_cost_account_currency"]), capital)
    economics_ok = bool(predictions["eligible"] and predictions["conditional_payoff_support"] and predictions["positive_entry_utility"]
                         and Fraction(lower) > number(request["plan"]["minimum_improvement"]) + project_hurdle)
    raw_gates = {"causal_and_uncontaminated": True, "dependence_and_search_adjusted": inference["sufficient"],
                 "risk_not_deteriorated": risk_ok, "neighborhood_checks_passed": neighborhood_ok,
                 "economic_feasibility_passed": economics_ok}
    production = lock["evidence_class"] == "PRODUCTION"
    payload = {"run_id": request["run_id"], "dataset_hash": expected["dataset_hash"], "plan_hash": request["plan_hash"],
               "outer_intervals": len(folds), "closed_trades": len(accounting[candidate, "base"]["closed_trade_ids"]),
               "completed_shadow_weeks": shadow_metrics["completed_weeks"], "shadow_closed_trades": shadow_metrics["closed_trades"],
               "shadow_report_id": "shadow_" + sha(shadow) if shadow_metrics["covered_sessions"] else None,
               "cost_stress_multiple": decimal_string(max(number(s["cost_multiple"]) for s in scenarios.values())),
               "improvement_lower_bound": rounded_lower(lower), **{key: bool(value and production) for key, value in raw_gates.items()}}
    diagnostics = {"schema_version": 1, "evidence_class": lock["evidence_class"], "input_hash": request["input_hash"],
                   "bindings": expected, "statistics": statistics,
                   "inference": inference, "comparisons": [{"strategy": k[0], "scenario": k[1], "baseline": k[2], **v} for k, v in sorted(bounds.items())],
                   "predictions": predictions, "shadow": shadow_metrics, "project_cost_return_hurdle": float(project_hurdle),
                   "computed_gates_before_evidence_class": raw_gates,
                   "acceptance_scope": "frozen observed data; block-resampling assumptions are not future-profit guarantees"}
    return payload, diagnostics


@contextmanager
def diagnostics_directory(directory):
    """Open a private output directory without following any path symlinks."""
    directory = pathlib.Path(directory)
    require(directory.is_absolute() and ".." not in directory.parts,
            "absolute private diagnostics directory required")
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
    descriptor = os.open(directory.anchor, flags)
    try:
        for part in directory.parts[1:]:
            parent = os.fstat(descriptor)
            # Root-owned sticky /tmp is safe for private children; ordinary
            # writable ancestors could redirect an otherwise valid output path.
            require(parent.st_uid in (0, os.geteuid()) and
                    (parent.st_mode & 0o022 == 0 or parent.st_mode & stat.S_ISVTX),
                    "unsafe diagnostics directory ancestor")
            try:
                child = os.open(part, flags, dir_fd=descriptor)
            except FileNotFoundError:
                try:
                    os.mkdir(part, mode=0o700, dir_fd=descriptor)
                except FileExistsError:
                    pass
                child = os.open(part, flags, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        meta = os.fstat(descriptor)
        require(meta.st_uid == os.geteuid() and stat.S_IMODE(meta.st_mode) == 0o700,
                "diagnostics directory requires evaluator ownership and exact mode 0700")
        yield descriptor
        current = directory.stat(follow_symlinks=False)
        final = os.fstat(descriptor)
        require((current.st_dev, current.st_ino) == (meta.st_dev, meta.st_ino)
                and final.st_uid == os.geteuid() and stat.S_IMODE(final.st_mode) == 0o700,
                "diagnostics directory changed during use")
    finally:
        os.close(descriptor)


def write_artifact(directory, content):
    data = canonical(content)
    require(len(data) <= 32 * 1024 * 1024, "diagnostics size budget exceeded")
    digest = hashlib.sha256(data).hexdigest()
    with diagnostics_directory(directory) as directory_fd:
        try:
            fd = os.open(digest, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC,
                         0o600, dir_fd=directory_fd)
        except FileExistsError:
            fd = os.open(digest, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC,
                         dir_fd=directory_fd)
            with os.fdopen(fd, "rb") as existing:
                meta = os.fstat(existing.fileno())
                require(stat.S_ISREG(meta.st_mode) and meta.st_uid == os.geteuid()
                        and stat.S_IMODE(meta.st_mode) == 0o600 and meta.st_nlink == 1,
                        "immutable diagnostics identity or mode invalid")
                require(existing.read(32 * 1024 * 1024 + 1) == data, "immutable diagnostics tampered")
        else:
            with os.fdopen(fd, "wb") as destination:
                require(stat.S_IMODE(os.fstat(destination.fileno()).st_mode) == 0o600,
                        "diagnostics file requires exact mode 0600")
                destination.write(data); destination.flush(); os.fsync(destination.fileno())
            os.fsync(directory_fd)
    return digest


def main():
    parser = argparse.ArgumentParser()
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--lock", type=pathlib.Path)
    source.add_argument("--socket", type=pathlib.Path)
    parser.add_argument("--worker-uid", type=int)
    args = parser.parse_args()
    started = time.monotonic()
    data = sys.stdin.buffer.read(4 * 1024 * 1024 + 1)
    require(len(data) <= 4 * 1024 * 1024, "request exceeds helper limit")
    if args.socket:
        from .transport import request_evaluation
        require(args.worker_uid is not None and args.worker_uid >= 0, "explicit evaluator worker UID required")
        sys.stdout.buffer.write(request_evaluation(args.socket, data, args.worker_uid))
        return
    lock = read_lock(args.lock)
    require(lock["evidence_class"] == "TEST_ONLY", "production evaluation requires the independent socket worker")
    request = strict_json(data)
    artifacts = lock["artifacts"]
    dataset, search, shadow = (pinned_json(artifacts[key]) for key in ("dataset", "search", "shadow"))
    print(json.dumps(reply(request, lock, dataset, search, shadow, started), allow_nan=False, separators=(",", ":")))


def read_lock(path):
    require(path.is_absolute() and not path.is_symlink(), "absolute sealed lock required")
    meta = path.stat()
    require(meta.st_uid == 0 and meta.st_mode & 0o022 == 0 and meta.st_size <= 4 * 1024 * 1024, "deployment-owned sealed lock required")
    return strict_json(path.read_bytes())


def reply(request, lock, dataset, search, shadow, started):
    payload, diagnostics = evaluate(request, lock, dataset, search, shadow)
    digest = write_artifact(lock["diagnostics_directory"], diagnostics)
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    payload["report"] = {"report_id": "evaluation_" + digest, "tool_id": "trady-sealed-evaluator-v1",
                         "input_hash": request["input_hash"], "output_hash": digest, "exit_status": 0,
                         "duration_ms": int((time.monotonic() - started) * 1000), "recorded_at": now}
    return {"schema_version": 1, "input_hash": request["input_hash"], "payload": payload}

"""Original forecast scoring and conditional net-payoff estimation from matured records."""
import math
import random
import statistics
from collections import defaultdict
from fractions import Fraction
from .common import EVENTS, integer, number, numeric_margin, rational_json, ratio, require, timestamp, unique, valid_hash
from .statistics import circular_blocks, mean
from .observations import holding_span, validate_trades


def exact_mean(values):
    require(bool(values), "empty exact statistic")
    return ratio(sum(values), len(values))


def outcome_identity(sample):
    """Causal original-label identity, independent of sample/feature aliases."""
    return (sample["label_source_hash"], sample["session_id"],
            timestamp(sample["label_start_at"]), timestamp(sample["label_end_at"]))


def payoff_identity(payoff, sample):
    """Original label identity and estimator context, independent of export IDs.

    Feature/recording aliases do not create another realized outcome. One source
    may contain distinct causal windows; the event and availability are facts of
    that original window, rather than ways to name a second observation.
    """
    context = (payoff["regime"], payoff["horizon_sessions"], payoff["currency"],
               payoff["scenario_hash"], payoff["execution_convention_hash"], payoff["cost_convention"])
    return outcome_identity(sample), context


def conditional_bounds(observations, calendar, policy, alpha, seed):
    """Session-block CIs for validated unique original outcomes in one context."""
    block = policy["block_sessions"]
    repetitions = policy["bootstrap_repetitions"]
    margin = numeric_margin(policy)
    by_event = {event: [] for event in EVENTS}
    for observation in observations:
        normalized = ratio(number(observation["gross_profit"]) - number(observation["fixed_cost"]) - number(observation["variable_cost"]),
                           number(observation["opening_equity"]))
        require(abs(normalized) <= number(policy["maximum_payoff_abs_return"]), "payoff outside frozen finite domain")
        by_event[observation["event"]].append((observation["session_id"], normalized))
    result = {}
    for event, values in by_event.items():
        if len(values) < policy["minimum_payoff_samples_per_event"] or len({s for s, _ in values}) < 2 * block:
            return None
        first = min(calendar[s]["ordinal"] for s, _ in values)
        last = max(calendar[s]["ordinal"] for s, _ in values)
        groups = [[] for _ in range(last - first + 1)]
        for session, value in values:
            groups[calendar[session]["ordinal"] - first].append(value)
        point = exact_mean([value for _, value in values])
        totals = [sum(group, Fraction(0)) for group in groups]
        counts = [len(group) for group in groups]
        generator = random.Random(seed + ":" + event)
        errors = []
        for _ in range(repetitions):
            sampled = circular_blocks(len(groups), block, generator)
            count = sum(counts[i] for i in sampled)
            if not count:
                return None
            errors.append(ratio(sum((totals[i] for i in sampled), Fraction(0)), count) - point)
        if repetitions * alpha < 5:
            return None
        errors.sort()
        lo_index = max(0, math.floor(alpha * len(errors)))
        hi_index = min(len(errors) - 1, math.ceil((1 - alpha) * len(errors)) - 1)
        result[event] = {"mean": point, "lower": point - errors[hi_index] - margin,
                         "upper": point - errors[lo_index] + margin, "samples": len(values)}
    return result


def calibration_scores(rows, policy):
    if not rows:
        return {"samples": 0, "brier": None, "log_loss": None, "ece": None, "eligible": False}
    bins = integer(policy["reliability_bins"], 2, 100)
    margin = numeric_margin(policy)
    brier, prior_brier, losses, zeros = [], [], [], 0
    reliability = defaultdict(list)
    frequencies = {event: 0 for event in EVENTS}
    for row in rows:
        p, actual = row["probabilities"], row["outcome"]
        brier.append(sum((p[e] - int(e == actual)) ** 2 for e in EVENTS))
        prior_brier.append(sum((row["prior"][e] - int(e == actual)) ** 2 for e in EVENTS))
        frequencies[actual] += 1
        if p[actual] == 0:
            zeros += 1
        else:
            losses.append(-math.log(float(p[actual])))
        for event in EVENTS:
            reliability[event, min(bins - 1, int(p[event] * bins))].append((p[event], int(event == actual)))
    ece = ratio(sum(len(values) * abs(exact_mean([p for p, _ in values]) - exact_mean([y for _, y in values]))
                    for values in reliability.values()), len(rows) * len(EVENTS))
    half = len(rows) // 2
    older, newer = rows[:half], rows[half:]
    shift = max((abs(ratio(sum(r["outcome"] == e for r in older), len(older))
                     - ratio(sum(r["outcome"] == e for r in newer), len(newer))) for e in EVENTS), default=Fraction(1)) if older else Fraction(1)
    score_drift = exact_mean(brier[half:]) - exact_mean(brier[:half]) if older else None
    brier_mean, prior_mean = exact_mean(brier), exact_mean(prior_brier)
    log_loss = mean(losses) if losses and not zeros else None
    # Logarithms are the only approximate calibration arithmetic. This guard
    # does not depend on an economic penalty or a caller's calibration tolerance.
    log_upper = Fraction(log_loss) + margin if log_loss is not None else None
    eligible = (len(rows) >= policy["minimum_calibration_samples"] and zeros == 0
                and len({r["session_id"] for r in rows}) >= policy["minimum_calibration_sessions"]
                and all(v >= policy["minimum_calibration_samples_per_event"] for v in frequencies.values())
                and brier_mean <= prior_mean + number(policy["maximum_brier_deterioration"])
                and log_upper is not None and log_upper <= number(policy["maximum_log_loss"])
                and ece <= number(policy["maximum_ece"])
                and shift <= number(policy["maximum_class_shift"])
                and score_drift is not None and score_drift <= number(policy["maximum_brier_drift"]))
    exact = {"brier": brier_mean, "prior_brier": prior_mean, "ece": ece, "class_shift": shift,
             "brier_drift": score_drift, "log_loss_numerical_upper": log_upper}
    return {"samples": len(rows), "brier": float(brier_mean), "prior_brier": float(prior_mean),
            "log_loss": log_loss, "log_loss_numerical_upper": float(log_upper) if log_upper is not None else None,
            "zero_likelihood_events": zeros, "ece": float(ece),
            "class_counts": frequencies, "class_shift": float(shift), "brier_drift": float(score_drift) if score_drift is not None else None,
            "exact": {key: rational_json(value) if value is not None else None for key, value in exact.items()},
            "reliability": [{"event": key[0], "bin": key[1], "samples": len(values),
                             "mean_probability": float(exact_mean([p for p, _ in values])),
                             "observed_frequency": float(exact_mean([y for _, y in values])),
                             "exact": {"mean_probability": rational_json(exact_mean([p for p, _ in values])),
                                       "observed_frequency": rational_json(exact_mean([y for _, y in values]))}}
                            for key, values in sorted(reliability.items())], "eligible": eligible}


def forecast_context(row, calendar, fits, bundle, policy, cutoff):
    """Original forecast facts required before scoring or using an entry link."""
    require(row["session_id"] in calendar, "forecast exchange session missing")
    require(row["model_fit_id"] in fits, "forecast has no training/calibration fit")
    fit = fits[row["model_fit_id"]]
    session = calendar[row["session_id"]]
    decision = timestamp(row["decision_at"])
    observed, available = timestamp(row["observed_at"]), timestamp(row["available_at"])
    generated, recorded = timestamp(row["generated_at"]), timestamp(row["recorded_at"])
    require(observed <= available <= generated <= recorded <= decision < timestamp(row["valid_until"])
            and decision <= cutoff, "forecast is stale, unavailable or backfilled after the decision")
    require(timestamp(fit["fitted_at"]) <= generated, "forecast generated before model/transform fit completed")
    require(timestamp(session["opens_at"]) <= decision <= timestamp(session["closes_at"])
            and (decision - observed).total_seconds() <= policy["maximum_forecast_age_seconds"]
            and row["model_hash"] == fit["model_hash"]
            and row["model_hash"] == bundle["approved_models"].get(row["model_version"])
            and row["transform_hash"] == fit["transform_hash"]
            and (session["phase"] != "OUTER" or session["fold_id"] in fit["outer_fold_ids"]), "forecast/fit/horizon binding failed")
    require(row["regime"] == session["regime"] and valid_hash(row["scenario_hash"])
            and valid_hash(row["source_hash"]) and row["decision"] in ("ENTER", "ABSTAIN"), "forecast scenario/provenance missing")
    require(isinstance(row["dependence_group_id"], str) and 0 < len(row["dependence_group_id"]) <= 160,
            "invalid dependence group identity")
    require(set(row["probabilities"]) == set(EVENTS), "complete unconditional outcome partition required")
    probabilities = {e: number(row["probabilities"][e]) for e in EVENTS}
    require(all(0 <= p <= 1 for p in probabilities.values()) and sum(probabilities.values()) == 1,
            "invalid or renormalized forecast probabilities")
    return decision, probabilities


def score_predictions(bundle, calendar, fits, samples, candidate, policy, cutoff, trial_count, seed, max_holding_sessions):
    raw = unique(bundle["forecasts"], "prediction_id")
    payoffs = unique(bundle["payoff_observations"], "observation_id")
    payoff_keys, original_labels, original_payoffs = {}, {}, {}
    for sample in samples.values():
        require(valid_hash(sample["label_source_hash"]) and sample["session_id"] in calendar and sample["outcome"] in EVENTS,
                "original outcome provenance unavailable")
        key = outcome_identity(sample)
        label_facts = (sample["outcome"], timestamp(sample["label_available_at"]), timestamp(sample["label_recorded_at"]))
        require(key not in original_labels or original_labels[key] == label_facts,
                "conflicting original outcome aliases")
        original_labels[key] = label_facts
    for payoff in payoffs.values():
        require(payoff["event"] in EVENTS and payoff["session_id"] in calendar and payoff["sample_id"] in samples,
                "payoff lacks explicit event, calendar or original sample")
        require(number(payoff["opening_equity"]) > 0 and number(payoff["fixed_cost"]) >= 0 and number(payoff["variable_cost"]) >= 0
                and valid_hash(payoff["source_hash"]) and payoff["cost_convention"] == "EXECUTABLE_PRICES_PLUS_RESIDUAL",
                "payoff normalization, costs or provenance unavailable")
        sample = samples[payoff["sample_id"]]
        require(payoff["session_id"] == sample["session_id"] and calendar[payoff["session_id"]]["phase"] == "TRAIN"
                and payoff["event"] == sample["outcome"] and payoff["source_hash"] == sample["label_source_hash"],
                "payoff outcome/calendar differs from its original matured label")
        payoff_horizon = integer(payoff["horizon_sessions"], 1, min(5, policy["max_label_horizon_sessions"], max_holding_sessions))
        holding_span(calendar, payoff["session_id"], timestamp(sample["label_start_at"]), timestamp(sample["label_end_at"]), payoff_horizon)
        require(isinstance(payoff["regime"], str) and isinstance(payoff["currency"], str)
                and valid_hash(payoff["scenario_hash"]) and valid_hash(payoff["execution_convention_hash"]),
                "payoff estimator context unavailable")
        key = payoff_identity(payoff, sample)
        payoff_facts = (number(payoff["gross_profit"]), number(payoff["fixed_cost"]), number(payoff["variable_cost"]),
                        number(payoff["opening_equity"]), timestamp(payoff["available_at"]), timestamp(payoff["recorded_at"]))
        require(key not in original_payoffs or original_payoffs[key] == payoff_facts,
                "conflicting original payoff aliases")
        original_payoffs[key] = payoff_facts
        payoff_keys[payoff["observation_id"]] = key
    selected = [r for r in raw.values() if r["strategy_id"] == candidate]
    trades, _ = validate_trades(bundle["trades"], calendar,
                               {(run["strategy_id"], run["scenario_id"]) for run in bundle["equity"]}, cutoff)
    neighbor_specs = unique(bundle["neighbors"], "strategy_id")
    for trade in trades.values():
        if trade["strategy_id"] == candidate or trade.get("prediction_id") is not None:
            require(trade.get("prediction_id") in raw, "trade has no original forecast")
            forecast = raw[trade["prediction_id"]]
            decision, _ = forecast_context(forecast, calendar, fits, bundle, policy, cutoff)
            require(forecast["strategy_id"] == trade["strategy_id"] and forecast["decision"] == "ENTER"
                    and forecast["scenario_hash"] == trade["scenario_hash"]
                    and decision <= timestamp(trade["entry_at"]),
                    "trade forecast/scenario/decision binding failed")
            holding_limit = max_holding_sessions if trade["strategy_id"] == candidate else (
                neighbor_specs[trade["strategy_id"]]["parameters"]["max_holding_sessions"]
                if trade["strategy_id"] in neighbor_specs else policy["max_label_horizon_sessions"])
            trade_horizon = integer(forecast["horizon_sessions"], 1, min(5, policy["max_label_horizon_sessions"], holding_limit))
            require(timestamp(trade["entry_at"]) < timestamp(forecast["valid_until"]), "trade entered after forecast expiry")
            holding_span(calendar, forecast["session_id"], decision, timestamp(trade["entry_at"]), trade_horizon)
            if trade["state"] == "CLOSED" and trade["strategy_id"] not in policy["benchmark_ids"]:
                holding_span(calendar, forecast["session_id"], timestamp(forecast["decision_at"]), timestamp(trade["exit_at"]), trade_horizon)
    estimators = {}
    for row in selected:
        ids = tuple(sorted(row["payoff_sample_ids"]))
        require(len(ids) == len(set(ids)) and ids, "missing/duplicated payoff samples")
        require(all(identity in payoffs for identity in ids), "conditional payoff source missing")
        key = tuple(sorted(payoff_keys[identity] for identity in ids))
        require(len(key) == len(set(key)), "reused original payoff outcome within estimator context")
        estimators[row["prediction_id"]] = ids, key
    # Equivalent aliases across separate forecasts share one estimator/search
    # group; each forecast still validates its own fit membership and chronology.
    groups = {key for _, key in estimators.values()}
    alpha = number(policy["alpha"]) / (8 * max(1, len(groups)) * trial_count * policy["planned_looks"])
    bounds_cache, matured, pending, coverage, economics = {}, [], 0, set(), []
    utilities = []
    margin = numeric_margin(policy)
    cohort_rows = defaultdict(list)
    cohort_activity = defaultdict(lambda: {"forecasts": 0, "enter_forecasts": 0, "abstain_forecasts": 0,
                                            "pending_labels": 0, "conditional_payoff_support": True})
    # Export order cannot choose the older/newer calibration halves at a time tie.
    for row in sorted(selected, key=lambda r: (
            timestamp(r["decision_at"]), timestamp(r["recorded_at"]), r["prediction_id"])):
        require(row["session_id"] in calendar and calendar[row["session_id"]]["phase"] == "OUTER", "forecast outside outer calendar")
        decision, probabilities = forecast_context(row, calendar, fits, bundle, policy, cutoff)
        fit = fits[row["model_fit_id"]]
        horizon = integer(row["horizon_sessions"], 1, min(5, policy["max_label_horizon_sessions"], max_holding_sessions))
        ids, estimator_key = estimators[row["prediction_id"]]
        estimation = []
        for identity in ids:
            require(identity in payoffs, "conditional payoff source missing")
            item = payoffs[identity]
            sample = samples[item["sample_id"]]
            require(item["sample_id"] in fit["train"]["sample_ids"]
                    and timestamp(sample["label_recorded_at"]) <= timestamp(item["available_at"])
                    <= timestamp(item["recorded_at"]) <= timestamp(fit["fitted_at"]) <= decision,
                    "payoff estimator used future, calibration, development or sealed labels")
            require(item["regime"] == row["regime"] and item["horizon_sessions"] == row["horizon_sessions"]
                    and item["currency"] == row["currency"] == policy["account_currency"]
                    and item["scenario_hash"] == row["scenario_hash"]
                    and item["execution_convention_hash"] == row["execution_convention_hash"],
                    "payoff scenario/regime/horizon/account unit mismatch")
            estimation.append(item)
        if estimator_key not in bounds_cache:
            bounds_cache[estimator_key] = conditional_bounds(estimation, calendar, policy, alpha, seed)
        estimates = bounds_cache[estimator_key]
        counts = {e: sum(x["event"] == e for x in estimation) for e in EVENTS}
        prior = {e: ratio(counts[e], len(estimation)) for e in EVENTS}
        coverage.add(row["session_id"])
        utility, exact_economics = None, {}
        if estimates:
            expected = sum(probabilities[e] * estimates[e]["mean"] for e in EVENTS)
            lower = sum(probabilities[e] * estimates[e]["lower"] for e in EVENTS)
            upper = sum(probabilities[e] * estimates[e]["upper"] for e in EVENTS)
            radius = max(Fraction(0), expected - lower, upper - expected)
            downside = sum(probabilities[e] * max(Fraction(0), -estimates[e]["lower"]) for e in EVENTS)
            complexity = 7 * number(policy["utility_complexity_weight"])
            utility = (expected - number(policy["utility_risk_weight"]) * downside
                       - number(policy["utility_uncertainty_weight"]) * radius - complexity)
            exact_economics = {"expected_return": expected, "lower": lower, "upper": upper, "uncertainty_radius": radius,
                               "downside": downside, "complexity_penalty": complexity, "utility": utility,
                               "utility_numerical_lower": utility - margin}
        utilities.append((row["decision"], utility))
        economics.append({"prediction_id": row["prediction_id"], "decision": row["decision"],
                          "utility": float(utility) if utility is not None else None,
                          "utility_numerical_lower": float(utility - margin) if utility is not None else None,
                          "exact": {key: rational_json(value) for key, value in exact_economics.items()},
                          "conditional_means": {event: {**{key: float(values[key]) for key in ("mean", "lower", "upper")},
                                                         "samples": values["samples"],
                                                         "exact": {key: rational_json(values[key]) for key in ("mean", "lower", "upper")}}
                                                for event, values in estimates.items()} if estimates else None})
        label_end = timestamp(row["label_end_at"])
        require(decision <= label_end and (label_end - decision).total_seconds() <= policy["max_label_horizon_seconds"], "forecast outcome horizon changed")
        holding_span(calendar, row["session_id"], decision, label_end, horizon)
        # Register the decision cohort before censoring pending labels. An ENTER
        # cannot borrow calibration from other models, regimes or horizons merely
        # by leaving its own outcome unscored.
        cohort = (row["model_version"], row["regime"], row["horizon_sessions"])
        cohort_values = cohort_rows[cohort]
        activity = cohort_activity[cohort]
        activity["forecasts"] += 1
        activity["enter_forecasts" if row["decision"] == "ENTER" else "abstain_forecasts"] += 1
        activity["conditional_payoff_support"] = activity["conditional_payoff_support"] and estimates is not None
        if row["outcome"] is None:
            pending += 1
            activity["pending_labels"] += 1
            continue
        require(row["outcome"] in EVENTS and label_end <= timestamp(row["label_available_at"])
                <= timestamp(row["label_recorded_at"]) <= cutoff and valid_hash(row["label_source_hash"]),
                "unmatured or unavailable outcome label")
        require(number(row["opening_equity"]) > 0 and number(row["realized_fixed_cost"]) >= 0
                and number(row["realized_variable_cost"]) >= 0, "invalid realized payoff basis/costs")
        item = {"prediction_id": row["prediction_id"], "outcome": row["outcome"], "probabilities": probabilities,
                "prior": prior, "model_version": row["model_version"], "regime": row["regime"], "horizon_sessions": row["horizon_sessions"],
                "dependence_group_id": row["dependence_group_id"], "session_id": row["session_id"], "scenario_hash": row["scenario_hash"]}
        # Realized payoff validation uses the same initial account-equity units.
        actual = ratio(number(row["realized_gross_profit"]) - number(row["realized_fixed_cost"]) - number(row["realized_variable_cost"]),
                       number(row["opening_equity"]))
        item["actual_net_account_return"] = actual
        item["estimated_conditional_return"] = estimates[row["outcome"]]["mean"] if estimates else None
        matured.append(item)
        cohort_values.append(item)
    cohorts = {}
    for key, values in sorted(cohort_rows.items()):
        identity = "|".join(map(str, key))
        require(identity not in cohorts, "ambiguous calibration cohort diagnostic identity")
        activity = cohort_activity[key]
        cohorts[identity] = {**calibration_scores(values, policy), **activity,
                             "required_for_acceptance": bool(values) or activity["enter_forecasts"] > 0}
    calibration = calibration_scores(matured, policy)
    calibration["cohorts"] = cohorts
    calibration["pending_labels"] = pending
    coverage_fraction = ratio(len(coverage), max(1, sum(s["phase"] == "OUTER" for s in calendar.values())))
    calibration["session_coverage"] = float(coverage_fraction)
    errors = [r["actual_net_account_return"] - r["estimated_conditional_return"] for r in matured if r["estimated_conditional_return"] is not None]
    mean_error = exact_mean(errors) if errors else None
    calibration["payoff_mean_error"] = float(mean_error) if mean_error is not None else None
    calibration["payoff_rmse"] = math.sqrt(float(exact_mean([e * e for e in errors]))) if errors else None
    calibration.setdefault("exact", {}).update(session_coverage=rational_json(coverage_fraction),
                                               payoff_mean_error=rational_json(mean_error) if mean_error is not None else None)
    calibration["eligible"] = (calibration["eligible"] and bool(cohorts)
                               and all(c["eligible"] for c in cohorts.values() if c["required_for_acceptance"])
                               and coverage_fraction >= number(policy["minimum_forecast_coverage"])
                               and mean_error is not None and abs(mean_error) <= number(policy["maximum_payoff_mean_error"]))
    support = bool(utilities) and all(utility is not None for _, utility in utilities)
    positive_entries = any(decision == "ENTER" for decision, _ in utilities) and all(
        utility is not None and utility - margin > number(policy["cash_hurdle"]) for decision, utility in utilities if decision == "ENTER")
    calibration["conditional_payoff_support"] = support
    calibration["positive_entry_utility"] = positive_entries
    calibration["forecast_utilities"] = economics
    # Preserve every actual observation. Multiple forecasts from the same
    # dependence group/session/scenario contribute to its exact residual mean;
    # choosing the last row would make the reported pairing input-order dependent.
    by_group = defaultdict(lambda: defaultdict(list))
    for row in matured:
        key = (row["session_id"], row["scenario_hash"])
        by_group[row["dependence_group_id"]][key].append(1 - row["probabilities"][row["outcome"]])
    correlations = []
    names = sorted(by_group)
    for i, left in enumerate(names):
        for right in names[i + 1:]:
            common = sorted(set(by_group[left]) & set(by_group[right]))
            x, y = [float(exact_mean(by_group[left][k])) for k in common], [float(exact_mean(by_group[right][k])) for k in common]
            correlation = statistics.correlation(x, y) if len(common) >= max(3, 2 * policy["block_sessions"]) and len(set(x)) > 1 and len(set(y)) > 1 else None
            correlations.append({"left": left, "right": right, "paired_samples": len(common), "residual_correlation": correlation})
    calibration["dependence"] = correlations
    return calibration

"""Synthetic, TEST_ONLY observations for evaluator regression tests.

These deliberately simple account records are never historical performance or
production evidence. Tests cannot upgrade their evidence class.
"""
from copy import deepcopy
from datetime import datetime, timedelta, timezone
from decimal import Decimal
from evaluator.common import EVENTS, sha


def stamp(day, hour=9, minute=0):
    return datetime(2025, 1, 1, hour, minute, tzinfo=timezone.utc).__add__(timedelta(days=day)).isoformat().replace("+00:00", "Z")


def fixture():
    parameters = {"family": "TREND", "version": 1, "entry_allowance_fraction": "0.001",
                  "stop_atr_multiple": "2", "target_net_fraction": "0.04", "max_holding_sessions": 3,
                  "atr_lookback": 14, "bar_seconds": 3600}
    neighbor = {**parameters, "stop_atr_multiple": "2.1"}
    candidate = "candidate_" + sha(parameters)
    source_manifest = {"schema_version": 1, "input_ledger_hashes": [sha("TEST_ONLY-original-market-and-label-ledger")],
                       "normalization_hash": sha("TEST_ONLY-original-availability-normalization")}
    model, transform, engine, scenario, convention = [sha(s) for s in ("test-model", "test-transform", "test-engine", "test-scenario", "test-cost-convention")]
    sessions = []
    for day in range(36):
        sessions.append({"session_id": f"train-{day}", "opens_at": stamp(day), "closes_at": stamp(day, 16),
                         "phase": "TRAIN", "regime": "TEST_REGIME"})
    for index in range(36):
        sessions.append({"session_id": f"outer-{index}", "opens_at": stamp(60 + index), "closes_at": stamp(60 + index, 16),
                         "phase": "OUTER", "fold_id": f"fold-{index // 6}", "regime": "TEST_REGIME"})
    for index in range(10):
        sessions.append({"session_id": f"shadow-{index}", "opens_at": stamp(100 + index), "closes_at": stamp(100 + index, 16),
                         "phase": "SHADOW", "week_id": f"week-{index // 5}", "regime": "TEST_REGIME"})
    weeks = [{"week_id": f"week-{i}", "session_ids": [f"shadow-{j}" for j in range(i * 5, i * 5 + 5)],
              "closes_at": stamp(104 + 5 * i, 17)} for i in range(2)]
    samples, payoffs = [], []
    partition_ids = {"train": [], "calibration": [], "development": []}
    payoff_gross = dict(zip(EVENTS, ["1000", "-100", "10", "-2"]))
    for role, days in (("train", range(12)), ("calibration", range(18, 22)), ("development", range(27, 31))):
        for day in days:
            for event in EVENTS:
                identity = f"{role}-{day}-{event}"
                source = sha(identity)
                samples.append({"sample_id": identity, "session_id": f"train-{day}", "outcome": event, "label_source_hash": source,
                                "feature_start_at": stamp(day, 8), "feature_end_at": stamp(day), "available_at": stamp(day),
                                "recorded_at": stamp(day, 9, 1), "label_start_at": stamp(day, 10), "label_end_at": stamp(day, 11),
                                "label_available_at": stamp(day, 11), "label_recorded_at": stamp(day, 11, 1)})
                partition_ids[role].append(identity)
                if role == "train":
                    payoffs.append({"observation_id": "payoff-" + identity, "sample_id": identity, "session_id": f"train-{day}",
                                    "event": event, "gross_profit": payoff_gross[event], "fixed_cost": "1", "variable_cost": "1",
                                    "opening_equity": "100000", "source_hash": source, "regime": "TEST_REGIME", "horizon_sessions": 3,
                                    "currency": "EUR", "scenario_hash": scenario, "execution_convention_hash": convention,
                                    "cost_convention": "EXECUTABLE_PRICES_PLUS_RESIDUAL", "available_at": stamp(day, 11, 1),
                                    "recorded_at": stamp(day, 11, 2)})
    fit = {"fit_id": "fit-test", "model_hash": model, "transform_hash": transform, "fitted_at": stamp(40),
           "outer_fold_ids": [f"fold-{i}" for i in range(6)]}
    for role, start, end in (("train", 0, 11), ("calibration", 18, 21), ("development", 27, 30)):
        fit[role] = {"opens_at": stamp(start), "closes_at": stamp(end, 16), "sample_ids": partition_ids[role]}
    forecasts = []
    for index in range(36):
        day, outcome = 60 + index, EVENTS[index % 4]
        forecasts.append({"prediction_id": f"forecast-{index}", "strategy_id": candidate, "session_id": f"outer-{index}",
                          "model_fit_id": "fit-test", "model_hash": model, "model_version": "test-model-v1", "transform_hash": transform,
                          "regime": "TEST_REGIME", "horizon_sessions": 3, "scenario_hash": scenario, "execution_convention_hash": convention,
                          "currency": "EUR", "source_hash": sha(f"test-original-forecast-{index}"), "dependence_group_id": "test-model-lineage",
                          "observed_at": stamp(day), "available_at": stamp(day), "generated_at": stamp(day, 9, 1),
                          "recorded_at": stamp(day, 9, 2), "decision_at": stamp(day, 10), "valid_until": stamp(day, 16),
                          "decision": "ABSTAIN" if outcome == "NON_FILL" else "ENTER", "probabilities": {event: "0.25" for event in EVENTS},
                          "payoff_sample_ids": [p["observation_id"] for p in payoffs], "label_end_at": stamp(day, 11), "outcome": outcome,
                          "label_available_at": stamp(day, 11), "label_recorded_at": stamp(day, 11, 1), "label_source_hash": sha(f"test-label-{index}"),
                          "opening_equity": "100000", "realized_gross_profit": payoff_gross[outcome], "realized_fixed_cost": "1", "realized_variable_cost": "1"})
    stress = [{"scenario_id": "base", "cost_multiple": "1", "spread_multiple": "1", "latency_ms": 0},
              {"scenario_id": "stress", "cost_multiple": "2", "spread_multiple": "2", "latency_ms": 250}]
    policy = {"schema_version": 1, "frozen_at": stamp(42), "benchmark_ids": ["champion", "cash", "simple_baseline"],
              "benchmark_parameters": {"champion": {"version": "test-champion-v1"}, "cash": {}, "simple_baseline": {"version": "test-long-only-v1"}},
              "benchmark_complexities": {"champion": 7, "cash": 0, "simple_baseline": 1}, "account_currency": "EUR", "stress_cases": stress,
              "neighborhood_steps": {"stop_atr_multiple": "0.1"}, "block_sessions": 3, "bootstrap_repetitions": 1200, "planned_looks": 1,
              "alpha": "0.05", "numeric_margin": "0.000000001", "annual_sessions": 252,
              "max_label_horizon_sessions": 3, "max_label_horizon_seconds": 345600, "embargo_seconds": 345660, "publication_latency_seconds": 60,
              "minimum_calibration_samples": 24, "minimum_calibration_sessions": 6, "minimum_calibration_samples_per_event": 2,
              "minimum_payoff_samples_per_event": 6, "maximum_forecast_age_seconds": 7200, "reliability_bins": 10,
              "maximum_drawdown_increase": "0", "maximum_cvar_increase": "0", "maximum_worst_loss_increase": "0", "maximum_turnover_increase": "0",
              "maximum_drawdown": "0.1", "maximum_exposure": "0.5", "maximum_session_return": "0.5", "maximum_payoff_abs_return": "0.5",
              "maximum_brier_deterioration": "0.01", "maximum_log_loss": "1.5", "maximum_ece": "0.1", "maximum_class_shift": "0.2",
              "maximum_brier_drift": "0.1", "minimum_forecast_coverage": "0.9", "maximum_payoff_mean_error": "0.001",
              "utility_risk_weight": "1", "utility_uncertainty_weight": "1", "utility_complexity_weight": "0", "cash_hurdle": "0.0001",
              "project_cost_account_currency": "1", "neighborhood_maximum_objective_drop": "0.001"}
    strategies = {candidate: parameters, "neighbor-test": neighbor, **policy["benchmark_parameters"]}
    equity, trades = [], []
    for strategy, spec in strategies.items():
        for case in stress:
            execution = {"engine_hash": engine, "dataset_hash": sha(source_manifest), "strategy_id": strategy, "parameters": deepcopy(spec), **case}
            run = {"strategy_id": strategy, "scenario_id": case["scenario_id"], "currency": "EUR", "complexity_count": 7 if strategy not in policy["benchmark_ids"] else policy["benchmark_complexities"][strategy],
                   "spread_multiple": case["spread_multiple"], "latency_ms": case["latency_ms"], "execution_spec": execution,
                   "execution_receipt_hash": sha(execution), "sessions": []}
            opening = Decimal("100000")
            gross = Decimal("120" if strategy == candidate else "115" if strategy == "neighbor-test" else "20" if strategy == "champion" else "10" if strategy == "simple_baseline" else "0")
            fixed, variable = (Decimal(0), Decimal(0)) if strategy == "cash" else (Decimal(1), Decimal(2))
            for index in range(36):
                day = 60 + index
                closing = opening + gross - fixed - variable * Decimal(case["cost_multiple"])
                closed = []
                if strategy == candidate and index % 4 != 3:
                    trade_id = f"trade-{case['scenario_id']}-{index}"
                    closed.append(trade_id)
                    trades.append({"trade_id": trade_id, "strategy_id": strategy, "scenario_id": case["scenario_id"], "state": "CLOSED",
                                   "entry_at": stamp(day, 10), "exit_at": stamp(day, 11), "recorded_at": stamp(day, 11, 1),
                                   "source_hash": sha(trade_id), "prediction_id": f"forecast-{index}", "scenario_hash": scenario})
                run["sessions"].append({"session_id": f"outer-{index}", "opening_equity": str(opening), "closing_equity": str(closing),
                                        "boundary_cash_flow": "0", "gross_pnl": str(gross), "fixed_cost": str(fixed), "variable_cost_unscaled": str(variable),
                                        "available_at": stamp(day, 16), "recorded_at": stamp(day, 16, 1), "unresolved_order_ids": [],
                                        "source_receipts": [sha(f"test-receipt-{strategy}-{index}")], "start_snapshot_hash": sha(f"test-end-{strategy}-{index - 1}"),
                                        "end_snapshot_hash": sha(f"test-end-{strategy}-{index}"), "exposure_fraction": "0" if strategy == "cash" else "0.2",
                                        "sector_gross_pnl": {"CASH" if strategy == "cash" else "TEST_SECTOR": str(gross)},
                                        "sector_exposure_fraction": {"CASH" if strategy == "cash" else "TEST_SECTOR": "0" if strategy == "cash" else "0.2"},
                                        "turnover": "0" if strategy == "cash" else "1", "orders_attempted": 0 if strategy == "cash" else 1,
                                        "entry_fills": 0 if strategy == "cash" else 1, "closed_trade_ids": closed})
                opening = closing
            equity.append(run)
    bundle = {"schema_version": 1, "evidence_class": "TEST_ONLY", "source_manifest": source_manifest, "calendar": {"sessions": sessions, "weeks": weeks},
              "approved_models": {"test-model-v1": model}, "approved_transforms": {"test-transform-v1": transform}, "samples": samples, "fits": [fit],
              "equity": equity, "trades": trades, "neighbors": [{"strategy_id": "neighbor-test", "parameters": neighbor}],
              "forecasts": forecasts, "payoff_observations": payoffs}
    request = {"schema_version": 1, "stage": "evaluate", "input_hash": sha("test-input"), "run_id": "weekly_test",
               "plan_hash": sha("test-plan"), "parameters_hash": sha(parameters),
               "manifest": {"dataset_hash": "", "account_id": "paper_test", "mode": "paper", "cutoff_at": stamp(108, 17), "champion_id": "champion", "random_seed": 7,
                            "component_hashes": {"calendar": sha(bundle["calendar"]), "models": sha(bundle["approved_models"]),
                                                 "calibrators": sha(bundle["approved_transforms"]), "engine": engine, "sealed_evaluator_policy": sha(policy)}},
               "plan": {"candidates": [parameters], "envelope": {"max_entry_allowance_fraction": "0.02", "min_stop_atr_multiple": "1", "max_stop_atr_multiple": "3",
                                                                "min_target_net_fraction": "0.01", "max_target_net_fraction": "0.1"},
                        "development_folds": [{"opens_at": stamp(27), "closes_at": stamp(30, 16)}], "max_cumulative_trials": 1,
                        "minimum_improvement": "0.0001", "objective": {"drawdown_weight": "1", "instability_weight": "1", "complexity_weight": "0"}},
               "trial": {"candidate_id": candidate, "parameters": parameters, "registered_at": stamp(43), "development": None, "failure": None}}
    search = {"schema_version": 1, "evidence_class": "TEST_ONLY", "trials": [{"trial_id": "trial-test", "candidate_id": candidate, "status": "COMPLETE",
               "account_id": "paper_test", "mode": "paper", "dataset_hash": "", "parameters_hash": request["parameters_hash"], "run_id": request["run_id"],
               "plan_hash": request["plan_hash"], "registered_at": stamp(43)}], "holdout_reads": []}
    shadow = {"schema_version": 1, "evidence_class": "TEST_ONLY", "candidate_id": candidate, "program_started_at": stamp(99), "observations": [], "trades": []}
    for index in range(8):
        shadow["observations"].append({"session_id": f"shadow-{index}", "first_observed_at": stamp(100 + index), "last_observed_at": stamp(100 + index, 16),
                                       "recorded_at": stamp(100 + index, 16, 1), "data_kind": "REAL", "order_mutations": 0,
                                       "observation_hashes": [sha(f"test-shadow-{index}-open"), sha(f"test-shadow-{index}-close")], "market_data_hash": sha(f"test-market-{index}")})
    lock = {"schema_version": 1, "evidence_class": "TEST_ONLY", "candidate_frozen_at": stamp(44), "sealed_at": stamp(43),
            "policy": policy, "bindings": {}, "request_sha256": "", "artifacts": {k: {"path": "/unused/test-only", "sha256": ""} for k in ("dataset", "search", "shadow")},
            "diagnostics_directory": "/unused/test-only"}
    reseal(request, lock, bundle, search, shadow)
    return request, lock, bundle, search, shadow


def reseal(request, lock, bundle, search, shadow):
    """TEST ONLY administrator normalization; never a production release path."""
    request["manifest"]["dataset_hash"] = sha(bundle["source_manifest"])
    request["manifest"]["component_hashes"].update(calendar=sha(bundle["calendar"]), models=sha(bundle["approved_models"]),
                                                  calibrators=sha(bundle["approved_transforms"]), sealed_evaluator_policy=sha(lock["policy"]))
    for trial in search["trials"]:
        trial["dataset_hash"] = request["manifest"]["dataset_hash"]
    lock["bindings"] = {"input_hash": request["input_hash"], "run_id": request["run_id"], "plan_hash": request["plan_hash"],
                        "parameters_hash": request["parameters_hash"], "dataset_hash": request["manifest"]["dataset_hash"],
                        "candidate_id": request["trial"]["candidate_id"], "account_id": request["manifest"]["account_id"], "mode": request["manifest"]["mode"]}
    lock["request_sha256"] = sha(request)
    for key, value in (("dataset", bundle), ("search", search), ("shadow", shadow)):
        lock["artifacts"][key]["sha256"] = sha(value)


def copied_fixture():
    return deepcopy(fixture())

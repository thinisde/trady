"""TEST_ONLY trade lifecycle and pending-cohort causality regressions."""
import pathlib
import sys
import unittest
from copy import deepcopy

LAB = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(LAB))
from evaluator.common import EVENTS, canonical, sha
from evaluator.run import evaluate
from sealed_fixture import fixture, reseal, stamp


class EvaluatorLifecycle(unittest.TestCase):
    def setUp(self):
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        self.source_hash = self.request["manifest"]["dataset_hash"]

    def seal(self):
        reseal(self.request, self.lock, self.bundle, self.search, self.shadow)
        self.assertEqual(self.request["manifest"]["dataset_hash"], self.source_hash)

    def evaluate(self):
        payload, diagnostics = evaluate(self.request, self.lock, self.bundle, self.search, self.shadow)
        self.assertEqual(self.lock["evidence_class"], "TEST_ONLY")
        self.assertTrue(all(payload[gate] is False for gate in diagnostics["computed_gates_before_evidence_class"]))
        canonical(diagnostics)
        return payload, diagnostics

    def open_trade(self, identity="trade-base-0"):
        trade = next(row for row in self.bundle["trades"] if row["trade_id"] == identity)
        trade.update(state="OPEN", exit_at=None)
        self.unreference(identity)
        return trade

    def unreference(self, identity):
        for run in self.bundle["equity"]:
            for row in run["sessions"]:
                row["closed_trade_ids"] = [item for item in row["closed_trade_ids"] if item != identity]

    def extra_trade(self, strategy="neighbor-test", state="OPEN", **changes):
        identity = "TEST_ONLY-trade-" + str(len(self.bundle["trades"]))
        row = {"trade_id": identity, "strategy_id": strategy, "scenario_id": "base", "state": state,
               "entry_at": stamp(60, 10), "exit_at": stamp(60, 11) if state == "CLOSED" else None,
               "recorded_at": stamp(60, 11, 1), "source_hash": sha(identity), **changes}
        self.bundle["trades"].append(row)
        return row

    def distinct_model(self, index=0):
        fit = deepcopy(self.bundle["fits"][0])
        fit.update(fit_id="TEST_ONLY-second-fit", model_hash=sha("TEST_ONLY-second-model"))
        self.bundle["fits"].append(fit)
        version = "TEST_ONLY-second-model-v1"
        self.bundle["approved_models"][version] = fit["model_hash"]
        row = self.bundle["forecasts"][index]
        row.update(model_fit_id=fit["fit_id"], model_hash=fit["model_hash"], model_version=version)
        return row, version

    def pending(self, row):
        row.update(outcome=None, label_available_at=None, label_recorded_at=None, label_source_hash=None,
                   opening_equity=None, realized_gross_profit=None, realized_fixed_cost=None, realized_variable_cost=None)

    def model_cohort(self):
        original = deepcopy(self.bundle["forecasts"])
        first, version = self.distinct_model()
        second_fit, second_hash = first["model_fit_id"], first["model_hash"]
        self.bundle["forecasts"][0] = deepcopy(original[0])
        copies = []
        for row in original:
            row.update(prediction_id="TEST_ONLY-second-" + row["prediction_id"], model_fit_id=second_fit,
                       model_hash=second_hash, model_version=version)
            row["source_hash"] = sha(row["prediction_id"])
            copies.append(row)
        self.bundle["forecasts"].extend(copies)
        return copies, version

    def shadow_trade(self, state="OPEN", **changes):
        row = {"trade_id": "TEST_ONLY-shadow-lifecycle", "candidate_id": self.request["trial"]["candidate_id"],
               "source_hash": sha("TEST_ONLY-shadow-lifecycle"), "state": state,
               "entry_session_id": "shadow-0", "exit_session_id": "shadow-1" if state == "CLOSED" else None,
               "entry_at": stamp(100, 10), "exit_at": stamp(101, 11) if state == "CLOSED" else None,
               "recorded_at": stamp(101, 11, 1), "net_profit": "0" if state == "CLOSED" else None, **changes}
        self.shadow["trades"] = [row]
        return row

    def test_open_entry_cannot_use_an_expired_forecast(self):
        trade = self.open_trade()
        trade["entry_at"] = stamp(60, 10, 2)
        self.bundle["forecasts"][0]["valid_until"] = stamp(60, 10, 1)
        self.seal()
        with self.assertRaisesRegex(ValueError, "forecast expiry"):
            self.evaluate()

    def test_open_entry_and_recording_after_cutoff_are_rejected(self):
        trade = self.open_trade()
        trade.update(entry_at=stamp(109, 10), recorded_at=stamp(109, 11))
        self.seal()
        with self.assertRaisesRegex(ValueError, "trade.*cutoff|trade.*chronology"):
            self.evaluate()

    def test_open_recording_after_cutoff_is_rejected_independently_of_entry(self):
        self.open_trade()["recorded_at"] = stamp(109, 11)
        self.seal()
        with self.assertRaisesRegex(ValueError, "trade.*cutoff|trade.*chronology"):
            self.evaluate()

    def test_pending_only_enter_model_cannot_disappear_from_calibration(self):
        row, version = self.distinct_model()
        self.pending(row)
        self.seal()
        self.assertEqual(sum(trade["prediction_id"] == row["prediction_id"] for trade in self.bundle["trades"]), 2)
        _, diagnostics = self.evaluate()
        report = diagnostics["predictions"]
        self.assertEqual(report["samples"], 35)
        self.assertEqual(report["pending_labels"], 1)
        self.assertFalse(report["eligible"])
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])
        cohort = report["cohorts"][version + "|TEST_REGIME|3"]
        self.assertEqual(cohort["samples"], 0)
        self.assertTrue(cohort["required_for_acceptance"])
        self.assertFalse(cohort["eligible"])

    def test_valid_open_holding_remains_unrealized_and_is_not_counted_closed(self):
        _, before = self.evaluate()
        self.open_trade()
        self.seal()
        payload, after = self.evaluate()
        candidate = self.request["trial"]["candidate_id"]
        measured = after["statistics"][candidate + "|base"]
        self.assertTrue(all(after["computed_gates_before_evidence_class"].values()))
        self.assertEqual(measured["open_trades"], 1)
        self.assertEqual(measured["closed_trades"], 26)
        self.assertEqual(payload["closed_trades"], 26)
        self.assertEqual(measured["net_return"], before["statistics"][candidate + "|base"]["net_return"])

    def test_valid_open_entry_recording_and_calendar_boundaries_are_allowed(self):
        for entry, recorded, expiry in ((stamp(60, 10), stamp(60, 10), stamp(60, 16)),
                                        (stamp(60, 15, 59), stamp(60, 17), stamp(60, 16, 1))):
            with self.subTest(entry=entry):
                trade = self.open_trade()
                trade.update(entry_at=entry, recorded_at=recorded)
                trade.pop("exit_at")  # An absent exit is also an unrealized holding.
                self.bundle["forecasts"][0]["valid_until"] = expiry
                self.seal()
                _, diagnostics = self.evaluate()
                self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_expiry_is_strict_for_both_open_and_closed_entries(self):
        for state in ("OPEN", "CLOSED"):
            with self.subTest(state=state):
                self.setUp()
                trade = self.open_trade() if state == "OPEN" else self.bundle["trades"][0]
                trade["entry_at"] = stamp(60, 10, 1)
                self.bundle["forecasts"][0]["valid_until"] = stamp(60, 10, 1)
                self.seal()
                with self.assertRaisesRegex(ValueError, "forecast expiry"):
                    self.evaluate()

    def test_all_strategy_trade_rows_require_causal_recording_and_real_sources(self):
        for strategy in ("neighbor-test", "champion", "simple_baseline"):
            for state in ("OPEN", "CLOSED"):
                for field, value, message in (("recorded_at", stamp(109, 11), "trade.*cutoff"),
                                               ("recorded_at", stamp(60, 9, 59), "trade.*chronology"),
                                               ("source_hash", "TEST_ONLY-unresolved", "trade provenance")):
                    with self.subTest(strategy=strategy, state=state, field=field, value=value):
                        self.setUp()
                        self.extra_trade(strategy, state, **{field: value})
                        self.seal()
                        with self.assertRaisesRegex(ValueError, message):
                            self.evaluate()

    def test_non_candidate_open_and_closed_entries_need_covered_exchange_sessions(self):
        for strategy in ("neighbor-test", "champion"):
            for state in ("OPEN", "CLOSED"):
                with self.subTest(strategy=strategy, state=state):
                    self.setUp()
                    self.extra_trade(strategy, state, entry_at=stamp(60, 8, 59))
                    self.seal()
                    with self.assertRaisesRegex(ValueError, "trade entry.*exchange sessions"):
                        self.evaluate()

    def test_unreferenced_closed_trades_still_validate_actual_closure(self):
        for changes, message in (({"exit_at": None}, "CLOSED trade exit missing"),
                                 ({"exit_at": stamp(60, 10)}, "closure chronology"),
                                 ({"exit_at": stamp(60, 12)}, "closure chronology"),
                                 ({"exit_at": stamp(60, 20), "recorded_at": stamp(60, 21)}, "trade exit.*exchange sessions")):
            with self.subTest(changes=changes):
                self.setUp()
                self.extra_trade("champion", "CLOSED", **changes)
                self.seal()
                with self.assertRaisesRegex(ValueError, message):
                    self.evaluate()

    def test_valid_unreferenced_closed_trade_is_not_invented_into_report_counts(self):
        self.unreference("trade-base-0")
        self.seal()
        payload, diagnostics = self.evaluate()
        self.assertEqual(payload["closed_trades"], 26)
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_benchmark_carry_in_holdings_do_not_acquire_candidate_holding_limits(self):
        for state in ("OPEN", "CLOSED"):
            with self.subTest(state=state):
                self.setUp()
                self.extra_trade("champion", state, entry_at=stamp(0, 10))
                self.seal()
                _, diagnostics = self.evaluate()
                self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
                self.assertEqual(diagnostics["statistics"]["champion|base"]["open_trades"], int(state == "OPEN"))

    def test_missing_and_unknown_trade_states_cannot_be_silently_ignored(self):
        for state in ("PENDING", "CANCELLED", "open", None):
            with self.subTest(state=state):
                self.setUp()
                self.extra_trade(state=state)
                self.seal()
                with self.assertRaisesRegex(ValueError, "invalid trade state"):
                    self.evaluate()
        for field in ("state", "entry_at", "recorded_at", "source_hash"):
            with self.subTest(missing=field):
                self.setUp()
                self.extra_trade().pop(field)
                self.seal()
                with self.assertRaisesRegex(ValueError, "missing trade lifecycle fields"):
                    self.evaluate()

    def test_open_exit_or_closed_reference_cannot_fabricate_a_realized_trade(self):
        self.open_trade()["exit_at"] = stamp(60, 11)
        self.seal()
        with self.assertRaisesRegex(ValueError, "OPEN trade has a realized exit"):
            self.evaluate()
        self.setUp()
        self.bundle["trades"][0].update(state="OPEN", exit_at=None)
        self.seal()
        with self.assertRaisesRegex(ValueError, "open or misbound trade counted as closed"):
            self.evaluate()

    def test_missing_record_or_wrong_run_cannot_satisfy_a_closed_reference(self):
        for change in ("missing", "strategy_id", "scenario_id"):
            with self.subTest(change=change):
                self.setUp()
                if change == "missing":
                    self.bundle["trades"] = [row for row in self.bundle["trades"] if row["trade_id"] != "trade-base-0"]
                else:
                    self.bundle["trades"][0][change] = "neighbor-test" if change == "strategy_id" else "stress"
                self.seal()
                with self.assertRaisesRegex(ValueError, "missing/duplicate closed trade|misbound trade counted as closed"):
                    self.evaluate()

    def test_unreferenced_trade_scope_and_cash_are_not_exempt_from_validation(self):
        for strategy, scenario, message in (("TEST_ONLY-unknown", "base", "no evaluated strategy/run"),
                                             ("champion", "TEST_ONLY-unknown", "no evaluated strategy/run"),
                                             ("cash", "base", "cash benchmark contains asset trading")):
            with self.subTest(strategy=strategy, scenario=scenario):
                self.setUp()
                self.extra_trade(strategy, scenario_id=scenario)
                self.seal()
                with self.assertRaisesRegex(ValueError, message):
                    self.evaluate()

    def test_open_entry_still_requires_the_original_enter_forecast_and_clock(self):
        for mutation, message in (("missing", "no original forecast"), ("abstain", "trade forecast/scenario/decision"),
                                  ("generated", "backfilled after the decision"), ("recorded", "backfilled after the decision"),
                                  ("fit", "generated before.*fit completed"), ("entry", "trade forecast/scenario/decision"),
                                  ("horizon", "holding-session horizon")):
            with self.subTest(mutation=mutation):
                self.setUp()
                trade = self.open_trade()
                row = self.bundle["forecasts"][0]
                if mutation == "missing":
                    trade.pop("prediction_id")
                elif mutation == "abstain":
                    row["decision"] = "ABSTAIN"
                elif mutation == "generated":
                    row["generated_at"] = stamp(60, 10, 1)
                elif mutation == "recorded":
                    row["recorded_at"] = stamp(60, 10, 1)
                elif mutation == "fit":
                    self.bundle["fits"][0]["fitted_at"] = stamp(60, 8, 59)
                    row.update(observed_at=stamp(60, 8), available_at=stamp(60, 8),
                               generated_at=stamp(60, 8, 1), recorded_at=stamp(60, 8, 2))
                elif mutation == "horizon":
                    trade.update(entry_at=stamp(63, 10), recorded_at=stamp(63, 11))
                    row["valid_until"] = stamp(64, 16)
                else:
                    trade["entry_at"] = stamp(60, 9, 59)
                self.seal()
                with self.assertRaisesRegex(ValueError, message):
                    self.evaluate()

    def test_optional_neighbor_and_benchmark_forecast_links_are_scope_bound(self):
        for strategy in ("neighbor-test", "champion"):
            with self.subTest(strategy=strategy):
                self.setUp()
                self.extra_trade(strategy, prediction_id="forecast-0", scenario_hash=self.bundle["forecasts"][0]["scenario_hash"])
                self.seal()
                with self.assertRaisesRegex(ValueError, "trade forecast/scenario/decision binding"):
                    self.evaluate()

    def test_valid_optional_neighbor_forecast_has_the_same_open_expiry_gate(self):
        for expired in (False, True):
            with self.subTest(expired=expired):
                self.setUp()
                forecast = {**self.bundle["forecasts"][0], "prediction_id": "TEST_ONLY-neighbor-forecast",
                            "strategy_id": "neighbor-test", "source_hash": sha("TEST_ONLY-neighbor-forecast")}
                if expired:
                    forecast["valid_until"] = stamp(60, 10, 1)
                self.bundle["forecasts"].append(forecast)
                self.extra_trade(prediction_id=forecast["prediction_id"], scenario_hash=forecast["scenario_hash"], entry_at=stamp(60, 10, 2))
                self.seal()
                if expired:
                    with self.assertRaisesRegex(ValueError, "forecast expiry"):
                        self.evaluate()
                else:
                    _, diagnostics = self.evaluate()
                    self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
                    self.assertEqual(diagnostics["statistics"]["neighbor-test|base"]["open_trades"], 1)

    def test_shadow_open_records_are_real_causal_unrealized_observations(self):
        self.shadow_trade()
        self.seal()
        payload, diagnostics = self.evaluate()
        self.assertEqual(payload["shadow_closed_trades"], 0)
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        for changes, message in (({"recorded_at": stamp(109, 11)}, "trade.*cutoff"),
                                  ({"entry_at": stamp(109, 10), "recorded_at": stamp(109, 11)}, "trade.*cutoff"),
                                  ({"state": "PENDING"}, "invalid trade state"),
                                  ({"exit_at": stamp(101, 11)}, "OPEN trade has a realized exit"),
                                  ({"entry_session_id": "shadow-1"}, "shadow trade entry/program/calendar")):
            with self.subTest(changes=changes):
                self.shadow_trade(**changes)
                self.seal()
                with self.assertRaisesRegex(ValueError, message):
                    self.evaluate()

    def test_uncovered_shadow_closures_still_need_real_state_time_and_horizon(self):
        self.shadow_trade("CLOSED", entry_session_id="shadow-6", exit_session_id="shadow-8",
                          entry_at=stamp(106, 10), exit_at=stamp(108, 11), recorded_at=stamp(108, 11, 1))
        self.seal()
        payload, _ = self.evaluate()
        self.assertEqual(payload["shadow_closed_trades"], 0)
        self.shadow_trade("CLOSED", exit_session_id="shadow-8", exit_at=stamp(108, 11), recorded_at=stamp(108, 11, 1))
        self.seal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()

    def test_distinct_matured_single_forecast_cohort_remains_an_ineligible_control(self):
        _, version = self.distinct_model()
        self.seal()
        _, diagnostics = self.evaluate()
        report = diagnostics["predictions"]
        cohort = report["cohorts"][version + "|TEST_REGIME|3"]
        self.assertEqual(cohort["samples"], 1)
        self.assertEqual(cohort["pending_labels"], 0)
        self.assertTrue(cohort["required_for_acceptance"])
        self.assertFalse(cohort["eligible"])
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])

    def test_supported_existing_cohort_can_mix_matured_and_null_pending_labels(self):
        self.pending(self.bundle["forecasts"][0])
        self.seal()
        _, diagnostics = self.evaluate()
        report = diagnostics["predictions"]
        cohort = next(iter(report["cohorts"].values()))
        self.assertEqual(report["samples"], 35)
        self.assertEqual(report["brier"], 0.75)
        self.assertEqual(report["payoff_mean_error"], 0)
        self.assertEqual(cohort["forecasts"], 36)
        self.assertEqual(cohort["samples"], 35)
        self.assertEqual(cohort["pending_labels"], 1)
        self.assertEqual(cohort["enter_forecasts"], 27)
        self.assertTrue(cohort["eligible"])
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_supported_distinct_models_can_each_mix_matured_and_pending(self):
        copies, _ = self.model_cohort()
        self.pending(self.bundle["forecasts"][0])
        self.pending(copies[0])
        self.seal()
        _, diagnostics = self.evaluate()
        report = diagnostics["predictions"]
        self.assertEqual(report["samples"], 70)
        self.assertEqual(report["pending_labels"], 2)
        self.assertEqual(len(report["cohorts"]), 2)
        for cohort in report["cohorts"].values():
            self.assertTrue(cohort["eligible"])
            self.assertTrue(cohort["required_for_acceptance"])
            self.assertEqual(cohort["samples"], 35)
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_pending_only_abstain_cohort_is_diagnostic_without_borrowing_calibration(self):
        row, version = self.distinct_model(3)
        self.assertEqual(row["decision"], "ABSTAIN")
        self.pending(row)
        self.seal()
        _, diagnostics = self.evaluate()
        cohort = diagnostics["predictions"]["cohorts"][version + "|TEST_REGIME|3"]
        self.assertFalse(cohort["eligible"])
        self.assertFalse(cohort["required_for_acceptance"])
        self.assertEqual(cohort["samples"], 0)
        self.assertEqual(cohort["abstain_forecasts"], 1)
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_matured_abstain_cohorts_keep_the_existing_calibration_gate(self):
        _, version = self.distinct_model(3)
        self.seal()
        _, diagnostics = self.evaluate()
        cohort = diagnostics["predictions"]["cohorts"][version + "|TEST_REGIME|3"]
        self.assertEqual(cohort["samples"], 1)
        self.assertTrue(cohort["required_for_acceptance"])
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])

    def test_existing_entry_cannot_hide_an_uncalibrated_model_by_claiming_abstention(self):
        row, _ = self.distinct_model()
        self.pending(row)
        row["decision"] = "ABSTAIN"
        self.seal()
        with self.assertRaisesRegex(ValueError, "trade forecast/scenario/decision binding"):
            self.evaluate()

    def test_all_pending_labels_remain_unscored_and_cannot_approve_entries(self):
        for row in self.bundle["forecasts"]:
            self.pending(row)
        self.seal()
        _, diagnostics = self.evaluate()
        report = diagnostics["predictions"]
        self.assertEqual(report["samples"], 0)
        self.assertEqual(report["pending_labels"], 36)
        for field in ("brier", "log_loss", "payoff_mean_error", "payoff_rmse"):
            self.assertIsNone(report[field])
        cohort = next(iter(report["cohorts"].values()))
        self.assertTrue(cohort["required_for_acceptance"])
        self.assertFalse(cohort["eligible"])
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])

    def test_pending_labels_cannot_fill_a_missing_real_outcome_class(self):
        for row in self.bundle["forecasts"]:
            if row["outcome"] == "TARGET_FIRST":
                self.pending(row)
        self.seal()
        _, diagnostics = self.evaluate()
        report = diagnostics["predictions"]
        self.assertEqual(report["samples"], 27)
        self.assertEqual(report["class_counts"]["TARGET_FIRST"], 0)
        self.assertFalse(report["eligible"])

    def test_pending_entry_still_requires_supported_conditional_payoffs(self):
        row, version = self.distinct_model()
        self.pending(row)
        row["payoff_sample_ids"] = [next(item["observation_id"] for item in self.bundle["payoff_observations"] if item["event"] == event)
                                    for event in EVENTS]
        self.lock["policy"]["bootstrap_repetitions"] = 2400
        self.seal()
        _, diagnostics = self.evaluate()
        report = diagnostics["predictions"]
        own = report["cohorts"][version + "|TEST_REGIME|3"]
        self.assertFalse(own["conditional_payoff_support"])
        self.assertFalse(report["conditional_payoff_support"])
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])

    def test_same_version_refit_does_not_invent_a_new_statistical_cohort(self):
        fit = deepcopy(self.bundle["fits"][0])
        fit["fit_id"] = "TEST_ONLY-same-model-refit"
        self.bundle["fits"].append(fit)
        row = self.bundle["forecasts"][0]
        row["model_fit_id"] = fit["fit_id"]
        self.pending(row)
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertEqual(len(diagnostics["predictions"]["cohorts"]), 1)
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_pending_enter_regime_and_horizon_cohorts_need_their_own_matured_support(self):
        for dimension in ("regime", "horizon_sessions"):
            with self.subTest(dimension=dimension):
                self.setUp()
                row = self.bundle["forecasts"][0]
                value = "TEST_ONLY-new-regime" if dimension == "regime" else 2
                row[dimension] = value
                if dimension == "regime":
                    next(item for item in self.bundle["calendar"]["sessions"] if item["session_id"] == row["session_id"])["regime"] = value
                ids = []
                for item in list(self.bundle["payoff_observations"]):
                    clone = {**item, "observation_id": "TEST_ONLY-cohort-" + item["observation_id"], dimension: value}
                    ids.append(clone["observation_id"])
                    self.bundle["payoff_observations"].append(clone)
                row["payoff_sample_ids"] = ids
                self.pending(row)
                self.lock["policy"]["bootstrap_repetitions"] = 2400
                self.seal()
                _, diagnostics = self.evaluate()
                key = "|".join(map(str, (row["model_version"], row["regime"], row["horizon_sessions"])))
                cohort = diagnostics["predictions"]["cohorts"][key]
                self.assertEqual(cohort["samples"], 0)
                self.assertTrue(cohort["required_for_acceptance"])
                self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])

    def test_pending_only_cohort_cannot_evade_diagnostic_identity_collision_checks(self):
        base = self.bundle["forecasts"][0]["model_version"]
        other = base + "|A"
        self.bundle["approved_models"][other] = self.bundle["approved_models"][base]
        ids = {"A|B": [], "B": []}
        for item in list(self.bundle["payoff_observations"]):
            item["regime"] = "A|B"
            ids["A|B"].append(item["observation_id"])
            clone = {**item, "observation_id": "TEST_ONLY-collision-" + item["observation_id"], "regime": "B"}
            ids["B"].append(clone["observation_id"])
            self.bundle["payoff_observations"].append(clone)
        for index, row in enumerate(self.bundle["forecasts"]):
            row.update(model_version=base if index == 0 else other, regime="A|B" if index == 0 else "B")
            row["payoff_sample_ids"] = ids[row["regime"]]
            next(item for item in self.bundle["calendar"]["sessions"] if item["session_id"] == row["session_id"])["regime"] = row["regime"]
        self.pending(self.bundle["forecasts"][0])
        self.lock["policy"]["bootstrap_repetitions"] = 2400
        self.seal()
        with self.assertRaisesRegex(ValueError, "ambiguous calibration cohort diagnostic identity"):
            self.evaluate()

    def test_equal_time_forecasts_cannot_change_acceptance_by_export_order(self):
        original = self.bundle["forecasts"][17]
        for suffix, actual, other in (("a", "0.1", "0.3"), ("b", "0.7", "0.1"), ("c", "0.25", "0.25")):
            identity = "z_TEST_ONLY_tie_" + suffix
            clone = deepcopy(original)
            clone.update(prediction_id=identity, source_hash=sha(identity), decision="ABSTAIN",
                         probabilities={event: actual if event == original["outcome"] else other for event in EVENTS})
            self.bundle["forecasts"].append(clone)
        self.lock["policy"]["maximum_brier_drift"] = "0"
        self.seal()
        _, first = self.evaluate()
        self.assertTrue(all(first["computed_gates_before_evidence_class"].values()))
        self.bundle["forecasts"].reverse()
        self.seal()
        _, second = self.evaluate()
        self.assertEqual(first["predictions"]["brier_drift"], second["predictions"]["brier_drift"])
        self.assertEqual(first["computed_gates_before_evidence_class"], second["computed_gates_before_evidence_class"])


if __name__ == "__main__":
    unittest.main()

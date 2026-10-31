"""TEST_ONLY original-payoff identity and exclusive exchange-entry regressions."""
import pathlib
import sys
import unittest
from copy import deepcopy
from fractions import Fraction

LAB = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(LAB))
from evaluator.common import EVENTS, canonical, sha
from evaluator.run import evaluate
from sealed_fixture import fixture, reseal, stamp


def exact(value):
    return Fraction(int(value["numerator"]), int(value["denominator"]))


class EvaluatorOutcomeIdentity(unittest.TestCase):
    def setUp(self):
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        self.source_hash = self.request["manifest"]["dataset_hash"]

    def seal(self):
        reseal(self.request, self.lock, self.bundle, self.search, self.shadow)
        self.assertEqual(self.request["manifest"]["dataset_hash"], self.source_hash)

    def evaluate(self):
        payload, diagnostics = evaluate(self.request, self.lock, self.bundle, self.search, self.shadow)
        self.assertTrue(all(payload[gate] is False for gate in diagnostics["computed_gates_before_evidence_class"]))
        canonical(diagnostics)
        return payload, diagnostics

    def alias(self, row=None, *, sample_alias=False, selected=True, **changes):
        row = row or self.bundle["payoff_observations"][0]
        clone = {**row, "observation_id": "TEST_ONLY-alias-" + row["observation_id"], **changes}
        if sample_alias:
            sample = deepcopy(next(s for s in self.bundle["samples"] if s["sample_id"] == row["sample_id"]))
            sample["sample_id"] = "TEST_ONLY-alias-" + sample["sample_id"]
            self.bundle["samples"].append(sample)
            self.bundle["fits"][0]["train"]["sample_ids"].append(sample["sample_id"])
            clone["sample_id"] = sample["sample_id"]
        self.bundle["payoff_observations"].append(clone)
        if selected:
            for forecast in self.bundle["forecasts"]:
                forecast["payoff_sample_ids"].append(clone["observation_id"])
        return clone

    def weighting_probe(self):
        self.lock["policy"].update(utility_risk_weight="0", utility_uncertainty_weight="0",
                                   utility_complexity_weight="0", cash_hurdle="0.0003")
        for row in self.bundle["payoff_observations"]:
            row.update(fixed_cost="0", variable_cost="0", gross_profit=(
                "2000" if row["event"] == EVENTS[0] and row["sample_id"].startswith("train-0-")
                else "-100" if row["event"] == EVENTS[1] else "0"))
        for row in self.bundle["forecasts"]:
            row.update(realized_gross_profit="100" if row["outcome"] == EVENTS[0]
                       else "-100" if row["outcome"] == EVENTS[1] else "0",
                       realized_fixed_cost="0", realized_variable_cost="0")

    def new_original(self, event, *, same_source=False):
        row = next(p for p in self.bundle["payoff_observations"] if p["event"] == event)
        clone = self.alias(row, sample_alias=True)
        sample = next(s for s in self.bundle["samples"] if s["sample_id"] == clone["sample_id"])
        if same_source:
            sample.update(label_start_at=stamp(0, 10, 5), label_end_at=stamp(0, 11, 5),
                          label_available_at=stamp(0, 11, 5), label_recorded_at=stamp(0, 11, 6))
            clone.update(available_at=stamp(0, 11, 6), recorded_at=stamp(0, 11, 7))
        else:
            sample["label_source_hash"] = sha("TEST_ONLY-independent-original-" + event)
            clone["source_hash"] = sample["label_source_hash"]
        return clone

    def unreference(self, identity):
        for run in self.bundle["equity"]:
            for row in run["sessions"]:
                row["closed_trade_ids"] = [item for item in row["closed_trade_ids"] if item != identity]

    def candidate_entry(self, state, entry):
        trade = self.bundle["trades"][0]
        trade.update(state=state, entry_at=entry, exit_at=stamp(61, 11) if state == "CLOSED" else None,
                     recorded_at=stamp(61, 11, 1))
        self.unreference(trade["trade_id"])
        if state == "CLOSED":
            run = next(r for r in self.bundle["equity"] if r["strategy_id"] == trade["strategy_id"] and r["scenario_id"] == "base")
            run["sessions"][1]["closed_trade_ids"].append(trade["trade_id"])
        self.bundle["forecasts"][0].update(valid_until=stamp(61, 16), label_end_at=stamp(61, 11),
                                             label_available_at=stamp(61, 11), label_recorded_at=stamp(61, 11, 1))
        return trade

    def other_entry(self, strategy, state, entry):
        row = {"trade_id": "TEST_ONLY-boundary", "strategy_id": strategy, "scenario_id": "base", "state": state,
               "entry_at": entry, "exit_at": stamp(61, 11) if state == "CLOSED" else None,
               "recorded_at": stamp(61, 11, 1), "source_hash": sha("TEST_ONLY-boundary")}
        self.bundle["trades"].append(row)
        return row

    def shadow_entry(self, state, day=100, entry=None):
        row = {"trade_id": "TEST_ONLY-shadow-boundary", "candidate_id": self.request["trial"]["candidate_id"],
               "source_hash": sha("TEST_ONLY-shadow-boundary"), "state": state,
               "entry_session_id": "shadow-" + str(day - 100),
               "exit_session_id": "shadow-" + str(day - 99) if state == "CLOSED" else None,
               "entry_at": entry or stamp(day, 16), "exit_at": stamp(day + 1, 16) if state == "CLOSED" else None,
               "recorded_at": stamp(day + 1, 16, 1), "net_profit": "0" if state == "CLOSED" else None}
        self.shadow["trades"] = [row]
        return row

    def test_renamed_payoffs_cannot_supply_thirteenth_original_per_event(self):
        self.lock["policy"]["minimum_payoff_samples_per_event"] = 13
        self.seal()
        _, before = self.evaluate()
        self.assertFalse(before["predictions"]["conditional_payoff_support"])
        self.assertFalse(before["computed_gates_before_evidence_class"]["economic_feasibility_passed"])
        for event in EVENTS:
            self.alias(next(p for p in self.bundle["payoff_observations"] if p["event"] == event))
        self.seal()
        with self.assertRaisesRegex(ValueError, "reused original payoff outcome"):
            self.evaluate()

    def test_favorable_alias_cannot_cross_the_frozen_cash_hurdle(self):
        self.weighting_probe()
        self.seal()
        _, before = self.evaluate()
        utility = before["predictions"]["forecast_utilities"][0]
        self.assertEqual(exact(utility["exact"]["utility"]), Fraction(1, 6000))
        self.assertEqual(utility["conditional_means"][EVENTS[0]]["samples"], 12)
        self.assertFalse(before["predictions"]["positive_entry_utility"])
        self.alias()
        self.seal()
        with self.assertRaisesRegex(ValueError, "reused original payoff outcome"):
            self.evaluate()

    def test_renaming_both_sample_and_observation_ids_does_not_create_an_outcome(self):
        self.alias(sample_alias=True)
        self.seal()
        with self.assertRaisesRegex(ValueError, "reused original payoff outcome"):
            self.evaluate()

    def test_feature_aliases_still_share_the_original_causal_outcome(self):
        for feature in ("feature_start_at", "feature_end_at", "available_at", "recorded_at"):
            with self.subTest(feature=feature):
                self.setUp()
                alias = self.alias(sample_alias=True)
                sample = next(s for s in self.bundle["samples"] if s["sample_id"] == alias["sample_id"])
                sample[feature] = {"feature_start_at": stamp(0, 7), "feature_end_at": stamp(0, 8, 59),
                                   "available_at": stamp(0, 9).replace("09:00:00", "09:00:01"),
                                   "recorded_at": stamp(0, 9)}[feature]
                self.seal()
                with self.assertRaisesRegex(ValueError, "reused original payoff outcome"):
                    self.evaluate()

    def test_conflicting_original_payoff_facts_are_rejected_even_if_unselected(self):
        for field, value in (("gross_profit", "1001"), ("fixed_cost", "2"),
                             ("variable_cost", "2"), ("opening_equity", "100001")):
            with self.subTest(field=field):
                self.setUp()
                self.alias(selected=False, **{field: value})
                self.seal()
                with self.assertRaisesRegex(ValueError, "conflicting original payoff aliases"):
                    self.evaluate()

    def test_source_alias_cannot_relabel_one_original_outcome_as_another_event(self):
        alias = self.alias(sample_alias=True, selected=False)
        sample = next(s for s in self.bundle["samples"] if s["sample_id"] == alias["sample_id"])
        alias["event"] = sample["outcome"] = EVENTS[1]
        self.seal()
        with self.assertRaisesRegex(ValueError, "conflicting original outcome aliases"):
            self.evaluate()

    def test_original_label_availability_cannot_be_rewritten_through_a_sample_alias(self):
        for field in ("label_available_at", "label_recorded_at"):
            with self.subTest(field=field):
                self.setUp()
                alias = self.alias(sample_alias=True, selected=False)
                sample = next(s for s in self.bundle["samples"] if s["sample_id"] == alias["sample_id"])
                sample[field] = stamp(0, 11)
                if field == "label_available_at":
                    sample[field] = stamp(0, 11).replace("11:00:00", "11:00:01")
                self.seal()
                with self.assertRaisesRegex(ValueError, "conflicting original outcome aliases"):
                    self.evaluate()

    def test_removing_the_original_payoff_does_not_hide_conflicting_sample_aliases(self):
        for field, value in (("outcome", EVENTS[1]), ("label_recorded_at", stamp(0, 11))):
            with self.subTest(field=field):
                self.setUp()
                original = self.bundle["payoff_observations"][0]
                alias = self.alias(original, sample_alias=True, selected=False)
                sample = next(s for s in self.bundle["samples"] if s["sample_id"] == alias["sample_id"])
                sample[field] = value
                if field == "outcome":
                    alias["event"] = value
                self.bundle["payoff_observations"].remove(original)
                for row in self.bundle["forecasts"]:
                    row["payoff_sample_ids"] = [alias["observation_id"] if identity == original["observation_id"] else identity
                                                for identity in row["payoff_sample_ids"]]
                self.seal()
                with self.assertRaisesRegex(ValueError, "conflicting original outcome aliases"):
                    self.evaluate()

    def test_unselected_sample_aliases_cannot_rewrite_original_label_facts(self):
        sample = deepcopy(self.bundle["samples"][0])
        sample.update(sample_id="TEST_ONLY-unselected-sample-alias", outcome=EVENTS[1])
        self.bundle["samples"].append(sample)
        self.seal()
        with self.assertRaisesRegex(ValueError, "conflicting original outcome aliases"):
            self.evaluate()

    def test_payoff_availability_and_recording_aliases_cannot_change_causal_facts(self):
        for field in ("available_at", "recorded_at"):
            with self.subTest(field=field):
                self.setUp()
                self.alias(selected=False, **{field: stamp(0, 11, 3)})
                self.seal()
                with self.assertRaisesRegex(ValueError, "conflicting original payoff aliases"):
                    self.evaluate()

    def test_repeating_the_same_observation_reference_is_still_invalid(self):
        row = self.bundle["forecasts"][0]
        row["payoff_sample_ids"].append(row["payoff_sample_ids"][0])
        self.seal()
        with self.assertRaisesRegex(ValueError, "missing/duplicated payoff samples"):
            self.evaluate()

    def test_distinct_original_source_outcomes_can_supply_additional_support(self):
        self.lock["policy"]["minimum_payoff_samples_per_event"] = 13
        for event in EVENTS:
            self.new_original(event)
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        self.assertTrue(all(c["samples"] == 13 for c in diagnostics["predictions"]["forecast_utilities"][0]["conditional_means"].values()))

    def test_distinct_causal_windows_from_one_original_source_remain_distinct(self):
        self.lock["policy"]["minimum_payoff_samples_per_event"] = 13
        for event in EVENTS:
            self.new_original(event, same_source=True)
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        self.assertTrue(all(c["samples"] == 13 for c in diagnostics["predictions"]["forecast_utilities"][0]["conditional_means"].values()))

    def test_reuse_across_forecasts_never_multiplies_estimator_support(self):
        self.lock["policy"]["minimum_payoff_samples_per_event"] = 13
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertEqual(len(diagnostics["predictions"]["forecast_utilities"]), 36)
        self.assertTrue(all(row["conditional_means"] is None for row in diagnostics["predictions"]["forecast_utilities"]))
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])

    def test_equivalent_alias_replacements_share_original_estimator_and_search_group(self):
        _, before = self.evaluate()
        aliases = {row["observation_id"]: self.alias(row, sample_alias=True, selected=False)["observation_id"]
                   for row in list(self.bundle["payoff_observations"])}
        for index, row in enumerate(self.bundle["forecasts"]):
            if index % 2:
                row["payoff_sample_ids"] = [aliases[identity] for identity in reversed(row["payoff_sample_ids"])]
        self.bundle["payoff_observations"].reverse()
        self.seal()
        _, after = self.evaluate()
        self.assertTrue(all(after["computed_gates_before_evidence_class"].values()))
        self.assertEqual(before["predictions"]["forecast_utilities"], after["predictions"]["forecast_utilities"])
        self.assertEqual(before["predictions"]["exact"], after["predictions"]["exact"])

    def test_equivalent_decimal_and_timezone_encodings_preserve_original_facts(self):
        _, before = self.evaluate()
        original = self.bundle["payoff_observations"][0]
        alias = self.alias(original, sample_alias=True, selected=False, gross_profit="1000.000",
                           fixed_cost="1.000", variable_cost="1.00", opening_equity="100000.0",
                           available_at=stamp(0, 12, 1).replace("Z", "+01:00"),
                           recorded_at=stamp(0, 12, 2).replace("Z", "+01:00"))
        sample = next(s for s in self.bundle["samples"] if s["sample_id"] == alias["sample_id"])
        sample.update(label_start_at=stamp(0, 11).replace("Z", "+01:00"),
                      label_end_at=stamp(0, 12).replace("Z", "+01:00"),
                      label_available_at=stamp(0, 12).replace("Z", "+01:00"),
                      label_recorded_at=stamp(0, 12, 1).replace("Z", "+01:00"))
        for row in self.bundle["forecasts"]:
            row["payoff_sample_ids"] = [alias["observation_id"] if identity == original["observation_id"] else identity
                                        for identity in row["payoff_sample_ids"]]
        self.seal()
        _, after = self.evaluate()
        self.assertTrue(all(after["computed_gates_before_evidence_class"].values()))
        self.assertEqual(before["predictions"]["forecast_utilities"], after["predictions"]["forecast_utilities"])

    def test_cached_alias_reuse_still_checks_each_forecasts_own_fit_membership(self):
        original = self.bundle["payoff_observations"][0]
        alias = self.alias(original, sample_alias=True, selected=False)
        self.bundle["fits"][0]["train"]["sample_ids"].remove(alias["sample_id"])
        forecast = self.bundle["forecasts"][-1]
        forecast["payoff_sample_ids"] = [alias["observation_id"] if identity == original["observation_id"] else identity
                                          for identity in forecast["payoff_sample_ids"]]
        self.seal()
        with self.assertRaisesRegex(ValueError, "payoff estimator used future, calibration, development or sealed labels"):
            self.evaluate()

    def test_sample_reuse_across_independent_scenario_estimators_is_allowed(self):
        scenario = sha("TEST_ONLY-independent-payoff-context")
        ids = []
        for row in list(self.bundle["payoff_observations"]):
            clone = self.alias(row, selected=False, scenario_hash=scenario)
            if clone["event"] == EVENTS[0]:
                clone["gross_profit"] = "1001"
            ids.append(clone["observation_id"])
        for row in list(self.bundle["forecasts"]):
            clone = {**row, "prediction_id": "TEST_ONLY-context-" + row["prediction_id"],
                     "source_hash": sha("TEST_ONLY-context-" + row["prediction_id"]),
                     "scenario_hash": scenario, "payoff_sample_ids": ids}
            self.bundle["forecasts"].append(clone)
        self.lock["policy"]["bootstrap_repetitions"] = 2400
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        utilities = diagnostics["predictions"]["forecast_utilities"]
        self.assertEqual(len(utilities), 72)
        self.assertTrue(all(c["samples"] == 12 for row in utilities for c in row["conditional_means"].values()))

    def test_original_outcomes_may_be_reused_by_a_separate_model_refit(self):
        fit = deepcopy(self.bundle["fits"][0])
        fit["fit_id"] = "TEST_ONLY-independent-refit"
        self.bundle["fits"].append(fit)
        for row in self.bundle["forecasts"][::2]:
            row["model_fit_id"] = fit["fit_id"]
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        self.assertTrue(all(c["samples"] == 12 for row in diagnostics["predictions"]["forecast_utilities"]
                            for c in row["conditional_means"].values()))

    def test_separate_estimator_contexts_do_not_borrow_each_others_support(self):
        scenario = sha("TEST_ONLY-unsupported-context")
        ids = [self.alias(next(p for p in self.bundle["payoff_observations"] if p["event"] == event),
                          selected=False, scenario_hash=scenario)["observation_id"] for event in EVENTS]
        for row in list(self.bundle["forecasts"]):
            self.bundle["forecasts"].append({**row, "prediction_id": "TEST_ONLY-thin-" + row["prediction_id"],
                                             "source_hash": sha("TEST_ONLY-thin-" + row["prediction_id"]),
                                             "scenario_hash": scenario, "payoff_sample_ids": ids})
        self.lock["policy"]["bootstrap_repetitions"] = 2400
        self.seal()
        _, diagnostics = self.evaluate()
        utilities = diagnostics["predictions"]["forecast_utilities"]
        self.assertEqual(sum(row["conditional_means"] is not None for row in utilities), 36)
        self.assertEqual(sum(row["conditional_means"] is None for row in utilities), 36)
        self.assertFalse(diagnostics["predictions"]["conditional_payoff_support"])
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])

    def test_one_estimator_cannot_mix_aliases_from_different_contexts(self):
        for field, value in (("scenario_hash", sha("TEST_ONLY-other-scenario")), ("horizon_sessions", 2),
                             ("regime", "OTHER_REGIME"), ("currency", "USD"),
                             ("execution_convention_hash", sha("TEST_ONLY-other-execution"))):
            with self.subTest(field=field):
                self.setUp()
                self.alias(**{field: value})
                self.seal()
                with self.assertRaisesRegex(ValueError, "payoff scenario/regime/horizon/account unit mismatch"):
                    self.evaluate()

    def test_entry_at_close_is_rejected_consistently_before_and_after_closure(self):
        for state in ("OPEN", "CLOSED"):
            with self.subTest(state=state):
                self.setUp()
                self.candidate_entry(state, stamp(60, 16))
                self.seal()
                with self.assertRaisesRegex(ValueError, "trade entry is outside covered exchange sessions"):
                    self.evaluate()

    def test_just_before_close_remains_valid_without_changing_account_valuations(self):
        _, before = self.evaluate()
        candidate = self.request["trial"]["candidate_id"]
        baseline_return = before["statistics"][candidate + "|base"]["net_return"]
        for state in ("OPEN", "CLOSED"):
            for entry in (stamp(60, 15, 59), stamp(60, 16).replace("16:00:00", "15:59:59.999999")):
                with self.subTest(state=state, entry=entry):
                    self.setUp()
                    self.candidate_entry(state, entry)
                    self.seal()
                    payload, diagnostics = self.evaluate()
                    self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
                    stats = diagnostics["statistics"][candidate + "|base"]
                    self.assertEqual(stats["net_return"], baseline_return)
                    self.assertEqual(stats["open_trades"], int(state == "OPEN"))
                    self.assertEqual(payload["closed_trades"], 26 + int(state == "CLOSED"))

    def test_every_actual_strategy_and_unreferenced_trade_uses_exclusive_entry_close(self):
        for strategy in ("neighbor-test", "champion", "simple_baseline"):
            for state in ("OPEN", "CLOSED"):
                with self.subTest(strategy=strategy, state=state):
                    self.setUp()
                    self.other_entry(strategy, state, stamp(60, 16))
                    self.seal()
                    with self.assertRaisesRegex(ValueError, "trade entry is outside covered exchange sessions"):
                        self.evaluate()

    def test_benchmark_carry_in_stays_valid_with_the_same_entry_boundary(self):
        for state in ("OPEN", "CLOSED"):
            for valid in (False, True):
                with self.subTest(state=state, valid=valid):
                    self.setUp()
                    self.other_entry("champion", state, stamp(0, 15, 59) if valid else stamp(0, 16))
                    self.seal()
                    if valid:
                        _, diagnostics = self.evaluate()
                        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
                        self.assertEqual(diagnostics["statistics"]["champion|base"]["open_trades"], int(state == "OPEN"))
                    else:
                        with self.assertRaisesRegex(ValueError, "trade entry is outside covered exchange sessions"):
                            self.evaluate()

    def test_shadow_entries_use_exclusive_close_even_without_covered_closures(self):
        for state in ("OPEN", "CLOSED"):
            for day in (100, 106):
                with self.subTest(state=state, day=day):
                    self.setUp()
                    self.shadow_entry(state, day)
                    self.seal()
                    with self.assertRaisesRegex(ValueError, "trade entry is outside covered exchange sessions"):
                        self.evaluate()

    def test_legal_shadow_entry_can_close_at_session_close_and_record_after_hours(self):
        for state in ("OPEN", "CLOSED"):
            with self.subTest(state=state):
                self.setUp()
                self.shadow_entry(state, entry=stamp(100, 15, 59))
                self.seal()
                payload, diagnostics = self.evaluate()
                self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
                self.assertEqual(payload["shadow_closed_trades"], int(state == "CLOSED"))

    def test_actual_closed_exit_at_session_close_remains_inclusive(self):
        trade = self.candidate_entry("CLOSED", stamp(60, 15, 59))
        trade.update(exit_at=stamp(61, 16), recorded_at=stamp(61, 16, 1))
        self.bundle["forecasts"][0].update(label_end_at=stamp(61, 16), label_available_at=stamp(61, 16),
                                             label_recorded_at=stamp(61, 16, 1))
        self.seal()
        payload, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        self.assertEqual(payload["closed_trades"], 27)

    def test_actual_entry_at_session_open_stays_inclusive(self):
        for state in ("OPEN", "CLOSED"):
            with self.subTest(state=state):
                self.setUp()
                self.other_entry("neighbor-test", state, stamp(60, 9))
                self.seal()
                _, diagnostics = self.evaluate()
                self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_shadow_entry_at_session_open_stays_inclusive(self):
        for state in ("OPEN", "CLOSED"):
            with self.subTest(state=state):
                self.setUp()
                self.shadow_entry(state, entry=stamp(100, 9))
                self.seal()
                payload, diagnostics = self.evaluate()
                self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
                self.assertEqual(payload["shadow_closed_trades"], int(state == "CLOSED"))


if __name__ == "__main__":
    unittest.main()

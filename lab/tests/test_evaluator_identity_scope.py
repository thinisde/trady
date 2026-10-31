"""TEST_ONLY frozen-identity and source-dataset accounting counterexamples."""
import pathlib
import sys
import unittest
from copy import deepcopy
from datetime import datetime, timedelta, timezone
from unittest.mock import patch

LAB = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(LAB))
from evaluator.common import EVENTS, canonical, sha, timestamp
from evaluator.observations import calendar_index, validate_fits
from evaluator.predictions import score_predictions
from evaluator.run import evaluate
from sealed_fixture import fixture, reseal, stamp


class EvaluatorIdentityScope(unittest.TestCase):
    def setUp(self):
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        self.source_hash = self.request["manifest"]["dataset_hash"]

    def seal(self):
        # The general fixture exporter rewrites trial scope. Preserve deliberate
        # cross-dataset records in these synthetic scope counterexamples.
        datasets = [row["dataset_hash"] for row in self.search["trials"]]
        reseal(self.request, self.lock, self.bundle, self.search, self.shadow)
        for row, dataset in zip(self.search["trials"], datasets):
            row["dataset_hash"] = dataset
        self.lock["artifacts"]["search"]["sha256"] = sha(self.search)
        self.assertEqual(self.request["manifest"]["dataset_hash"], self.source_hash)

    def evaluate(self):
        payload, diagnostics = evaluate(self.request, self.lock, self.bundle, self.search, self.shadow)
        self.assertEqual(self.lock["evidence_class"], "TEST_ONLY")
        self.assertTrue(all(payload[key] is False for key in diagnostics["computed_gates_before_evidence_class"]))
        return payload, diagnostics

    def receipt_parameters(self, strategy, parameters):
        for run in self.bundle["equity"]:
            if run["strategy_id"] == strategy:
                run["execution_spec"]["parameters"] = deepcopy(parameters)
                run["execution_receipt_hash"] = sha(run["execution_spec"])

    def read(self, suffix, dataset=None):
        return {"read_id": "TEST_ONLY-read-" + suffix,
                "dataset_hash": dataset or self.source_hash,
                "run_id": self.request["run_id"], "candidate_id": self.request["trial"]["candidate_id"],
                "input_hash": self.request["input_hash"], "consumed_at": stamp(43)}

    def other_reads(self):
        return [{**self.read(str(index), sha("TEST_ONLY-other-source-" + str(index))),
                 "run_id": "TEST_ONLY-other-run-" + str(index),
                 "candidate_id": "candidate_" + sha("TEST_ONLY-other-parameters-" + str(index)),
                 "input_hash": sha("TEST_ONLY-other-input-" + str(index))} for index in range(2)]

    def assert_early_rejection(self, pattern):
        self.seal()
        with patch("evaluator.run.calendar_index", side_effect=AssertionError("observations reached")), \
                self.assertRaisesRegex(ValueError, pattern):
            self.evaluate()

    def predictions(self):
        cutoff = timestamp(self.request["manifest"]["cutoff_at"])
        sessions, outer, _ = calendar_index(self.bundle, cutoff)
        fits, samples = validate_fits(self.bundle, sessions, outer, self.lock["policy"])
        report = score_predictions(self.bundle, sessions, fits, samples, self.request["trial"]["candidate_id"],
                                   self.lock["policy"], cutoff, 1, "identity-scope", 3)
        canonical(report)
        return report

    def test_benchmark_parameters_cannot_replace_frozen_candidate_target(self):
        candidate = self.request["trial"]["candidate_id"]
        changed = {**self.request["trial"]["parameters"], "target_net_fraction": "0.09"}
        self.lock["policy"]["benchmark_parameters"][candidate] = changed
        self.receipt_parameters(candidate, changed)
        self.seal()
        self.assertEqual(self.request["trial"]["parameters"]["target_net_fraction"], "0.04")
        with self.assertRaisesRegex(ValueError, "benchmark parameter.*identit|benchmark parameter.*keys"):
            self.evaluate()

    def test_benchmark_parameters_cannot_replace_neighbor_horizon_without_trades(self):
        neighbor = self.bundle["neighbors"][0]
        changed = {**neighbor["parameters"], "max_holding_sessions": 5}
        self.lock["policy"]["benchmark_parameters"][neighbor["strategy_id"]] = changed
        self.receipt_parameters(neighbor["strategy_id"], changed)
        self.seal()
        self.assertEqual(neighbor["parameters"]["max_holding_sessions"], 3)
        self.assertFalse(any(row["strategy_id"] == neighbor["strategy_id"] for row in self.bundle["trades"]))
        with self.assertRaisesRegex(ValueError, "benchmark parameter.*identit|benchmark parameter.*keys"):
            self.evaluate()

    def test_valid_other_dataset_reads_do_not_consume_current_look_budget(self):
        self.search["holdout_reads"] = self.other_reads()
        self.seal()
        self.assertEqual(self.lock["policy"]["planned_looks"], 1)
        _, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        self.assertEqual(diagnostics["inference"]["search_trials"], 1)

    def test_exact_legal_benchmark_maps_preserve_all_private_gates(self):
        policy = self.lock["policy"]
        self.assertEqual(set(policy["benchmark_parameters"]), set(policy["benchmark_ids"]))
        self.assertEqual(set(policy["benchmark_complexities"]), set(policy["benchmark_ids"]))
        _, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_both_benchmark_maps_reject_extra_and_omitted_keys_before_observations(self):
        policy = self.lock["policy"]
        for field, values, pattern in (("benchmark_parameters", {}, "benchmark parameter keys"),
                                       ("benchmark_complexities", 7, "benchmark complexity keys")):
            original = deepcopy(policy[field])
            for key in (self.request["trial"]["candidate_id"], "neighbor-test", "TEST_ONLY-unregistered"):
                with self.subTest(field=field, extra=key):
                    policy[field] = {**original, key: deepcopy(values)}
                    self.assert_early_rejection(pattern)
            for key in policy["benchmark_ids"]:
                with self.subTest(field=field, omitted=key):
                    policy[field] = {name: value for name, value in original.items() if name != key}
                    self.assert_early_rejection(pattern)
            policy[field] = original

    def test_benchmark_complexities_require_bounded_integers(self):
        for value in (True, -1, 10001, "7"):
            with self.subTest(value=value):
                self.lock["policy"]["benchmark_complexities"]["champion"] = value
                self.assert_early_rejection("bounded integer")

    def test_benchmark_parameters_require_actual_objects(self):
        self.lock["policy"]["benchmark_parameters"]["champion"] = []
        self.assert_early_rejection("benchmark parameters must be objects")

    def test_candidate_cannot_alias_the_frozen_champion(self):
        candidate = self.request["trial"]["candidate_id"]
        policy = self.lock["policy"]
        self.request["manifest"]["champion_id"] = candidate
        policy["benchmark_ids"][0] = candidate
        for field in ("benchmark_parameters", "benchmark_complexities"):
            policy[field][candidate] = policy[field].pop("champion")
        self.assert_early_rejection("candidate aliases a benchmark")

    def test_neighbors_cannot_alias_candidate_or_benchmarks_before_observations(self):
        for identity in (self.request["trial"]["candidate_id"], "champion"):
            with self.subTest(identity=identity):
                self.bundle["neighbors"][0]["strategy_id"] = identity
                self.assert_early_rejection("neighbor aliases a candidate/benchmark")

    def test_duplicate_benchmark_and_neighbor_identities_fail_closed(self):
        self.request["manifest"]["champion_id"] = "cash"
        self.lock["policy"]["benchmark_ids"][0] = "cash"
        self.assert_early_rejection("benchmark identities overlap")
        self.setUp()
        self.bundle["neighbors"].append(deepcopy(self.bundle["neighbors"][0]))
        self.assert_early_rejection("duplicate or rewritten record identity")

    def test_valid_maps_cannot_rebind_candidate_neighbor_or_benchmark_receipts(self):
        cases = ((self.request["trial"]["candidate_id"], {**self.request["trial"]["parameters"], "target_net_fraction": "0.09"}),
                 ("neighbor-test", {**self.bundle["neighbors"][0]["parameters"], "max_holding_sessions": 5}),
                 ("champion", {"version": "TEST_ONLY-unfrozen-champion"}))
        for strategy, parameters in cases:
            with self.subTest(strategy=strategy):
                original = deepcopy(self.bundle["equity"])
                self.receipt_parameters(strategy, parameters)
                self.seal()
                with self.assertRaisesRegex(ValueError, "execution receipt parameters/engine"):
                    self.evaluate()
                self.bundle["equity"] = original

    def test_compound_strategy_scenario_labels_cannot_replace_diagnostics(self):
        candidate = self.request["trial"]["candidate_id"]
        neighbor = candidate + "|base"
        self.bundle["neighbors"][0]["strategy_id"] = neighbor
        for run in self.bundle["equity"]:
            if run["strategy_id"] == "neighbor-test":
                run["strategy_id"] = neighbor
                run["execution_spec"]["strategy_id"] = neighbor
                run["execution_receipt_hash"] = sha(run["execution_spec"])
        scenario = {**self.lock["policy"]["stress_cases"][1], "scenario_id": "base|stress"}
        self.lock["policy"]["stress_cases"].append(scenario)
        self.lock["policy"]["bootstrap_repetitions"] = 2400
        for run in list(self.bundle["equity"]):
            if run["scenario_id"] == "stress":
                clone = deepcopy(run)
                clone["scenario_id"] = clone["execution_spec"]["scenario_id"] = scenario["scenario_id"]
                clone["execution_receipt_hash"] = sha(clone["execution_spec"])
                for row in clone["sessions"]:
                    row["closed_trade_ids"] = []
                self.bundle["equity"].append(clone)
        self.assertEqual("|".join((candidate, scenario["scenario_id"])), "|".join((neighbor, "stress")))
        self.assert_early_rejection("ambiguous strategy/scenario diagnostic identity")

    def test_compound_cohort_labels_cannot_replace_an_ineligible_cohort(self):
        version = self.bundle["forecasts"][0]["model_version"]
        second_version = version + "|A"
        self.bundle["approved_models"][second_version] = self.bundle["approved_models"][version]
        old_ids, new_ids = [], []
        for payoff in list(self.bundle["payoff_observations"]):
            payoff["regime"] = "A|B"
            old_ids.append(payoff["observation_id"])
            clone = {**payoff, "observation_id": "TEST_ONLY-copy-" + payoff["observation_id"], "regime": "B"}
            new_ids.append(clone["observation_id"])
            self.bundle["payoff_observations"].append(clone)
        for index, row in enumerate(self.bundle["forecasts"]):
            row.update(model_version=version if index < 9 else second_version,
                       regime="A|B" if index < 9 else "B", payoff_sample_ids=old_ids if index < 9 else new_ids)
            session = next(item for item in self.bundle["calendar"]["sessions"] if item["session_id"] == row["session_id"])
            session["regime"] = row["regime"]
        self.lock["policy"]["bootstrap_repetitions"] = 2400
        self.seal()
        self.assertEqual("|".join((version, "A|B", "3")), "|".join((second_version, "B", "3")))
        with self.assertRaisesRegex(ValueError, "ambiguous calibration cohort diagnostic identity"):
            self.evaluate()

    def test_shared_group_residuals_use_every_row_and_are_order_invariant(self):
        partitions = {"0.1": ["0.3", "0.3", "0.3"], "0.2": ["0.3", "0.3", "0.2"],
                      "0.7": ["0.1", "0.1", "0.1"], "0.6": ["0.2", "0.1", "0.1"]}

        def probabilities(outcome, actual):
            result = dict(zip([event for event in EVENTS if event != outcome], partitions[actual]))
            return {**result, outcome: actual}

        for index, row in enumerate(list(self.bundle["forecasts"])):
            row.update(dependence_group_id="TEST_ONLY-A", probabilities=probabilities(row["outcome"], "0.1" if index % 2 == 0 else "0.2"))
            for suffix, group, actual in (("replica", "TEST_ONLY-A", "0.7" if index % 2 == 0 else "0.6"),
                                          ("paired", "TEST_ONLY-B", "0.1" if index % 2 == 0 else "0.2")):
                clone = deepcopy(row)
                identity = row["prediction_id"] + "-" + suffix
                clone.update(prediction_id=identity, source_hash=sha(identity), dependence_group_id=group,
                             probabilities=probabilities(row["outcome"], actual))
                self.bundle["forecasts"].append(clone)
        first = self.predictions()["dependence"]
        self.bundle["forecasts"].reverse()
        second = self.predictions()["dependence"]
        self.assertEqual(first, second)
        self.assertEqual(first[0]["paired_samples"], 36)
        self.assertIsNone(first[0]["residual_correlation"])

    def test_dependence_group_ids_cannot_alias_through_boolean_or_integer_keys(self):
        for identity in (False, 0, "", [], "x" * 161):
            with self.subTest(identity=identity):
                self.bundle["forecasts"][0]["dependence_group_id"] = identity
                with self.assertRaisesRegex(ValueError, "invalid dependence group identity"):
                    self.predictions()

    def test_one_matching_current_read_and_other_dataset_history_are_legal(self):
        self.search["holdout_reads"] = [self.read("current"), *self.other_reads()]
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_two_current_dataset_exposures_are_rejected_despite_other_history(self):
        self.search["holdout_reads"] = [self.read("current-1"), self.read("current-2"), *self.other_reads()]
        self.seal()
        with self.assertRaisesRegex(ValueError, "sealed period already exposed"):
            self.evaluate()

    def test_read_id_cannot_be_rewritten_under_another_dataset_scope(self):
        first = self.read("same")
        self.search["holdout_reads"] = [first, {**first, "dataset_hash": sha("TEST_ONLY-other-source")}]
        self.seal()
        with self.assertRaisesRegex(ValueError, "duplicate or rewritten record identity"):
            self.evaluate()

    def test_current_dataset_read_must_match_run_candidate_and_input(self):
        for field, value in (("run_id", "TEST_ONLY-other-account-run"),
                             ("candidate_id", "candidate_" + sha("TEST_ONLY-other-parameters")),
                             ("input_hash", sha("TEST_ONLY-other-input"))):
            with self.subTest(field=field):
                self.search["holdout_reads"] = [{**self.read("current"), field: value}, *self.other_reads()]
                self.seal()
                with self.assertRaisesRegex(ValueError, "sealed period already exposed"):
                    self.evaluate()

    def test_unrelated_read_scope_hashes_and_timestamps_remain_validated(self):
        for field in ("dataset_hash", "input_hash"):
            with self.subTest(field=field):
                self.search["holdout_reads"] = [{**self.other_reads()[0], field: "TEST_ONLY-not-a-hash"}]
                self.seal()
                with self.assertRaisesRegex(ValueError, "invalid holdout read scope hash"):
                    self.evaluate()
        future = (datetime.now(timezone.utc) + timedelta(days=1)).isoformat()
        self.search["holdout_reads"] = [{**self.other_reads()[0], "consumed_at": future}]
        self.seal()
        with self.assertRaisesRegex(ValueError, "future holdout read"):
            self.evaluate()

    def test_current_trial_budget_still_includes_failed_related_attempts(self):
        extra = {**self.search["trials"][0], "trial_id": "TEST_ONLY-failed-trial", "status": "FAILED",
                 "run_id": "TEST_ONLY-earlier-run", "parameters_hash": sha("TEST_ONLY-failed-parameters")}
        extra["candidate_id"] = "candidate_" + extra["parameters_hash"]
        self.search["trials"].append(extra)
        self.search["holdout_reads"] = self.other_reads()
        self.seal()
        with self.assertRaisesRegex(ValueError, "cumulative trial budget exhausted"):
            self.evaluate()
        self.request["plan"]["max_cumulative_trials"] = 2
        self.lock["policy"]["bootstrap_repetitions"] = 2400
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertEqual(diagnostics["inference"]["search_trials"], 2)
        self.assertEqual(diagnostics["inference"]["comparison_family"], 24)
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))

    def test_unrelated_trial_scopes_do_not_change_current_search_correction(self):
        for index, scope in enumerate(({"account_id": "TEST_ONLY-other-account"}, {"mode": "shadow"},
                                       {"dataset_hash": sha("TEST_ONLY-other-source")})):
            self.search["trials"].append({**self.search["trials"][0], "trial_id": "TEST_ONLY-other-trial-" + str(index), **scope})
        self.search["holdout_reads"] = self.other_reads()
        self.seal()
        _, diagnostics = self.evaluate()
        self.assertEqual(diagnostics["inference"]["search_trials"], 1)
        self.assertEqual(diagnostics["inference"]["comparison_family"], 12)
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))


if __name__ == "__main__":
    unittest.main()

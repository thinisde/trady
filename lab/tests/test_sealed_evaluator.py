"""Numerical and hostile-input tests using exclusively TEST_ONLY observations."""
import hashlib
import json
import math
import os
import pathlib
import subprocess
import sys
import tempfile
import time
import unittest
from copy import deepcopy
from datetime import datetime, timezone
from decimal import Decimal

LAB = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(LAB))
from evaluator.common import EVENTS, GATE_FIELDS, canonical, number, sha, strict_json, timestamp, unique
from evaluator.observations import calendar_index, equity_series, search_accounting, shadow_coverage, validate_fits
from evaluator.predictions import calibration_scores, score_predictions
from evaluator.run import evaluate, write_artifact
from evaluator.statistics import drawdown, objective, paired_bounds, risk_metrics
from evaluator.transport import consume, request_evaluation
from sealed_fixture import fixture, reseal, stamp


class SealedEvaluator(unittest.TestCase):
    def setUp(self):
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()

    def reseal(self):
        reseal(self.request, self.lock, self.bundle, self.search, self.shadow)

    def evaluate(self):
        return evaluate(self.request, self.lock, self.bundle, self.search, self.shadow)

    def predictions(self):
        sessions, outer, _ = calendar_index(self.bundle, timestamp(self.request["manifest"]["cutoff_at"]))
        fits, samples = validate_fits(self.bundle, sessions, outer, self.lock["policy"])
        return score_predictions(self.bundle, sessions, fits, samples, self.request["trial"]["candidate_id"],
                                 self.lock["policy"], timestamp(self.request["manifest"]["cutoff_at"]), 1, "test-seed",
                                 self.request["trial"]["parameters"]["max_holding_sessions"])

    def test_actual_numeric_metrics_and_test_only_exclusion(self):
        payload, diagnostics = self.evaluate()
        self.assertEqual(payload["outer_intervals"], 6)
        self.assertEqual(payload["closed_trades"], 27)
        self.assertEqual(payload["completed_shadow_weeks"], 1)
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        for gate in diagnostics["computed_gates_before_evidence_class"]:
            self.assertIs(payload[gate], False, "TEST_ONLY cannot authorize production acceptance")
        self.assertGreater(Decimal(payload["improvement_lower_bound"]), Decimal("0.0009"))
        self.assertAlmostEqual(diagnostics["predictions"]["brier"], 0.75)
        self.assertAlmostEqual(diagnostics["predictions"]["log_loss"], math.log(4))
        self.assertEqual(diagnostics["predictions"]["payoff_mean_error"], 0)
        candidate = self.request["trial"]["candidate_id"]
        base = diagnostics["statistics"][candidate + "|base"]
        stressed = diagnostics["statistics"][candidate + "|stress"]
        self.assertAlmostEqual(base["net_return"], 36 * 117 / 100000)
        self.assertAlmostEqual(stressed["net_return"], 36 * 115 / 100000)
        self.assertEqual(base["max_drawdown"], 0)
        self.assertEqual(base["sector_gross_attribution"]["TEST_SECTOR"]["gross_pnl"], "4320")
        self.assertEqual(base["unallocated_execution_costs"], "108")
        self.assertEqual(base["regime_breakdown"]["TEST_REGIME"]["sessions"], 36)
        self.assertGreater(base["mean_turnover"], 0)
        self.assertIsNone(diagnostics["statistics"]["cash|base"]["sharpe"])

    def test_request_and_all_sealed_artifacts_are_content_bound(self):
        mutations = [lambda: self.request["plan"].update(minimum_improvement="0"),
                     lambda: self.request.update(run_id="weekly_other"),
                     lambda: self.bundle["equity"][0]["sessions"][0].update(gross_pnl="999"),
                     lambda: self.search["trials"][0].update(status="FAILED"),
                     lambda: self.shadow.update(program_started_at=stamp(98))]
        original = fixture()
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                self.request, self.lock, self.bundle, self.search, self.shadow = deepcopy(original)
                mutation()
                with self.assertRaisesRegex(ValueError, "binding|content"):
                    self.evaluate()

    def test_supplied_acceptance_flags_and_evidence_reclassification_are_rejected(self):
        self.bundle["economic_feasibility_passed"] = True
        self.reseal()
        with self.assertRaisesRegex(ValueError, "caller-supplied"):
            self.evaluate()
        self.bundle.pop("economic_feasibility_passed")
        self.lock["evidence_class"] = "PRODUCTION"
        self.reseal()
        with self.assertRaisesRegex(ValueError, "cannot be reclassified"):
            self.evaluate()

    def test_duplicates_invalid_decimals_and_json_keys_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "duplicate"):
            unique([{"id": "same"}, {"id": "same"}], "id")
        with self.assertRaisesRegex(ValueError, "duplicate"):
            strict_json(b'{"x":1,"x":2}')
        for invalid in (b'{"x":0.1}', b'{"x":NaN}'):
            with self.assertRaises(ValueError):
                strict_json(invalid)
        for invalid in ("NaN", "Infinity", "1e999999", "1e-999999", "12345678901234567890123456789"):
            with self.assertRaises(ValueError):
                number(invalid)

    def test_cumulative_trials_and_tail_resolution_are_not_reset_per_run(self):
        now = datetime.now(timezone.utc)
        self.request["plan"]["max_cumulative_trials"] = 32
        trials, _ = search_accounting(self.search, self.request, now)
        self.assertEqual(trials, 32)
        self.reseal()
        with self.assertRaisesRegex(ValueError, "tail resolution"):
            self.evaluate()

    def test_previous_holdout_exposure_is_rejected(self):
        self.search["holdout_reads"] = [{"read_id": "prior-read", "dataset_hash": self.request["manifest"]["dataset_hash"],
                                         "run_id": "weekly_prior", "candidate_id": "other", "input_hash": sha("other"), "consumed_at": stamp(45)}]
        self.reseal()
        with self.assertRaisesRegex(ValueError, "already exposed"):
            self.evaluate()

    def test_unpurged_fit_partitions_and_future_features_or_labels_are_rejected(self):
        original = fixture()
        mutations = [lambda: self.bundle["fits"][0]["calibration"].update(opens_at=stamp(12)),
                     lambda: self.bundle["samples"][0].update(recorded_at=stamp(1)),
                     lambda: self.bundle["samples"][0].update(label_recorded_at=stamp(100)),
                     lambda: self.bundle["samples"][0].update(session_id="outer-0")]
        for mutation in mutations:
            self.request, self.lock, self.bundle, self.search, self.shadow = deepcopy(original)
            mutation(); self.reseal()
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.evaluate()

    def test_contiguous_equal_outer_folds_and_sufficient_blocks_are_required(self):
        self.bundle["calendar"]["sessions"][36]["fold_id"] = "fold-1"
        self.reseal()
        with self.assertRaisesRegex(ValueError, "interleaved|unequal"):
            self.evaluate()
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        self.lock["policy"]["block_sessions"] = 4
        self.lock["policy"]["minimum_calibration_sessions"] = 8
        self.reseal()
        with self.assertRaisesRegex(ValueError, "independent blocks"):
            self.evaluate()

    def test_forecasts_are_original_predecision_and_model_bound(self):
        original = fixture()
        for field, value in (("recorded_at", stamp(60, 10, 1)), ("available_at", stamp(60, 10, 1)),
                             ("model_version", "unapproved"), ("transform_hash", sha("other-transform"))):
            self.request, self.lock, self.bundle, self.search, self.shadow = deepcopy(original)
            self.bundle["forecasts"][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.predictions()

    def test_payoff_estimator_uses_only_bound_matured_training_labels(self):
        payoff = self.bundle["payoff_observations"][0]
        payoff["recorded_at"] = stamp(61)
        with self.assertRaisesRegex(ValueError, "future, calibration"):
            self.predictions()
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        self.bundle["payoff_observations"][0]["scenario_hash"] = sha("other-scenario")
        with self.assertRaisesRegex(ValueError, "scenario"):
            self.predictions()
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        payoff = self.bundle["payoff_observations"][0]
        payoff["sample_id"] = self.bundle["fits"][0]["calibration"]["sample_ids"][0]
        payoff["session_id"] = "train-18"
        payoff["source_hash"] = next(s["label_source_hash"] for s in self.bundle["samples"] if s["sample_id"] == payoff["sample_id"])
        with self.assertRaisesRegex(ValueError, "calibration"):
            self.predictions()

    def test_pending_outcomes_never_become_scored_labels(self):
        self.bundle["forecasts"][0].update(outcome=None, label_available_at=None, label_recorded_at=None, label_source_hash=None)
        report = self.predictions()
        self.assertEqual(report["samples"], 35)
        self.assertEqual(report["pending_labels"], 1)
        self.assertAlmostEqual(report["brier"], 0.75)

    def test_probability_partition_zero_likelihood_and_sparse_events_abstain(self):
        row = self.bundle["forecasts"][0]
        row["probabilities"]["TARGET_FIRST"] = "0.3"
        with self.assertRaisesRegex(ValueError, "renormalized"):
            self.predictions()
        row["probabilities"] = dict(zip(EVENTS, ["0", "0.5", "0.25", "0.25"]))
        report = self.predictions()
        self.assertIsNone(report["log_loss"])
        self.assertEqual(report["zero_likelihood_events"], 1)
        self.assertFalse(report["eligible"])
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        self.lock["policy"]["minimum_payoff_samples_per_event"] = 13
        report = self.predictions()
        self.assertFalse(report["conditional_payoff_support"])
        self.assertFalse(report["eligible"])
        self.assertIsNone(report["forecast_utilities"][0]["utility"])

    def test_accounting_rejects_cost_double_charging_equity_resets_and_fake_cash(self):
        original = fixture()
        for mutation in (lambda: self.bundle["equity"][1]["sessions"][0].update(closing_equity="100113"),
                         lambda: self.bundle["equity"][0]["sessions"][1].update(opening_equity="100000"),
                         lambda: next(r for r in self.bundle["equity"] if r["strategy_id"] == "cash")["sessions"][0].update(exposure_fraction="0.2", sector_exposure_fraction={"CASH": "0.2"})):
            self.request, self.lock, self.bundle, self.search, self.shadow = deepcopy(original)
            mutation(); self.reseal()
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, "equity|cash benchmark"):
                self.evaluate()

    def test_missing_stress_unresolved_orders_and_rebound_execution_fail(self):
        original = fixture()
        mutations = [lambda: self.bundle["equity"].pop(),
                     lambda: self.bundle["equity"][0]["sessions"][0].update(unresolved_order_ids=["unresolved"]),
                     lambda: self.bundle["equity"][0]["execution_spec"]["parameters"].update(target_net_fraction="0.09")]
        for mutation in mutations:
            self.request, self.lock, self.bundle, self.search, self.shadow = deepcopy(original)
            mutation(); self.reseal()
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.evaluate()

    def test_snapshot_sector_and_forecast_trade_links_cannot_be_discarded(self):
        original = fixture()
        mutations = [lambda: self.bundle["equity"][0]["sessions"][1].update(start_snapshot_hash=sha("reset")),
                     lambda: self.bundle["equity"][0]["sessions"][0].update(sector_gross_pnl={"TEST_SECTOR": "0"}),
                     lambda: self.bundle["trades"][0].update(prediction_id="missing-original")]
        for mutation in mutations:
            self.request, self.lock, self.bundle, self.search, self.shadow = deepcopy(original)
            mutation(); self.reseal()
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.evaluate()

    def test_economic_hurdle_and_parameter_neighborhood_are_real_calculations(self):
        self.lock["policy"]["project_cost_account_currency"] = "1000"
        self.reseal()
        _, diagnostics = self.evaluate()
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        for run in self.bundle["equity"]:
            if run["strategy_id"] == "neighbor-test":
                opening = Decimal("100000")
                for row in run["sessions"]:
                    row.update(opening_equity=str(opening), gross_pnl="0")
                    row["sector_gross_pnl"] = {"TEST_SECTOR": "0"}
                    opening -= Decimal(row["fixed_cost"]) + Decimal(row["variable_cost_unscaled"]) * (2 if run["scenario_id"] == "stress" else 1)
                    row["closing_equity"] = str(opening)
        self.reseal()
        _, diagnostics = self.evaluate()
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["neighborhood_checks_passed"])

    def test_downside_and_turnover_deterioration_blocks_acceptance(self):
        candidate = self.request["trial"]["candidate_id"]
        for run in self.bundle["equity"]:
            if run["strategy_id"] == candidate:
                opening = Decimal("100000")
                for index, row in enumerate(run["sessions"]):
                    gross = Decimal("-1000" if index == 0 else "120")
                    row.update(opening_equity=str(opening), gross_pnl=str(gross), turnover="2")
                    row["sector_gross_pnl"] = {"TEST_SECTOR": str(gross)}
                    opening += gross - Decimal(row["fixed_cost"]) - Decimal(row["variable_cost_unscaled"]) * (2 if run["scenario_id"] == "stress" else 1)
                    row["closing_equity"] = str(opening)
        self.reseal()
        _, diagnostics = self.evaluate()
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["risk_not_deteriorated"])

    def test_partial_future_shadow_weeks_and_simulated_sessions_cannot_count(self):
        cutoff = timestamp(self.request["manifest"]["cutoff_at"])
        sessions, _, _ = calendar_index(self.bundle, cutoff)
        args = (sessions, self.bundle["calendar"]["weeks"], self.request["trial"]["candidate_id"], timestamp(self.lock["candidate_frozen_at"]), cutoff,
                self.request["trial"]["parameters"]["max_holding_sessions"])
        report = shadow_coverage(self.shadow, *args)
        self.assertEqual(report["completed_weeks"], 1)
        self.assertEqual(len(report["covered_sessions"]), 8)
        self.shadow["observations"][0]["first_observed_at"] = stamp(100, 9, 1)
        self.assertEqual(shadow_coverage(self.shadow, *args)["completed_weeks"], 0)
        self.shadow["observations"][0]["data_kind"] = "DEMO"
        with self.assertRaisesRegex(ValueError, "synthetic"):
            shadow_coverage(self.shadow, *args)

    def test_exposure_latch_survives_restart_and_fences_different_candidates(self):
        with tempfile.TemporaryDirectory(prefix="trady-evaluator-exposure-test-") as temporary:
            directory = pathlib.Path(temporary)
            consume(directory, self.lock, self.request)
            consume(directory, self.lock, self.request)
            marker = next(directory.iterdir())
            self.assertEqual(marker.stat().st_mode & 0o777, 0o600)
            other_request, other_lock = deepcopy(self.request), deepcopy(self.lock)
            other_request["input_hash"] = sha("another-frozen-input")
            other_lock["request_sha256"] = sha(other_request)
            with self.assertRaisesRegex(ValueError, "different candidate"):
                consume(directory, other_lock, other_request)
            marker.write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "different candidate"):
                consume(directory, self.lock, self.request)

    @unittest.skipUnless(os.getuid() == 0, "worker input pins require deployment-owned files")
    def test_local_worker_uses_bounded_identity_checked_ipc_and_no_database(self):
        with tempfile.TemporaryDirectory(prefix="trady-evaluator-ipc-test-") as temporary:
            directory = pathlib.Path(temporary)
            state = directory / "state"; state.mkdir(mode=0o700)
            for key, value in (("dataset", self.bundle), ("search", self.search), ("shadow", self.shadow)):
                path = directory / (key + ".json")
                path.write_bytes(canonical(value)); path.chmod(0o600)
                self.lock["artifacts"][key]["path"] = str(path)
            self.lock["diagnostics_directory"] = str(directory / "diagnostics")
            lock_path = directory / "lock.json"; lock_path.write_bytes(canonical(self.lock)); lock_path.chmod(0o600)
            socket_path = directory / "worker.sock"
            code = ("import pathlib,sys; from evaluator.transport import serve; "
                    "serve(*(pathlib.Path(p) for p in sys.argv[1:]), stop_after=2)")
            # Isolation is irreversible and process-wide; never apply it to the
            # unittest/client process or substitute a weaker TEST_ONLY filter.
            with subprocess.Popen([sys.executable, "-B", "-c", code, str(lock_path), str(socket_path), str(state)],
                                  cwd=LAB, stdout=subprocess.PIPE, stderr=subprocess.PIPE) as worker:
                try:
                    deadline = time.monotonic() + 5
                    while worker.poll() is None and time.monotonic() < deadline:
                        if socket_path.exists() and socket_path.stat().st_mode & 0o777 == 0o660:
                            break
                        time.sleep(0.01)
                    self.assertIsNone(worker.poll(), worker.communicate() if worker.poll() is not None else None)
                    self.assertTrue(socket_path.exists())
                    self.assertEqual(socket_path.stat().st_mode & 0o777, 0o660)
                    with self.assertRaisesRegex(ValueError, "identity"):
                        request_evaluation(socket_path, canonical(self.request), os.getuid() + 1)
                    response = strict_json(request_evaluation(socket_path, canonical(self.request), os.getuid()))
                    self.assertEqual(response["payload"]["outer_intervals"], 6)
                    changed = deepcopy(self.request); changed["plan"]["minimum_improvement"] = "0"
                    with self.assertRaisesRegex(ValueError, "blocked"):
                        request_evaluation(socket_path, canonical(changed), os.getuid())
                    stdout, stderr = worker.communicate(timeout=5)
                    self.assertEqual(worker.returncode, 0, stderr.decode())
                    self.assertEqual(stdout, b"")
                finally:
                    if worker.poll() is None:
                        worker.kill()
                        worker.communicate(timeout=5)
            self.assertEqual(len(list(state.iterdir())), 1)

    @unittest.skipUnless(os.getuid() == 0, "fixed-entrypoint pinning requires deployment-owned files")
    def test_executable_emits_bound_report_private_artifact_and_rejects_file_tampering(self):
        with tempfile.TemporaryDirectory(prefix="trady-sealed-evaluator-test-") as temporary:
            directory = pathlib.Path(temporary)
            for key, value in (("dataset", self.bundle), ("search", self.search), ("shadow", self.shadow)):
                path = directory / (key + ".json")
                path.write_bytes(canonical(value)); path.chmod(0o600)
                self.lock["artifacts"][key]["path"] = str(path)
            self.lock["diagnostics_directory"] = str(directory / "diagnostics")
            lock_path = directory / "lock.json"
            lock_path.write_bytes(canonical(self.lock)); lock_path.chmod(0o600)
            argv = [sys.executable, str(LAB / "evaluate.py"), "--lock", str(lock_path)]
            result = subprocess.run(argv, input=canonical(self.request), capture_output=True, timeout=15,
                                    env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"})
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            reply = json.loads(result.stdout)
            report = reply["payload"]["report"]
            self.assertEqual(reply["input_hash"], self.request["input_hash"])
            self.assertEqual(report["exit_status"], 0)
            artifact = directory / "diagnostics" / report["output_hash"]
            self.assertEqual(hashlib.sha256(artifact.read_bytes()).hexdigest(), report["output_hash"])
            self.assertEqual(artifact.stat().st_mode & 0o777, 0o600)
            self.assertEqual(artifact.parent.stat().st_mode & 0o777, 0o700)
            self.assertTrue(all(reply["payload"][gate] is False for gate in json.loads(artifact.read_bytes())["computed_gates_before_evidence_class"]))
            (directory / "dataset.json").write_bytes(b'{}')
            rejected = subprocess.run(argv, input=canonical(self.request), capture_output=True, timeout=15)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertEqual(rejected.stdout, b"")
            self.assertIn(b"BLOCKED", rejected.stderr)


class StatisticalConventions(unittest.TestCase):
    def test_objective_risk_and_paired_bootstrap_preserve_actual_pairing(self):
        weights = {"drawdown_weight": "1", "instability_weight": "1", "complexity_weight": "0.01"}
        self.assertAlmostEqual(objective([[0.1] * 6, [0.1] * 6], weights, 7), math.log1p(0.1) - 0.07)
        self.assertAlmostEqual(drawdown([0.1, -0.2, 0]), 0.2)
        measured = risk_metrics([0.1, -0.2, 0], [0.1, 0.2, 0], [1, 2, 0], 252)
        self.assertAlmostEqual(measured["net_return"], -0.12)
        self.assertEqual(measured["cvar_95_loss"], 0.2)
        policy = {"block_sessions": 3, "bootstrap_repetitions": 1000, "planned_looks": 1, "alpha": "0.05", "numeric_margin": "0.000000001"}
        observations = [0.01, -0.02, 0.03, 0.01, -0.01, 0.02] * 2
        bounds, inference = paired_bounds({("candidate", "base"): observations, ("champion", "base"): observations},
                                          [list(range(6)), list(range(6, 12))], weights, {"candidate": 7, "champion": 7},
                                          "candidate", [], ["champion"], ["base"], policy, 1, "test-seed")
        self.assertTrue(inference["sufficient"])
        self.assertEqual(bounds["candidate", "base", "champion"]["point"], 0)
        self.assertEqual(bounds["candidate", "base", "champion"]["lower"], -1e-9)


if __name__ == "__main__":
    unittest.main()

"""Goldfish counterexamples: exact money, generation causality and real sessions.

All records are synthetic TEST_ONLY data; resealing changes derived artifact
pins but never grants production evidence or a fresh source/holdout identity.
"""
import pathlib
import math
import sys
import unittest
from decimal import Inexact, Rounded, ROUND_CEILING, ROUND_FLOOR, localcontext
from fractions import Fraction

LAB = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(LAB))
from evaluator.common import decimal_string, number, rounded_lower, sha, timestamp
from evaluator.observations import calendar_index, holding_span, validate_fits
from evaluator.predictions import score_predictions
from evaluator.run import evaluate
from sealed_fixture import fixture, reseal, stamp

HUGE = "9999999999999999999999999996"


class GoldfishCounterexamples(unittest.TestCase):
    def setUp(self):
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        self.original_source = self.request["manifest"]["dataset_hash"]

    def reseal(self):
        reseal(self.request, self.lock, self.bundle, self.search, self.shadow)
        self.assertEqual(self.request["manifest"]["dataset_hash"], self.original_source)
        self.assertEqual(self.lock["evidence_class"], "TEST_ONLY")

    def evaluate(self):
        return evaluate(self.request, self.lock, self.bundle, self.search, self.shadow)

    def predictions(self, holding=3):
        cutoff = timestamp(self.request["manifest"]["cutoff_at"])
        sessions, outer, _ = calendar_index(self.bundle, cutoff)
        fits, samples = validate_fits(self.bundle, sessions, outer, self.lock["policy"])
        return score_predictions(self.bundle, sessions, fits, samples, self.request["trial"]["candidate_id"],
                                 self.lock["policy"], cutoff, 1, "counterexample-seed", holding)

    def flat_equity(self, fabricated_gain=0, cancelling_costs=False):
        benchmarks = self.lock["policy"]["benchmark_ids"]
        for run in self.bundle["equity"]:
            candidate = run["strategy_id"] not in benchmarks
            gain = fabricated_gain if candidate else 0
            for index, row in enumerate(run["sessions"]):
                gross = HUGE if candidate and cancelling_costs else "0"
                row.update(opening_equity=str(1000 + gain * index), closing_equity=str(1000 + gain * (index + 1)),
                           boundary_cash_flow="0", gross_pnl=gross, fixed_cost=gross, variable_cost_unscaled="0")
                row["sector_gross_pnl"] = {"CASH" if run["strategy_id"] == "cash" else "TEST_SECTOR": gross}

    def move_outcome(self, end_session, label=True, trades=True, pending=False):
        row = self.bundle["forecasts"][0]
        if label:
            row.update(label_end_at=stamp(60 + end_session, 10), label_available_at=stamp(60 + end_session, 10),
                       label_recorded_at=stamp(60 + end_session, 10, 1))
            if pending:
                row.update(outcome=None, label_available_at=None, label_recorded_at=None, label_source_hash=None)
        if trades:
            for trade in self.bundle["trades"]:
                if trade["prediction_id"] != row["prediction_id"]:
                    continue
                trade.update(exit_at=stamp(60 + end_session, 10), recorded_at=stamp(60 + end_session, 10, 1))
                run = next(run for run in self.bundle["equity"]
                           if run["strategy_id"] == trade["strategy_id"] and run["scenario_id"] == trade["scenario_id"])
                run["sessions"][0]["closed_trade_ids"].remove(trade["trade_id"])
                run["sessions"][end_session]["closed_trade_ids"].append(trade["trade_id"])

    def test_repeating_cancellation_cannot_fabricate_equity_or_a_private_acceptance(self):
        self.flat_equity(fabricated_gain=4, cancelling_costs=True)
        self.reseal()
        self.assertEqual(Fraction(1000) + Fraction(HUGE) - Fraction(HUGE), 1000)
        with self.assertRaisesRegex(ValueError, "unreconciled cash/cost/equity"):
            self.evaluate()

    def test_large_exact_cancellation_and_cross_session_totals_remain_valid(self):
        self.flat_equity(cancelling_costs=True)
        self.reseal()
        payload, diagnostics = self.evaluate()
        candidate = self.request["trial"]["candidate_id"]
        measured = diagnostics["statistics"][candidate + "|base"]
        self.assertEqual(measured["net_return"], 0)
        self.assertEqual(measured["sector_gross_attribution"]["TEST_SECTOR"]["gross_pnl"], str(36 * int(HUGE)))
        self.assertEqual(measured["unallocated_execution_costs"], str(36 * int(HUGE)))
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])
        self.assertTrue(all(payload[gate] is False for gate in diagnostics["computed_gates_before_evidence_class"]))

    def test_boundary_cash_flow_return_keeps_the_exact_small_residual(self):
        for run in self.bundle["equity"]:
            for index, row in enumerate(run["sessions"]):
                row.update(opening_equity=HUGE if index == 0 else "0.1", closing_equity="0.1",
                           boundary_cash_flow="-" + HUGE if index == 0 else "0", gross_pnl="0.1" if index == 0 else "0",
                           fixed_cost="0", variable_cost_unscaled="0")
                row["sector_gross_pnl"] = {name: row["gross_pnl"] for name in row["sector_gross_pnl"]}
        self.reseal()
        _, diagnostics = self.evaluate()
        candidate = self.request["trial"]["candidate_id"]
        measured = diagnostics["statistics"][candidate + "|base"]["mean_log_return"]
        expected = math.log1p(float(Fraction("0.1") / Fraction(HUGE))) / 36
        self.assertGreater(measured, 0)
        self.assertAlmostEqual(measured, expected, delta=1e-45)

    def test_stressed_cost_product_cannot_round_off_a_nonzero_residual(self):
        multiplier = "2.000000000000000000000000001"
        gross = "8.000000000000000000000000006"
        variable = "4.000000000000000000000000001"
        self.assertEqual(Fraction(variable) * Fraction(multiplier) - Fraction(gross), Fraction(1, 10**54))
        self.lock["policy"]["stress_cases"][1]["cost_multiple"] = multiplier
        for run in self.bundle["equity"]:
            if run["scenario_id"] == "stress":
                run["execution_spec"]["cost_multiple"] = multiplier
                run["execution_receipt_hash"] = sha(run["execution_spec"])
                for row in run["sessions"]:
                    row.update(opening_equity="100000", closing_equity="100000", gross_pnl="0", fixed_cost="0", variable_cost_unscaled="0")
                    row["sector_gross_pnl"] = {name: "0" for name in row["sector_gross_pnl"]}
        row = self.bundle["equity"][1]["sessions"][0]
        row.update(gross_pnl=gross, variable_cost_unscaled=variable)
        row["sector_gross_pnl"] = {"TEST_SECTOR": gross}
        self.reseal()
        with self.assertRaisesRegex(ValueError, "unreconciled cash/cost/equity"):
            self.evaluate()

    def test_sector_cash_and_probability_partitions_preserve_small_terms(self):
        self.flat_equity()
        row = self.bundle["equity"][0]["sessions"][0]
        row["sector_gross_pnl"] = {"A": HUGE, "B": "1e-28", "C": "-" + HUGE}
        row["sector_exposure_fraction"] = {"A": "0.2", "B": "0", "C": "0"}
        self.reseal()
        with self.assertRaisesRegex(ValueError, "sector attribution"):
            self.evaluate()
        self.setUp(); self.flat_equity()
        row = self.bundle["equity"][0]["sessions"][0]
        row.update(exposure_fraction="1", sector_gross_pnl={"A": "0", "B": "0"},
                   sector_exposure_fraction={"A": "1", "B": "1e-28"})
        self.reseal()
        with self.assertRaisesRegex(ValueError, "sector attribution"):
            self.evaluate()
        self.setUp()
        self.bundle["forecasts"][0]["probabilities"] = {"TARGET_FIRST": "1", "STOP_FIRST": "1e-28", "EXPIRY": "0", "NON_FILL": "0"}
        with self.assertRaisesRegex(ValueError, "renormalized forecast probabilities"):
            self.predictions()

    def test_conditional_payoff_and_realized_payoff_subtractions_are_exact(self):
        for payoff in self.bundle["payoff_observations"]:
            payoff.update(gross_profit=HUGE, fixed_cost="0.1", variable_cost=HUGE)
        for row in self.bundle["forecasts"]:
            row.update(realized_gross_profit=HUGE, realized_fixed_cost="0.1", realized_variable_cost=HUGE)
        report = self.predictions()
        means = report["forecast_utilities"][0]["conditional_means"]
        self.assertTrue(all(abs(event["mean"] + 1e-6) < 1e-18 for event in means.values()))
        self.assertEqual(report["payoff_mean_error"], 0)
        for payoff in self.bundle["payoff_observations"]:
            payoff["fixed_cost"] = "0"
        report = self.predictions()
        self.assertAlmostEqual(report["payoff_mean_error"], -1e-6, places=18)
        self.assertAlmostEqual(report["payoff_rmse"], 1e-6, places=18)

    def test_financial_arithmetic_ignores_ambient_decimal_precision_and_rounding(self):
        expected_payload, expected_diagnostics = self.evaluate()
        for precision, rounding in ((3, ROUND_CEILING), (28, ROUND_FLOOR), (70, ROUND_CEILING)):
            with self.subTest(precision=precision, rounding=rounding), localcontext() as context:
                context.prec = precision
                context.rounding = rounding
                context.traps[Inexact] = True
                context.traps[Rounded] = True
                payload, diagnostics = self.evaluate()
                self.assertEqual(payload, expected_payload)
                self.assertEqual(diagnostics, expected_diagnostics)
                self.assertEqual(number("1000") + number(HUGE) - number(HUGE), 1000)
                self.assertEqual(decimal_string(number("0.1") + number("0.2")), "0.3")
                self.assertLessEqual(Fraction(rounded_lower(-1e-13)), Fraction(-1e-13))

    def test_fit_must_exist_at_generation_not_merely_at_decision(self):
        self.bundle["fits"][0]["fitted_at"] = stamp(60, 8, 59)
        self.bundle["forecasts"][0].update(observed_at=stamp(60, 8), available_at=stamp(60, 8),
                                            generated_at=stamp(60, 8, 1), recorded_at=stamp(60, 8, 2))
        self.reseal()
        with self.assertRaisesRegex(ValueError, "generated before.*fit completed"):
            self.evaluate()

    def test_payoff_domain_is_checked_before_rounding_to_a_statistical_float(self):
        payoff = self.bundle["payoff_observations"][0]
        payoff.update(opening_equity="1", gross_profit="0.5000000000000000000000000001", fixed_cost="0", variable_cost="0")
        self.assertEqual(float(Fraction(payoff["gross_profit"])), 0.5)
        with self.assertRaisesRegex(ValueError, "payoff outside frozen finite domain"):
            self.predictions()

    def test_fit_generation_boundary_is_allowed_and_test_only_stays_false(self):
        self.bundle["fits"][0]["fitted_at"] = stamp(60, 8, 1)
        self.bundle["forecasts"][0].update(observed_at=stamp(60, 8), available_at=stamp(60, 8),
                                            generated_at=stamp(60, 8, 1), recorded_at=stamp(60, 8, 2))
        self.reseal()
        payload, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        self.assertTrue(all(payload[gate] is False for gate in diagnostics["computed_gates_before_evidence_class"]))

    def test_entry_is_session_one_and_the_third_session_close_is_inclusive(self):
        self.move_outcome(2)
        self.reseal()
        payload, diagnostics = self.evaluate()
        self.assertTrue(all(diagnostics["computed_gates_before_evidence_class"].values()))
        self.assertFalse(payload["causal_and_uncontaminated"])
        sessions, _, _ = calendar_index(self.bundle, timestamp(self.request["manifest"]["cutoff_at"]))
        self.assertEqual(holding_span(sessions, "outer-0", timestamp(stamp(60, 10)), timestamp(stamp(62, 16)), 3), 3)

    def test_long_label_and_reconciled_trade_cannot_hide_four_session_steps(self):
        self.move_outcome(4)
        self.reseal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()

    def test_the_fourth_session_is_already_too_late_for_three_sessions(self):
        self.move_outcome(3, trades=False)
        self.reseal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()

    def test_closed_trade_horizon_is_checked_independently_of_a_short_label(self):
        self.move_outcome(4, label=False)
        self.reseal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()

    def test_neighbor_trade_horizon_cannot_bypass_candidate_forecast_checks(self):
        self.bundle["trades"].append({"trade_id": "test-long-neighbor", "strategy_id": "neighbor-test", "scenario_id": "base", "state": "CLOSED",
                                      "entry_at": stamp(60, 10), "exit_at": stamp(64, 10), "recorded_at": stamp(64, 10, 1),
                                      "source_hash": sha("TEST_ONLY-long-neighbor")})
        run = next(run for run in self.bundle["equity"] if run["strategy_id"] == "neighbor-test" and run["scenario_id"] == "base")
        run["sessions"][4]["closed_trade_ids"].append("test-long-neighbor")
        self.reseal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()
        run["sessions"][4]["closed_trade_ids"].remove("test-long-neighbor")
        self.reseal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()

    def test_shadow_closed_trade_obeys_the_same_calendar_holding_boundary(self):
        trade = {"trade_id": "TEST_ONLY-shadow-trade", "candidate_id": self.request["trial"]["candidate_id"],
                 "source_hash": sha("TEST_ONLY-shadow-payoff"), "state": "CLOSED", "entry_session_id": "shadow-0",
                 "exit_session_id": "shadow-2", "entry_at": stamp(100, 10), "exit_at": stamp(102, 16),
                 "recorded_at": stamp(102, 16, 1), "net_profit": "0"}
        self.shadow["trades"] = [trade]
        self.reseal()
        payload, diagnostics = self.evaluate()
        self.assertEqual(payload["shadow_closed_trades"], 1)
        self.assertFalse(payload["economic_feasibility_passed"])
        trade.update(exit_session_id="shadow-3", exit_at=stamp(103, 10), recorded_at=stamp(103, 10, 1))
        self.reseal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()

    def test_training_payoff_label_must_obey_real_exchange_session_horizon(self):
        sample = self.bundle["samples"][0]
        sample.update(label_end_at=stamp(4, 10), label_available_at=stamp(4, 10), label_recorded_at=stamp(4, 10, 1))
        self.bundle["payoff_observations"][0].update(available_at=stamp(4, 10, 1), recorded_at=stamp(4, 10, 2))
        self.reseal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()

    def test_missing_future_calendar_is_not_assumed_from_a_short_observed_label(self):
        self.bundle["calendar"]["sessions"] = [s for s in self.bundle["calendar"]["sessions"] if s["phase"] != "SHADOW"]
        self.bundle["calendar"]["weeks"] = []
        self.shadow["observations"] = []
        self.reseal()
        with self.assertRaisesRegex(ValueError, "future exchange calendar coverage"):
            self.evaluate()

    def test_pending_labels_and_declared_horizons_cannot_extend_the_holding_limit(self):
        self.move_outcome(4, trades=False, pending=True)
        self.reseal()
        with self.assertRaisesRegex(ValueError, "holding-session horizon"):
            self.evaluate()
        self.setUp()
        with self.assertRaises(ValueError):
            self.predictions(holding=2)


if __name__ == "__main__":
    unittest.main()

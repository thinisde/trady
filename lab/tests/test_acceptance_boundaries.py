"""TEST_ONLY counterexamples for static horizons and deterministic acceptance."""
import pathlib
import sys
import unittest
from copy import deepcopy
from fractions import Fraction
from unittest.mock import patch

LAB = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(LAB))
from evaluator.common import EVENTS, canonical, decimal_string, number, numeric_margin, sha, timestamp
from evaluator.observations import calendar_index, validate_fits
from evaluator.predictions import calibration_scores, score_predictions
from evaluator.run import evaluate, neighbors, validate_policy
from evaluator.statistics import deterministic_risk
from sealed_fixture import fixture, reseal


def exact(value):
    return Fraction(int(value["numerator"]), int(value["denominator"]))


class AcceptanceBoundaries(unittest.TestCase):
    def setUp(self):
        self.request, self.lock, self.bundle, self.search, self.shadow = fixture()
        self.source_hash = self.request["manifest"]["dataset_hash"]

    def reseal(self):
        reseal(self.request, self.lock, self.bundle, self.search, self.shadow)
        self.assertEqual(self.request["manifest"]["dataset_hash"], self.source_hash)

    def evaluate(self):
        payload, diagnostics = evaluate(self.request, self.lock, self.bundle, self.search, self.shadow)
        self.assertEqual(self.lock["evidence_class"], "TEST_ONLY")
        self.assertTrue(all(payload[gate] is False for gate in diagnostics["computed_gates_before_evidence_class"]))
        return payload, diagnostics

    def holding_neighbor(self, horizon):
        parameters = {**self.request["trial"]["parameters"], "max_holding_sessions": horizon}
        self.bundle["neighbors"][0]["parameters"] = parameters
        self.lock["policy"]["neighborhood_steps"] = {"max_holding_sessions": "1"}
        for run in self.bundle["equity"]:
            if run["strategy_id"] == "neighbor-test":
                run["execution_spec"]["parameters"] = deepcopy(parameters)
                run["execution_receipt_hash"] = sha(run["execution_spec"])

    def payoff_mixture(self, values=("0.1", "0.2", "-0.3", "0")):
        returns = dict(zip(EVENTS, values))
        selected = []
        for payoff in self.bundle["payoff_observations"]:
            payoff.update(opening_equity="1", gross_profit=returns[payoff["event"]], fixed_cost="0", variable_cost="0")
            if int(payoff["session_id"].split("-")[1]) < 8:
                selected.append(payoff["observation_id"])
        for row in self.bundle["forecasts"]:
            row.update(payoff_sample_ids=selected, opening_equity="1", realized_gross_profit=returns[row["outcome"]],
                       realized_fixed_cost="0", realized_variable_cost="0")
        self.lock["policy"].update(utility_risk_weight="0", utility_uncertainty_weight="0",
                                    utility_complexity_weight="0", cash_hurdle="1e-28")

    def predictions(self):
        cutoff = timestamp(self.request["manifest"]["cutoff_at"])
        sessions, outer, _ = calendar_index(self.bundle, cutoff)
        fits, samples = validate_fits(self.bundle, sessions, outer, self.lock["policy"])
        report = score_predictions(self.bundle, sessions, fits, samples, self.request["trial"]["candidate_id"],
                                   self.lock["policy"], cutoff, 1, "acceptance-boundaries", 3)
        canonical(report)  # Exact internal values must have JSON-safe diagnostics.
        return report

    def flat_account_loss(self, loss):
        candidate = self.request["trial"]["candidate_id"]
        for run in self.bundle["equity"]:
            opening = Fraction(1000)
            for index, row in enumerate(run["sessions"]):
                gross = -number(loss) if run["strategy_id"] == candidate and index == 0 else Fraction(0)
                closing = opening + gross
                row.update(opening_equity=decimal_string(opening), closing_equity=decimal_string(closing),
                           boundary_cash_flow="0", gross_pnl=decimal_string(gross), fixed_cost="0", variable_cost_unscaled="0")
                row["sector_gross_pnl"] = {name: decimal_string(gross) for name in row["sector_gross_pnl"]}
                opening = closing

    def test_neighbor_horizon_is_checked_without_any_neighbor_trades(self):
        self.holding_neighbor(4)
        self.reseal()
        self.assertFalse(any(t["strategy_id"] == "neighbor-test" for t in self.bundle["trades"]))
        with patch("evaluator.run.calendar_index", side_effect=AssertionError("observations reached")), \
                self.assertRaisesRegex(ValueError, "registered.*horizon|registered holding|resampling block"):
            self.evaluate()

    def test_label_horizon_does_not_waive_a_short_block_for_a_neighbor(self):
        self.holding_neighbor(4)
        self.lock["policy"]["max_label_horizon_sessions"] = 4
        self.reseal()
        with patch("evaluator.run.equity_series", side_effect=AssertionError("accounting reached")), \
                self.assertRaisesRegex(ValueError, "resampling block"):
            self.evaluate()

    def test_untried_registered_candidates_are_in_the_static_horizon_family(self):
        self.request["plan"]["candidates"].append({**self.request["trial"]["parameters"], "max_holding_sessions": 5})
        self.reseal()
        with self.assertRaisesRegex(ValueError, "registered holding"):
            self.evaluate()

    def test_static_holding_block_equality_is_valid(self):
        self.holding_neighbor(4)
        self.lock["policy"].update(max_label_horizon_sessions=4, block_sessions=4, minimum_calibration_sessions=8)
        self.reseal()
        neighbors(self.bundle, self.request["trial"]["parameters"], self.request["plan"]["envelope"], self.lock["policy"])
        cases = validate_policy(self.lock["policy"], self.request, [n["parameters"] for n in self.bundle["neighbors"]])
        self.assertEqual(set(cases), {"base", "stress"})

    def test_zero_exact_utility_cannot_become_a_positive_entry(self):
        self.payoff_mixture()
        self.reseal()
        self.assertEqual(sum((Fraction(v) for v in ("0.1", "0.2", "-0.3", "0")), Fraction(0)) / 4, 0)
        _, diagnostics = self.evaluate()
        self.assertEqual(exact(diagnostics["predictions"]["forecast_utilities"][0]["exact"]["utility"]), 0)
        self.assertFalse(diagnostics["predictions"]["positive_entry_utility"])
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["economic_feasibility_passed"])

    def test_exposure_just_above_cap_cannot_round_back_under_it(self):
        candidate = self.request["trial"]["candidate_id"]
        for run in self.bundle["equity"]:
            if run["strategy_id"] == candidate:
                for row in run["sessions"]:
                    row.update(exposure_fraction="0.50000000000000001", sector_exposure_fraction={"TEST_SECTOR": "0.50000000000000001"})
        self.reseal()
        self.assertEqual(float("0.50000000000000001"), 0.5)
        _, diagnostics = self.evaluate()
        self.assertFalse(diagnostics["computed_gates_before_evidence_class"]["risk_not_deteriorated"])

    def test_exposure_cap_equality_is_not_rejected(self):
        candidate = self.request["trial"]["candidate_id"]
        for run in self.bundle["equity"]:
            if run["strategy_id"] == candidate:
                for row in run["sessions"]:
                    row.update(exposure_fraction="0.5", sector_exposure_fraction={"TEST_SECTOR": "0.5"})
        self.reseal()
        _, diagnostics = self.evaluate()
        self.assertTrue(diagnostics["computed_gates_before_evidence_class"]["risk_not_deteriorated"])

    def test_turnover_relative_deterioration_cannot_round_to_its_allowance(self):
        candidate = self.request["trial"]["candidate_id"]
        self.lock["policy"]["maximum_turnover_increase"] = "0.1"
        for value, accepted in (("1.1", True), ("1.10000000000000001", False)):
            with self.subTest(turnover=value):
                for run in self.bundle["equity"]:
                    if run["strategy_id"] == candidate:
                        for row in run["sessions"]:
                            row["turnover"] = value
                self.reseal()
                _, diagnostics = self.evaluate()
                measured = diagnostics["statistics"][candidate + "|base"]
                self.assertEqual(measured["mean_turnover"], 1.1)
                self.assertEqual(exact(measured["deterministic_risk_exact"]["mean_turnover"]), Fraction(value))
                self.assertEqual(diagnostics["computed_gates_before_evidence_class"]["risk_not_deteriorated"], accepted)

    def test_drawdown_tail_and_worst_loss_caps_use_exact_account_returns(self):
        candidate = self.request["trial"]["candidate_id"]
        cases = (("maximum_drawdown", "max_drawdown", "0.1"),
                 ("maximum_drawdown_increase", "max_drawdown", "0.1"),
                 ("maximum_cvar_increase", "cvar_95_loss", "0.05"),
                 ("maximum_worst_loss_increase", "worst_session_loss", "0.1"))
        for setting, metric, cap in cases:
            for loss, accepted in (("100", True), ("100.000000000000001", False)):
                with self.subTest(setting=setting, loss=loss):
                    self.lock["policy"].update(maximum_drawdown="1", maximum_drawdown_increase="1",
                                                maximum_cvar_increase="1", maximum_worst_loss_increase="1")
                    self.lock["policy"][setting] = cap
                    self.flat_account_loss(loss)
                    self.reseal()
                    _, diagnostics = self.evaluate()
                    measured = diagnostics["statistics"][candidate + "|base"]
                    self.assertEqual(measured[metric], float(cap))
                    self.assertEqual(exact(measured["deterministic_risk_exact"][metric]) <= Fraction(cap), accepted)
                    self.assertEqual(diagnostics["computed_gates_before_evidence_class"]["risk_not_deteriorated"], accepted)

    def test_empirical_cvar_tail_count_uses_the_exact_five_percent_boundary(self):
        for count, expected in ((20, Fraction(3, 10)), (21, Fraction(1, 5))):
            returns = [Fraction(-3, 10), Fraction(-1, 10), *([Fraction(0)] * (count - 2))]
            measured = deterministic_risk(returns, [Fraction(0)] * count, [Fraction(0)] * count)
            self.assertEqual(measured["cvar_95_loss"], expected)

    def test_entry_margin_applies_even_when_all_penalties_are_zero(self):
        cases = (("0.0000000000000000000000000001", False),
                 ("0.0000000010000000000000000001", False),
                 ("0.0000000010000000000000000002", True))
        for value, accepted in cases:
            with self.subTest(return_value=value):
                self.payoff_mixture((value,) * 4)
                report = self.predictions()
                self.assertEqual(exact(report["forecast_utilities"][0]["exact"]["utility"]), Fraction(value))
                self.assertEqual(report["positive_entry_utility"], accepted)

    def test_payoff_means_and_probability_weighting_keep_nonterminating_ratios(self):
        self.payoff_mixture(("0",) * 4)
        selected = []
        for payoff in self.bundle["payoff_observations"]:
            if int(payoff["session_id"].split("-")[1]) < 9:
                selected.append(payoff["observation_id"])
            if payoff["session_id"] == "train-0" and payoff["event"] == EVENTS[0]:
                payoff["gross_profit"] = "0.1"
        for row in self.bundle["forecasts"]:
            row["payoff_sample_ids"] = selected
        report = self.predictions()
        utility = report["forecast_utilities"][0]
        self.assertEqual(exact(utility["conditional_means"][EVENTS[0]]["exact"]["mean"]), Fraction(1, 90))
        self.assertEqual(exact(utility["exact"]["expected_return"]), Fraction(1, 360))
        self.assertTrue(report["positive_entry_utility"])

    def test_exact_weighted_downside_uncertainty_complexity_and_hurdle(self):
        self.payoff_mixture(("0.01", "-0.001", "0.001", "0"))
        self.lock["policy"].update(utility_risk_weight="0.2", utility_uncertainty_weight="0.3", utility_complexity_weight="0.0001")
        for hurdle, accepted in (("0.0017499986", False), ("0.0017499985999999999999999999", True)):
            self.lock["policy"]["cash_hurdle"] = hurdle
            report = self.predictions()
            values = report["forecast_utilities"][0]["exact"]
            self.assertEqual(exact(values["utility"]), Fraction("0.0017499996"))
            self.assertEqual(exact(values["complexity_penalty"]), Fraction("0.0007"))
            self.assertEqual(report["positive_entry_utility"], accepted)

    def test_forecast_session_coverage_threshold_does_not_round_down(self):
        removed = {row["prediction_id"] for row in self.bundle["forecasts"][:9]}
        self.bundle["forecasts"] = self.bundle["forecasts"][9:]
        removed_trades = {row["trade_id"] for row in self.bundle["trades"] if row["prediction_id"] in removed}
        self.bundle["trades"] = [row for row in self.bundle["trades"] if row["trade_id"] not in removed_trades]
        for run in self.bundle["equity"]:
            for row in run["sessions"]:
                row["closed_trade_ids"] = [identity for identity in row["closed_trade_ids"] if identity not in removed_trades]
        for minimum, accepted in (("0.75", True), ("0.7500000000000000000000000001", False)):
            self.lock["policy"]["minimum_forecast_coverage"] = minimum
            self.reseal()
            _, diagnostics = self.evaluate()
            self.assertEqual(diagnostics["predictions"]["session_coverage"], 0.75)
            self.assertEqual(diagnostics["predictions"]["eligible"], accepted)

    def test_realized_payoff_mean_error_cannot_round_under_its_cap(self):
        self.payoff_mixture(("0",) * 4)
        self.lock["policy"]["maximum_payoff_mean_error"] = "0.1"
        for actual, accepted in (("0.1", True), ("0.1000000000000000000000000001", False),
                                 ("-0.1", True), ("-0.1000000000000000000000000001", False)):
            with self.subTest(actual=actual):
                for row in self.bundle["forecasts"]:
                    row["realized_gross_profit"] = actual
                report = self.predictions()
                self.assertEqual(report["payoff_mean_error"], float(actual))
                self.assertEqual(exact(report["exact"]["payoff_mean_error"]), Fraction(actual))
                self.assertEqual(report["eligible"], accepted)

    def test_numerical_margin_bounds_cannot_round_into_the_allowed_interval(self):
        for value in ("0.0000000009999999999999999999", "0.0001000000000000000000000001"):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "numerical margin"):
                numeric_margin({"numeric_margin": value})
        self.assertEqual(numeric_margin({"numeric_margin": "1e-9"}), Fraction(1, 10**9))
        self.assertEqual(numeric_margin({"numeric_margin": "0.0001"}), Fraction(1, 10**4))

    def test_confidence_level_cannot_round_under_its_maximum(self):
        self.lock["policy"]["alpha"] = "0.0500000000000000000000000001"
        self.reseal()
        with self.assertRaisesRegex(ValueError, "confidence level"):
            self.evaluate()


class CalibrationBoundaries(unittest.TestCase):
    def setUp(self):
        self.policy = fixture()[1]["policy"]
        self.policy.update(minimum_calibration_samples=1, minimum_calibration_sessions=2,
                           minimum_calibration_samples_per_event=1, maximum_brier_deterioration="1",
                           maximum_log_loss="100", maximum_ece="1", maximum_class_shift="1", maximum_brier_drift="1")
        self.rows = [{"session_id": f"TEST_ONLY-{i}", "outcome": EVENTS[i % 4],
                      "probabilities": {event: Fraction(1, 4) for event in EVENTS},
                      "prior": {event: Fraction(1, 4) for event in EVENTS}} for i in range(32)]

    def scores(self):
        report = calibration_scores(self.rows, self.policy)
        canonical(report)
        return report

    def check_threshold(self, setting, metric, boundary):
        for cap, accepted in ((Fraction(boundary) - Fraction(1, 10**28), False), (Fraction(boundary), True)):
            with self.subTest(setting=setting, cap=cap):
                self.policy[setting] = decimal_string(cap)
                report = self.scores()
                self.assertEqual(float(cap), float(boundary))
                measured = exact(report["exact"][metric])
                if metric == "brier":
                    measured -= exact(report["exact"]["prior_brier"])
                self.assertEqual(measured, Fraction(boundary))
                self.assertEqual(report["eligible"], accepted)

    def biased_probabilities(self):
        for row in self.rows:
            row["probabilities"] = dict(zip(EVENTS, map(Fraction, ("0.5", "0.25", "0.125", "0.125"))))

    def test_brier_deterioration_is_compared_against_an_exact_prior(self):
        self.biased_probabilities()
        self.check_threshold("maximum_brier_deterioration", "brier", "0.09375")

    def test_expected_calibration_error_cannot_round_under_its_cap(self):
        self.biased_probabilities()
        self.check_threshold("maximum_ece", "ece", "0.125")

    def test_class_frequency_shift_uses_exact_empirical_counts(self):
        outcomes = ([EVENTS[0]] * 8 + [EVENTS[1]] * 4 + [EVENTS[2]] * 2 + [EVENTS[3]] * 2
                    + [EVENTS[1]] * 4 + [EVENTS[2]] * 6 + [EVENTS[3]] * 6)
        for row, outcome in zip(self.rows, outcomes):
            row["outcome"] = outcome
        self.check_threshold("maximum_class_shift", "class_shift", "0.5")

    def test_brier_drift_cannot_round_under_its_cap(self):
        for row in self.rows[16:]:
            other = next(event for event in EVENTS if event != row["outcome"])
            row["probabilities"][row["outcome"]] = Fraction(1, 8)
            row["probabilities"][other] = Fraction(3, 8)
        self.check_threshold("maximum_brier_drift", "brier_drift", "0.28125")

    def test_reliability_bins_and_diagnostics_keep_exact_probabilities(self):
        p = Fraction("0.2999999999999999999999999999")
        for row in self.rows:
            row["probabilities"] = dict(zip(EVENTS, (p, Fraction(1, 2) - p, Fraction(1, 4), Fraction(1, 4))))
        report = self.scores()
        target = next(item for item in report["reliability"] if item["event"] == EVENTS[0])
        self.assertEqual(float(p), 0.3)
        self.assertEqual(target["bin"], 2)
        self.assertEqual(exact(target["exact"]["mean_probability"]), p)
        self.assertTrue(report["eligible"])

    def test_empirical_prior_support_keeps_nonterminating_counts(self):
        prior = dict(zip(EVENTS, (Fraction(1, 3), Fraction(1, 6), Fraction(1, 3), Fraction(1, 6))))
        for row in self.rows:
            row["prior"] = prior
        report = self.scores()
        self.assertEqual(exact(report["exact"]["prior_brier"]), Fraction(7, 9))
        self.assertEqual(report["class_counts"], dict.fromkeys(EVENTS, 8))
        self.policy["minimum_calibration_samples_per_event"] = 9
        self.assertFalse(self.scores()["eligible"])

    def test_log_loss_uses_an_independent_conservative_numerical_margin(self):
        report = self.scores()
        self.policy["maximum_log_loss"] = str(report["log_loss"])
        self.assertFalse(self.scores()["eligible"])
        self.policy["maximum_log_loss"] = "1.386294363"
        self.assertTrue(self.scores()["eligible"])
        self.assertGreater(exact(report["exact"]["log_loss_numerical_upper"]), Fraction(report["log_loss"]))

if __name__ == "__main__":
    unittest.main()

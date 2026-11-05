"""Execution chronology regressions for the independent daily-bar evaluator."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("donchian_evaluator", ROOT / "scripts/evaluate-long-donchian-atr.py")
evaluator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluator)


class DonchianReplayTests(unittest.TestCase):
    def bars(self, opening):
        rows = [{"observed_at": f"session_{i:02}", "open": "100", "high": "101",
                 "low": "99", "close": "100"} for i in range(40)]
        rows[35].update(open="109", high="111", low="108", close="110")
        rows[36].update(open="110", high="110.1", low="109.9", close="110")
        rows[37].update(open=opening, high="121", low="90", close="110")
        return rows

    def test_opening_target_wins_over_a_later_stop_touch(self):
        trade = evaluator.simulate_symbol("FIXTURE", self.bars("120"))[0]
        self.assertEqual(trade["status"], "TARGET")
        self.assertEqual(evaluator.dec(trade["gross_return"]), evaluator.dec("120") / evaluator.dec(trade["entry"]) - 1)

    def test_ambiguous_intrabar_touch_still_uses_the_stop_first(self):
        trade = evaluator.simulate_symbol("FIXTURE", self.bars("110"))[0]
        self.assertEqual(trade["status"], "STOP_SAME_BAR_FIRST")
        self.assertEqual(evaluator.dec(trade["gross_return"]), evaluator.dec(trade["stop"]) / evaluator.dec(trade["entry"]) - 1)


if __name__ == "__main__":
    unittest.main()

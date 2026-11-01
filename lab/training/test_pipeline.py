"""Offline regression tests for causal research dataset preparation."""
import copy
import datetime as dt
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("training_pipeline", Path(__file__).with_name("pipeline.py"))
pipeline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pipeline)


def bar(open_=100, high=100.5, low=99.5, close=100, volume=1000):
    return {"open": open_, "high": high, "low": low, "close": close, "volume": volume}


class HistoricalReplayTests(unittest.TestCase):
    def test_same_bar_order_is_not_invented(self):
        label, reason = pipeline.barrier_label([bar(high=102, low=98)])
        self.assertIsNone(label)
        self.assertEqual(reason, "ambiguous")

    def test_known_open_precedes_intraday_extremes(self):
        label, reason = pipeline.barrier_label([bar(), bar(open_=102, high=103, low=98, close=101)])
        self.assertEqual((label, reason), (1.0, "target_first"))
        label, reason = pipeline.barrier_label([bar(), bar(open_=98, high=102, low=97, close=99)])
        self.assertEqual((label, reason), (0.0, "stop_first"))

    def test_expiry_is_not_reported_as_target_hit(self):
        self.assertEqual(pipeline.barrier_label([bar()] * 5), (0.0, "expiry_without_target"))

    def test_features_do_not_depend_on_future_bars(self):
        bars = [bar(close=100 + i / 10, high=110) for i in range(30)]
        original = pipeline.features(bars, 20)
        changed = copy.deepcopy(bars)
        for future in changed[21:]:
            future.update(close=100000, volume=1000000, high=200000)
        self.assertEqual(original, pipeline.features(changed, 20))

    def test_missing_volume_is_not_imputed(self):
        self.assertIsNone(pipeline.features([bar(volume=0)] * 21, 20))

    def test_corrupt_duplicate_and_foreign_bars_are_rejected(self):
        start = dt.date(2020, 1, 1)
        values = [{"datetime": (start + dt.timedelta(days=i)).isoformat(), **bar()}
                  for i in range(100)]
        document = {"status": "ok", "meta": {"symbol": "MSFT", "currency": "USD", "interval": "1day"}, "values": values}
        end = start + dt.timedelta(days=150)
        self.assertEqual(len(pipeline.validate_bars(document, "MSFT", start, end)), 100)
        for change in ["duplicate", "symbol", "nonfinite", "inconsistent"]:
            bad = copy.deepcopy(document)
            if change == "duplicate":
                bad["values"][1]["datetime"] = bad["values"][0]["datetime"]
            elif change == "symbol":
                bad["meta"]["symbol"] = "JPM"
            elif change == "nonfinite":
                bad["values"][0]["close"] = "NaN"
            else:
                bad["values"][0]["low"] = 200
            with self.subTest(change=change), self.assertRaises(ValueError):
                pipeline.validate_bars(bad, "MSFT", start, end)

    def test_artifacts_cannot_overwrite_prior_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "dataset.json"
            pipeline.publish(path, {"first": True})
            with self.assertRaises(FileExistsError):
                pipeline.publish(path, {"first": False})
            self.assertIn('"first": true', path.read_text())


if __name__ == "__main__":
    unittest.main()

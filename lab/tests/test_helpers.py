import hashlib
import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


reconciliation = load("trady_reconcile", ROOT / "reconcile.py")
lean = load("trady_lean", ROOT / "lean/run.py")


class OfflineHelpers(unittest.TestCase):
    def test_reconciliation_requires_exact_scope_hash_and_unresolved_free_data(self):
        content = {"cutoff_at": "2026-10-30T20:00:00Z", "report": {"unresolved_count": 0}, "unresolved_items": []}
        item = {"content": content, "hash": hashlib.sha256(reconciliation.canonical(content)).hexdigest()}
        export = {"account_id": "paper_test", "mode": "paper", "reconciliations": [item]}
        manifest = {"account_id": "paper_test", "mode": "paper", "cutoff_at": content["cutoff_at"]}
        self.assertEqual(reconciliation.reconcile(export, manifest), content["report"])
        content["report"]["unresolved_count"] = 1
        with self.assertRaises(ValueError):
            reconciliation.reconcile(export, manifest)
        export["reconciliations"] = []
        with self.assertRaises(ValueError):
            reconciliation.reconcile(export, manifest)

    def test_fold_metrics_use_real_continuous_equity_and_reject_missing_valuations(self):
        interval = {"opens_at": "open", "closes_at": "close"}
        report = {"data_kind": "REAL", "closed_trades": 0, "equity": [{"at": "open", "equity": "100"}, {"at": "close", "equity": "90"}]}
        measured = lean.fold_metrics(report, interval, 2)
        self.assertEqual(measured["max_drawdown"], "0.1")
        self.assertLess(float(measured["mean_log_return"]), 0)
        self.assertEqual(measured["closed_trades"], 0)
        report["equity"].pop()
        with self.assertRaises(ValueError):
            lean.fold_metrics(report, interval, 2)
        report["data_kind"] = "DEMO"
        with self.assertRaises(ValueError):
            lean.fold_metrics(report, interval, 1)


if __name__ == "__main__":
    unittest.main()

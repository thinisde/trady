import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("cleanup", ROOT / "scripts/cleanup-eur-demo-probe.py")
cleanup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cleanup)


class CleanupTests(unittest.TestCase):
    def test_account_summary_id_never_becomes_an_order_id(self):
        rows = [{"phase": "RESPONSE", "method": "GET", "path": "account/summary", "http_status": 200, "response": {"id": 52211463}},
                {"phase": "RESPONSE", "method": "POST", "path": "orders/limit", "http_status": 200, "response": {"id": 123}},
                {"request": {"side": "SELL"}, "response": {"broker_order_id": "456"}}]
        self.assertEqual(cleanup.acknowledged_ids("transport.jsonl", rows), {"123", "456"})

    def test_unknown_and_server_error_mutations_require_review(self):
        before = {"phase": "BEFORE_SEND", "method": "POST", "path": "orders/market"}
        for status, unresolved in [(200, False), (400, False), (500, True), (408, True), (429, True)]:
            rows = [before, {"phase": "RESPONSE", "method": "POST", "path": "orders/market", "http_status": status}]
            self.assertEqual(cleanup.unresolved_mutation(rows) is not None, unresolved)
        self.assertIsNotNone(cleanup.unresolved_mutation([before, {"phase": "UNKNOWN", "method": "POST", "path": "orders/market"}]))

    def test_cancel_ack_or_definite_not_found_clears_its_marker(self):
        before = {"phase": "BEFORE_SEND", "method": "DELETE", "path": "orders/123"}
        self.assertIsNone(cleanup.unresolved_mutation([before, {"phase": "RESPONSE", "method": "DELETE", "path": "orders/123", "accepted": True}]))
        self.assertIsNone(cleanup.unresolved_mutation([before, {"phase": "RESPONSE", "method": "DELETE", "path": "orders/123", "http_status": 404}]))


if __name__ == "__main__":
    unittest.main()

import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("eur_analysis", ROOT / "scripts/analyze-eur-demo-evidence.py")
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)


class AnalysisTests(unittest.TestCase):
    def row(self):
        return {"order": {"id": 1, "ticker": "AIRp_EQ", "side": "BUY", "instrument": {"ticker": "AIRp_EQ", "isin": "NL0000235190"}},
                "fill": {"id": 2, "price": 200, "quantity": 1, "filledAt": "2026-11-05T09:00:02Z",
                         "walletImpact": {"currency": "EUR", "fxRate": 1, "netValue": 200.1, "taxes": []}}}

    def capture(self):
        return {"name": "source", "contracts": {"contracts": [{"con_id": 29612256, "identifiers": {"ISIN": "NL0000235190"},
                    "currency": "EUR", "primary_exchange": "SBF", "exchange": "SMART"}]},
                "market_data_types": [{"request_id": 1000, "data_type": 1, "received_at": "2026-11-05T09:00:00Z"}],
                "books": {2000: [{"kind": "bid_ask", "request_id": 2000, "observed_at_seconds": 1793869201,
                    "received_at": "2026-11-05T09:00:01.1Z", "bid": "199.95", "ask": "200", "bid_size": "1", "ask_size": "1"}]}}

    def test_missing_wallet_is_not_zero_and_buy_sell_cost_signs_differ(self):
        row = self.row()
        self.assertEqual(analysis.wallet_cost(row)["observed_wallet_charge_eur"], "0.1")
        row["order"]["side"] = "SELL"
        row["fill"]["walletImpact"]["netValue"] = 199.9
        self.assertEqual(analysis.wallet_cost(row)["observed_wallet_charge_eur"], "0.1")
        del row["fill"]["walletImpact"]["taxes"]
        self.assertFalse(analysis.wallet_cost(row)["reconciled"])

    def test_conflicting_duplicate_fill_is_rejected(self):
        row, duplicate = self.row(), self.row()
        self.assertEqual(len(analysis.unique_fills([row, duplicate], "2026-11-05")), 1)
        duplicate["fill"]["price"] = 201
        with self.assertRaisesRegex(ValueError, "DUPLICATE_FILL_CONFLICT"):
            analysis.unique_fills([row, duplicate], "2026-11-05")

    def test_only_preceding_received_live_books_can_match(self):
        row, capture = self.row(), self.capture()
        self.assertIsNotNone(analysis.quote_match(row, [capture]))
        for change in ["late_receipt", "delayed", "future_provider", "zero_size"]:
            bad = copy.deepcopy(capture)
            book = bad["books"][2000][0]
            if change == "late_receipt":
                book["received_at"] = "2026-11-05T09:00:03Z"
            elif change == "delayed":
                bad["market_data_types"][0]["data_type"] = 3
            elif change == "future_provider":
                book["observed_at_seconds"] += 5
            else:
                book["ask_size"] = "0"
            self.assertIsNone(analysis.quote_match(row, [bad]), change)


if __name__ == "__main__":
    unittest.main()

#!/usr/bin/env python3
"""Offline tests; run with the same official SDK environment as the probe."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import unittest

from ibapi.common import PROTOBUF_MSG_ID
from ibapi.message import OUT

spec = importlib.util.spec_from_file_location("history_probe", Path(__file__).with_name("probe-ibkr-historical-quotes.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class HistoricalQuoteTests(unittest.TestCase):
    def setUp(self):
        self.fill = {"side":"BUY","fill_price":"100.01","filled_at":"2026-11-03T16:13:51Z"}
        self.time = 1793722431
        self.book = {"observed_at_seconds":self.time-1,"bid":"100","ask":"100.01","bid_size":"20","ask_size":"30"}

    def test_side_appropriate_reference_and_one_second_preceding_quote(self):
        result = probe.compare(self.fill, [self.book])
        self.assertEqual(result["reference"], "ASK")
        self.assertEqual(result["adverse_price_difference_bps"], "0")
        self.assertEqual(result["quote_age_seconds"], "1.0")
        self.fill.update(side="SELL",fill_price="100")
        result = probe.compare(self.fill,[self.book])
        self.assertEqual(result["reference"],"BID")
        self.assertEqual(result["adverse_price_difference_bps"],"0")
        self.fill["fill_price"]="99.99"
        self.assertEqual(probe.compare(self.fill,[self.book])["adverse_price_difference_bps"],"1.0000")

    def test_same_second_future_stale_crossed_and_empty_books_do_not_match(self):
        for change in [{"observed_at_seconds":self.time},{"observed_at_seconds":self.time+1},
                       {"observed_at_seconds":self.time-11},{"ask":"99"},{"bid":"0"},{"ask_size":"0"}]:
            result = probe.compare(self.fill,[{**self.book,**change}])
            self.assertEqual(result["status"],"NO_PRECEDING_VALID_BOOK_WITHIN_10_SECONDS")

    def test_subscription_warning_does_not_pretend_historical_response_is_done(self):
        app = probe.HistoricalProbe()
        app.history_ends[1] = threading.Event()
        app.error(1,0,2188,"synthetic warning")
        self.assertFalse(app.history_ends[1].is_set())
        app.error(1,0,10187,"synthetic permission error")
        self.assertTrue(app.history_ends[1].is_set())
        self.assertNotIn(1,app.history_done)

    def test_plain_and_protobuf_order_messages_are_rejected_before_transport(self):
        app = probe.HistoricalProbe()
        for identifier in [OUT.PLACE_ORDER,OUT.CANCEL_ORDER,OUT.REQ_GLOBAL_CANCEL]:
            with self.assertRaises(RuntimeError):app.sendMsg(identifier,b"")
            with self.assertRaises(RuntimeError):app.sendMsgProtoBuf(identifier+PROTOBUF_MSG_ID,b"")

    def test_manifest_rebinding_and_page_escape_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            page = root/"page.json"
            data = json.dumps({"status":"OK","response":{"items":[]}}).encode()
            page.write_bytes(data)
            manifest = {"broker_environment":"demo","broker_account_id":52211463,"base_currency":"EUR",
                        "broker_mutations":0,"history_complete":True,"pages":[{"path":"page.json","sha256":hashlib.sha256(data).hexdigest()}]}
            path = root/"manifest.json"
            path.write_text(json.dumps(manifest))
            self.assertEqual(probe.read_history(path)[1],[])
            page.write_text("{}")
            with self.assertRaises(ValueError):probe.read_history(path)
            manifest["pages"][0]["path"]="../outside.json"
            path.write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):probe.read_history(path)


if __name__ == "__main__":
    unittest.main()

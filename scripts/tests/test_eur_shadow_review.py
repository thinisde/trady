"""Regression checks for credential-safe capture and non-orderable review evidence."""
import base64
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


capture = module("eur_capture", "scripts/capture-eur-review.py")
review = module("eur_review", "scripts/verify-eur-shadow-review.py")


class Response:
    status = 200

    def __init__(self, body, content_type="application/json"):
        self.body = body
        self.headers = {"Content-Type": content_type}

    def read(self, limit):
        return self.body[:limit]

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False


class ReviewTests(unittest.TestCase):
    def fetch(self, response, provider="primary_public_source", params=None, secrets=()):
        with patch.object(capture, "build_opener") as opener:
            opener.return_value.open.return_value = response
            return capture.fetch(provider, "https://example.invalid/evidence", params=params, secrets=secrets)

    def test_echoed_secret_is_redacted_and_provider_limit_is_not_success(self):
        value = "review-fixture-private-key"
        response = Response(json.dumps({"Information": "Daily limit for " + value}).encode())
        result = self.fetch(response, "alpha_vantage", {"apikey": value}, (value,))
        self.assertNotIn(value, json.dumps(result))
        self.assertNotIn("apikey", result["parameters"])
        self.assertEqual(result["status"], "UNAVAILABLE")
        self.assertEqual(result["error_state"], "PROVIDER_LIMIT_OR_ERROR_PAYLOAD")

    def test_pdf_evidence_preserves_exact_bytes_and_hash(self):
        body = b"%PDF-1.7\n\x00\xff\x81review fixture"
        result = self.fetch(Response(body, "application/pdf"))
        self.assertEqual(result["response_encoding"], "base64")
        self.assertEqual(base64.b64decode(result["response"]), body)
        self.assertEqual(result["response_sha256"], hashlib.sha256(body).hexdigest())

    def test_wrong_account_stops_before_catalog_read_and_does_not_store_balances(self):
        document = {"status": "OK", "response": {"id": 7, "currency": "EUR", "cash": {"total": 12345}}}
        with tempfile.TemporaryDirectory() as directory:
            argv = ["capture", "--source", "broker_catalog", "--output", directory]
            with patch.object(sys, "argv", argv), patch.object(capture, "credentials", return_value={
                "TRADY_T212_DEMO_API_KEY": "fixture-key", "TRADY_T212_DEMO_API_SECRET": "fixture-secret"
            }), patch.object(capture, "fetch", return_value=document) as fetch:
                with self.assertRaisesRegex(ValueError, "DEMO_ACCOUNT_IDENTITY"):
                    capture.main()
                self.assertEqual(fetch.call_count, 1)
            saved = json.loads((Path(directory) / "account-binding-failed.json").read_text())
            self.assertEqual(saved["response"], {"id": 7, "currency": "EUR"})

    def test_unknown_precision_cannot_be_replaced_by_an_assumption(self):
        member = {"orderable": False, "minimum_quantity": "1", "quantity_step": None, "price_tick": None}
        with self.assertRaisesRegex(ValueError, "UNPROVEN_ORDER_PRECISION"):
            review.check_member(member, [], [], {}, {})

    def test_catalog_isin_change_invalidates_review(self):
        member = {"orderable": False, "minimum_quantity": None, "quantity_step": None,
                  "price_tick": None, "fractional_support": "UNVERIFIED", "broker_ticker": "AIRp_EQ",
                  "broker_instrument_id": "AIRp_EQ", "isin": "NL0000235190", "currency": "EUR",
                  "working_schedule_id": 76}
        catalog = [{"ticker": "AIRp_EQ", "isin": "FR0000120404", "currencyCode": "EUR", "type": "STOCK",
                    "workingScheduleId": 76}]
        with self.assertRaisesRegex(ValueError, "BROKER_IDENTITY_CHANGED"):
            review.check_member(member, catalog, [], {}, {})


if __name__ == "__main__":
    unittest.main()

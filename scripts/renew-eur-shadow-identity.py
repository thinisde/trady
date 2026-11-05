#!/usr/bin/env python3
"""Revalidate unchanged SHADOW identities against fresh broker and Yahoo reads.

Writes a new 24-hour identity-only review. Never edits a sealed review, order
rules, content rights, provider entitlements, release approval or a deployment.
"""
import argparse
import base64
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--crosswalk", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    spec = importlib.util.spec_from_file_location("capture", ROOT / "scripts/capture-eur-review.py")
    capture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(capture)
    original = json.loads(args.crosswalk.read_text())
    assert original["review_scope"] == "SHADOW_IDENTITY_ONLY" and original["broker_mutations_authorized"] is False
    args.out.mkdir(mode=0o700)
    secrets = capture.credentials()
    key, secret = (capture.secret(secrets, "TRADY_T212_DEMO_API_" + s) for s in ["KEY", "SECRET"])
    auth = base64.b64encode((key + ":" + secret).encode()).decode()
    documents = {}
    for name, suffix in [("account", "account/summary"), ("catalog", "metadata/instruments"), ("sessions", "metadata/exchanges")]:
        d = capture.fetch("trading212_demo", "https://demo.trading212.com/api/v0/equity/" + suffix,
                          headers={"Authorization": "Basic " + auth}, secrets=(key, secret, auth))
        if d["status"] != "OK":
            raise ValueError("CURRENT_BROKER_IDENTITY_UNAVAILABLE")
        if name == "account":
            assert d["response"]["id"] == 52211463 and d["response"]["currency"] == "EUR"
            d["response"] = {"id": 52211463, "currency": "EUR"}
        documents[name] = d
    for m in original["members"]:
        rows = [r for r in documents["catalog"]["response"] if r["ticker"] == m["broker_ticker"]]
        assert len(rows) == 1 and rows[0]["isin"] == m["isin"] and rows[0]["currencyCode"] == "EUR" and rows[0]["type"] == "STOCK"
        assert rows[0]["workingScheduleId"] == m["working_schedule_id"]
        assert any(v["id"] == m["broker_exchange_id"] and v["name"] == m["exchange"] and any(s["id"] == m["working_schedule_id"] for s in v["workingSchedules"]) for v in documents["sessions"]["response"])
        d = capture.fetch("yfinance_listing_metadata", "https://query1.finance.yahoo.com/v8/finance/chart/" + m["yahoo_symbol"], {"interval": "1d", "range": "5d"})
        if d["status"] != "OK":
            raise ValueError("CURRENT_YAHOO_IDENTITY_UNAVAILABLE")
        meta = d["response"]["chart"]["result"][0]["meta"]
        assert meta["symbol"] == m["yahoo_symbol"] and meta["currency"] == "EUR" and meta["instrumentType"] == "EQUITY"
        assert meta["exchangeName"] == m["yahoo_exchange_code"] and meta["exchangeTimezoneName"] == m["session_timezone"]
        assert not m["orderable"] and all(m[k] is None for k in ["minimum_quantity", "quantity_step", "price_tick"])
        documents[m["internal_instrument_id"]] = d
    current = datetime.now(timezone.utc)
    crosswalk = {**original, "effective_at": current.isoformat(), "expires_at": (current + timedelta(hours=24)).isoformat(),
                 "review_reference": "eur_shadow_identity_renewed_" + current.strftime("%Y%m%dT%H%M%SZ"),
                 "renewal_scope": "UNCHANGED_BROKER_AND_YAHOO_LISTING_IDENTITIES_ONLY",
                 "previous_crosswalk_sha256": hashlib.sha256(args.crosswalk.read_bytes()).hexdigest()}
    for name, document in documents.items():
        capture.write_new(args.out, name, document)
    crosswalk["catalog_capture"] = str((args.out / "catalog.json").resolve())
    crosswalk["exchange_capture"] = str((args.out / "sessions.json").resolve())
    for member in crosswalk["members"]:
        member["yahoo_capture"] = str((args.out / (member["internal_instrument_id"] + ".json")).resolve())
    capture.write_new(args.out, "crosswalk", crosswalk)
    hashes = {f.name: hashlib.sha256(f.read_bytes()).hexdigest() for f in args.out.glob("*.json")}
    capture.write_new(args.out, "renewal", {"schema_version": 1, "status": "IDENTITIES_REVALIDATED_FOR_SHADOW",
        "observed_at": current.isoformat(), "expires_at": crosswalk["expires_at"], "hashes": hashes,
        "independent_release_review": False, "entry_authorized": False, "broker_mutations": 0})
    print(json.dumps({"status": "IDENTITIES_REVALIDATED_FOR_SHADOW", "crosswalk": str((args.out / "crosswalk.json").resolve()),
                      "expires_at": crosswalk["expires_at"], "entry_authorized": False}))


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, ValueError, KeyError, OSError, TypeError):
        raise SystemExit("Identity renewal failed closed; existing review was not changed.") from None

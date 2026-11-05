#!/usr/bin/env python3
"""Bounded DEMO validation probes, journalled before send. Never retries a mutation.

Only AIRp_EQ/ASMLa_EQ, buy limits at EUR 1.xx and at most two shares are
permitted. Accepted probes are cancelled by exact ID and verified in history.
Acceptance proves the tested request only, not fills or a universal tick rule.
"""
import argparse
import base64
from datetime import datetime, timezone
from decimal import Decimal
import hashlib
import json
import os
from pathlib import Path
import time
from urllib.error import HTTPError
from urllib.request import Request, build_opener, HTTPRedirectHandler

ROOT = Path(__file__).resolve().parents[1]
BASE = "https://demo.trading212.com/api/v0/equity/"


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args):
        return None


def now():
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--increments-only", action="store_true")
    parser.add_argument("--precision-boundary-only", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(mode=0o700)
    store = Path(os.environ.get("TRADY_CREDENTIALS_FILE", ROOT / "config/credentials.local.json"))
    if store.stat().st_mode & 0o077:
        raise ValueError("insecure credential store")
    secrets = json.loads(store.read_text())["credentials"]
    auth = "Basic " + base64.b64encode((secrets["TRADY_T212_DEMO_API_KEY"] + ":" + secrets["TRADY_T212_DEMO_API_SECRET"]).encode()).decode()
    opener = build_opener(NoRedirect())
    events = []

    def journal(value):
        value = {"at": now(), **value}
        data = (json.dumps(value, sort_keys=True) + "\n").encode()
        fd = os.open(args.out / "journal.jsonl", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
        with os.fdopen(fd, "ab") as f:
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        events.append(value)

    def request(method, path, body=None):
        bound = 12 * 1024 * 1024 if path.startswith("metadata/") else 1024 * 1024
        journal({"phase": "BEFORE_SEND", "method": method, "path": path, "body": body})
        req = Request(BASE + path, data=body.encode() if body else None, method=method,
                      headers={"Authorization": auth, "Content-Type": "application/json"})
        try:
            with opener.open(req, timeout=15) as res:
                status, raw = res.status, res.read(bound + 1)
        except HTTPError as exc:
            status, raw = exc.code, exc.read(bound + 1)
        except OSError:
            journal({"phase": "UNKNOWN", "method": method, "path": path})
            raise ValueError("unknown request outcome; inspect journal, never retry mutation") from None
        if len(raw) > bound:
            raise ValueError("oversized response; outcome unknown")
        text = raw.decode()
        for secret in [auth, secrets["TRADY_T212_DEMO_API_KEY"], secrets["TRADY_T212_DEMO_API_SECRET"]]:
            text = text.replace(secret, "[REDACTED]")
        value = json.loads(text) if text else None
        journal({"phase": "RESPONSE", "method": method, "path": path, "http_status": status, "response": value})
        return status, value

    status, account = request("GET", "account/summary")
    if status != 200 or account["id"] != 52211463 or account["currency"] != "EUR":
        raise ValueError("demo binding mismatch")
    if request("GET", "positions") != (200, []) or request("GET", "orders") != (200, []):
        raise ValueError("probes require flat account")
    status, catalog = request("GET", "metadata/instruments")
    if status != 200:
        raise ValueError("catalog unavailable")
    cases = [("0.000001", "1"), ("0.01", "1"), ("1", "1.001"), ("1", "1.0001")]
    if args.increments_only:
        cases = [("1.1", "1"), ("1.0001", "1"), ("1.00001", "1")]
    if args.precision_boundary_only:
        cases = [("1.000001", "1"), ("1", "1.000000001")]
    results = []
    for ticker, isin in [("AIRp_EQ", "NL0000235190"), ("ASMLa_EQ", "NL0010273215")]:
        rows = [r for r in catalog if r.get("ticker") == ticker and r.get("isin") == isin and r.get("currencyCode") == "EUR"]
        if len(rows) != 1:
            raise ValueError("exact EUR identity missing")
        for quantity, price in cases:
            assert Decimal(quantity) <= 2 and Decimal(price) < 2
            body = '{"ticker":' + json.dumps(ticker) + ',"quantity":' + quantity + ',"limitPrice":' + price + ',"timeValidity":"DAY"}'
            status, response = request("POST", "orders/limit", body)
            result = {"ticker": ticker, "isin": isin, "quantity": quantity, "limit_price": price,
                      "http_status": status, "response": response, "terminally_cancelled": False}
            results.append(result)
            if 200 <= status < 300:
                order_id = response.get("id")
                if not isinstance(order_id, int) or order_id <= 0:
                    raise ValueError("accepted order has no exact ID; recovery required")
                time.sleep(1.3)
                cancel_status, _ = request("DELETE", f"orders/{order_id}")
                if not 200 <= cancel_status < 300:
                    raise ValueError("cancellation unresolved; inspect exact order")
                time.sleep(3.1)
                hist_status, history = request("GET", f"history/orders?limit=50&ticker={ticker}")
                exact = [r["order"] for r in history.get("items", []) if r.get("order", {}).get("id") == order_id] if hist_status == 200 else []
                if not exact or any(r["status"] != "CANCELLED" or Decimal(str(r.get("filledQuantity", 0))) != 0 for r in exact):
                    raise ValueError("exact terminal unfilled history not proven")
                result["terminally_cancelled"] = True
            elif status >= 500 or status in [408, 429]:
                raise ValueError("transient or ambiguous probe outcome; no retry")
            time.sleep(2.1)
    time.sleep(5.1)
    flat = request("GET", "positions") == (200, []) and request("GET", "orders") == (200, [])
    report = {"schema_version": 1, "kind": "TRADING212_EUR_ORDER_VALIDATION_PROBES", "environment": "demo",
              "broker_account_id": 52211463, "observed_at": now(), "results": results, "reconciled_flat": flat,
              "journal_sha256": hashlib.sha256((args.out / "journal.jsonl").read_bytes()).hexdigest(),
              "universal_tick_size_proven": False, "fill_capability_proven": False, "entry_authorized": False}
    with (args.out / "report.json").open("x") as f:
        json.dump(report, f, indent=2)
        f.write("\n")
    print(json.dumps(report))
    return 0 if flat else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, KeyError, OSError):
        raise SystemExit("EUR probe stopped; inspect the durable journal for exact-ID recovery; credentials omitted.") from None

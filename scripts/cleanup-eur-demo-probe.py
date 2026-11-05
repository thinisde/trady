#!/usr/bin/env python3
"""Exact-ID cleanup for an authorized one-holding EUR DEMO probe.

Only IDs acknowledged in its private artifacts may be cancelled. History must
prove them terminal before any flatten; no mutation is retried after dispatch.
"""
import argparse
import base64
from datetime import datetime, timezone
from decimal import Decimal
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


JOURNALS = ["transport.jsonl", "faults.jsonl", "manual-safety-stop.jsonl", "cleanup.jsonl"]


def unresolved_mutation(rows):
    pending = None
    for row in rows:
        key = (row.get("method"), row.get("path"))
        if row.get("phase") == "BEFORE_SEND" and row.get("method") != "GET":
            if pending is not None:
                return pending
            pending = key
        elif row.get("phase") == "RESPONSE" and key == pending:
            status, response = row.get("http_status"), row.get("response")
            acknowledged = row.get("accepted") is True or (
                isinstance(response, dict) and bool(response.get("broker_order_id")))
            definitive = isinstance(status, int) and (200 <= status < 300 or status in [400, 401, 403, 404, 405, 415, 422])
            if definitive or acknowledged:
                pending = None
    return pending


def acknowledged_ids(name, rows):
    result = set()
    for row in rows:
        response = row.get("response")
        order = (name == "transport.jsonl" and isinstance(response, dict)
                 and "broker_order_id" in response and "request" in row) or (
            row.get("phase") == "RESPONSE" and row.get("method") == "POST"
            and row.get("path", "").startswith("orders/")
            and 200 <= row.get("http_status", 0) < 300)
        if order and isinstance(response, dict):
            identifier = response.get("id", response.get("broker_order_id"))
            if identifier is not None:
                result.add(str(identifier))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    journals = {name: [json.loads(line) for line in (args.directory / name).read_text().splitlines()]
                for name in JOURNALS if (args.directory / name).exists()}
    if any(unresolved_mutation(rows) is not None for rows in journals.values()):
        raise ValueError("PRIOR_MUTATION_OUTCOME_REQUIRES_EXACT_ID_REVIEW")
    context = json.loads((args.directory / "authority.json").read_text())["context"]
    if context["broker_account_id"] != 52211463 or context["runtime_mode"] != "DEMO" or context["broker_ticker"] not in ["AIRp_EQ", "ASMLa_EQ"]:
        raise ValueError("INVALID_DEMO_PROBE_SCOPE")
    known = set()
    for name, rows in journals.items():
        known.update(acknowledged_ids(name, rows))
    if not known:
        raise ValueError("NO_ACKNOWLEDGED_EXACT_PROBE_IDS")
    secrets = json.loads((ROOT / "config/credentials.local.json").read_text())["credentials"]
    auth = "Basic " + base64.b64encode((secrets["TRADY_T212_DEMO_API_KEY"] + ":" + secrets["TRADY_T212_DEMO_API_SECRET"]).encode()).decode()
    opener = build_opener(NoRedirect())

    def journal(value):
        with os.fdopen(os.open(args.directory / "cleanup.jsonl", os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600), "a") as f:
            f.write(json.dumps({"at": datetime.now(timezone.utc).isoformat(), **value}) + "\n")
            f.flush()
            os.fsync(f.fileno())

    def request(method, path, body=None):
        for _ in range(10):
            journal({"phase": "BEFORE_SEND", "method": method, "path": path, "body": body})
            req = Request(BASE + path, data=body.encode() if body else None, method=method,
                          headers={"Authorization": auth, "Content-Type": "application/json"})
            try:
                with opener.open(req, timeout=10) as res:
                    status, data = res.status, res.read(1024 * 1024)
            except HTTPError as error:
                status, data = error.code, error.read(1024 * 1024)
            except OSError:
                journal({"phase": "UNKNOWN", "method": method, "path": path})
                raise ValueError("UNKNOWN_OUTCOME_NO_MUTATION_RETRY") from None
            value = json.loads(data) if data else None
            journal({"phase": "RESPONSE", "method": method, "path": path, "http_status": status, "response": value})
            if method == "GET" and status == 429:
                time.sleep(5.1)
                continue
            if not 200 <= status < 300:
                raise ValueError("BROKER_READ_OR_MUTATION_UNRESOLVED")
            return value
        raise ValueError("READ_RETRY_BOUND")

    account = request("GET", "account/summary")
    if account["id"] != 52211463 or account["currency"] != "EUR":
        raise ValueError("DEMO_IDENTITY_CHANGED")
    pending = request("GET", "orders")
    for order in pending:
        if str(order["id"]) not in known or order.get("ticker", order.get("instrument", {}).get("ticker")) != context["broker_ticker"]:
            raise ValueError("UNRELATED_ORDER_REQUIRES_REVIEW")
        request("DELETE", "orders/" + str(order["id"]))
        time.sleep(1.3)
    for _ in range(10):
        history = request("GET", "history/orders?limit=50&ticker=" + context["broker_ticker"])
        exact = {str(r["order"]["id"]): r["order"] for r in history["items"]}
        if all(identifier in exact and exact[identifier]["status"] in ["FILLED", "CANCELLED", "REJECTED"] for identifier in known):
            break
        time.sleep(3.1)
    else:
        raise ValueError("EXACT_TERMINAL_HISTORY_MISSING")
    if request("GET", "orders"):
        raise ValueError("PENDING_ORDERS_STILL_PRESENT")
    positions = request("GET", "positions")
    if positions:
        if len(positions) != 1 or positions[0]["instrument"]["ticker"] != context["broker_ticker"]:
            raise ValueError("UNRELATED_HOLDING")
        quantity = Decimal(str(positions[0]["quantity"]))
        available = Decimal(str(positions[0]["quantityAvailableForTrading"]))
        if not 0 < quantity <= Decimal(context["quantity"]) or available != quantity:
            raise ValueError("UNAVAILABLE_OR_CONFLICTING_SELL_QUANTITY")
        body = '{"ticker":' + json.dumps(context["broker_ticker"]) + ',"quantity":-' + str(quantity) + ',"extendedHours":false}'
        result = request("POST", "orders/market", body)
        known.add(str(result["id"]))
    time.sleep(5.1)
    history = request("GET", "history/orders?limit=50&ticker=" + context["broker_ticker"])
    exact = {str(r["order"]["id"]): r["order"] for r in history["items"]}
    if not all(identifier in exact and exact[identifier]["status"] in ["FILLED", "CANCELLED", "REJECTED"] for identifier in known):
        raise ValueError("FINAL_TERMINAL_HISTORY_MISSING")
    if request("GET", "positions") or request("GET", "orders"):
        raise ValueError("FINAL_BROKER_NOT_FLAT")
    account = request("GET", "account/summary")
    result = {"status": "EXACT_ID_DEMO_PROBE_CLEANUP_FLAT", "broker_account_id": 52211463,
              "known_order_ids": sorted(known), "cash_eur": account["cash"]["availableToTrade"],
              "runtime_acceptance_passed": False, "live_enabled": False}
    journal(result)
    print(json.dumps(result))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, TypeError):
        raise SystemExit("Exact-ID cleanup stopped; inspect its durable journal before any further mutation.") from None

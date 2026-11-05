#!/usr/bin/env python3
"""Authorized bounded DEMO fractional-position experiment, never a release.

One exact EUR holding, maximum 1+step shares and EUR 2,500 current notional.
Every mutation is written before send and never retried. A single one-share
top-up is permitted solely if a full residual market liquidation is rejected.
Partial position reduction is not called a PARTIALLY_FILLED broker order.
"""
import argparse
import base64
from datetime import datetime, timezone
from decimal import Decimal, ROUND_CEILING
import json
import os
import subprocess
import sys
from pathlib import Path
import time
from urllib.error import HTTPError
from urllib.request import Request, build_opener, HTTPRedirectHandler

ROOT = Path(__file__).resolve().parents[1]
IDENTITIES = {"AIRp_EQ": ("AIR_PA_EQ", "NL0000235190", "0.0001", 29612256),
              "ASMLa_EQ": ("ASML_AS_EQ", "NL0010273215", "0.00001", 117589399)}


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args):
        return None


def now():
    return datetime.now(timezone.utc).isoformat()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ticker", choices=list(IDENTITIES), required=True)
    parser.add_argument("--stream", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--authorized-demo-evidence", action="store_true", required=True)
    parser.add_argument("--discovery-only", action="store_true")
    parser.add_argument("--small-residual", action="store_true")
    parser.add_argument("--minimum-value-stop", action="store_true")
    args = parser.parse_args()
    session_time = datetime.now(timezone.utc)
    if session_time.date().isoformat() != "2026-11-05" or not 8 <= session_time.hour < 16:
        raise ValueError("OUTSIDE_AUTHORIZED_VERIFIED_PRIMARY_SESSION")
    os.umask(0o077)
    args.out.mkdir(mode=0o700)
    internal, isin, quantum, con_id = IDENTITIES[args.ticker]
    step, maximum = Decimal(quantum), Decimal(1) + Decimal(quantum)
    residual_target = Decimal("0.01") if args.ticker == "AIRp_EQ" else Decimal("0.001")
    reduction_quantity = Decimal(1)
    if args.small_residual:
        maximum = residual_target * 2
        reduction_quantity = residual_target
    else:
        residual_target = step
    credential_path = ROOT / "config/credentials.local.json"
    if credential_path.stat().st_mode & 0o077:
        raise ValueError("PRIVATE_CREDENTIAL_STORE_REQUIRED")
    secrets = json.loads(credential_path.read_text())["credentials"]
    key, secret = secrets["TRADY_T212_DEMO_API_KEY"], secrets["TRADY_T212_DEMO_API_SECRET"]
    auth = "Basic " + base64.b64encode((key + ":" + secret).encode()).decode()
    opener, next_read, mutations, known, partials = build_opener(NoRedirect()), {}, 0, [], []

    def journal(value):
        with os.fdopen(os.open(args.out / "transport.jsonl", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600), "a") as f:
            f.write(json.dumps({"at": now(), **value}) + "\n")
            f.flush()
            os.fsync(f.fileno())

    def request(method, path, body=None):
        nonlocal mutations
        if method != "GET":
            mutations += 1
            if mutations > 10:
                raise ValueError("MUTATION_BOUND")
            mutation_bucket = method + ":" + ("cancel" if method == "DELETE" else path)
            time.sleep(max(0, next_read.get(mutation_bucket, 0) - time.monotonic()))
            next_read[mutation_bucket] = time.monotonic() + (2.2 if method == "POST" else 1.4)
        bucket = "history" if path.startswith("history/") else path if path in ["positions", "orders", "account/summary"] else "order"
        interval = {"history": 4.1, "positions": 2.1, "orders": 6.1, "account/summary": 6.1, "order": 2.1}[bucket]
        for _ in range(12 if method == "GET" else 1):
            if method == "GET":
                time.sleep(max(0, next_read.get(bucket, 0) - time.monotonic()))
                next_read[bucket] = time.monotonic() + interval
            journal({"phase": "BEFORE_SEND", "method": method, "path": path, "body": body})
            req = Request("https://demo.trading212.com/api/v0/equity/" + path,
                          data=body.encode() if body is not None else None, method=method,
                          headers={"Authorization": auth, "Content-Type": "application/json"})
            try:
                with opener.open(req, timeout=10) as result:
                    status, raw = result.status, result.read(4 * 1024 * 1024)
            except HTTPError as error:
                status, raw = error.code, error.read(4 * 1024 * 1024)
            except OSError:
                journal({"phase": "UNKNOWN", "method": method, "path": path})
                raise ValueError("UNKNOWN_OUTCOME_NO_MUTATION_RETRY") from None
            text = raw.decode()
            for sensitive in [auth, key, secret]:
                text = text.replace(sensitive, "[REDACTED]")
            response = json.loads(text) if text else None
            journal({"phase": "RESPONSE", "method": method, "path": path, "http_status": status, "response": response})
            if method == "GET" and status == 429:
                time.sleep(5.1)
                continue
            if status >= 500 or status in [408, 429]:
                raise ValueError("AMBIGUOUS_DISPATCH_NO_RETRY")
            return status, response
        raise ValueError("READ_BOUND")

    def get(path):
        status, value = request("GET", path)
        if status != 200:
            raise ValueError("BROKER_READ_UNRESOLVED")
        return value

    def position():
        positions = get("positions")
        if not positions:
            return Decimal(0), Decimal(0)
        if len(positions) != 1 or positions[0]["instrument"]["ticker"] != args.ticker or positions[0]["instrument"]["isin"] != isin:
            raise ValueError("UNRELATED_HOLDING")
        quantity = Decimal(str(positions[0]["quantity"]))
        available = Decimal(str(positions[0]["quantityAvailableForTrading"]))
        if not 0 < quantity <= maximum or not 0 <= available <= quantity:
            raise ValueError("HOLDING_BOUND_OR_AVAILABLE_CONFLICT")
        return quantity, available

    def submit(endpoint, quantity, price=None):
        if quantity > 0:
            current = json.loads((args.stream / "latest.json").read_text())["instruments"][internal]
            if current["market_data_type"] != 1 or current["isin"] != isin or not 0 <= time.time() - current["observed_at_seconds"] < 5:
                raise ValueError("FRESH_LIVE_BOOK_REQUIRED_BEFORE_EACH_BUY")
        body = '{"ticker":' + json.dumps(args.ticker) + ',"quantity":' + str(quantity)
        body += ',"extendedHours":false' if endpoint == "market" else ',"timeValidity":"DAY","' + ("stopPrice" if endpoint == "stop" else "limitPrice") + '":' + str(price)
        status, response = request("POST", "orders/" + endpoint, body + "}")
        if 200 <= status < 300:
            identifier = response.get("id")
            if not isinstance(identifier, int) or identifier <= 0:
                raise ValueError("UNKNOWN_ACKNOWLEDGED_ID")
            known.append(str(identifier))
            if response.get("instrument", {}).get("isin") != isin or response.get("ticker") != args.ticker:
                raise ValueError("ACK_IDENTITY_CONFLICT")
            return str(identifier)
        if status != 400:
            raise ValueError("NONVALIDATION_REJECTION")
        return None

    def terminal(identifier):
        started = time.monotonic()
        while time.monotonic() - started < 35:
            history = get("history/orders?limit=50&ticker=" + args.ticker)
            rows = [r for r in history["items"] if str(r["order"]["id"]) == identifier]
            if rows:
                for row in rows:
                    if row["order"]["status"] == "PARTIALLY_FILLED":
                        partials.append(row)
                if all(r["order"]["status"] in ["FILLED", "CANCELLED", "REJECTED"] for r in rows):
                    return rows
            status, detail = request("GET", "orders/" + identifier)
            if status == 200 and detail["status"] == "PARTIALLY_FILLED":
                partials.append(detail)
                cancel_status, _ = request("DELETE", "orders/" + identifier)
                if not 200 <= cancel_status < 300 and cancel_status != 404:
                    raise ValueError("PARTIAL_CANCEL_UNRESOLVED")
            elif status not in [200, 404]:
                raise ValueError("EXACT_ORDER_UNRESOLVED")
        raise ValueError("TERMINAL_HISTORY_TIMEOUT")

    def cancel(identifier):
        status, _ = request("DELETE", "orders/" + identifier)
        if not 200 <= status < 300 and status != 404:
            raise ValueError("EXACT_CANCEL_UNRESOLVED")
        terminal(identifier)

    account = get("account/summary")
    if account["id"] != 52211463 or account["currency"] != "EUR" or position()[0] != 0 or get("orders"):
        raise ValueError("DEMO_SCOPE_NOT_FLAT")
    book = json.loads((args.stream / "latest.json").read_text())["instruments"][internal]
    received = datetime.fromisoformat(book["received_at"].replace("Z", "+00:00"))
    age = time.time() - book["observed_at_seconds"]
    if book["market_data_type"] != 1 or book["isin"] != isin or book["con_id"] != con_id or not 0 <= age < 5 or (datetime.now(timezone.utc) - received).total_seconds() > 5:
        raise ValueError("FRESH_EXACT_LIVE_SMART_REQUIRED")
    bid, ask = Decimal(book["bid"]), Decimal(book["ask"])
    if not 0 < bid <= ask or (ask - bid) / bid * 10000 > 20 or min(Decimal(book["bid_size"]), Decimal(book["ask_size"])) <= 0:
        raise ValueError("BOOK_BOUND")
    entry = (ask * Decimal("1.0005")).quantize(Decimal("0.01"), rounding=ROUND_CEILING)
    stop = (entry * Decimal("0.99")).quantize(Decimal("0.01"), rounding=ROUND_CEILING)
    if args.minimum_value_stop:
        maximum = (Decimal(1) / entry).quantize(step, rounding=ROUND_CEILING)
        stop = (entry * Decimal("0.98")).quantize(Decimal("0.01"), rounding=ROUND_CEILING)
    if entry * maximum > 2500 or Decimal(str(account["cash"]["availableToTrade"])) < entry * maximum:
        raise ValueError("NOTIONAL_OR_CASH_BOUND")
    authority = {"classification": "FRACTIONAL_POSITION_CAPABILITY_NOT_PARTIAL_ORDER_OR_RELEASE", "context": {
        "runtime_mode": "DEMO", "broker_account_id": 52211463, "broker_ticker": args.ticker,
        "isin": isin, "quantity": str(maximum), "created_at": now()}, "initial_broker_snapshot": account,
        "initial_ibkr_book": book, "max_holding_count": 1, "max_entry_notional_eur": 2500,
        "maximum_holding_quantity": str(maximum), "conditional_top_up_quantity": "1", "top_up_only_after_full_residual_market_rejection": True}
    (args.out / "authority.json").write_text(json.dumps(authority, indent=2))
    if args.discovery_only:
        price = (bid * Decimal("0.98")).quantize(Decimal("0.01"))
        minimum = (Decimal(1) / price).quantize(step, rounding=ROUND_CEILING)
        cases = [(minimum - step, price), (minimum, price), (minimum + step / 10, price),
                 (minimum, price + Decimal("0.000000001")), (minimum, price + Decimal("0.0000000001"))]
        results = []
        for quantity, limit in cases:
            identifier = submit("limit", quantity, limit)
            result = {"quantity": str(quantity), "limit_price": str(limit), "notional_eur": str(quantity * limit),
                      "accepted": identifier is not None, "broker_order_id": identifier}
            if identifier:
                cancel(identifier)
                acquired, available = position()
                if acquired:
                    if available != acquired:
                        raise ValueError("UNEXPECTED_FILL_NOT_AVAILABLE")
                    exit_id = submit("market", -acquired)
                    if exit_id is None:
                        raise ValueError("UNEXPECTED_FILL_FULL_EXIT_REJECTED")
                    terminal(exit_id)
                if position()[0] != 0:
                    raise ValueError("DISCOVERY_NOT_FLAT")
                result["exact_terminal_rows"] = terminal(identifier)
            results.append(result)
        if get("orders") or position()[0] != 0:
            raise ValueError("FINAL_DISCOVERY_NOT_FLAT")
        report = {"classification": "ACTUAL_PRICE_BAND_REQUEST_RULE_OBSERVATIONS_NOT_RELEASE", "ticker": args.ticker,
                  "isin": isin, "results": results, "reconciled_flat": True, "passed": False, "live_enabled": False}
        (args.out / "result.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report))
        return
    entry_id = submit("limit", maximum, entry)
    if entry_id is None:
        raise ValueError("ENTRY_REJECTED")
    terminal(entry_id)
    acquired, available = position()
    if acquired != maximum or available != acquired:
        raise ValueError("ENTRY_NOT_FULL_OR_AVAILABLE; exact-ID cleanup required")
    full_stop = submit("stop", -acquired, stop)
    full_stop_confirmed = False
    if full_stop:
        detail = get("orders/" + full_stop)
        full_stop_confirmed = (detail["type"] == "STOP" and detail["status"] in ["NEW", "CONFIRMED"]
            and detail["timeInForce"] == "DAY" and Decimal(str(detail["filledQuantity"])) == 0
            and Decimal(str(detail["stopPrice"])) == stop and Decimal(str(detail["quantity"])).copy_abs() == acquired
            and position() == (acquired, Decimal(0)))
        if not full_stop_confirmed:
            raise ValueError("FULL_FRACTIONAL_STOP_CONFLICT")
        cancel(full_stop)
    if args.minimum_value_stop:
        quantity, available = position()
        if quantity != maximum or available != quantity:
            raise ValueError("MINIMUM_VALUE_POSITION_NOT_SELLABLE")
        exit_id = submit("market", -quantity)
        if exit_id is None:
            raise ValueError("MINIMUM_VALUE_FULL_LIQUIDATION_REJECTED")
        terminal(exit_id)
        if position()[0] != 0 or get("orders"):
            raise ValueError("MINIMUM_VALUE_PROBE_NOT_FLAT")
        report = {"classification": "REAL_MINIMUM_VALUE_PROTECTION_AND_LIQUIDATION_NOT_RELEASE",
                  "ticker": args.ticker, "isin": isin, "entry_quantity": str(maximum), "entry_limit": str(entry),
                  "stop_price": str(stop), "stop_order_value_eur": str(maximum * stop),
                  "native_stop_rejected": full_stop is None, "native_stop_confirmed": full_stop_confirmed,
                  "full_market_exit_confirmed": True, "real_partial_order_observations": partials,
                  "exact_order_ids": known, "reconciled_flat": True, "passed": False, "live_enabled": False}
        (args.out / "result.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report))
        return
    if position() != (maximum, maximum):
        raise ValueError("FULL_POSITION_NOT_SELLABLE")
    reduction = submit("market", -reduction_quantity)
    if reduction is None:
        raise ValueError("WHOLE_SHARE_REDUCTION_REJECTED")
    terminal(reduction)
    residual, available = position()
    if residual != residual_target or available != residual:
        raise ValueError("EXACT_RESIDUAL_CONFLICT")
    residual_stop = submit("stop", -residual, stop)
    if residual_stop:
        detail = get("orders/" + residual_stop)
        if detail["type"] != "STOP" or detail["status"] not in ["NEW", "CONFIRMED"] or detail["timeInForce"] != "DAY" or Decimal(str(detail["filledQuantity"])) != 0 or Decimal(str(detail["stopPrice"])) != stop or Decimal(str(detail["quantity"])).copy_abs() != residual or position() != (residual, Decimal(0)):
            raise ValueError("RESIDUAL_STOP_CONFLICT")
        cancel(residual_stop)
    if position() != (residual_target, residual_target):
        raise ValueError("RESIDUAL_NOT_SELLABLE")
    liquidation = submit("market", -residual_target)
    top_up = None
    if liquidation is None:
        if args.small_residual:
            raise ValueError("SMALL_RESIDUAL_FULL_EXIT_REJECTED; no increased exposure allowed")
        top_up = submit("limit", Decimal(1), entry)
        if top_up is None:
            raise ValueError("BOUNDED_REPAIR_TOP_UP_REJECTED")
        terminal(top_up)
        quantity, available = position()
        if quantity != maximum or available != quantity:
            raise ValueError("REPAIR_POSITION_CONFLICT")
        liquidation = submit("market", -quantity)
        if liquidation is None:
            raise ValueError("REPAIR_LIQUIDATION_REJECTED")
    terminal(liquidation)
    if position() != (Decimal(0), Decimal(0)) or get("orders"):
        raise ValueError("FINAL_NOT_FLAT")
    final = get("account/summary")
    result = {"classification": authority["classification"], "ticker": args.ticker, "isin": isin,
              "full_fractional_stop_confirmed": full_stop_confirmed, "residual_quantity": str(residual_target),
              "residual_stop_accepted": residual_stop is not None, "residual_market_exit_accepted": top_up is None,
              "conditional_top_up_used": top_up is not None, "real_partial_order_observations": partials,
              "exact_order_ids": known, "reconciled_flat": True, "ending_cash_eur": final["cash"]["availableToTrade"],
              "passed": False, "paper_day_passed": False, "release_activation_authorized": False, "live_enabled": False}
    (args.out / "result.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError) as error:
        if "--out" in sys.argv:
            directory = Path(sys.argv[sys.argv.index("--out") + 1])
            if (directory / "authority.json").exists():
                cleanup = subprocess.run([sys.executable, str(ROOT / "scripts/cleanup-eur-demo-probe.py"), "--directory", str(directory)],
                                         capture_output=True, text=True)
                (directory / "supervised-cleanup.log").write_text(cleanup.stdout + cleanup.stderr)
        raise SystemExit("Fractional DEMO probe stopped: " + str(error)[:160] + "; inspect its exact-ID durable journal and cleanup result.") from None

#!/usr/bin/env python3
"""Reconcile retained DEMO fills against preceding provider-timed SMART books.

Read-only, offline arithmetic. Observations do not grant independent acceptance.
Missing wallet fields or quotes stay missing; duplicate conflicting fills fail.
"""
import argparse
from bisect import bisect_right
from datetime import datetime, timezone
from decimal import Decimal
import hashlib
import json
import os
from pathlib import Path

IDENTITIES = {"AIRp_EQ": ("NL0000235190", 29612256, "SBF", 2000, 1000),
              "ASMLa_EQ": ("NL0010273215", 117589399, "AEB", 2001, 1001)}


def instant(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.utcoffset() is None:
        raise ValueError("TIMEZONE_REQUIRED")
    return parsed


def decimal(value):
    result = Decimal(str(value))
    if not result.is_finite():
        raise ValueError("NONFINITE_EVIDENCE")
    return result


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as f:
        compact = path.name == "quote-observations.json"
        json.dump(value, f, indent=None if compact else 2, separators=(",", ":") if compact else None,
                  sort_keys=True, allow_nan=False)
        f.write("\n")
        f.flush()
        os.fsync(f.fileno())


def history_rows(value):
    if isinstance(value, dict):
        if isinstance(value.get("items"), list):
            yield from (row for row in value["items"] if isinstance(row, dict) and "order" in row)
        for child in value.values():
            yield from history_rows(child)
    elif isinstance(value, list):
        for child in value:
            yield from history_rows(child)


def unique_fills(rows, day):
    fills = {}
    for row in rows:
        fill, order = row.get("fill"), row["order"]
        if not fill or not fill.get("id") or not fill.get("filledAt"):
            continue
        if instant(fill["filledAt"]).date().isoformat() != day:
            continue
        ticker = order.get("ticker", order.get("instrument", {}).get("ticker"))
        if ticker not in IDENTITIES or order.get("instrument", {}).get("isin") != IDENTITIES[ticker][0]:
            raise ValueError("FILL_IDENTITY_CONFLICT")
        key = str(fill["id"])
        fingerprint = (str(order["id"]), ticker, order["side"], json.dumps(fill, sort_keys=True))
        if key in fills and fills[key][0] != fingerprint:
            raise ValueError("DUPLICATE_FILL_CONFLICT")
        fills[key] = (fingerprint, row)
    return sorted((v[1] for v in fills.values()), key=lambda r: (instant(r["fill"]["filledAt"]), str(r["fill"]["id"])))


def wallet_cost(row):
    fill, order = row["fill"], row["order"]
    wallet = fill.get("walletImpact")
    if not wallet or not all(k in wallet for k in ["currency", "fxRate", "netValue", "taxes"]):
        return {"reconciled": False, "reason": "WALLET_FIELDS_MISSING"}
    if wallet["currency"] != "EUR" or decimal(wallet["fxRate"]) != 1 or not isinstance(wallet["taxes"], list):
        return {"reconciled": False, "reason": "WALLET_CURRENCY_FX_OR_TAXES_UNRESOLVED"}
    quantity, price, net = abs(decimal(fill["quantity"])), decimal(fill["price"]), decimal(wallet["netValue"])
    if quantity <= 0 or price <= 0 or net < 0 or order["side"] not in ["BUY", "SELL"]:
        raise ValueError("INVALID_FILL_ECONOMICS")
    gross = quantity * price
    difference = net - gross if order["side"] == "BUY" else gross - net
    return {"reconciled": True, "gross_eur": str(gross), "net_value_eur": str(net),
            "signed_cash_eur": str(-net if order["side"] == "BUY" else net),
            "observed_wallet_charge_eur": str(difference),
            "gross_to_wallet_difference_eur": str(difference),
            "difference_within_half_cent": abs(difference) <= Decimal("0.005"),
            "difference_interpretation": "NET_WALLET_DIFFERENCE_INCLUDING_CURRENCY_ROUNDING; NOT_A_DISCLOSED_COMMISSION_FIELD",
            "taxes": wallet["taxes"], "fx_rate": str(decimal(wallet["fxRate"]))}


def quote_match(row, captures):
    order, fill = row["order"], row["fill"]
    ticker = order.get("ticker", order["instrument"]["ticker"])
    isin, con_id, primary, book_id, quote_id = IDENTITIES[ticker]
    when = instant(fill["filledAt"])
    candidates = []
    for capture in captures:
        contract = next((c for c in capture["contracts"]["contracts"] if c["con_id"] == con_id), None)
        if not contract or contract["identifiers"].get("ISIN") != isin or contract["currency"] != "EUR" or contract["primary_exchange"] != primary or contract["exchange"] != "SMART":
            continue
        books = capture["books"].get(book_id, [])
        index = bisect_right([instant(e["received_at"]) for e in books], when) - 1
        if index < 0:
            continue
        event = books[index]
        types = [e for e in capture["market_data_types"] if e["request_id"] == quote_id and instant(e["received_at"]) <= instant(event["received_at"])]
        if not types or types[-1]["data_type"] != 1:
            continue
        quoted = datetime.fromtimestamp(event["observed_at_seconds"], timezone.utc)
        age = (when - quoted).total_seconds()
        if not 0 <= age <= 10 or quoted > instant(event["received_at"]) or decimal(event["bid_size"]) <= 0 or decimal(event["ask_size"]) <= 0:
            continue
        candidates.append((instant(event["received_at"]), capture, event, types[-1]))
    if not candidates:
        return None
    _, capture, event, live_type = max(candidates, key=lambda v: v[0])
    bid, ask, price = decimal(event["bid"]), decimal(event["ask"]), decimal(fill["price"])
    if not 0 < bid <= ask:
        raise ValueError("INVALID_QUOTE_BOOK")
    reference = ask if order["side"] == "BUY" else bid
    return {"ticker": ticker, "isin": isin, "con_id": con_id, "primary_exchange": primary,
            "source_venue": "SMART", "currency": "EUR", "side": order["side"],
            "broker_order_id": str(order["id"]), "broker_fill_id": str(fill["id"]),
            "filled_at": fill["filledAt"], "fill_price": str(price), "quantity": str(abs(decimal(fill["quantity"]))),
            "quote_at": datetime.fromtimestamp(event["observed_at_seconds"], timezone.utc).isoformat(),
            "quote_received_at": event["received_at"], "bid": str(bid), "ask": str(ask),
            "bid_size": str(decimal(event["bid_size"])), "ask_size": str(decimal(event["ask_size"])),
            "market_data_type": 1, "age_seconds": str(decimal((when - datetime.fromtimestamp(event["observed_at_seconds"], timezone.utc)).total_seconds())),
            "spread_bps": str((ask - bid) / bid * 10000),
            "absolute_divergence_bps": str(abs(price - reference) / reference * 10000),
            "signed_adverse_divergence_bps": str((price - reference if order["side"] == "BUY" else reference - price) / reference * 10000),
            "capture": capture["name"], "raw_book": event, "raw_live_type": live_type}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--day", default="2026-11-05")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--final-snapshot", type=Path)
    parser.add_argument("--matched-books-only", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(mode=0o700)
    captures = []
    for directory in sorted(args.session.glob("stream*")):
        if not directory.is_dir() or not (directory / "events.jsonl").exists():
            continue
        events = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
        captures.append({"name": directory.name, "contracts": json.loads((directory / "contracts.json").read_text()),
                         "events_sha256": sha(directory / "events.jsonl"),
                         "market_data_types": [e for e in events if e["kind"] == "market_data_type"],
                         "books": {rid: [e for e in events if e["kind"] == "bid_ask" and e["request_id"] == rid] for rid in [2000, 2001]}})
    rows, scopes = [], []
    final_path = args.final_snapshot or args.session / "final-demo-snapshot.json"
    final_snapshot = json.loads(final_path.read_text()) if final_path.exists() else None
    if final_snapshot is not None:
        rows.extend(history_rows(final_snapshot))
    for directory in sorted(args.session.iterdir()):
        authority = directory / "authority.json"
        if not directory.is_dir() or not authority.exists():
            continue
        scope = {"name": directory.name, "authority": json.loads(authority.read_text()), "artifacts": {}}
        for path in sorted(directory.iterdir()):
            if path.suffix not in [".json", ".jsonl"]:
                continue
            value = json.loads(path.read_text()) if path.suffix == ".json" else [json.loads(line) for line in path.read_text().splitlines()]
            rows.extend(history_rows(value))
            scope["artifacts"][path.name] = {"sha256": sha(path), "value": value}
        scopes.append(scope)
    fills = unique_fills(rows, args.day)
    samples = [matched for row in fills if (matched := quote_match(row, captures)) is not None]
    if args.matched_books_only:
        # Full immutable event streams stay retained separately. Keep every
        # callback actually used by the complete fill set in the verifier JSON.
        for capture in captures:
            used = {(s["raw_book"]["request_id"],s["raw_book"]["received_at"]) for s in samples if s["capture"] == capture["name"]}
            capture["books"] = {rid:[e for e in events if (e["request_id"],e["received_at"]) in used] for rid,events in capture["books"].items()}
            capture["book_scope"] = "ALL_MATCHED_FILL_CALLBACKS; FULL_EVENTS_RETAINED_SEPARATELY"
    costs = [{"broker_fill_id": str(r["fill"]["id"]), "broker_order_id": str(r["order"]["id"]), **wallet_cost(r)} for r in fills]
    initial = json.loads((args.session / "account-preflight.json").read_text())
    delta = sum((decimal(r["signed_cash_eur"]) for r in costs if r["reconciled"]), Decimal(0))
    final_account = next((r["response"] for r in final_snapshot["responses"] if r["path"] == "account/summary"), None) if final_snapshot else None
    actual_ending_cash = str(decimal(final_account["cash"]["availableToTrade"])) if final_account else None
    expected_ending_cash = str(decimal(initial["cash"]["availableToTrade"]) + delta)
    common = {"schema_version": 1, "broker_account_id": 52211463, "environment": "demo", "live_enabled": False,
              "passed": False, "reviewer": None, "reviewed_at": None, "valid_until": None,
              "observations": [], "acceptance_status": "PENDING_INDEPENDENT_REVIEW"}
    write(args.out / "broker-history.json", {"items": fills, "nextPagePath": None, "classification": "DEDUPLICATED_RAW_RETAINED_FILL_ROWS"})
    write(args.out / "broker-observations.json", {"classification": "RETAINED_REAL_DEMO_PROBE_ARTIFACTS", "initial_account": initial, "final_snapshot": final_snapshot, "scopes": scopes})
    write(args.out / "quote-observations.json", {"provider": "IBKR_SMART_LIVE", "captures": captures})
    write(args.out / "quote-compatibility.json", {**common, "source": "IBKR_SMART_LIVE", "maximum_divergence_bps": None,
          "observed_ceiling_bps": str(max((decimal(s["absolute_divergence_bps"]) for s in samples), default=Decimal(0))),
          "samples": samples, "unmatched_broker_fill_ids": [str(r["fill"]["id"]) for r in fills if str(r["fill"]["id"]) not in {s["broker_fill_id"] for s in samples}],
          "note": "Observed sample arithmetic only; no divergence limit has been independently accepted. Failed runtime probes remain excluded from protocol acceptance."})
    write(args.out / "costs.json", {**common, "currency": "EUR", "source": "REAL_TRADING212_DEMO_WALLET_IMPACT",
          "maximum_round_trip_cost_bps": None, "broker_costs_reconciled": False, "fills": costs,
          "initial_cash_eur": str(decimal(initial["cash"]["availableToTrade"])), "observed_signed_cash_delta_eur": str(delta),
          "expected_ending_cash_eur": expected_ending_cash, "observed_ending_cash_eur": actual_ending_cash,
          "cash_arithmetic_reconciled": actual_ending_cash is not None and decimal(actual_ending_cash) == decimal(expected_ending_cash),
          "all_fill_wallet_fields_present": all(r["reconciled"] for r in costs),
          "note": "Net wallet differences include currency rounding; fractional fills can differ from unrounded gross by fractions of a cent. No separate commission field was supplied. Spread and quote divergence are separate; missing fees are not zero and observed zero differences are not a future guarantee."})
    print(json.dumps({"fills": len(fills), "matched_quotes": len(samples), "cash_delta_eur": str(delta),
                      "observed_maximum_divergence_bps": str(max((decimal(s["absolute_divergence_bps"]) for s in samples), default=Decimal(0))), "independently_accepted": False}))


if __name__ == "__main__":
    main()

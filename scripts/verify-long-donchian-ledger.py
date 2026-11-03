#!/usr/bin/env python3
"""Verify the append-only Donchian trade and portfolio ledgers.

The replay writer uses a hash chain, but a hash chain is only useful when a
later process recomputes it.  This verifier checks the chain, record identity,
trade arithmetic, cost application, and chronological portfolio equity without
trusting the replay report's summary fields.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
from decimal import Decimal, InvalidOperation, getcontext
from typing import Any

getcontext().prec = 28

FORMULA_ID = "long_donchian_atr_v1"
ROUND_TRIP_COST = Decimal("0.0020")
RISK_FRACTION = Decimal("0.02")
ZERO = Decimal("0")


def canonical(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=False, separators=(",", ":")).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def dec(value: Any) -> Decimal:
    return Decimal(str(value))


def close(left: Any, right: Any, tolerance: Decimal = Decimal("1e-24")) -> bool:
    try:
        return abs(dec(left) - dec(right)) <= tolerance
    except (InvalidOperation, TypeError, ValueError):
        return False


def load_chain(path: pathlib.Path) -> tuple[list[dict[str, Any]], list[str]]:
    errors: list[str] = []
    rows: list[dict[str, Any]] = []
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as exc:
        return [], [f"cannot read {path.name}: {exc}"]
    previous = None
    for expected_sequence, line in enumerate(lines, start=1):
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            errors.append(f"{path.name}: invalid JSON at sequence {expected_sequence}")
            continue
        if not isinstance(row, dict):
            errors.append(f"{path.name}: record {expected_sequence} is not an object")
            continue
        if row.get("sequence") != expected_sequence:
            errors.append(f"{path.name}: sequence discontinuity at {expected_sequence}")
        if row.get("previous_record_sha256") != previous:
            errors.append(f"{path.name}: previous hash mismatch at {expected_sequence}")
        claimed = row.get("record_sha256")
        unsigned = dict(row)
        unsigned.pop("record_sha256", None)
        if not isinstance(claimed, str) or claimed != digest(unsigned):
            errors.append(f"{path.name}: record hash mismatch at {expected_sequence}")
        previous = claimed
        rows.append(row)
    if not rows:
        errors.append(f"{path.name}: ledger is empty")
    return rows, errors


def verify(
    trade_path: pathlib.Path,
    portfolio_path: pathlib.Path,
    expected_authority: str | None = None,
    expected_code: str | None = None,
    expected_dataset: str | None = None,
) -> dict[str, Any]:
    errors: list[str] = []
    trades, trade_errors = load_chain(trade_path)
    portfolio, portfolio_errors = load_chain(portfolio_path)
    errors.extend(trade_errors)
    errors.extend(portfolio_errors)

    authority_hashes = set()
    code_hashes = set()
    dataset_hashes = set()
    positions: list[dict[str, Any]] = []
    for row in trades:
        if row.get("authority_sha256"):
            authority_hashes.add(row["authority_sha256"])
        if row.get("code_sha256"):
            code_hashes.add(row["code_sha256"])
        if row.get("dataset_hash"):
            dataset_hashes.add(row["dataset_hash"])
        if row.get("event_type") != "position":
            if any(row.get(field) is not None for field in ("gross_return", "net_return", "cost_amount")):
                errors.append(f"signal record {row.get('sequence')} contains trade economics")
            continue
        decision = row.get("signal_decision")
        position_status = row.get("position_status")
        if decision == "ACCEPTED" and position_status not in {"CLOSED", "UNRESOLVED_MISSING_EXIT_BAR"}:
            errors.append(f"accepted position {row.get('sequence')} has invalid lifecycle status")
        if position_status == "CLOSED":
            required = ("entry", "stop", "target", "quantity", "risk_budget", "notional", "gross_return", "cost_amount", "net_return", "exit_at", "exit_reason")
            for field in required:
                if row.get(field) is None:
                    errors.append(f"closed position {row.get('sequence')} is missing {field}")
            if all(row.get(field) is not None for field in required):
                try:
                    entry = dec(row["entry"])
                    stop = dec(row["stop"])
                    target = dec(row["target"])
                    quantity = dec(row["quantity"])
                    risk_budget = dec(row["risk_budget"])
                    notional = dec(row["notional"])
                    gross = dec(row["gross_return"])
                    cost_amount = dec(row["cost_amount"])
                    net = dec(row["net_return"])
                    if not (stop < entry < target and quantity > ZERO and risk_budget > ZERO and notional > ZERO):
                        errors.append(f"position {row.get('sequence')} violates price/risk ordering")
                    if not close(notional, quantity * entry):
                        errors.append(f"position {row.get('sequence')} notional does not match quantity and entry")
                    if not close(gross, dec(row["exit"]) / entry - Decimal(1)):
                        errors.append(f"position {row.get('sequence')} gross return is inconsistent")
                    if not close(cost_amount, notional * ROUND_TRIP_COST):
                        errors.append(f"position {row.get('sequence')} cost is not the single registered round trip")
                    if not close(net, gross - ROUND_TRIP_COST):
                        errors.append(f"position {row.get('sequence')} net return applies the cost incorrectly")
                except (InvalidOperation, KeyError, TypeError, ValueError):
                    errors.append(f"position {row.get('sequence')} has unreadable decimal economics")
            positions.append(row)
        elif position_status == "UNRESOLVED_MISSING_EXIT_BAR":
            for field in ("gross_return", "cost_amount", "net_return", "exit_at", "exit"):
                if row.get(field) is not None:
                    errors.append(f"unresolved position {row.get('sequence')} has a fabricated {field}")
            positions.append(row)
        elif decision not in {"REJECTED", "BLOCKED_OPEN_POSITION"}:
            errors.append(f"position {row.get('sequence')} has an unknown decision/status")

    if len(authority_hashes) > 1 or len(code_hashes) > 1 or len(dataset_hashes) > 1:
        errors.append("ledger records contain mixed authority, implementation, or dataset hashes")
    if expected_authority and authority_hashes != {expected_authority}:
        errors.append("ledger authority hash does not match the expected authority")
    if expected_code and code_hashes != {expected_code}:
        errors.append("ledger implementation hash does not match the expected implementation")
    if expected_dataset and dataset_hashes != {expected_dataset}:
        errors.append("ledger dataset hash does not match the expected dataset")

    by_symbol: dict[str, list[dict[str, Any]]] = {}
    for row in positions:
        if row.get("position_status") == "CLOSED":
            by_symbol.setdefault(str(row.get("symbol")), []).append(row)
    for symbol, symbol_positions in by_symbol.items():
        ordered = sorted(symbol_positions, key=lambda row: (row["entry_at"], row["signal_at"]))
        previous_exit = None
        previous_signal = None
        for row in ordered:
            if previous_exit is not None and row["entry_at"] < previous_exit:
                errors.append(f"symbol {symbol} has overlapping positions")
            if previous_signal is not None and row["signal_at"] <= previous_signal:
                errors.append(f"symbol {symbol} reopens without a later signal event")
            previous_exit = row["exit_at"]
            previous_signal = row["signal_at"]

    equity = None
    for row in portfolio:
        if row.get("event_type") != "portfolio_exit":
            errors.append(f"portfolio record {row.get('sequence')} is not an exit event")
            continue
        try:
            before = dec(row["equity_before"])
            after = dec(row["equity_after"])
            net = dec(row["net_return"])
            if equity is not None and not close(before, equity):
                errors.append(f"portfolio equity does not carry forward at {row.get('sequence')}")
            if not close(after, before + RISK_FRACTION * net):
                errors.append(f"portfolio equity arithmetic is inconsistent at {row.get('sequence')}")
            equity = after
        except (InvalidOperation, KeyError, TypeError, ValueError):
            errors.append(f"portfolio record {row.get('sequence')} has unreadable equity")

    return {
        "schema_version": 1,
        "formula_id": FORMULA_ID,
        "status": "LEDGER_VERIFICATION_PASS" if not errors else "LEDGER_VERIFICATION_BLOCKED",
        "trade_records": len(trades),
        "portfolio_records": len(portfolio),
        "closed_positions": sum(row.get("position_status") == "CLOSED" for row in positions),
        "unresolved_positions": sum(row.get("position_status") == "UNRESOLVED_MISSING_EXIT_BAR" for row in positions),
        "authority_hashes": sorted(authority_hashes),
        "code_hashes": sorted(code_hashes),
        "dataset_hashes": sorted(dataset_hashes),
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trade-ledger", type=pathlib.Path, required=True)
    parser.add_argument("--portfolio-ledger", type=pathlib.Path, required=True)
    parser.add_argument("--authority-hash")
    parser.add_argument("--code-hash")
    parser.add_argument("--dataset-hash")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    report = verify(args.trade_ledger, args.portfolio_ledger, args.authority_hash, args.code_hash, args.dataset_hash)
    if args.output.exists():
        raise SystemExit(f"ledger verification output already exists: {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": report["status"], "errors": report["errors"][:5]}, indent=2))
    return 0 if report["status"] == "LEDGER_VERIFICATION_PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())


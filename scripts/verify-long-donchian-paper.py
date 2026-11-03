#!/usr/bin/env python3
"""Validate the paper-execution ledger required by the Donchian release gate.

This is an evidence verifier, not a broker client.  It accepts an append-only
JSONL ledger produced by the paper adapter and fails closed when order state,
protection, costs, reconciliation, or restart recovery is incomplete.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
from decimal import Decimal, InvalidOperation
from typing import Any

FORMULA_ID = "long_donchian_atr_v1"
MAX_COST_BPS = Decimal("20")


def canonical(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=False, separators=(",", ":")).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def dec(value: Any) -> Decimal:
    return Decimal(str(value))


def read_chain(path: pathlib.Path) -> tuple[list[dict[str, Any]], list[str]]:
    errors: list[str] = []
    rows: list[dict[str, Any]] = []
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as exc:
        return [], [f"cannot read paper ledger: {exc}"]
    previous = None
    for sequence, line in enumerate(lines, start=1):
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            errors.append(f"invalid JSON at sequence {sequence}")
            continue
        if not isinstance(row, dict):
            errors.append(f"non-object record at sequence {sequence}")
            continue
        if row.get("sequence") != sequence or row.get("previous_record_sha256") != previous:
            errors.append(f"sequence or previous hash mismatch at {sequence}")
        claimed = row.get("record_sha256")
        unsigned = dict(row)
        unsigned.pop("record_sha256", None)
        if not isinstance(claimed, str) or claimed != digest(unsigned):
            errors.append(f"record hash mismatch at {sequence}")
        previous = claimed
        rows.append(row)
    if not rows:
        errors.append("paper ledger is empty")
    return rows, errors


def verify(path: pathlib.Path, expected_authority: str | None = None) -> dict[str, Any]:
    rows, errors = read_chain(path)
    authority_hashes = set()
    orders: dict[str, list[str]] = {}
    filled_orders: set[str] = set()
    protected_orders: set[str] = set()
    reconciliations = 0
    unresolved_discrepancies = 0
    restart_events = 0
    cost_bps: list[Decimal] = []
    for row in rows:
        if row.get("formula_id") not in {None, FORMULA_ID}:
            errors.append(f"unsupported formula id at sequence {row.get('sequence')}")
        authority = row.get("authority_sha256")
        if authority:
            authority_hashes.add(authority)
        event = row.get("event_type")
        order_id = row.get("client_order_id")
        if event in {"ORDER_SUBMITTED", "ORDER_ACKNOWLEDGED", "ORDER_PARTIAL_FILL", "ORDER_FILLED", "ORDER_REJECTED", "ORDER_CANCELLED", "PROTECTION_CREATED", "PROTECTION_CANCELLED", "EXIT_FILLED"}:
            if not isinstance(order_id, str) or not order_id:
                errors.append(f"order event {event} lacks client_order_id at sequence {row.get('sequence')}")
            else:
                orders.setdefault(order_id, []).append(event)
        if event == "ORDER_FILLED":
            filled_orders.add(str(order_id))
            for field in ("fill_price", "bid", "ask", "fees"):
                if field not in row:
                    errors.append(f"fill is missing {field} at sequence {row.get('sequence')}")
            if "slippage_bps" not in row:
                errors.append(f"fill is missing slippage_bps at sequence {row.get('sequence')}")
        elif event == "ORDER_ACKNOWLEDGED":
            if not row.get("broker_order_id") or not row.get("acknowledged_at"):
                errors.append(f"broker acknowledgement is incomplete at sequence {row.get('sequence')}")
        elif event == "ORDER_SUBMITTED":
            if not row.get("submitted_at") or "intended_price" not in row:
                errors.append(f"order submission evidence is incomplete at sequence {row.get('sequence')}")
        elif event == "ORDER_PARTIAL_FILL":
            if "fill_price" not in row or "filled_quantity" not in row:
                errors.append(f"partial fill evidence is incomplete at sequence {row.get('sequence')}")
        elif event == "PROTECTION_CREATED":
            protected_orders.add(str(order_id))
            if row.get("parent_client_order_id") not in filled_orders:
                errors.append(f"protection precedes its entry fill at sequence {row.get('sequence')}")
            if row.get("state") not in {"SUBMITTED", "ACKNOWLEDGED", "ACTIVE"}:
                errors.append(f"protection state is not explicit at sequence {row.get('sequence')}")
        elif event == "RECONCILIATION":
            reconciliations += 1
            unresolved = row.get("unresolved_discrepancies", 1)
            try:
                unresolved_discrepancies += int(unresolved)
            except (TypeError, ValueError):
                errors.append(f"reconciliation discrepancy count is invalid at sequence {row.get('sequence')}")
            for field in ("local_position_state", "broker_position_state", "local_cash", "broker_cash", "buying_power"):
                if field not in row:
                    errors.append(f"reconciliation is missing {field} at sequence {row.get('sequence')}")
            if row.get("positions_match") is not True or row.get("cash_match") is not True:
                errors.append(f"reconciliation does not attest matching positions and cash at sequence {row.get('sequence')}")
        elif event == "RESTART_RECOVERY":
            restart_events += 1
            if row.get("recovered") is not True or row.get("unresolved_discrepancies", 1) != 0:
                errors.append(f"restart recovery is incomplete at sequence {row.get('sequence')}")
        if "cost_bps" in row:
            try:
                value = dec(row["cost_bps"])
                cost_bps.append(value)
                if value < 0 or value > MAX_COST_BPS:
                    errors.append(f"observed cost exceeds the 20 bps budget at sequence {row.get('sequence')}")
            except (InvalidOperation, TypeError, ValueError):
                errors.append(f"cost_bps is unreadable at sequence {row.get('sequence')}")
    if len(authority_hashes) > 1:
        errors.append("paper records contain multiple authority hashes")
    if expected_authority and authority_hashes != {expected_authority}:
        errors.append("paper authority hash does not match the expected authority")
    if not filled_orders:
        errors.append("paper ledger contains no acknowledged fill")
    if not protected_orders:
        errors.append("paper ledger contains no protective-order state")
    if reconciliations == 0:
        errors.append("paper ledger contains no position/cash reconciliation")
    if unresolved_discrepancies:
        errors.append("paper ledger contains unresolved reconciliation discrepancies")
    if restart_events == 0:
        errors.append("paper ledger contains no restart/recovery evidence")
    return {
        "schema_version": 1,
        "formula_id": FORMULA_ID,
        "status": "PAPER_RECONCILED" if not errors else "PAPER_RECONCILIATION_BLOCKED",
        "records": len(rows),
        "client_orders": len(orders),
        "fills": len(filled_orders),
        "protected_orders": len(protected_orders),
        "reconciliations": reconciliations,
        "unresolved_discrepancies": unresolved_discrepancies,
        "restart_recoveries": restart_events,
        "costs_within_budget": bool(cost_bps) and all(value <= MAX_COST_BPS for value in cost_bps),
        "observed_max_cost_bps": str(max(cost_bps)) if cost_bps else None,
        "authority_hashes": sorted(authority_hashes),
        "errors": errors,
        "limitations": [
            "Paper fills validate the adapter and reconciliation path; they do not prove live queue position or market impact.",
            "A gap through a stop can exceed the ex-ante initial-risk estimate.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger", type=pathlib.Path, required=True)
    parser.add_argument("--authority-hash")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    report = verify(args.ledger, args.authority_hash)
    if args.output.exists():
        raise SystemExit(f"paper verification output already exists: {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": report["status"], "errors": report["errors"][:8]}, indent=2))
    return 0 if report["status"] == "PAPER_RECONCILED" else 1


if __name__ == "__main__":
    raise SystemExit(main())


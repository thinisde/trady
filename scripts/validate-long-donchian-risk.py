#!/usr/bin/env python3
"""Evaluate the explicit risk and execution gates for a replay ledger."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import pathlib
from decimal import Decimal, InvalidOperation
from typing import Any

ROOT = pathlib.Path(__file__).resolve().parents[1]
FORMULA_ID = "long_donchian_atr_v1"
MAX_INITIAL_RISK = Decimal("0.02")
MAX_AGGREGATE_OPEN_RISK = Decimal("0.60")
MAX_NOTIONAL_EXPOSURE = Decimal("1.00")
MAX_SIMULTANEOUS_POSITIONS = 30
MAX_DRAWDOWN = Decimal("0.20")
MAX_CONSECUTIVE_LOSSES = 10

_verifier_spec = importlib.util.spec_from_file_location(
    "long_donchian_ledger_verifier", pathlib.Path(__file__).with_name("verify-long-donchian-ledger.py")
)
if _verifier_spec is None or _verifier_spec.loader is None:
    raise RuntimeError("cannot load the Donchian ledger verifier")
_verifier = importlib.util.module_from_spec(_verifier_spec)
_verifier_spec.loader.exec_module(_verifier)


def dec(value: Any) -> Decimal:
    return Decimal(str(value))


def file_hash(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_json(path: pathlib.Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def max_consecutive_losses(positions: list[dict[str, Any]]) -> int:
    ordered = sorted((row for row in positions if row.get("position_status") == "CLOSED"), key=lambda row: row["exit_at"])
    maximum = current = 0
    for row in ordered:
        current = current + 1 if dec(row["net_return"]) < 0 else 0
        maximum = max(maximum, current)
    return maximum


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trade-ledger", type=pathlib.Path, required=True)
    parser.add_argument("--portfolio-ledger", type=pathlib.Path, required=True)
    parser.add_argument("--replay-report", type=pathlib.Path, default=None)
    parser.add_argument("--root", type=pathlib.Path, default=ROOT)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    verifier = _verifier.verify(args.trade_ledger, args.portfolio_ledger)
    trades, chain_errors = _verifier.load_chain(args.trade_ledger)
    positions = [row for row in trades if row.get("event_type") == "position"]
    closed = [row for row in positions if row.get("position_status") == "CLOSED"]
    unresolved = [row for row in positions if row.get("position_status") == "UNRESOLVED_MISSING_EXIT_BAR"]
    errors: list[str] = []
    gates: dict[str, dict[str, Any]] = {}

    if verifier["status"] != "LEDGER_VERIFICATION_PASS":
        errors.append("ledger verification failed")
    gates["ledger_integrity"] = {"passed": verifier["status"] == "LEDGER_VERIFICATION_PASS", "errors": verifier["errors"][:10]}

    initial_risks = [dec(row["risk_budget"]) for row in closed if row.get("risk_budget") is not None]
    maximum_initial_risk = max(initial_risks, default=Decimal("0"))
    initial_ok = maximum_initial_risk <= MAX_INITIAL_RISK
    gates["per_position_initial_risk"] = {
        "passed": initial_ok,
        "maximum_observed": str(maximum_initial_risk),
        "limit": str(MAX_INITIAL_RISK),
    }
    if not initial_ok:
        errors.append("per-position initial risk exceeds the registered limit")

    maximum_open_count = 0
    maximum_open_risk = Decimal("0")
    maximum_open_notional = Decimal("0")
    for row in closed:
        entry_at = row["entry_at"]
        active = [
            other
            for other in closed
            if other["entry_at"] <= entry_at < other["exit_at"]
        ]
        maximum_open_count = max(maximum_open_count, len(active))
        maximum_open_risk = max(maximum_open_risk, sum((dec(other["risk_budget"]) for other in active), Decimal("0")))
        maximum_open_notional = max(maximum_open_notional, sum((dec(other["notional"]) for other in active), Decimal("0")))
    aggregate_ok = maximum_open_risk <= MAX_AGGREGATE_OPEN_RISK
    simultaneous_ok = maximum_open_count <= MAX_SIMULTANEOUS_POSITIONS
    notional_ok = maximum_open_notional <= MAX_NOTIONAL_EXPOSURE
    gates["aggregate_open_risk"] = {"passed": aggregate_ok, "maximum_observed": str(maximum_open_risk), "limit": str(MAX_AGGREGATE_OPEN_RISK)}
    gates["simultaneous_positions"] = {"passed": simultaneous_ok, "maximum_observed": maximum_open_count, "limit": MAX_SIMULTANEOUS_POSITIONS}
    gates["maximum_notional_exposure"] = {"passed": notional_ok, "maximum_observed": str(maximum_open_notional), "limit": str(MAX_NOTIONAL_EXPOSURE)}
    if not aggregate_ok:
        errors.append("aggregate open risk exceeds the registered limit")
    if not simultaneous_ok:
        errors.append("simultaneous positions exceed the registered limit")
    if not notional_ok:
        errors.append("notional exposure exceeds the cash-only limit")

    spec = load_json(root / "config/formulas/long-donchian-atr-v1.json")
    policy_flags_ok = all(spec.get(field) is False for field in ("leverage", "averaging", "pyramiding", "trailing_stop"))
    gates["no_leverage_averaging_pyramiding"] = {"passed": policy_flags_ok, "spec_flags": {field: spec.get(field) for field in ("leverage", "averaging", "pyramiding", "trailing_stop")}}
    if not policy_flags_ok:
        errors.append("strategy specification enables leverage, averaging, pyramiding, or trailing")

    symbols: dict[str, list[dict[str, Any]]] = {}
    for row in closed:
        symbols.setdefault(str(row["symbol"]), []).append(row)
    duplicate_or_reopen = any(
        any(right["entry_at"] < left["exit_at"] for left, right in zip(ordered, ordered[1:]))
        for ordered in (sorted(rows, key=lambda row: row["entry_at"]) for rows in symbols.values())
    )
    gates["duplicate_and_reopen"] = {"passed": not duplicate_or_reopen}
    if duplicate_or_reopen:
        errors.append("duplicate or unintended reopening detected")

    missing_open = sum(row.get("decision_reason") == "MISSING_NEXT_REGULAR_SESSION_OPEN" for row in trades)
    unresolved_ok = not unresolved
    data_ok = missing_open == 0 and unresolved_ok and not chain_errors
    gates["data_and_exit_completeness"] = {"passed": data_ok, "missing_next_open_signals": missing_open, "unresolved_positions": len(unresolved), "chain_errors": chain_errors[:10]}
    if not data_ok:
        errors.append("missing or unresolved market data remains in the replay")

    replay = load_json(args.replay_report) if args.replay_report and args.replay_report.exists() else {}
    drawdown = dec(replay.get("portfolio_metrics", {}).get("maximum_drawdown", "0")) if isinstance(replay, dict) else Decimal("0")
    loss_streak = max_consecutive_losses(positions)
    drawdown_ok = drawdown <= MAX_DRAWDOWN
    loss_streak_ok = loss_streak <= MAX_CONSECUTIVE_LOSSES
    gates["drawdown_kill_switch"] = {"passed": drawdown_ok, "maximum_observed": str(drawdown), "limit": str(MAX_DRAWDOWN)}
    gates["loss_streak_kill_switch"] = {"passed": loss_streak_ok, "maximum_observed": loss_streak, "limit": MAX_CONSECUTIVE_LOSSES}
    if not drawdown_ok:
        errors.append("maximum drawdown exceeds the kill-switch threshold")
    if not loss_streak_ok:
        errors.append("consecutive loss streak exceeds the kill-switch threshold")

    result = {
        "schema_version": 1,
        "formula_id": FORMULA_ID,
        "status": "RISK_POLICY_PASS" if not errors else "RISK_POLICY_BLOCKED",
        "policy": {
            "max_initial_risk_fraction": str(MAX_INITIAL_RISK),
            "max_aggregate_open_risk_fraction": str(MAX_AGGREGATE_OPEN_RISK),
            "max_notional_exposure_fraction": str(MAX_NOTIONAL_EXPOSURE),
            "max_simultaneous_positions": MAX_SIMULTANEOUS_POSITIONS,
            "max_drawdown": str(MAX_DRAWDOWN),
            "max_consecutive_losses": MAX_CONSECUTIVE_LOSSES,
            "leverage": False,
            "averaging": False,
            "pyramiding": False,
        },
        "observed": {
            "closed_positions": len(closed),
            "unresolved_positions": len(unresolved),
            "maximum_open_positions": maximum_open_count,
            "maximum_open_risk": str(maximum_open_risk),
            "maximum_open_notional": str(maximum_open_notional),
            "maximum_drawdown": str(drawdown),
            "maximum_consecutive_losses": loss_streak,
        },
        "authority_sha256": verifier["authority_hashes"][0] if len(verifier["authority_hashes"]) == 1 else None,
        "code_sha256": verifier["code_hashes"][0] if len(verifier["code_hashes"]) == 1 else None,
        "dataset_hash": verifier["dataset_hashes"][0] if len(verifier["dataset_hashes"]) == 1 else None,
        "gates": gates,
        "errors": errors,
        "limitations": [
            "Initial risk is ex-ante; a gap through the stop can create a larger realized loss.",
            "The notional gate uses unit-equity replay sizing and remains conservative until account-specific sizing evidence exists.",
        ],
    }
    if args.output.exists():
        raise SystemExit(f"risk report output already exists: {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": result["status"], "errors": errors[:8]}, indent=2))
    return 0 if result["status"] == "RISK_POLICY_PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())


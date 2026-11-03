#!/usr/bin/env python3
"""Build a machine-checkable, fail-closed release-evidence manifest.

The command never promotes a strategy by itself.  It records which gates are
present and leaves `live_enabled=false` until every evidence source, reviewer,
and reconciliation result is explicitly supplied.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import pathlib
import subprocess
from datetime import datetime, timezone
from typing import Any

_integrity_spec = importlib.util.spec_from_file_location(
    "long_donchian_integrity", pathlib.Path(__file__).with_name("verify-long-donchian-integrity.py")
)
if _integrity_spec is None or _integrity_spec.loader is None:
    raise RuntimeError("cannot load the Donchian integrity verifier")
_integrity_module = importlib.util.module_from_spec(_integrity_spec)
_integrity_spec.loader.exec_module(_integrity_module)
integrity_check = _integrity_module.check

FORMULA_ID = "long_donchian_atr_v1"


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path: pathlib.Path) -> Any | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return None


def commit(root: pathlib.Path) -> str | None:
    try:
        value = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD"],
            text=True,
            capture_output=True,
            check=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    return value if len(value) == 40 else None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1])
    parser.add_argument("--config", type=pathlib.Path, default=None)
    parser.add_argument("--parity-report", type=pathlib.Path, default=None)
    parser.add_argument("--proof-report", type=pathlib.Path, default=None)
    parser.add_argument("--replay-report", type=pathlib.Path, default=None)
    parser.add_argument("--ledger-verification", type=pathlib.Path, default=None)
    parser.add_argument("--shadow-report", type=pathlib.Path, default=None)
    parser.add_argument("--shadow-ledger", type=pathlib.Path, default=None)
    parser.add_argument("--paper-report", type=pathlib.Path, default=None)
    parser.add_argument("--risk-report", type=pathlib.Path, default=None)
    parser.add_argument("--reconciliation-report", type=pathlib.Path, default=None)
    parser.add_argument("--reviewer", required=False, default=None)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    config = (args.config or root / "config/autonomous.iex30.demo.json").resolve()
    reports = root / "var/formula/long_donchian_atr_v1/reports"
    paths = {
        "parity": (args.parity_report or reports / "parity-report.json").resolve(),
        "proof": (args.proof_report or reports / "proof-report.json").resolve(),
        "replay": (args.replay_report or reports / "replay-report.json").resolve(),
        "ledger_verification": (args.ledger_verification or reports / "ledger-verification.json").resolve(),
        "shadow": (args.shadow_report or reports / "shadow-report.json").resolve(),
        "shadow_ledger": (args.shadow_ledger or reports / "shadow-ledger-verification.json").resolve(),
        "paper": (args.paper_report or reports / "paper-report.json").resolve(),
        "risk": (args.risk_report or reports / "risk-policy-report.json").resolve(),
        "reconciliation": (args.reconciliation_report or reports / "reconciliation-report.json").resolve(),
    }
    errors: list[str] = []
    try:
        integrity = integrity_check(root, config)
    except (OSError, ValueError, KeyError) as exc:
        integrity = {"status": "BLOCKED", "errors": [str(exc)]}
    if integrity.get("status") != "PASS":
        errors.extend(f"integrity: {error}" for error in integrity.get("errors", ["integrity failed"]))

    documents = {name: read(path) for name, path in paths.items()}
    gates: dict[str, dict[str, Any]] = {}

    parity = documents["parity"]
    parity_ok = bool(
        isinstance(parity, dict)
        and parity.get("passed") is True
        and parity.get("status") == "PARITY_PASS"
        and parity.get("boundary_case_count", 0) >= 9
        and all(case.get("passed") is True for case in parity.get("boundary_cases", []))
    )
    gates["parity"] = {"passed": parity_ok, "report": str(paths["parity"])}

    proof = documents["proof"]
    proof_ok = bool(
        isinstance(proof, dict)
        and proof.get("status") == "PROPERTY_TESTS_PASS"
        and all(value == "PASS" for value in proof.get("checks", {}).values())
    )
    gates["deterministic_tests"] = {"passed": proof_ok, "report": str(paths["proof"])}

    replay = documents["replay"]
    replay_ok = bool(
        isinstance(replay, dict)
        and replay.get("status") == "RESEARCH_REPLAY_COMPLETE"
        and replay.get("trade_metrics", {}).get("closed_trades", 0) > 0
        and replay.get("portfolio_metrics", {}).get("chronological_equity_ledger") is True
    )
    gates["historical_replay"] = {"passed": replay_ok, "report": str(paths["replay"])}

    ledger_verification = documents["ledger_verification"]
    ledger_ok = bool(
        isinstance(ledger_verification, dict)
        and ledger_verification.get("status") == "LEDGER_VERIFICATION_PASS"
        and ledger_verification.get("trade_records", 0) > 0
        and ledger_verification.get("portfolio_records", 0) > 0
    )
    gates["ledger_integrity"] = {"passed": ledger_ok, "report": str(paths["ledger_verification"])}

    shadow = documents["shadow"]
    shadow_ok = bool(
        isinstance(shadow, dict)
        and shadow.get("prospective") is True
        and shadow.get("release_gate", {}).get("passed") is True
        and shadow.get("metrics", {}).get("closed_trades", 0) >= 50
        and shadow.get("sessions", 0) >= 60
    )
    gates["prospective_shadow"] = {"passed": shadow_ok, "report": str(paths["shadow"])}

    shadow_ledger = documents["shadow_ledger"]
    shadow_ledger_ok = bool(
        isinstance(shadow_ledger, dict)
        and shadow_ledger.get("status") == "SHADOW_LEDGER_PASS"
        and shadow_ledger.get("counts", {}).get("cycle", 0) > 0
    )
    gates["shadow_ledger_integrity"] = {"passed": shadow_ledger_ok, "report": str(paths["shadow_ledger"])}

    paper = documents["paper"]
    paper_ok = bool(
        isinstance(paper, dict)
        and paper.get("status") == "PAPER_RECONCILED"
        and paper.get("unresolved_discrepancies", 1) == 0
        and paper.get("costs_within_budget") is True
        and paper.get("fills", 0) > 0
        and paper.get("protected_orders", 0) > 0
        and paper.get("restart_recoveries", 0) > 0
    )
    gates["paper_execution"] = {"passed": paper_ok, "report": str(paths["paper"])}

    risk = documents["risk"]
    risk_ok = bool(isinstance(risk, dict) and risk.get("status") == "RISK_POLICY_PASS")
    gates["risk_policy"] = {"passed": risk_ok, "report": str(paths["risk"])}

    reconciliation = documents["reconciliation"]
    reconciliation_ok = bool(
        isinstance(reconciliation, dict)
        and reconciliation.get("status") == "RECONCILIATION_PASS"
        and reconciliation.get("unresolved_discrepancies", 1) == 0
    )
    gates["reconciliation"] = {"passed": reconciliation_ok, "report": str(paths["reconciliation"])}

    reviewer_ok = isinstance(args.reviewer, str) and bool(args.reviewer.strip())
    gates["attributable_review"] = {"passed": reviewer_ok, "reviewer": args.reviewer}
    for name, gate in gates.items():
        if not gate["passed"]:
            errors.append(f"{name} gate is missing or failed")

    all_passed = not errors
    if all_passed:
        status = "approved"
    elif isinstance(paper, dict) and paper.get("status") == "PAPER_RECONCILED":
        status = "paper_validated"
    elif isinstance(shadow, dict) and shadow.get("prospective") is True:
        status = "shadow"
    else:
        status = "research"
    result = {
        "schema_version": 1,
        "formula_id": FORMULA_ID,
        "authority_id": FORMULA_ID,
        "release_status": status,
        "live_enabled": False,
        "entries_paused": True,
        "authority_integrity": integrity,
        "authority_hashes": integrity.get("hashes", {}),
        "code_commit": commit(root),
        "dataset_hash": replay.get("dataset_hash") if isinstance(replay, dict) else None,
        "gates": gates,
        "reviewer": args.reviewer,
        "release_timestamp": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "errors": errors,
        "limitations": [
            "Frozen current IEX30 membership is not a point-in-time universe study.",
            "IEX daily bars do not establish live queue position or market impact.",
            "Opening-price and gap-through-stop behavior is a deterministic replay policy.",
            "Paper execution evidence cannot prove live-market slippage.",
        ],
    }
    output = args.output.resolve()
    if output.exists():
        raise SystemExit(f"release-evidence output already exists: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": status, "live_enabled": False, "entries_paused": True, "failed_gates": [name for name, gate in gates.items() if not gate["passed"]]}, indent=2))
    return 0 if all_passed else 1


if __name__ == "__main__":
    raise SystemExit(main())


#!/usr/bin/env python3
"""Verify the content-addressed long Donchian deployment boundary.

This verifier is intentionally independent of the Rust runtime.  It checks the
canonical authority, formula specification, IEX30 universe, runtime copy, and
registered implementation bytes before a deployment may be considered for a
release transition.  A research/shadow manifest is valid evidence; it never
turns live execution on.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys
from typing import Any

FORMULA_ID = "long_donchian_atr_v1"
REQUIRED_EVIDENCE = ("proof", "parity", "evaluation", "shadow", "owner")
STATES = {"research", "shadow", "paper_validated", "approved", "paused", "retired"}


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(path: pathlib.Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read JSON input: {path.name}") from exc


def digest(value: Any) -> str:
    return hashlib.sha256(
        json.dumps(value, ensure_ascii=False, sort_keys=False, separators=(",", ":")).encode()
    ).hexdigest()


def check(root: pathlib.Path, config_path: pathlib.Path) -> dict[str, Any]:
    config = load(config_path)
    authority_path = root / "config/formulas/long-donchian-atr-v1.authority.json"
    spec_path = root / "config/formulas/long-donchian-atr-v1.json"
    universe_path = root / "config/iex30.universe.json"
    implementation_path = root / "crates/trady-policy/src/limits/donchian.rs"
    manifest_path = root / "config/formulas/long-donchian-atr-v1.integrity.json"
    authority = load(authority_path)
    spec = load(spec_path)
    universe = load(universe_path)
    manifest = load(manifest_path)
    runtime_authority = config.get("trading", {}).get("limits_authority")
    errors: list[str] = []

    def require(condition: bool, message: str) -> None:
        if not condition:
            errors.append(message)

    require(manifest.get("schema_version") == 1, "integrity schema version is unsupported")
    require(manifest.get("formula_id") == FORMULA_ID, "integrity formula id is incorrect")
    require(manifest.get("authority_id") == FORMULA_ID, "integrity authority id is incorrect")
    for field, path in {
        "authority_sha256": authority_path,
        "spec_sha256": spec_path,
        "universe_sha256": universe_path,
        "runtime_sha256": config_path,
        "implementation_sha256": implementation_path,
    }.items():
        expected = manifest.get(field)
        actual = sha256(path) if path.exists() else None
        require(
            isinstance(expected, str) and len(expected) == 64 and expected == actual,
            f"{field} does not match the registered bytes",
        )

    status = manifest.get("status")
    require(status in STATES, "integrity release status is invalid")
    permits_live = status == "approved"
    require(manifest.get("live_enabled") is permits_live, "release state/live flag is inconsistent")
    require(manifest.get("entries_paused") is (not permits_live), "release state/entry pause is inconsistent")
    evidence = manifest.get("evidence")
    require(isinstance(evidence, dict), "release evidence map is missing")
    for key in REQUIRED_EVIDENCE:
        value = evidence.get(key) if isinstance(evidence, dict) else None
        require(isinstance(value, str) and 0 < len(value) <= 160, f"release evidence is missing: {key}")

    active = authority.get("active", {})
    strategy = authority.get("strategy", {})
    policy = config.get("trading", {}).get("policy")
    require(runtime_authority == authority, "runtime authority differs from canonical authority")
    require(active.get("revision_id") == FORMULA_ID, "authority revision id is incorrect")
    require(active.get("parameters", {}).get("version") == 1, "authority parameter version is incorrect")
    require(
        active.get("formula_hash") == digest([active.get("parameters"), strategy]),
        "authority formula hash does not match parameters and strategy",
    )
    require(
        isinstance(policy, dict) and active.get("policy_hash") == digest(policy),
        "authority policy hash does not match runtime policy",
    )
    require(strategy.get("formula_id") == FORMULA_ID, "authority strategy id is incorrect")
    require(strategy.get("model_fusion") == "DISABLED", "model fusion must remain disabled")
    require(authority.get("prediction") is None, "prediction authority must remain absent")
    expected_evidence = {
        "proof": active.get("proof_report_id"),
        "parity": active.get("parity_report_id"),
        "evaluation": active.get("evaluation_report_id"),
        "shadow": active.get("shadow_report_id"),
        "owner": active.get("owner_approval_id"),
    }
    for key, expected in expected_evidence.items():
        require(evidence.get(key) == expected, f"release evidence does not match authority: {key}")

    expected_spec = {
        "schema_version": 1,
        "formula_id": FORMULA_ID,
        "universe": "IEX30",
        "direction": "LONG_ONLY",
        "bar_interval": "1D_REGULAR_US_SESSION",
        "breakout_window": 20,
        "atr_method": "WILDER",
        "atr_window": 14,
        "minimum_warmup_bars": 35,
        "entry_allowance_atr": "0.10",
        "stop_atr_multiple": "1.50",
        "target_r_multiple": "1.50",
        "maximum_hold_sessions": 5,
        "same_bar_rule": "STOP_FIRST",
        "gap_stop_rule": "FIRST_EXECUTABLE_PRICE",
        "trailing_stop": False,
        "averaging": False,
        "pyramiding": False,
        "leverage": False,
        "model_fusion": "DISABLED",
    }
    for field, expected in expected_spec.items():
        require(spec.get(field) == expected, f"formula specification diverges: {field}")
    symbols = universe.get("symbols")
    require(
        universe.get("universe_id") == "IEX30"
        and universe.get("venue") == "ALPACA_IEX"
        and universe.get("feed") == "iex"
        and universe.get("currency") == "USD"
        and isinstance(symbols, list)
        and len(symbols) == 30
        and len(set(symbols)) == 30,
        "IEX30 universe is incomplete or duplicated",
    )
    if status == "research":
        require(config.get("enabled") is False, "research runtime must remain disabled")
        require(config.get("controls", {}).get("pause_entries") is True, "research entries must remain paused")
        require(config.get("trading", {}).get("mode") == "paper", "research runtime must remain paper mode")
    elif status == "shadow":
        require(config.get("enabled") is True, "shadow runtime must be enabled")
        require(config.get("execution_stage") == "SHADOW", "shadow runtime must use SHADOW execution stage")
        require(config.get("trading", {}).get("mode") == "shadow", "shadow runtime must use a shadow account")
        require(config.get("controls", {}).get("pause_entries") is False, "shadow entries must be recordable")
    else:
        require(config.get("enabled") is False, "non-approved runtime must remain disabled")
        require(config.get("controls", {}).get("pause_entries") is True, "non-approved entries must remain paused")
        require(config.get("trading", {}).get("mode") == "paper", "non-approved runtime must remain paper mode")
    require(config.get("trading", {}).get("live_release") is None, "research runtime cannot carry live release")
    require(config.get("broker", {}).get("environment") == "demo", "research runtime must use demo broker")

    result = {
        "schema_version": 1,
        "status": "PASS" if not errors else "BLOCKED",
        "formula_id": FORMULA_ID,
        "release_status": status,
        "live_enabled": bool(manifest.get("live_enabled")),
        "entries_paused": bool(manifest.get("entries_paused")),
        "hashes": {field: manifest.get(field) for field in (
            "authority_sha256", "spec_sha256", "universe_sha256", "runtime_sha256", "implementation_sha256"
        )},
        "errors": errors,
    }
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1])
    parser.add_argument("--config", type=pathlib.Path, default=None)
    args = parser.parse_args()
    root = args.root.resolve()
    config = (args.config or root / "config/autonomous.iex30.demo.json").resolve()
    try:
        result = check(root, config)
    except ValueError as exc:
        result = {"schema_version": 1, "status": "BLOCKED", "formula_id": FORMULA_ID, "errors": [str(exc)]}
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())


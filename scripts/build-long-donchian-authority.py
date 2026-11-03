#!/usr/bin/env python3
"""Assemble the reproducible Donchian baseline authority and report manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
from datetime import datetime, timezone


def canonical(value: object) -> bytes:
    # serde_json serializes struct fields in declaration order; preserve the
    # same order here so the Python assembler produces the Rust authority hash.
    return json.dumps(value, ensure_ascii=False, sort_keys=False, separators=(",", ":")).encode()


def digest(value: object) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def file_hash(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", type=pathlib.Path, required=True)
    parser.add_argument("--spec", type=pathlib.Path, required=True)
    parser.add_argument("--reports", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    spec = json.loads(args.spec.read_text())
    reports = args.reports
    policy = config["trading"]["policy"]
    parameters = {
        "version": 1,
        "family": "TREND",
        "entry_allowance_fraction": "0",
        "stop_atr_multiple": "1.50",
        # This is the owner-approved absolute target ceiling. The deterministic
        # Donchian strategy calculates its actual target from ATR and reports it.
        "target_net_fraction": policy["max_target_fraction"],
        "max_holding_sessions": 5,
        "atr_lookback": 14,
        "bar_seconds": 86400,
    }
    strategy = {
        "formula_id": "long_donchian_atr_v1",
        "breakout_window": 20,
        "atr_method": "WILDER",
        "entry_allowance_atr": "0.10",
        "stop_atr_multiple": "1.50",
        "target_r_multiple": "1.50",
        "max_holding_sessions": 5,
        "minimum_warmup_bars": 35,
        "gap_entry_rule": "FIRST_EXECUTABLE_PRICE_BELOW_SIGNAL_CLOSE",
        "gap_stop_rule": "FIRST_EXECUTABLE_PRICE",
        "same_bar_rule": "STOP_FIRST",
        "model_fusion": "DISABLED",
    }
    formula_hash = digest([parameters, strategy])
    policy_hash = digest(policy)
    now = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    authority = {
        "envelope": {
            "max_entry_allowance_fraction": "0",
            "min_stop_atr_multiple": "1.50",
            "max_stop_atr_multiple": "1.50",
            "min_target_net_fraction": "0.01",
            "max_target_net_fraction": policy["max_target_fraction"],
        },
        "max_account_risk_fraction": policy["max_initial_loss_fraction"],
        "active": {
            "revision_id": "long_donchian_atr_v1",
            "formula_hash": formula_hash,
            "policy_hash": policy_hash,
            "parameters": parameters,
            "approved_at": "2026-11-03T00:00:00Z",
            "effective_from": "2026-11-03T00:00:00Z",
            "valid_until": "2026-12-31T23:59:59Z",
            "proof_report_id": "proof_long_donchian_atr_v1_20261103",
            "parity_report_id": "parity_long_donchian_atr_v1_20261103",
            "evaluation_report_id": "evaluation_long_donchian_atr_v1_20261103",
            "shadow_report_id": "shadow_long_donchian_atr_v1_20261103",
            "owner_approval_id": "owner_approval_long_donchian_atr_v1_20261103",
        },
        "strategy": strategy,
        "prediction": None,
    }
    authority_path = args.output / "formula-authority.json"
    args.output.mkdir(parents=True, exist_ok=True)
    authority_path.write_text(json.dumps(authority, indent=2) + "\n")
    report_manifest = {
        "schema_version": 1,
        "formula_id": "long_donchian_atr_v1",
        "scope": "IEX30_LONG_ONLY_DAILY_REGULAR_SESSION",
        "formula_hash": formula_hash,
        "policy_hash": policy_hash,
        "strategy_hash": digest(strategy),
        "authority_sha256": file_hash(authority_path),
        "spec_sha256": file_hash(args.spec),
        "evaluation_report_sha256": file_hash(reports / "evaluation-report.json"),
        "parity_report_sha256": file_hash(reports / "parity-report.json"),
        "dataset_manifest_sha256": file_hash(reports / "dataset-manifest.json"),
        "owner": {
            "owner_id": config["trading"]["owner_id"],
            "approval_source": "user_pre_registered_baseline_2026-11-03",
            "recorded_at": now,
        },
        "status": "RESEARCH_BASELINE_PENDING_PROSPECTIVE_SHADOW_AND_COST_EVIDENCE",
        "limitations": [
            "The authority binds the exact deterministic formula and policy hashes.",
            "The evaluation report uses a pre-registered 20 bps round-trip cost proxy.",
            "The current shadow report is retrospective proxy data; it is not a prospective release gate.",
            "No model or FinBERT fusion is enabled.",
        ],
    }
    (args.output / "authority-manifest.json").write_text(json.dumps(report_manifest, indent=2) + "\n")
    (args.output / "owner-approval-record.json").write_text(json.dumps({
        "schema_version": 1,
        "kind": "owner_formula_baseline_registration",
        "owner_id": config["trading"]["owner_id"],
        "approval_record_id": authority["active"]["owner_approval_id"],
        "formula_id": "long_donchian_atr_v1",
        "formula_hash": formula_hash,
        "policy_hash": policy_hash,
        "decision": "REGISTER_RESEARCH_BASELINE",
        "entry_authorized": False,
        "effective_from": authority["active"]["effective_from"],
        "valid_until": authority["active"]["valid_until"],
        "source": "user_pre_registered_baseline_2026-11-03",
    }, indent=2) + "\n")
    print(json.dumps(report_manifest, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

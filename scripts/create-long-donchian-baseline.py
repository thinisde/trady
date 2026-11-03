#!/usr/bin/env python3
"""Capture immutable Phase 0 provenance for a Donchian validation run."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
import subprocess
from datetime import datetime, timezone


def file_hash(path: pathlib.Path) -> str | None:
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError:
        return None


def command(root: pathlib.Path, args: list[str]) -> str | None:
    try:
        return subprocess.run(
            args, cwd=root, text=True, capture_output=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--dataset-manifest", type=pathlib.Path, default=None)
    parser.add_argument("--test-command", default="cargo test --workspace --locked")
    parser.add_argument("--test-result", default="NOT_RUN")
    parser.add_argument("--report-command", default="python3 scripts/replay-long-donchian-ledger.py")
    args = parser.parse_args()
    root = args.root.resolve()
    files = {
        "authority_json_sha256": root / "config/formulas/long-donchian-atr-v1.authority.json",
        "spec_json_sha256": root / "config/formulas/long-donchian-atr-v1.json",
        "universe_json_sha256": root / "config/iex30.universe.json",
        "runtime_config_sha256": root / "config/autonomous.iex30.demo.json",
        "implementation_sha256": root / "crates/trady-policy/src/limits/donchian.rs",
        "integrity_manifest_sha256": root / "config/formulas/long-donchian-atr-v1.integrity.json",
    }
    output = args.output.resolve()
    if output.exists():
        raise SystemExit(f"baseline output already exists: {output}")
    manifest = {
        "schema_version": 1,
        "kind": "long_donchian_atr_v1_baseline_manifest",
        "captured_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "git_commit": command(root, ["git", "rev-parse", "HEAD"]),
        "git_status": command(root, ["git", "status", "--short"]),
        "toolchain": {
            "rustc": command(root, ["rustc", "--version"]),
            "cargo": command(root, ["cargo", "--version"]),
            "python": platform.python_version(),
            "requirements_dev_sha256": file_hash(root / "scripts/requirements-dev.txt"),
        },
        "inputs": {name: file_hash(path) for name, path in files.items()},
        "dataset_manifest_sha256": file_hash((args.dataset_manifest or root / "var/formula/long_donchian_atr_v1/reports/dataset-manifest.json").resolve()),
        "report_generation_command": args.report_command,
        "test_command": args.test_command,
        "test_result": args.test_result,
        "entry_state": {"status": "research", "live_enabled": False, "entries_paused": True},
        "limitations": [
            "The frozen current IEX30 list is not point-in-time membership history.",
            "The IEX feed scope and opening-price semantics are defined by the registered source contract.",
            "The 20 bps round-trip cost remains a proxy until paper fills provide measured evidence.",
        ],
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(manifest, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())


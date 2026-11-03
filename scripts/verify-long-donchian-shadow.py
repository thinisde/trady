#!/usr/bin/env python3
"""Verify immutable local shadow-cycle and proposal records."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import stat
from typing import Any

FORMULA_ID = "long_donchian_atr_v1"


def canonical(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=False, separators=(",", ":")).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def valid_hash(value: Any) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def load(path: pathlib.Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def verify(directory: pathlib.Path, expected_authority: str | None = None, expected_runtime: str | None = None) -> dict[str, Any]:
    errors: list[str] = []
    counts = {"cycle": 0, "proposal": 0, "observation": 0, "session": 0}
    if not directory.exists() or not directory.is_dir() or directory.is_symlink():
        return {"schema_version": 1, "status": "SHADOW_LEDGER_BLOCKED", "counts": counts, "errors": ["shadow directory is missing or unsafe"]}
    try:
        mode = stat.S_IMODE(directory.stat().st_mode)
        if mode & 0o077:
            errors.append("shadow directory is accessible outside its owner")
    except OSError:
        errors.append("shadow directory permissions are unreadable")
    for path in sorted(directory.iterdir()):
        if path.is_symlink() or not path.is_file():
            errors.append(f"unexpected non-regular shadow entry: {path.name}")
            continue
        if path.suffix != ".json":
            errors.append(f"unexpected shadow file: {path.name}")
            continue
        try:
            if stat.S_IMODE(path.stat().st_mode) & 0o077:
                errors.append(f"shadow record is not private: {path.name}")
            if path.stat().st_size > 4 * 1024 * 1024:
                errors.append(f"shadow record exceeds size limit: {path.name}")
            value = load(path)
        except (OSError, UnicodeDecodeError, json.JSONDecodeError):
            errors.append(f"shadow record is unreadable: {path.name}")
            continue
        stem = path.stem
        if stem.startswith("cycle_"):
            counts["cycle"] += 1
            if digest(value) != stem.removeprefix("cycle_"):
                errors.append(f"cycle filename hash mismatch: {path.name}")
            if not isinstance(value, dict) or value.get("schema_version") != 1 or value.get("mode") != "shadow":
                errors.append(f"cycle schema or mode invalid: {path.name}")
            if isinstance(value, dict) and value.get("order_mutations") != 0:
                errors.append(f"cycle reports broker mutation: {path.name}")
            integrity = value.get("authority_integrity") if isinstance(value, dict) else None
            if not isinstance(integrity, dict):
                errors.append(f"cycle is missing authority integrity: {path.name}")
            else:
                if integrity.get("live_enabled") is not False or integrity.get("entries_paused") is not True:
                    errors.append(f"cycle is not paused: {path.name}")
                if expected_authority and integrity.get("authority_sha256") != expected_authority:
                    errors.append(f"cycle authority hash mismatch: {path.name}")
                if expected_runtime and integrity.get("runtime_sha256") != expected_runtime:
                    errors.append(f"cycle runtime hash mismatch: {path.name}")
        elif stem.startswith("observation_"):
            counts["observation"] += 1
            if digest(value) != stem.removeprefix("observation_"):
                errors.append(f"observation filename hash mismatch: {path.name}")
            source = value.get("source", {}) if isinstance(value, dict) else {}
            if not isinstance(value, dict) or value.get("kind") != "OBSERVATION" or not valid_hash(source.get("market_data_hash")):
                errors.append(f"observation provenance is invalid: {path.name}")
        elif stem.startswith("intent_"):
            counts["proposal"] += 1
            request = value.get("request", {}) if isinstance(value, dict) else {}
            intent_id = request.get("intent_id") if isinstance(request, dict) else None
            if not isinstance(value, dict) or value.get("kind") != "PROPOSAL" or intent_id != stem.removeprefix("intent_"):
                errors.append(f"proposal intent identity is invalid: {path.name}")
            if not isinstance(value, dict) or value.get("order_mutations") != 0:
                errors.append(f"proposal reports broker mutation: {path.name}")
            if isinstance(request, dict) and any(field in request for field in ("broker_order_id", "filled_quantity", "fill_price")):
                errors.append(f"proposal contains broker fill state: {path.name}")
        elif stem.startswith("session_"):
            counts["session"] += 1
        else:
            errors.append(f"unrecognized shadow record: {path.name}")
    if counts["cycle"] == 0:
        errors.append("shadow ledger contains no cycle records")
    return {
        "schema_version": 1,
        "formula_id": FORMULA_ID,
        "status": "SHADOW_LEDGER_PASS" if not errors else "SHADOW_LEDGER_BLOCKED",
        "counts": counts,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=pathlib.Path, required=True)
    parser.add_argument("--authority-hash")
    parser.add_argument("--runtime-hash")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    report = verify(args.directory, args.authority_hash, args.runtime_hash)
    if args.output.exists():
        raise SystemExit(f"shadow verification output already exists: {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": report["status"], "counts": report["counts"], "errors": report["errors"][:8]}, indent=2))
    return 0 if report["status"] == "SHADOW_LEDGER_PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())


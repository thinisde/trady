#!/usr/bin/python3
"""Read-only selection of an independently reconciled, immutable as-of export.

Never calculates wins from open holdings or fills gaps with zero observations.
The fixed export path is pinned by the deployment-owned helper configuration.
"""
import argparse
import hashlib
import json
import pathlib
import sys

MAX = 4 * 1024 * 1024


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def reconcile(export, manifest):
    if export["account_id"] != manifest["account_id"] or export["mode"] != manifest["mode"]:
        raise ValueError("reconciliation account/mode differs")
    matches = []
    for item in export["reconciliations"]:
        content = item["content"]
        if hashlib.sha256(canonical(content)).hexdigest() != item["hash"]:
            raise ValueError("reconciliation export was tampered")
        if content["cutoff_at"] == manifest["cutoff_at"]:
            report = content["report"]
            if report["unresolved_count"] or content["unresolved_items"]:
                raise ValueError("unresolved reconciliation")
            matches.append(report)
    if len(matches) != 1:
        raise ValueError("exact independently reconciled cutoff unavailable")
    return matches[0]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--export", type=pathlib.Path, required=True)
    args = parser.parse_args()
    request = json.loads(sys.stdin.buffer.read(MAX + 1))
    if args.export.stat().st_size > MAX or request["stage"] != "reconcile":
        raise ValueError("invalid reconciliation input")
    result = reconcile(json.loads(args.export.read_text()), request["manifest"])
    print(json.dumps({"schema_version": 1, "input_hash": request["input_hash"], "payload": result}, separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("independently reconciled export unavailable", file=sys.stderr)
        sys.exit(1)

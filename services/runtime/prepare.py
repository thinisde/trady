#!/usr/bin/python3
"""Prepare disabled, reviewable application configs; never install/start services."""
import argparse
import json
import os
import pathlib


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate configuration key")
        result[key] = value
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=pathlib.Path, required=True,
                        help="complete reviewed autonomous config with real approved model/formula/source references")
    parser.add_argument("--destination", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if args.source.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("configuration too large")
    config = json.loads(args.source.read_text(), object_pairs_hook=unique)
    for key in ["config_version", "trading", "broker", "inputs", "universe", "models", "controls"]:
        if key not in config:
            raise ValueError("complete reviewed autonomous configuration required")
    config["analyst_socket"] = "/run/trady-analyst/analyst.sock"
    config["enabled"] = False
    args.destination.mkdir(mode=0o755, parents=True, exist_ok=True)
    data = json.dumps(config, indent=2) + "\n"
    for name in ["analyst.json", "executor.json"]:
        target = args.destination / name
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        with os.fdopen(fd, "w") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
    print("Prepared analyst.json and executor.json, enabled=false; review before deployment.")


if __name__ == "__main__":
    main()

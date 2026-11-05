#!/usr/bin/python3
"""Prepare a disabled EUR SHADOW config; never install/start services."""
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
                        help="reviewed EUR configuration; legacy runtime shapes are rejected")
    parser.add_argument("--destination", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if args.source.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("configuration too large")
    config = json.loads(args.source.read_text(), object_pairs_hook=unique)
    if "approved_universe_snapshot_id" not in config:
        raise ValueError("only EUR configurations are supported; legacy runtime retired")
    if (config.get("base_currency") != "EUR"
            or config.get("live_enabled") is not False
            or config.get("broker", {}).get("environment") != "demo"
            or config.get("execution_stage") not in ["REPLAY", "SHADOW", "DEMO"]):
        raise ValueError("EUR preparation requires a non-live EUR demo authority")
    if any(key in config for key in ["trading", "alpaca_market_data", "inputs", "universe"]):
        raise ValueError("mixed legacy and EUR authority")
    config["enabled"] = False
    config["execution_stage"] = "SHADOW"
    names = ["eur-shadow.json"]
    args.destination.mkdir(mode=0o755, parents=True, exist_ok=True)
    data = json.dumps(config, indent=2) + "\n"
    for name in names:
        target = args.destination / name
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
        with os.fdopen(fd, "w") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
    print("Prepared " + ", ".join(names) + ", enabled=false; review before deployment.")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Supervise the explicitly authorized, ignored EUR DEMO protocol experiment.

One actual holding at a time. The child exits after verified protection, then a
new process reconstructs the durable protocol and exits. Failed runs trigger
exact-ID cleanup; unknown unacknowledged outcomes still require operator review.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--instrument", choices=["AIR_PA_EQ", "ASML_AS_EQ"], required=True)
    p.add_argument("--case", choices=["normal", "retest", "unknown", "precision", "cancel_race", "timing", "paced", "reservation_race", "reservation_fast", "small_fast", "unknown_small", "depth_partial", "depth_partial_followup", "passive_partial", "stop_cancel_race", "stop_fill_race", "stop_fill_trigger"], required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--stream", type=Path, required=True)
    p.add_argument("--authorized-demo-evidence", action="store_true", required=True)
    args = p.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(mode=0o700)
    environment = os.environ.copy()
    environment.update({"TRADY_CREDENTIALS_FILE": str(ROOT / "config/credentials.local.json"),
        "TRADY_DEMO_EVIDENCE_AUTHORIZED": "EUR_CAPABILITY_20261105", "TRADY_PROBE_DIRECTORY": str(args.out),
        "TRADY_PROBE_INSTRUMENT": args.instrument, "TRADY_PROBE_CASE": args.case,
        "TRADY_IBKR_STREAM_DIRECTORY": str(args.stream.resolve())})
    for phase in ["protect", "exit"]:
        environment["TRADY_PROBE_PHASE"] = phase
        child = subprocess.run(["cargo", "test", "-p", "trady-cli", "--test", "eur_demo_capability", "--locked", "--", "--ignored", "--nocapture"],
                               cwd=ROOT, env=environment, capture_output=True, text=True)
        (args.out / (phase + ".log")).write_text(child.stdout + child.stderr)
        print(json.dumps({"phase": phase, "exit_code": child.returncode}), flush=True)
        if child.returncode:
            if (args.out / "authority.json").exists():
                cleanup = subprocess.run([sys.executable, str(ROOT / "scripts/cleanup-eur-demo-probe.py"), "--directory", str(args.out)],
                                         cwd=ROOT, capture_output=True, text=True)
                (args.out / "supervised-cleanup.log").write_text(cleanup.stdout + cleanup.stderr)
                print(json.dumps({"exact_id_cleanup_exit_code": cleanup.returncode}), flush=True)
            print((child.stdout + child.stderr)[-3000:], flush=True)
            return 1
        result = json.loads((args.out / ("result_" + phase + ".json")).read_text())
        print(json.dumps({"phase": phase, "state": result["state"]["phase"]}), flush=True)
        if result["state"]["phase"] != "PROTECTED":
            return 0 if result["state"]["phase"] in ["FLAT", "REJECTED"] else 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

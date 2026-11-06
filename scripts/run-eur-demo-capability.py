#!/usr/bin/env python3
"""Supervise the explicitly authorized, ignored EUR DEMO protocol experiment.

One actual holding at a time. The child exits after verified protection, then a
new process reconstructs the durable protocol and exits. Failed runs trigger
exact-ID cleanup; unknown unacknowledged outcomes still require operator review.
"""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def claim_continuation(directory, experiment, out, now):
    start = json.loads((directory / "START.json").read_text())
    cutoff = datetime.fromisoformat(start["new_experiment_cutoff"].replace("Z", "+00:00"))
    began = datetime.fromisoformat(start["started_at"])
    if not (start["self_disable_confirmed"] is True and start["maximum_new_entry_attempts"] == 1
            and start["maximum_entry_notional_eur"] == "2500" and start["activation_authorized"] is False
            and start["live_enabled"] is False and began.date() == now.date() and began <= now < cutoff):
        raise ValueError("CONTINUATION_AUTHORITY_OR_TIME_WINDOW_INVALID")
    if not (1 <= len(experiment) <= 32 and experiment.isascii()
            and all(c.isalnum() or c == "_" for c in experiment)):
        raise ValueError("FRESH_SAFE_EXPERIMENT_ID_REQUIRED")
    # A failed/no-opportunity watch consumes this continuation's single attempt
    # slot too. Restarting or choosing another directory cannot create a BUY.
    with (directory / "ENTRY_ATTEMPT.json").open("x") as marker:
        os.chmod(marker.fileno(), 0o600)
        json.dump({"at": now.isoformat(), "experiment_id": experiment,
                   "probe_directory": str(out), "entry_cutoff": cutoff.isoformat()}, marker)
        marker.flush()
        os.fsync(marker.fileno())
    return cutoff.isoformat()


def claim_manual(directory, experiment, out, now):
    """A new bounded manual run, never a receipt for the closed morning task."""
    start = json.loads((directory / "START.json").read_text())
    began = datetime.fromisoformat(start["started_at"])
    cutoff = datetime.fromisoformat(start["new_experiment_cutoff"].replace("Z", "+00:00"))
    if not (start["kind"] == "OWNER_AUTHORIZED_MANUAL_EVIDENCE_AFTER_30_ENTRY_POLICY_CHANGE"
            and start["maximum_new_entry_attempts"] == 1 and start["maximum_entry_notional_eur"] == "2500"
            and start["maximum_acquired_holdings"] == 1 and start["activation_authorized"] is False
            and start["live_enabled"] is False and start["scheduled_task_used"] is False
            and now.date().isoformat() == "2026-11-06" and began.date() == now.date()
            and began <= now < cutoff and (cutoff-began).total_seconds() <= 900
            and 1 <= len(experiment) <= 32 and experiment.isascii()
            and all(c.isalnum() or c == "_" for c in experiment)):
        raise ValueError("MANUAL_BOUNDED_AUTHORITY_OR_TIME_WINDOW_INVALID")
    with (directory / "ENTRY_ATTEMPT.json").open("x") as marker:
        os.chmod(marker.fileno(), 0o600)
        json.dump({"at":now.isoformat(),"experiment_id":experiment,"probe_directory":str(out),
                   "entry_cutoff":cutoff.isoformat(),"classification":"MANUAL_ONE_ATTEMPT_NOT_SCHEDULED_CONTINUATION"},marker)
        marker.flush();os.fsync(marker.fileno())
    return cutoff.isoformat()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--instrument", choices=["AIR_PA_EQ", "ASML_AS_EQ"], required=True)
    p.add_argument("--case", choices=["normal", "retest", "unknown", "precision", "cancel_race", "timing", "paced", "reservation_race", "reservation_fast", "small_fast", "unknown_small", "depth_partial", "depth_partial_followup", "passive_partial", "stop_cancel_race", "stop_fill_race", "stop_fill_trigger"], required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--stream", type=Path, required=True)
    p.add_argument("--experiment-id", required=True)
    authority=p.add_mutually_exclusive_group(required=True)
    authority.add_argument("--continuation", type=Path)
    authority.add_argument("--manual-run", type=Path)
    p.add_argument("--authorized-demo-evidence", action="store_true", required=True)
    args = p.parse_args()
    if args.case not in ["depth_partial", "depth_partial_followup"]:
        raise ValueError("THIS_CONTINUATION_AUTHORIZES_ONLY_A_NATIVE_THIN_DEPTH_PARTIAL_WATCH")
    args.out = args.out.resolve()
    args.out.mkdir(mode=0o700)
    if args.manual_run:
        if args.case != "depth_partial_followup":
            raise ValueError("MANUAL_FOLLOWUP_REQUIRES_MATERIALLY_THINNER_NATIVE_DEPTH")
        cutoff=claim_manual(args.manual_run.resolve(),args.experiment_id,args.out,datetime.now(timezone.utc))
    else:
        cutoff = claim_continuation(args.continuation.resolve(), args.experiment_id, args.out, datetime.now(timezone.utc))
    environment = os.environ.copy()
    environment.update({"TRADY_CREDENTIALS_FILE": str(ROOT / "config/credentials.local.json"),
        "TRADY_DEMO_EVIDENCE_AUTHORIZED": "EUR_CAPABILITY_20261105", "TRADY_PROBE_DIRECTORY": str(args.out),
        "TRADY_PROBE_INSTRUMENT": args.instrument, "TRADY_PROBE_CASE": args.case,
        "TRADY_PROBE_EXPERIMENT_ID": args.experiment_id, "TRADY_PROBE_ENTRY_CUTOFF": cutoff,
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

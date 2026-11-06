#!/usr/bin/env python3
"""Exact-ID cleanup for an authorized one-holding EUR DEMO probe.

Python has no broker transport. It routes only an unfinished original scope
to the physically fenced Rust exit driver; unknown submissions require review.
"""
import argparse
from datetime import datetime, timezone
from decimal import Decimal
import json
import os
import subprocess
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
JOURNALS = ["transport.jsonl", "faults.jsonl", "manual-safety-stop.jsonl", "cleanup.jsonl"]


def unresolved_mutation(rows):
    pending = None
    for row in rows:
        key = (row.get("method"), row.get("path"))
        # Old probe journals predate phased mutation records. An explicitly
        # ambiguous outcome in those records still requires exact-ID review.
        failure = row.get("failure")
        if row.get("method") != "GET" and (row.get("phase") == "UNKNOWN" or (isinstance(failure, dict) and failure.get("outcome_unknown") is True) or row.get("outcome_unknown") is True):
            return key if key != (None, None) else ("UNKNOWN", row.get("cancel_exact_id", "legacy_submission"))
        if "cancel_exact_id" in row and row.get("accepted") is not True and row.get("phase") is None:
            status = row.get("failure_http_status")
            if status not in [400, 401, 403, 404, 405, 415, 422]:
                return ("DELETE", "orders/" + str(row["cancel_exact_id"]))
        if row.get("phase") == "BEFORE_SEND" and row.get("method") != "GET":
            if pending is not None:
                return pending
            pending = key
        elif row.get("phase") == "RESPONSE" and key == pending:
            status, response = row.get("http_status"), row.get("response")
            acknowledged = row.get("accepted") is True or (
                isinstance(response, dict) and bool(response.get("broker_order_id")))
            definitive = isinstance(status, int) and (200 <= status < 300 or status in [400, 401, 403, 404, 405, 415, 422])
            if definitive or acknowledged or row.get("definitive_no_effect") is True:
                pending = None
    return pending


def acknowledged_ids(name, rows):
    result = set()
    for row in rows:
        response = row.get("response")
        order = (name == "transport.jsonl" and isinstance(response, dict)
                 and "broker_order_id" in response and "request" in row) or (
            row.get("phase") == "RESPONSE" and row.get("method") == "POST"
            and row.get("path", "").startswith("orders/")
            and 200 <= row.get("http_status", 0) < 300)
        if order and isinstance(response, dict):
            identifier = response.get("id", response.get("broker_order_id"))
            if identifier is not None:
                result.add(str(identifier))
    return result


def cleanup_scope(directory):
    """Local preflight only. A closed scope never loads credentials or transport."""
    journals = {name: [json.loads(line) for line in (directory / name).read_text().splitlines()]
                for name in JOURNALS if (directory / name).exists()}
    if any(row.get("status") == "EXACT_ID_DEMO_PROBE_CLEANUP_FLAT"
           for rows in journals.values() for row in rows):
        raise ValueError("PROBE_SCOPE_ALREADY_CLEANED")
    for name in ["result_exit.json", "result_cleanup.json"]:
        path = directory / name
        if path.exists() and json.loads(path.read_text())["state"]["phase"] in ["FLAT", "REJECTED"]:
            raise ValueError("PROBE_SCOPE_ALREADY_TERMINAL")
    if (directory / "cleanup-start.json").exists() or (directory / "result_cleanup.json").exists():
        raise ValueError("PRIOR_CLEANUP_REQUIRES_ORIGINAL_CONTEXT_REVIEW")
    if any(unresolved_mutation(rows) is not None for rows in journals.values()):
        raise ValueError("PRIOR_MUTATION_OUTCOME_REQUIRES_EXACT_ID_REVIEW")
    authority = json.loads((directory / "authority.json").read_text())
    context = authority["context"]
    experiment = authority.get("experiment_id", "")
    if (context["broker_account_id"] != 52211463 or context["runtime_mode"] != "DEMO"
            or context["broker_ticker"] not in ["AIRp_EQ", "ASMLa_EQ"]
            or not re.fullmatch(r"[A-Za-z0-9_]{1,32}", experiment)):
        raise ValueError("ORIGINAL_DEMO_EXPERIMENT_REQUIRED_LEGACY_SCOPE_REVIEW_ONLY")
    instrument = "AIR_PA_EQ" if context["broker_ticker"] == "AIRp_EQ" else "ASML_AS_EQ"
    short = "air" if instrument == "AIR_PA_EQ" else "asml"
    prefix = "paper_cap_" + experiment + "_" + short + "_"
    case = context["account_id"].removeprefix(prefix)
    if (not context["account_id"].startswith(prefix)
            or case not in ["depth_partial", "depth_partial_followup"]
            or context["intent_id"] != context["account_id"].removeprefix("paper_")
            or context["instrument_id"] != instrument):
        raise ValueError("EXACT_ORIGINAL_PROBE_SCOPE_REQUIRED")
    known = set()
    for name, rows in journals.items():
        known.update(acknowledged_ids(name, rows))
    if not known:
        raise ValueError("NO_ACKNOWLEDGED_EXACT_PROBE_IDS")
    return context, experiment, instrument, case


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    directory = args.directory.resolve(strict=True)
    context, experiment, instrument, case = cleanup_scope(directory)
    if os.geteuid() != 0:
        raise ValueError("TRUSTED_ROOT_SUPERVISION_REQUIRED")
    with os.fdopen(os.open(directory / "cleanup-start.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as marker:
        json.dump({"started_at":datetime.now(timezone.utc).isoformat(), "experiment_id":experiment,
                   "context":context, "route":"FENCED_RUST_ORIGINAL_CONTEXT_EXIT_ONLY",
                   "new_buy_authorized":False}, marker)
        marker.flush();os.fsync(marker.fileno())
    environment = os.environ.copy()
    environment.update({"TRADY_CREDENTIALS_FILE":str(ROOT / "config/credentials.local.json"),
        "TRADY_DEMO_EVIDENCE_AUTHORIZED":"EUR_CAPABILITY_20261105", "TRADY_PROBE_DIRECTORY":str(directory),
        "TRADY_PROBE_INSTRUMENT":instrument, "TRADY_PROBE_CASE":case, "TRADY_PROBE_EXPERIMENT_ID":experiment,
        "TRADY_PROBE_ENTRY_CUTOFF":context["flatten_at"], "TRADY_PROBE_PHASE":"cleanup"})
    result = subprocess.run(["cargo", "test", "--locked", "-p", "trady-cli", "--test", "eur_demo_capability",
                             "supervised_eur_demo_capability", "--", "--ignored", "--exact", "--nocapture"],
                            cwd=ROOT,env=environment,capture_output=True,text=True)
    with os.fdopen(os.open(directory / "cleanup-protocol.log",os.O_WRONLY | os.O_CREAT | os.O_EXCL,0o600),"w") as log:
        log.write(result.stdout + result.stderr)
    if result.returncode:
        raise ValueError("FENCED_EXACT_ID_CLEANUP_REQUIRES_REVIEW_NO_BLIND_RETRY")
    value=json.loads((directory / "result_cleanup.json").read_text())
    if value["state"]["phase"] not in ["FLAT","REJECTED"] or Decimal(value["state"]["holding_quantity"]) != 0:
        raise ValueError("FENCED_CLEANUP_NOT_TERMINAL")
    print(json.dumps({"status":"FENCED_RUST_EXACT_ID_PROBE_CLEANUP_TERMINAL","intent_id":context["intent_id"],
                      "phase":value["state"]["phase"],"runtime_acceptance_passed":False,"live_enabled":False}))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, TypeError, subprocess.SubprocessError):
        raise SystemExit("Exact-ID cleanup stopped; inspect its durable journal before any further mutation.") from None

#!/usr/bin/env python3
"""Recover the explicitly identified volatile Trady demo database.

This cannot recreate unarchived history. It preserves archived SHADOW reports,
acknowledged delivery state and known X costs, then creates a persistent backup.
All credentials remain in private files/stdin. Other containers are untouched.
"""
import argparse
import csv
from datetime import datetime, timezone
import hashlib
import io
import json
import os
from pathlib import Path
import secrets
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
NAME = "trady-live-demo-db-20261031"
RETIRED = "trady-volatile-db-retired-20261104"
VOLUME = "trady-eur-postgres18-data"
STATE = Path("/var/lib/trady-eur-shadow/recovery-20261104")
ROLES = {
    "TRADY_MIGRATOR_DSN": "trady_migrator", "TRADY_ANALYST_DSN": "trady_analyst",
    "TRADY_EXECUTOR_DSN": "trady_executor", "TRADY_GATEWAY_DSN": "trady_gateway",
    "TRADY_LAB_DSN": "trady_lab", "TRADY_VERIFIER_DSN": "trady_verifier",
    "TRADY_SHADOW_DSN": "trady_shadow", "TRADY_RELEASE_DSN": "trady_releaser",
    "TRADY_RELEASE_OWNER_DSN": "trady_release_owner",
}


def run(*args, data=None, stdout=None):
    result = subprocess.run(args, input=data, stdout=stdout or subprocess.PIPE,
                            stderr=subprocess.PIPE, check=True)
    return result.stdout


def sql(statement, database="trady"):
    return run("docker", "exec", "-i", "--user", "postgres", NAME, "psql", "-X", "-q",
               "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", database, data=statement.encode())


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def digest(value):
    return hashlib.sha256(canonical(value).encode()).hexdigest()


def copy_rows(table, columns, rows):
    stream = io.StringIO()
    writer = csv.writer(stream)
    for row in rows:
        writer.writerow(["" if value is None else value for value in row])
    run("docker", "exec", "-i", "--user", "postgres", NAME, "psql", "-X", "-q",
        "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", "trady", "-c",
        "COPY " + table + "(" + ",".join(columns) + ") FROM STDIN WITH (FORMAT CSV)",
        data=stream.getvalue().encode())


def private_json(path, value):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as output:
        json.dump(value, output, indent=2)
        output.write("\n")


def restore_archived_shadow():
    directory = ROOT / "var/eur-first-run-checks"
    report = json.loads((directory / "eur-real-report-after-restart.json").read_text())
    result = report["report"]
    # Restore only historical reports, not a resumable complete checkpoint:
    # the original provider snapshots were not all exported before the loss.
    rows = []
    for kind in ("selection", "daily_report"):
        rows.append([kind, result["cycle_id"], "eur_demo_owner", result["account_id"],
                     canonical(result), digest(result)])
    receipt = {
        "kind": "VOLATILE_DATABASE_LOSS_RECOVERY", "unarchived_history_restored": False,
        "provider_snapshot_completeness": "INCOMPLETE_DO_NOT_USE_AS_REPLAY_OR_RELEASE_EVIDENCE",
        "archive_sha256": hashlib.sha256((directory / "eur-real-report-after-restart.json").read_bytes()).hexdigest(),
        "restored_record_ids": [result["cycle_id"]], "broker_mutations": 0,
    }
    rows.append(["recovery", "volatile_db_recovery_20261104", "eur_demo_owner", result["account_id"],
                 canonical(receipt), digest(receipt)])
    copy_rows("target.records", ["kind", "record_id", "owner_id", "account_id", "document", "content_sha256"], rows)
    private_json(STATE / "archive-recovery.json", receipt)
    # The acknowledgement was archived before the crash. Preserve SENT rather
    # than requeueing an event that the owner already received.
    store = json.loads((ROOT / "config/credentials.local.json").read_text())["credentials"]
    chat = int(store["TRADY_TELEGRAM_WHITELIST_USER_ID"])
    recovered = []
    for delivery in report["telegram_deliveries"]:
        if delivery["status"] != "SENT" or delivery["attempt_count"] != 1:
            raise ValueError("Archived delivery requires manual ambiguity review")
        blocked = "\n".join(item["instrument_id"] + ": " + ", ".join(item["blockers"]) for item in result["candidates"])
        body = ("EUR SHADOW completed. No broker orders submitted.\nSelection blocked by input/execution evidence.\n"
                + blocked + "\nBroker flat: " + str(result["reconciled_flat"]).lower()
                + ". Source gaps: " + str(len(result["source_gaps"])) + ".")
        rendered = ("Trady DailyReport\nAccount: " + result["account_id"] + "\nSession: " + result["cycle_id"]
                    + "\n" + body + "\nEvidence: " + result["cycle_id"])
        payload = {"event_id": delivery["event_id"], "kind": "DAILY_REPORT", "account_id": result["account_id"],
                   "session_id": result["cycle_id"], "instrument_id": None, "evidence_ids": [result["cycle_id"]],
                   "text": rendered, "occurred_at": result["completed_at"]}
        recovered.append(["eur_shadow_v1", delivery["event_id"], delivery["notification_kind"], chat,
                          "eur_demo_owner", result["account_id"], digest(payload), canonical(payload), "SENT",
                          delivery["attempt_count"], delivery["sent_at"], "RECOVERED_FROM_ARCHIVED_SENT_ACK"])
    copy_rows("target.telegram_outbox", ["deployment_id", "event_id", "notification_kind", "chat_id", "owner_id",
              "account_id", "payload_hash", "payload", "status", "attempt_count", "sent_at", "last_error"], recovered)


def restore_known_x_spend():
    policy = json.loads((ROOT / "config/research.eur.json").read_text())["x_budget"]
    fields = ["enabled", "billing_scope", "version", "cycle_start", "cycle_end", "ceiling_microusd", "ordinary_microusd",
              "support_microusd", "post_read_microusd", "user_read_microusd", "recent_counts_microusd", "burst_margin_percent",
              "approval_reference", "entitlement_reference", "provider_limit_reference"]
    ordered = {name: policy[name] for name in fields}
    policy_hash = hashlib.sha256(json.dumps(ordered, separators=(",", ":")).encode()).hexdigest()
    copy_rows("research.x_budget_cycles", ["billing_scope", "cycle_start", "cycle_end", "policy_hash", "policy"],
              [[policy["billing_scope"], policy["cycle_start"], policy["cycle_end"], policy_hash, canonical(policy)]])
    plan = {"ordinary": {"posts": 0, "users": 0, "counts": 0}, "support": {"posts": 0, "users": 1, "counts": 0}}
    rows = []
    directory = ROOT / "var/eur-shadow-review/2026-11-04/provider-snapshots"
    for suffix in ("airbus", "ecb"):
        capture = json.loads((directory / ("x_" + suffix + ".json")).read_text())
        config = json.loads((directory / ("x-resolution-config-" + suffix + ".json")).read_text())
        source = config["sources"][0]
        source_hash = hashlib.sha256(json.dumps(source, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
        if capture["status"] != "OK" or capture["post_content_requested"]:
            raise ValueError("X identity archive does not prove known consumption")
        rows.append([policy["billing_scope"], policy["cycle_start"], source["id"], source_hash, canonical(plan), canonical(plan),
                     0, 10000, "OBSERVED", capture["requested_at"], capture["received_at"]])
    copy_rows("research.x_cost_reservations", ["billing_scope", "cycle_start", "source_id", "registry_hash", "resource_bound",
              "actual_resources", "ordinary_microusd", "support_microusd", "outcome", "reserved_at", "settled_at"], rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--acknowledge-volatile-loss", action="store_true", required=True)
    args = parser.parse_args()
    if not args.acknowledge_volatile_loss or STATE.exists():
        raise ValueError("Recovery is an explicit, one-time operation")
    current = json.loads(run("docker", "inspect", NAME))[0]
    if current["Config"]["Image"] != "postgres:18.3" or "/var/lib/postgresql" not in current["HostConfig"].get("Tmpfs", {}):
        raise ValueError("Refusing to replace a different or already persistent database")
    probe = run("docker", "exec", "--user", "postgres", NAME, "psql", "-X", "-At", "-U", "postgres", "-d", "postgres",
                "-c", "SELECT count(*) FROM pg_database WHERE datname='trady'")
    if probe.strip() != b"0":
        raise ValueError("A surviving database must be backed up and migrated rather than reconstructed")
    private = ROOT / "config/credentials.local.json"
    if private.stat().st_mode & 0o077:
        raise ValueError("Central credential permissions are insecure")
    values = json.loads(private.read_text())["credentials"]
    passwords = {}
    for key, role in ROLES.items():
        fields = dict(item.split("=", 1) for item in shlex.split(values[key]))
        if fields.get("host") not in ("127.0.0.1", "localhost") or fields.get("port") != "32811":
            if key in ("TRADY_MIGRATOR_DSN", "TRADY_ANALYST_DSN", "TRADY_EXECUTOR_DSN", "TRADY_GATEWAY_DSN"):
                raise ValueError("Unexpected core application database credential scope")
            # A separately hosted lab is outside this recovery. Do not touch its
            # database, credentials or local role password.
            continue
        if fields.get("user") != role or fields.get("dbname") != "trady":
            raise ValueError("Unexpected application database credential scope")
        password = fields.get("password", "")
        if not password or "\n" in password or "\r" in password:
            raise ValueError("Unusable application database credential")
        passwords[role] = password
    for unit in ("executor", "gateway", "analyst"):
        run("systemctl", "stop", "trady-eur-" + unit + ".service")
    STATE.mkdir(parents=True, mode=0o700)
    private_json(STATE / "incident.json", {"kind": "VOLATILE_POSTGRESQL_DATA_LOSS", "container": NAME,
        "old_storage": "tmpfs", "old_host_port": "ephemeral", "full_backup_found": False,
        "unarchived_prior_history_recoverable": False, "new_volume": VOLUME, "new_loopback_port": 32811,
        "recorded_at": datetime.now(timezone.utc).isoformat(), "broker_mutations": 0})
    hba = Path("/etc/trady/postgres/pg_hba.conf")
    hba.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "services/runtime/postgres/pg_hba.conf", hba)
    hba.chmod(0o644)
    run("docker", "stop", NAME)
    run("docker", "rename", NAME, RETIRED)
    run("docker", "volume", "create", VOLUME)
    with tempfile.TemporaryDirectory(prefix="trady-db-recovery-", dir="/run") as temporary:
        environment = Path(temporary) / "postgres.env"
        environment.write_text("POSTGRES_PASSWORD=" + secrets.token_urlsafe(48) + "\nPGDATA=/var/lib/postgresql/18/docker\n")
        environment.chmod(0o600)
        run("docker", "create", "--name", NAME, "--restart", "unless-stopped", "--publish", "127.0.0.1:32811:5432",
            "--mount", "type=volume,source=" + VOLUME + ",target=/var/lib/postgresql",
            "--mount", "type=bind,source=" + str(hba) + ",target=/etc/trady-pg_hba.conf,readonly",
            "--env-file", str(environment), current["Image"], "-c", "hba_file=/etc/trady-pg_hba.conf")
    run("docker", "start", NAME)
    for _ in range(150):
        ready = subprocess.run(["docker", "exec", "--user", "postgres", NAME, "pg_isready", "-h", "127.0.0.1", "-U", "postgres"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if ready.returncode == 0:
            break
        time.sleep(0.2)
    else:
        raise ValueError("Persistent database did not become ready")
    sql((ROOT / "migrations/0000_roles.sql").read_text() + (ROOT / "migrations/0000_lab_roles.sql").read_text()
        + "\nCREATE DATABASE trady OWNER trady_owner TEMPLATE template0;\n", "postgres")
    for role, password in passwords.items():
        run("docker", "exec", "-i", "--user", "postgres", NAME, "psql", "-X", "-U", "postgres", "-d", "postgres", "-c",
            "\\password " + role, data=(password + "\n" + password + "\n").encode())
    run(str(ROOT / "target/debug/trady"), "migrate")
    restore_archived_shadow()
    restore_known_x_spend()
    for filename, command in [("trady.dump", ["pg_dump", "-U", "postgres", "-d", "trady", "-Fc"]),
                              ("globals.sql", ["pg_dumpall", "-U", "postgres", "--globals-only"])]:
        destination = STATE / filename
        descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "wb") as output:
            run("docker", "exec", "--user", "postgres", NAME, *command, stdout=output)
    print(json.dumps({"status": "PERSISTENT_DATABASE_REBUILT_FROM_PARTIAL_ARCHIVES", "volume": VOLUME,
                      "loopback_port": 32811, "prior_unarchived_history_restored": False,
                      "acknowledged_delivery_restored_as_sent": True, "known_x_spend_restored_microusd": 20000,
                      "backup_directory": str(STATE), "services_started": False, "broker_mutations": 0}))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.SubprocessError):
        print("Trady database recovery stopped; inspect private recovery state. No credentials were printed.", file=sys.stderr)
        sys.exit(1)

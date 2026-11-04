#!/usr/bin/env python3
"""Back up only the persistent Trady demo database; optionally verify restart/restore.

Refuses tmpfs data, a different image or a changing/public endpoint. Backup data
and role hashes are root-only; diagnostics contain only counts and hashes.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

NAME = "trady-live-demo-db-20261031"
VOLUME = "trady-eur-postgres18-data"
DESTINATION = Path("/var/lib/trady-eur-backups")
FOOTPRINT = """SELECT jsonb_build_object(
 'record_count',(SELECT count(*) FROM target.records),
 'record_hash',(SELECT encode(sha256(convert_to(COALESCE(string_agg(kind||':'||record_id||':'||content_sha256,'|' ORDER BY kind,record_id),''),'UTF8')),'hex') FROM target.records),
 'outbox_count',(SELECT count(*) FROM target.telegram_outbox),
 'outbox_hash',(SELECT encode(sha256(convert_to(COALESCE(string_agg(event_id||':'||status||':'||payload_hash||':'||attempt_count::text,'|' ORDER BY event_id,notification_kind,chat_id),''),'UTF8')),'hex') FROM target.telegram_outbox),
 'source_items',(SELECT count(*) FROM research.source_items),
 'known_x_spend',(SELECT COALESCE(sum(ordinary_microusd+support_microusd),0) FROM research.x_cost_reservations)
);"""


def run(*args, data=None, stdin=None, stdout=None):
    return subprocess.run(args, input=data, stdin=stdin, stdout=stdout or subprocess.PIPE,
                          stderr=subprocess.PIPE, check=True).stdout


def inspect_persistence():
    container = json.loads(run("docker", "inspect", NAME))[0]
    if container["Config"]["Image"] != "postgres:18.3" and not container["Config"]["Image"].startswith("sha256:"):
        raise ValueError("UNEXPECTED_DATABASE_IMAGE")
    if container["HostConfig"].get("Tmpfs"):
        raise ValueError("VOLATILE_DATABASE_MOUNT_REFUSED")
    mounts = container["Mounts"]
    if not any(item["Type"] == "volume" and item.get("Name") == VOLUME
               and item["Destination"] == "/var/lib/postgresql" for item in mounts):
        raise ValueError("PERSISTENT_DATABASE_VOLUME_NOT_VERIFIED")
    if run("docker", "port", NAME, "5432/tcp").strip() != b"127.0.0.1:32811":
        raise ValueError("DATABASE_ENDPOINT_NOT_PINNED")
    return container["Image"]


def footprint(name=NAME):
    return json.loads(run("docker", "exec", "--user", "postgres", name, "psql", "-X", "-At", "-U", "postgres",
                          "-d", "trady", "-c", FOOTPRINT))


def ready(name):
    for _ in range(150):
        result = subprocess.run(["docker", "exec", "--user", "postgres", name, "pg_isready", "-h", "127.0.0.1", "-U", "postgres"],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if result.returncode == 0:
            return
        time.sleep(0.2)
    raise ValueError("DATABASE_READINESS_TIMEOUT")


def verify_restore(directory, image, expected):
    test_name = "trady-backup-restore-test-" + str(os.getpid())
    try:
        # Deliberately disposable and network-isolated: this is not the runtime
        # database, and its tmpfs contents are never claimed as durable state.
        run("docker", "run", "--detach", "--name", test_name, "--network", "none", "--tmpfs", "/var/lib/postgresql",
            "--env", "POSTGRES_HOST_AUTH_METHOD=trust", image)
        ready(test_name)
        globals_sql = (directory / "globals.sql").read_text()
        if globals_sql.count("CREATE ROLE postgres;") != 1:
            raise ValueError("UNEXPECTED_GLOBALS_BACKUP_FORMAT")
        globals_sql = globals_sql.replace("CREATE ROLE postgres;", "", 1)
        run("docker", "exec", "-i", "--user", "postgres", test_name, "psql", "-X", "-q", "-v", "ON_ERROR_STOP=1",
            "-U", "postgres", "-d", "postgres", data=globals_sql.encode())
        run("docker", "exec", "--user", "postgres", test_name, "createdb", "-U", "postgres", "--owner=trady_owner", "trady")
        with (directory / "trady.dump").open("rb") as source:
            run("docker", "exec", "-i", "--user", "postgres", test_name, "pg_restore", "--exit-on-error", "-U", "postgres",
                "--dbname=trady", stdin=source)
        if footprint(test_name) != expected:
            raise ValueError("RESTORED_DATABASE_FOOTPRINT_DIFFERS")
    finally:
        subprocess.run(["docker", "rm", "--force", test_name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify-restore", action="store_true")
    parser.add_argument("--restart-after-backup", action="store_true")
    args = parser.parse_args()
    image = inspect_persistence()
    before = footprint()
    DESTINATION.mkdir(parents=True, exist_ok=True, mode=0o700)
    if DESTINATION.stat().st_mode & 0o077:
        raise ValueError("BACKUP_DIRECTORY_PERMISSIONS_UNSAFE")
    directory = DESTINATION / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    directory.mkdir(mode=0o700)
    manifest = {"schema_version": 1, "container": NAME, "volume": VOLUME, "endpoint": "127.0.0.1:32811",
                "database_footprint": before, "files": {}, "full_restore_verified": False, "restart_verified": False}
    for filename, command in [("trady.dump", ["pg_dump", "-U", "postgres", "-d", "trady", "-Fc"]),
                              ("globals.sql", ["pg_dumpall", "-U", "postgres", "--globals-only"])]:
        path = directory / filename
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "wb") as output:
            run("docker", "exec", "--user", "postgres", NAME, *command, stdout=output)
            output.flush()
            os.fsync(output.fileno())
        manifest["files"][filename] = {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    with (directory / "trady.dump").open("rb") as source:
        run("docker", "exec", "-i", "--user", "postgres", NAME, "pg_restore", "--list", stdin=source, stdout=subprocess.DEVNULL)
    if args.verify_restore:
        verify_restore(directory, image, before)
        manifest["full_restore_verified"] = True
    if args.restart_after_backup:
        inspect_persistence()
        run("docker", "restart", NAME)
        ready(NAME)
        inspect_persistence()
        if footprint() != before:
            raise ValueError("POST_RESTART_DATABASE_FOOTPRINT_DIFFERS")
        manifest["restart_verified"] = True
    descriptor = os.open(directory / "manifest.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as output:
        json.dump(manifest, output, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    print(json.dumps({"status": "PERSISTENT_DATABASE_BACKUP_VERIFIED", "manifest": str(directory / "manifest.json"),
                      "full_restore_verified": manifest["full_restore_verified"], "restart_verified": manifest["restart_verified"],
                      "database_footprint": before}))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        reason = str(error) if isinstance(error, ValueError) else "PRIVATE_BACKUP_OPERATION_FAILED"
        print("Trady backup verification stopped: " + reason, file=sys.stderr)
        sys.exit(1)

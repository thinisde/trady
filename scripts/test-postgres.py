#!/usr/bin/env python3
"""Run database checks on a new disposable PostgreSQL container only."""
import os
import json
from pathlib import Path
import subprocess
import time
import secrets
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
NAME = f"trady-foundation-test-{os.getpid()}"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def sql(text, database="postgres"):
    run("docker", "exec", "-i", NAME, "psql", "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", database, input=text, stdout=subprocess.DEVNULL)


def rotation_check(child_env):
    keys = ["TRADY_ANALYST_DSN", "TRADY_EXECUTOR_DSN", "TRADY_GATEWAY_DSN",
            "TRADY_MIGRATOR_DSN", "TRADY_LAB_DSN", "TRADY_VERIFIER_DSN", "TRADY_SHADOW_DSN",
            "TRADY_RELEASE_OWNER_DSN", "TRADY_RELEASE_DSN"]
    with tempfile.TemporaryDirectory(prefix="trady-rotation-test-") as directory:
        path = Path(directory) / "credentials.json"
        path.touch(mode=0o600)
        path.write_text(json.dumps({"config_version": 1,
                                    "credentials": {key: child_env[key] for key in keys}}))
        command = [sys.executable, str(ROOT / "scripts/rotate-postgres-credentials.py"),
                   "--container", NAME, "--credentials", str(path)]
        planned = json.loads(run(*command, capture_output=True).stdout)
        assert planned["status"] == "rotation_plan_checked" and planned["dsn_endpoints_verified"]
        applied = json.loads(run(*command, "--apply", capture_output=True).stdout)
        assert applied["status"] == "rotated_and_verified" and applied["scram_roles_verified"] == 9
        child_env.update(json.loads(path.read_text())["credentials"])
        for service in ["analyst", "executor", "gateway"]:
            run("cargo", "run", "--locked", "--quiet", "--bin", "trady", "--", service, "--once",
                cwd=ROOT, env=child_env)
    print("Disposable credential plan, rotation and published-endpoint login checks passed.")


def main():
    started = False
    try:
        run("docker", "run", "--rm", "--detach", "--name", NAME,
            "--publish", "127.0.0.1::5432", "--tmpfs", "/var/lib/postgresql",
            "--mount", f"type=bind,src={ROOT / 'config/dev-pg_hba.conf'},dst=/etc/trady-pg_hba.conf,readonly",
            "--env", "POSTGRES_HOST_AUTH_METHOD=trust", "postgres:18.3",
            "-c", "hba_file=/etc/trady-pg_hba.conf", stdout=subprocess.DEVNULL)
        started = True
        for _ in range(150):
            ready = subprocess.run(["docker", "exec", NAME, "pg_isready", "-U", "postgres", "-d", "postgres"], capture_output=True)
            if ready.returncode == 0:
                # The entrypoint also starts a temporary server; wait for its final TCP listener.
                tcp = subprocess.run(["docker", "exec", NAME, "pg_isready", "-h", "127.0.0.1", "-U", "postgres"], capture_output=True)
                if tcp.returncode == 0:
                    break
            time.sleep(0.2)
        else:
            raise RuntimeError("disposable PostgreSQL did not become ready")
        port = run("docker", "port", NAME, "5432/tcp", capture_output=True).stdout.strip().rsplit(":", 1)[1]
        sql((ROOT / "migrations/0000_roles.sql").read_text())
        sql((ROOT / "migrations/0000_lab_roles.sql").read_text())
        sql("CREATE DATABASE trady OWNER trady_owner TEMPLATE template0;")
        sql("CREATE ROLE paperless_fixture_owner LOGIN;" )
        sql("CREATE DATABASE paperless_fixture OWNER paperless_fixture_owner TEMPLATE template0;")
        child_env = os.environ.copy()
        # Cleanup tests intentionally sweep every scope in this shared database.
        # Each concurrency regression interleaves its own connections explicitly.
        child_env.update(RUST_TEST_THREADS="1", TRADY_TEST_ISOLATED="1", TRADY_TEST_PORT=port,
                         TRADY_MIGRATOR_DSN=f"host=127.0.0.1 port={port} user=trady_migrator dbname=trady",
                         TRADY_EXECUTOR_DSN=f"host=127.0.0.1 port={port} user=trady_executor dbname=trady",
                         TRADY_ANALYST_DSN=f"host=127.0.0.1 port={port} user=trady_analyst dbname=trady",
                          TRADY_GATEWAY_DSN=f"host=127.0.0.1 port={port} user=trady_gateway dbname=trady",
                          TRADY_LAB_DSN=f"host=127.0.0.1 port={port} user=trady_lab dbname=trady",
                          TRADY_VERIFIER_DSN=f"host=127.0.0.1 port={port} user=trady_verifier dbname=trady",
                          TRADY_SHADOW_DSN=f"host=127.0.0.1 port={port} user=trady_shadow dbname=trady",
                          TRADY_RELEASE_DSN=f"host=127.0.0.1 port={port} user=trady_releaser dbname=trady",
                          TRADY_RELEASE_OWNER_DSN=f"host=127.0.0.1 port={port} user=trady_release_owner dbname=trady")
        for key in ["TRADY_MIGRATOR_DSN", "TRADY_EXECUTOR_DSN", "TRADY_ANALYST_DSN",
                    "TRADY_GATEWAY_DSN", "TRADY_LAB_DSN", "TRADY_VERIFIER_DSN",
                    "TRADY_SHADOW_DSN", "TRADY_RELEASE_DSN", "TRADY_RELEASE_OWNER_DSN"]:
            dsn = child_env[key]
            role = dsn.split("user=", 1)[1].split()[0]
            password = secrets.token_urlsafe(48)
            run("docker", "exec", "-i", NAME, "psql", "-X", "-v", "ON_ERROR_STOP=1",
                "-U", "postgres", "-d", "postgres", "-c", "\\password " + role,
                input=password + "\n" + password + "\n", stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL)
            child_env[key] = dsn + " password='" + password + "'"
        run("cargo", "test", "--locked", "-p", "trady-storage", "--tests", "--", "--ignored", "--nocapture", cwd=ROOT, env=child_env)
        run("cargo", "test", "--locked", "-p", "trady-cli", "--test", "opencode", "--", "--ignored", "--nocapture", cwd=ROOT, env=child_env)
        run("cargo", "test", "--locked", "-p", "trady-cli", "--test", "autonomous_cli", "--", "--ignored", "--nocapture", cwd=ROOT, env=child_env)
        run("cargo", "test", "--locked", "-p", "trady-cli", "--test", "telegram_jobs_cli", "--", "--ignored", "--nocapture", cwd=ROOT, env=child_env)
        for service in ["analyst", "executor", "gateway"]:
            run("cargo", "run", "--locked", "--quiet", "--bin", "trady", "--", service, "--once", cwd=ROOT, env=child_env)
        run("cargo", "run", "--locked", "--quiet", "--bin", "trady", "--", "demo-postgres", cwd=ROOT, env=child_env)
        run("cargo", "run", "--locked", "--quiet", "--bin", "trady", "--", "news", "collect", "--once", cwd=ROOT, env=child_env)
        run("cargo", "run", "--locked", "--quiet", "--bin", "trady", "--", "news", "read", "--limit", "5", "--for-model", cwd=ROOT, env=child_env)
        rotation_check(child_env)
        print("Disposable PostgreSQL integration and CLI checks passed.")
    finally:
        if started:
            subprocess.run(["docker", "rm", "--force", NAME], check=False, stdout=subprocess.DEVNULL)


if __name__ == "__main__":
    main()

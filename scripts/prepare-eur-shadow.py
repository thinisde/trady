#!/usr/bin/env python3
"""Prepare the isolated EUR SHADOW deployment; activation never enables orders.

Default output is disabled. --activate-shadow requires the recorded owner grant.
This copies public artifacts and role-scoped credential files, but does not start
services, run migrations, submit orders or purchase provider subscriptions.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_FILES = [
    "config/yfinance.eur-shadow.reviewed.json", "config/research.eur.json",
    "config/trading212.eur-demo.example.json", "config/opencode/llm.json",
    "docs/evidence/eur-shadow-review-2026-11-04/crosswalk.json",
    "docs/evidence/eur-shadow-review-2026-11-04/review.json",
    "docs/evidence/eur-demo-delegation-2026-11-04.json",
]
SCOPES = {
    "analyst": ["TRADY_ANALYST_DSN", "TRADY_ALPHA_VANTAGE_API_KEY", "TRADY_X_BEARER_TOKEN", "OPENCODE_API_KEY"],
    "executor": ["TRADY_EXECUTOR_DSN", "TRADY_T212_DEMO_API_KEY", "TRADY_T212_DEMO_API_SECRET", "TRADY_TELEGRAM_WHITELIST_USER_ID"],
    "gateway": ["TRADY_GATEWAY_DSN", "TRADY_TELEGRAM_BOT_TOKEN", "TRADY_TELEGRAM_WHITELIST_USER_ID"],
}


def write_new(path, value, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "w") as output:
        json.dump(value, output, indent=2)
        output.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--activate-shadow", action="store_true")
    parser.add_argument("--install-units", action="store_true")
    args = parser.parse_args()
    target = Path("/etc/trady/runtime/eur-shadow.json")
    if target.exists():
        raise ValueError("Existing EUR runtime configuration will not be overwritten")
    grant = json.loads((ROOT / "docs/evidence/eur-demo-delegation-2026-11-04.json").read_text())
    if args.activate_shadow and not (grant["broker_environment"] == "demo" and grant["broker_account_id"] == 52211463
                                    and grant["live_enabled"] is False):
        raise ValueError("Recorded demo/SHADOW delegation is invalid")
    source = Path(os.environ.get("TRADY_CREDENTIALS_FILE", str(ROOT / "config/credentials.local.json")))
    if not source.is_file() or source.stat().st_size > 262144 or stat.S_IMODE(source.stat().st_mode) & 0o077:
        raise ValueError("Private central credential store is unavailable or insecure")
    credentials = json.loads(source.read_text())["credentials"]
    scopes = {role: {name: os.environ.get(name, credentials.get(name, "")) for name in names}
              for role, names in SCOPES.items()}
    if any(not value for values in scopes.values() for value in values.values()):
        raise ValueError("Required scoped credential is unavailable")
    # Validate and plan all file creation before touching the installed deployment.
    if any((Path("/etc/trady/secrets") / (role + ".json")).exists() for role in SCOPES):
        raise ValueError("Existing service credential files will not be overwritten")
    binary = ROOT / "target/debug/trady"
    if not binary.is_file():
        raise ValueError("Build the verified runtime binary before preparation")
    for relative in PUBLIC_FILES:
        destination = Path("/opt/trady") / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists() and destination.read_bytes() != (ROOT / relative).read_bytes():
            raise ValueError("An installed public artifact differs; use a reviewed deployment update")
        if not destination.exists():
            shutil.copyfile(ROOT / relative, destination)
            destination.chmod(0o644)
    Path("/opt/trady/bin").mkdir(parents=True, exist_ok=True)
    if Path("/opt/trady/bin/trady").exists():
        raise ValueError("An installed runtime binary will not be overwritten")
    shutil.copyfile(binary, "/opt/trady/bin/trady")
    Path("/opt/trady/bin/trady").chmod(0o755)
    config = json.loads((ROOT / "config/eur-shadow.example.json").read_text())
    config["enabled"] = args.activate_shadow
    for name in ("yfinance_config", "research_config", "trading212_config", "opencode_config"):
        config[name] = str(Path("/opt/trady") / config[name])
    config["shadow"]["crosswalk_config"] = str(Path("/opt/trady") / config["shadow"]["crosswalk_config"])
    Path("/etc/trady/secrets").mkdir(parents=True, exist_ok=True, mode=0o700)
    for role, values in scopes.items():
        write_new(Path("/etc/trady/secrets") / (role + ".json"), {"config_version": 1, "credentials": values}, 0o600)
    write_new(target, config, 0o644)
    Path("/var/lib/trady-eur-shadow").mkdir(parents=True, exist_ok=True, mode=0o750)
    if args.install_units:
        for role in SCOPES:
            account = "trady-" + role
            if subprocess.run(["id", "-u", account], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
                subprocess.run(["useradd", "--system", "--user-group", "--no-create-home", "--shell", "/usr/sbin/nologin", account], check=True)
            unit = "trady-eur-" + role + ".service"
            destination = Path("/etc/systemd/system") / unit
            if destination.exists():
                raise ValueError("Existing EUR service unit will not be overwritten")
            shutil.copyfile(ROOT / "services/runtime" / unit, destination)
            destination.chmod(0o644)
        subprocess.run(["systemd-analyze", "verify", *[str(Path("/etc/systemd/system") / ("trady-eur-" + role + ".service")) for role in SCOPES]], check=True)
        subprocess.run(["systemctl", "daemon-reload"], check=True)
    print(json.dumps({"status": "EUR_SHADOW_PREPARED", "config": str(target), "enabled": config["enabled"],
                      "mode": "SHADOW", "services_started": False, "broker_mutations": 0, "live_enabled": False}))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.SubprocessError):
        print("EUR SHADOW preparation failed; existing deployment and credentials were not exposed.", file=sys.stderr)
        sys.exit(1)

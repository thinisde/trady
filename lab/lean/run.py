#!/usr/bin/python3
"""Launch a pinned offline LEAN engine, return measured development folds.

The engine must run in the isolated lab container, with no Docker socket,
network, paid data download, broker secret or writable sealed dataset. All paths
come from a root-owned fixed lock, never from the candidate request.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import sys
from decimal import Decimal, localcontext

MAX = 4 * 1024 * 1024


def checked_pin(item):
    path = pathlib.Path(item["path"])
    if not path.is_absolute() or path.is_symlink() or not path.is_file():
        raise ValueError("absolute non-symlink engine/data pin required")
    stat = path.stat()
    if stat.st_uid != 0 or stat.st_mode & 0o022:
        raise ValueError("engine/data pin is not deployment owned")
    if hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]:
        raise ValueError("pinned engine/data artifact changed")
    return path


def fold_metrics(report, interval, expected_samples):
    rows = report["equity"]
    if report["data_kind"] != "REAL" or len(rows) != expected_samples or len(rows) < 2:
        raise ValueError("incomplete real exchange-session valuations")
    values = [Decimal(row["equity"]) for row in rows]
    if any(not x.is_finite() or x <= 0 for x in values):
        raise ValueError("undefined equity return")
    if rows[0]["at"] != interval["opens_at"] or rows[-1]["at"] != interval["closes_at"]:
        raise ValueError("valuation boundaries differ from frozen folds")
    with localcontext() as context:
        context.prec = 28
        returns = [(values[i] / values[i - 1]).ln() for i in range(1, len(values))]
        peak = values[0]
        drawdown = Decimal(0)
        for value in values:
            peak = max(peak, value)
            drawdown = max(drawdown, 1 - value / peak)
        return {"interval": interval, "mean_log_return": str(sum(returns) / len(returns)),
                "max_drawdown": str(drawdown), "closed_trades": report["closed_trades"], "complete": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--lock", type=pathlib.Path, required=True)
    args = parser.parse_args()
    lock = json.loads(args.lock.read_text())
    request = json.loads(sys.stdin.buffer.read(MAX + 1))
    if request["stage"] != "develop" or lock["mode"] != "OFFLINE":
        raise ValueError("development-only offline driver")
    executable = checked_pin(lock["engine_executable"])
    for pin in lock["dependencies"]:
        checked_pin(pin)
    if lock["dataset_hash"] != request["manifest"]["dataset_hash"]:
        raise ValueError("dataset differs from frozen run")
    prepared = lock["trials"].get(request["input_hash"])
    if prepared is None:
        raise ValueError("engine job was not prepared for the frozen trial")
    folds = []
    if len(prepared["jobs"]) != len(request["plan"]["development_folds"]):
        raise ValueError("development folds incomplete")
    for job, interval in zip(prepared["jobs"], request["plan"]["development_folds"], strict=True):
        output = pathlib.Path(job["output"])
        # Existing output is never mistaken for this execution's result.
        if output.exists():
            raise ValueError("engine output already exists; use a fresh prepared job")
        result = subprocess.run([str(executable), *job["arguments"]], cwd=job["working_directory"],
                                stdin=subprocess.DEVNULL, capture_output=True, timeout=lock["timeout_seconds"],
                                env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "DOTNET_CLI_TELEMETRY_OPTOUT": "1"}, check=False)
        if result.returncode or len(result.stdout) + len(result.stderr) > MAX:
            raise RuntimeError("offline LEAN engine failed")
        if not output.is_file() or output.stat().st_size > MAX:
            raise ValueError("real LEAN output unavailable")
        report = json.loads(output.read_text())
        folds.append(fold_metrics(report, interval, request["plan"]["development_fold_sessions"] + 1))
    print(json.dumps({"schema_version": 1, "input_hash": request["input_hash"], "payload": {"folds": folds}}, separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("offline LEAN data, engine, configuration or measurements unavailable", file=sys.stderr)
        sys.exit(1)

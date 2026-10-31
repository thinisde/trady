#!/usr/bin/python3
"""Execute exact model/production fixtures. Passing is finite evidence only."""
import argparse
import json
import importlib.util
import pathlib
import re
import subprocess
from collections import Counter
from fixtures import fixtures
from bindings import SCOPE, sha

MAX = 4 * 1024 * 1024
TIME_FIELDS = {"as_of", "observed_at", "received_at", "last_completed_at", "available_at",
               "valid_until", "holding_deadline_at", "calculated_at"}


def timestamp(text):
    match = re.fullmatch(r"([+-]?\d{4,})-(\d\d)-(\d\d)T(\d\d):(\d\d):(\d\d)(?:\.(\d{1,9}))?(Z|[+-]\d\d:\d\d)", text)
    if not match:
        raise ValueError("fixture timestamp syntax unsupported")
    year, month, day, hour, minute, sec = map(int, match.groups()[:6])
    year -= month <= 2
    era = year // 400
    yoe = year - era * 400
    doy = (153 * (month + (-3 if month > 2 else 9)) + 2) // 5 + day - 1
    days = era * 146097 + yoe * 365 + yoe // 4 - yoe // 100 + doy - 719468
    fraction = int((match[7] or "").ljust(9, "0"))
    if sec == 60:
        sec = 59
        fraction += 10**9
    offset = match[8]
    offset_seconds = 0 if offset == "Z" else (1 if offset[0] == "+" else -1) * (int(offset[1:3]) * 3600 + int(offset[4:6]) * 60)
    return (((days * 24 + hour) * 3600 + minute * 60 + sec) - offset_seconds) * (2 * 10**9) + fraction


def transport(value):
    if isinstance(value, dict):
        return {key: timestamp(item) if key in TIME_FIELDS and isinstance(item, str) else transport(item)
                for key, item in value.items()}
    if isinstance(value, list):
        return [transport(item) for item in value]
    return value


def execute(argv, data):
    result = subprocess.run([str(arg) for arg in argv], input=json.dumps(data, separators=(",", ":")) + "\n",
                            text=True, capture_output=True, timeout=30, check=False,
                            env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"})
    if result.returncode or len(result.stdout) + len(result.stderr) > MAX:
        raise RuntimeError(f"bridge failed ({result.returncode}): {result.stderr[:200]}")
    return json.loads(result.stdout)


def run_fixtures(project, rust, extra=(), only=None):
    adapter_path = project.parents[1] / "lab/lean/bridge.py"
    specification = importlib.util.spec_from_file_location("trady_pinned_simulation_bridge", adapter_path)
    if specification is None or specification.loader is None:
        raise ValueError("simulation adapter unavailable")
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    adapter = module.RustFormulaBridge(str(rust), sha(rust))
    results = []
    for case in [*fixtures(), *extra] if only is None else only:
        rust_result = transport(execute([rust, "weekly", "formula-bridge", "--file", "-"], case.request))
        lean_result = execute([project / ".lake/build/bin/formulaParity"], transport(case.request))
        adapter_result = transport(adapter.fixture(case.request))
        if (case.status is not None and rust_result.get("status") != case.status) or (case.error is not None and rust_result.get("error") != case.error):
            raise ValueError(f"fixture {case.name} reached an unexpected production boundary: {rust_result}")
        if rust_result != lean_result:
            raise ValueError(f"fixture {case.name} differs: rust={rust_result}, lean={lean_result}")
        if adapter_result != lean_result:
            raise ValueError(f"simulation adapter fixture {case.name} differs from the reference")
        results.append({"name": case.name, "rust": rust_result, "lean": lean_result, "adapter": adapter_result})
    return results


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parent)
    parser.add_argument("--rust", type=pathlib.Path, required=True)
    args = parser.parse_args()
    results = run_fixtures(args.project, args.rust)
    print(json.dumps({"scope": SCOPE, "exact_numeric_parity": True,
                      "case_count": len(results), "outcomes": dict(Counter(case["rust"]["status"] for case in results)),
                      "adapter_case_count": len(results),
                      "rejection_messages": sorted({case["rust"]["error"] for case in results if case["rust"]["status"] == "REJECTED"}),
                      "trust_boundary": "finite parity reduces translation risk; it is not a formal refinement proof"}, sort_keys=True))


if __name__ == "__main__":
    main()

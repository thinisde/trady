#!/usr/bin/python3
"""Execute exact model/production fixtures. Passing is finite evidence only."""
import argparse
import json
import importlib.util
import pathlib
import re
import subprocess
import sys
from collections import Counter
from fixtures import UNSIGNED_FIELDS, baseline, changed, fixtures, malformed_fixtures
from bindings import SCOPE, sha, strict_json

MAX = 4 * 1024 * 1024
TIME_FIELDS = {"as_of", "observed_at", "received_at", "last_completed_at", "available_at",
               "valid_until", "holding_deadline_at", "calculated_at"}


def timestamp(text):
    if not isinstance(text, str):
        raise ValueError("fixture timestamp must be a string")
    match = re.fullmatch(r"([+-]?\d{4,})-(\d\d)-(\d\d)[Tt ](\d\d):(\d\d):(\d\d)(?:\.(\d+))?([Zz]|[+-]\d\d:\d\d)", text, re.ASCII)
    if not match:
        raise ValueError("fixture timestamp syntax unsupported")
    year, month, day, hour, minute, sec = map(int, match.groups()[:6])
    if not -262143 <= year <= 262142 or not 1 <= month <= 12:
        raise ValueError("fixture date out of range")
    leap_year = year % 4 == 0 and (year % 100 != 0 or year % 400 == 0)
    month_days = [31, 29 if leap_year else 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    if not 1 <= day <= month_days[month - 1] or not (0 <= hour <= 23 and 0 <= minute <= 59 and 0 <= sec <= 60):
        raise ValueError("invalid fixture calendar date or time")
    year -= month <= 2
    era = year // 400
    yoe = year - era * 400
    doy = (153 * (month + (-3 if month > 2 else 9)) + 2) // 5 + day - 1
    days = era * 146097 + yoe * 365 + yoe // 4 - yoe // 100 + doy - 719468
    # Chrono consumes additional fractional digits and truncates after nine.
    fraction = int((match[7] or "")[:9].ljust(9, "0"))
    if sec == 60:
        sec = 59
        fraction += 10**9
    offset = match[8]
    if offset.upper() == "Z":
        offset_seconds = 0
    else:
        if int(offset[1:3]) > 23 or int(offset[4:6]) > 59:
            raise ValueError("fixture timestamp offset out of range")
        offset_seconds = (1 if offset[0] == "+" else -1) * (int(offset[1:3]) * 3600 + int(offset[4:6]) * 60)
    utc_seconds = ((days * 24 + hour) * 3600 + minute * 60 + sec) - offset_seconds
    # Check Chrono's seconds/fraction pair, including its final represented leap
    # second. Positive duration addition has a separate ordinary-result bound.
    if not -8334601228800 <= utc_seconds <= 8210266876799:
        raise ValueError("fixture timestamp UTC value out of range")
    return utc_seconds * (2 * 10**9) + fraction


def transport(value, *, malformed_timestamp=False):
    if isinstance(value, dict):
        result = {}
        for key, item in value.items():
            if key in TIME_FIELDS:
                try:
                    result[key] = timestamp(item)
                except ValueError:
                    if not malformed_timestamp or not isinstance(item, str):
                        raise
                    # Negative fixtures send the malformed string to the actual
                    # Lean decoder; a Python exception never counts as parity.
                    result[key] = item
            else:
                result[key] = transport(item, malformed_timestamp=malformed_timestamp)
        return result
    if isinstance(value, list):
        return [transport(item, malformed_timestamp=malformed_timestamp) for item in value]
    return value


def invoke(argv, data):
    text = data if isinstance(data, str) else json.dumps(data, separators=(",", ":"), allow_nan=False)
    result = subprocess.run([str(arg) for arg in argv], input=text + "\n",
                            text=True, capture_output=True, timeout=30, check=False,
                            env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"})
    if len(result.stdout.encode()) + len(result.stderr.encode()) > MAX:
        raise RuntimeError("bridge output limit exceeded")
    return result


def execute(argv, data):
    result = invoke(argv, data)
    if result.returncode:
        raise RuntimeError(f"bridge failed ({result.returncode}): {result.stderr[:200]}")
    return strict_json(result.stdout)


def execute_parse_rejection(argv, data, *, diagnostic, exit_code=None, exact_diagnostic=False):
    result = invoke(argv, data)
    diagnostic_matches = result.stderr == diagnostic + "\n" if exact_diagnostic else diagnostic in result.stderr
    if (result.returncode <= 0 or (exit_code is not None and result.returncode != exit_code)
            or result.stdout or not diagnostic_matches):
        raise ValueError(f"expected parser rejection: exit={result.returncode}, stdout={result.stdout[:200]}, stderr={result.stderr[:200]}")


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
        original = case.request
        reference = transport(case.request)
        if case.wire_replacements:
            original = json.dumps(original, separators=(",", ":"))
            reference = json.dumps(reference, separators=(",", ":"))
            for source, replacement in case.wire_replacements:
                original = original.replace(source, replacement)
                reference = reference.replace(source, replacement)
        rust_result = transport(execute([rust, "weekly", "formula-bridge", "--file", "-"], original))
        lean_result = execute([project / ".lake/build/bin/formulaParity"], reference)
        adapter_result = transport(adapter.fixture_wire(original) if isinstance(original, str) else adapter.fixture(original))
        if (case.status is not None and rust_result.get("status") != case.status) or (case.error is not None and rust_result.get("error") != case.error):
            raise ValueError(f"fixture {case.name} reached an unexpected production boundary: {rust_result}")
        if rust_result != lean_result:
            raise ValueError(f"fixture {case.name} differs: rust={rust_result}, lean={lean_result}")
        if adapter_result != lean_result:
            raise ValueError(f"simulation adapter fixture {case.name} differs from the reference")
        results.append({"name": case.name, "rust": rust_result, "lean": lean_result, "adapter": adapter_result})
    return results


def malformed_wire_fixtures():
    for case in malformed_fixtures():
        yield case.name, case.request, transport(case.request, malformed_timestamp=True), "input schema mismatch", case.diagnostic
    # Mutate serialized documents so Python cannot normalize exponent forms or
    # signed zero before any of the three real parsers sees the input.
    for path, _ in UNSIGNED_FIELDS:
        request = changed(baseline(), {path: "__unsigned_lexeme__"})
        original = json.dumps(request, separators=(",", ":"))
        reference = json.dumps(transport(request), separators=(",", ":"))
        for token in ["1e0", "1E+0", "10e-1", "1.0e1", "0e0", "-0", "-0.0", "-0e0"]:
            yield ("parse/lexeme/" + path + "/" + token,
                   original.replace('"__unsigned_lexeme__"', token),
                   reference.replace('"__unsigned_lexeme__"', token),
                   "input schema mismatch", "unsigned JSON integer lexeme required")
    for name, replacement in [
        ("isolated-high", r"\ud800"), ("isolated-low", r"\udfff"),
        ("high-ordinary", r"\ud800\u0041"), ("high-high", r"\ud800\udbff"),
        ("low-high", r"\udc00\ud800"), ("high-literal", r"\ud800x"),
        ("invalid-low-hex", r"\ud83d\udexx"), ("truncated-low", r"\ud83d\u"),
    ]:
        for location in ["key", "value"]:
            source = '"snapshot_id"' if location == "key" else '"account_1"'
            mutation = lambda text: text.replace(source, '"' + replacement + '"')
            original = mutation(json.dumps(baseline(), separators=(",", ":")))
            reference = mutation(json.dumps(transport(baseline()), separators=(",", ":")))
            # The exact decoder diagnostic varies with the malformed pair.
            yield "parse/surrogate/" + location + "/" + name, original, reference, "invalid JSON or duplicate object key", "offset"
    for name, mutation, error, diagnostic in [
        ("duplicate-root", lambda s: s.replace('"kind":"FORMULA"', '"kind":"FORMULA","kind":"FORMULA"'),
         "invalid JSON or duplicate object key", "duplicate JSON key"),
        ("duplicate-nested", lambda s: s.replace('"mode":"paper"', '"mode":"paper","mode":"paper"'),
         "invalid JSON or duplicate object key", "duplicate JSON key"),
        ("duplicate-escaped", lambda s: s.replace('"mode":"paper"', '"mode":"paper","mo\\u0064e":"paper"'),
         "invalid JSON or duplicate object key", "duplicate JSON key"),
        ("duplicate-supplementary", lambda s: s.replace('"mode":"paper"', '"mode":"paper","😀":1,"\\ud83d\\ude00":2'),
         "invalid JSON or duplicate object key", "duplicate JSON key"),
        ("unsigned-plus", lambda s: s.replace('"version":1', '"version":+1'),
         "invalid JSON or duplicate object key", "unexpected input"),
        ("unsigned-leading-zero", lambda s: s.replace('"version":1', '"version":01'),
         "invalid JSON or duplicate object key", "expected"),
        ("trailing-line", lambda s: s + '\n{}', "trailing JSON data", "end of input"),
        ("broken-json", lambda s: s[:-1], "invalid JSON or duplicate object key", "end of input"),
        ("nesting-limit", lambda _: '[' * 129 + '0' + ']' * 129,
         "invalid JSON or duplicate object key", "JSON nesting limit exceeded"),
    ]:
        original = json.dumps(baseline(), separators=(",", ":"))
        reference = json.dumps(transport(baseline()), separators=(",", ":"))
        yield "parse/wire/" + name, mutation(original), mutation(reference), error, diagnostic


def run_malformed_fixtures(project, rust):
    # Verify the same commands can accept a real witness before treating any
    # nonzero process result as rejection evidence.
    run_fixtures(project, rust, only=[fixtures()[0]])
    adapter = project.parents[1] / "lab/lean/bridge.py"
    adapter_command = [sys.executable, adapter, "--fixture", "--binary", rust, "--sha256", sha(rust)]
    results = []
    for name, original, reference, rust_error, lean_error in malformed_wire_fixtures():
        try:
            execute_parse_rejection([rust, "weekly", "formula-bridge", "--file", "-"], original,
                                    diagnostic=rust_error, exit_code=1)
            execute_parse_rejection([project / ".lake/build/bin/formulaParity"], reference, diagnostic=lean_error, exit_code=2)
            execute_parse_rejection(adapter_command, original,
                                    diagnostic="LEAN formula fixture PARSE_REJECTED: " + rust_error,
                                    exit_code=2, exact_diagnostic=True)
        except (ValueError, RuntimeError) as error:
            raise ValueError(f"malformed fixture {name}: {error}") from error
        # Parser diagnostics are intentionally different. These records assert
        # observed parse rejection, not numeric/model-output equality.
        results.append({"name": name, "rust": {"status": "PARSE_REJECTED"},
                        "lean": {"status": "PARSE_REJECTED"}, "adapter": {"status": "PARSE_REJECTED"}})
    return results


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--project", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parent)
    parser.add_argument("--rust", type=pathlib.Path, required=True)
    args = parser.parse_args()
    results = run_fixtures(args.project, args.rust)
    malformed = run_malformed_fixtures(args.project, args.rust)
    print(json.dumps({"scope": SCOPE, "exact_numeric_parity": True,
                      "case_count": len(results), "outcomes": dict(Counter(case["rust"]["status"] for case in results)),
                      "adapter_case_count": len(results),
                      "malformed_input_case_count": len(malformed),
                      "rejection_messages": sorted({case["rust"]["error"] for case in results if case["rust"]["status"] == "REJECTED"}),
                      "trust_boundary": "finite parity reduces translation risk; it is not a formal refinement proof"}, sort_keys=True))


if __name__ == "__main__":
    main()

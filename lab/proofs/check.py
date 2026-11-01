#!/usr/bin/python3
"""Pinned kernel audit, revision membership/witness and exact executable parity.

No installation, network operation, caller pass flag, or cached former pass
substitutes for execution. README records the explicit proof/trust boundaries.
"""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys
from bindings import (FOUNDATIONS, SCOPE, THEOREMS, adapter_sources, proof_sources, rust_sources, safety_hash,
                      sha, strict_json, theorem_bindings, toolchain_tree, value_hash, verify_revision)
from fixtures import Fixture
from parity import run_fixtures, run_malformed_fixtures

MAX = 4 * 1024 * 1024


def execute(argv, cwd, timeout=120):
    result = subprocess.run([str(arg) for arg in argv], cwd=cwd, text=True, capture_output=True, timeout=timeout,
                            env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}, check=False)
    if result.returncode or len(result.stdout) + len(result.stderr) > MAX:
        raise RuntimeError("Lean verifier build/audit failed")
    return result.stdout


def verify_lock(project, lake, rust, lock):
    if lock["schema_version"] != 2 or lock["scope"] != SCOPE or not project.is_absolute():
        raise ValueError("unsupported proof scope, schema or project")
    for path in [lake, lake.parent / "lean", rust]:
        if not path.is_absolute() or not path.is_file() or path.is_symlink():
            raise ValueError("absolute real verifier binaries required")
    if sha(lake) != lock["lake_sha256"] or sha(lake.parent / "lean") != lock["lean_sha256"] or sha(rust) != lock["rust_sha256"]:
        raise ValueError("pinned verifier binaries changed")
    sources = proof_sources(project)
    if (sources != lock["sources"] or theorem_bindings(sources) != lock["theorem_bindings"]
            or safety_hash(sources) != lock["safety_sha256"]
            or rust_sources(project.parents[1]) != lock["rust_sources"]
            or adapter_sources(project.parents[1]) != lock["adapter_sources"]
            or toolchain_tree(lake) != lock["toolchain"]
            or not set(lock["approved_axioms"]).issubset(FOUNDATIONS)
            or value_hash(lock["revision"]) != lock["revision_sha256"]):
        raise ValueError("protected source, safety, theorem, dependency or revision binding changed")
    for name in sources:
        if name.endswith(".lean"):
            text = (project / name).read_text()
            text = re.sub(r'"(?:\\.|[^"\\])*"', '""', text)
            text = re.sub(r"--[^\n]*", "", text)
            if re.search(r"\b(sorry|admit|axiom|unsafe|partial|native_decide)\b", text):
                raise ValueError("unapproved proof declaration")


def audit_axioms(audit, approved):
    axioms = set()
    for theorem in THEOREMS:
        lines = [line for line in audit.splitlines() if line.startswith("'" + theorem + "'")]
        if len(lines) != 1:
            raise ValueError("kernel axiom audit missing or duplicated theorem")
        line = lines[0]
        if "does not depend on any axioms" not in line:
            match = re.fullmatch(r"'" + re.escape(theorem) + r"' depends on axioms: \[(.*)\]", line)
            if not match:
                raise ValueError("unrecognized kernel audit output")
            axioms.update(item.strip() for item in match[1].split(",") if item.strip())
    if not axioms.issubset(set(approved)):
        raise ValueError("unapproved kernel axiom")
    return sorted(axioms)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("stage", choices=["proof", "parity"])
    parser.add_argument("--project", type=pathlib.Path, required=True)
    parser.add_argument("--lake", type=pathlib.Path, required=True)
    parser.add_argument("--rust", type=pathlib.Path, required=True)
    parser.add_argument("--lock", type=pathlib.Path, required=True)
    args = parser.parse_args()
    data = sys.stdin.buffer.read(MAX + 1)
    if len(data) > MAX:
        raise ValueError("oversized verifier request")
    request = strict_json(data)
    lock = strict_json(args.lock.read_text())
    verify_lock(args.project, args.lake, args.rust, lock)
    verify_revision(request, lock["revision"])
    if request["manifest"]["component_hashes"]["rust_build"] != lock["rust_sha256"]:
        raise ValueError("manifest Rust build differs from executed calculator")
    version = execute([args.lake.parent / "lean", "--version"], args.project)
    if "version 4.19.0" not in version:
        raise ValueError("Lean toolchain version differs")
    toolchain_hash = hashlib.sha256(version.encode()).hexdigest()
    if request["manifest"]["component_hashes"]["lean_toolchain"] != toolchain_hash:
        raise ValueError("manifest toolchain differs from executed toolchain")
    execute([args.lake, "clean"], args.project)
    execute([args.lake, "build"], args.project)
    common = {"scope": SCOPE, "source_hash": lock["rust_sha256"], "safety_hash": lock["safety_sha256"]}
    witness = Fixture("revision/accepted-witness", lock["revision"]["request"], "ACCEPTED")
    if args.stage == "proof":
        audit = execute([args.lake, "env", "lean", "Audit.lean"], args.project)
        axioms = audit_axioms(audit, lock["approved_axioms"])
        run_fixtures(args.project, args.rust, only=[witness])
        common.update(axioms=axioms, theorems=list(THEOREMS), witness_count=2, toolchain_hash=toolchain_hash)
    else:
        cases = run_fixtures(args.project, args.rust, [witness])
        malformed = run_malformed_fixtures(args.project, args.rust)
        common["cases"] = [{"rust": case["rust"], "lean": case["lean"]} for case in [*cases, *malformed]]
    verify_lock(args.project, args.lake, args.rust, lock)
    print(json.dumps({"schema_version": 1, "input_hash": request["input_hash"], "payload": common}, separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("pinned proof/parity verification failed: " + str(error), file=sys.stderr)
        sys.exit(1)

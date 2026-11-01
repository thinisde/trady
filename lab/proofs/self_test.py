#!/usr/bin/python3
"""Offline, clean-snapshot kernel/bridge and fail-closed verifier regression.

Creates only worker-specific temporary files outside the repository. The protocol
request is a local test fixture, never release evidence for a real weekly run.
"""
import argparse
from copy import deepcopy
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
from bindings import (FOUNDATIONS, PARAMETER_FIELDS, POLICY_FIELDS, THEOREMS, adapter_sources, proof_sources, rust_sources,
                      rust_struct_hash, value_hash, verify_revision)
from check import audit_axioms
from fixtures import baseline
from parity import malformed_wire_fixtures
from pin import prepare


def rejected(action):
    try:
        action()
    except (ValueError, KeyError, TypeError):
        return
    raise AssertionError("invalid verifier input was accepted")


def build_state(project):
    state = {}
    for path in (project / ".lake/build").rglob("*"):
        if path.is_file():
            info = path.stat()
            state[str(path.relative_to(project))] = (info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)
    if not state:
        raise AssertionError("clean build artifacts are missing")
    return state


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--lake", type=pathlib.Path, required=True)
    parser.add_argument("--rust", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    project = pathlib.Path(__file__).resolve().parent
    repository = project.parents[1]
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repository, text=True).strip()
    formula = baseline()
    revision = {"schema_version": 1, "source_commit": commit, "candidate_id": "proof_fixture", "request": formula}
    negative_count = 0
    with tempfile.TemporaryDirectory(prefix="trady-proof-worker-") as temporary:
        snapshot = pathlib.Path(temporary)
        clean_project = snapshot / "lab/proofs"
        for base, files in [(repository, rust_sources(repository)), (project, proof_sources(project))]:
            for name in files:
                destination = snapshot / name if base == repository else clean_project / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(base / name, destination)
        for name in adapter_sources(repository):
            destination = snapshot / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(repository / name, destination)
        lock = prepare(args.lake, args.rust, revision, clean_project)
        migration_sources = {str(path.relative_to(repository)) for path in (repository / "migrations").glob("*.sql")
                             if path.is_file()}
        if not migration_sources or not migration_sources.issubset(lock["rust_sources"]):
            raise AssertionError("root migration/role SQL inventory is incomplete")
        lock_path = snapshot / "proof-lock.json"
        lock_path.write_text(json.dumps(lock))
        version = subprocess.check_output([str(args.lake.parent / "lean"), "--version"], text=True)
        request = {"schema_version": 1, "stage": "verify", "input_hash": value_hash(revision),
                   "run_id": "offline_fixture", "plan_hash": value_hash(formula["envelope"]),
                   "parameters_hash": rust_struct_hash(formula["parameters"], PARAMETER_FIELDS),
                   "manifest": {"source_commit": commit, "component_hashes": {
                       "policy": rust_struct_hash(formula["policy"], POLICY_FIELDS), "rust_build": lock["rust_sha256"],
                       "lean_toolchain": hashlib.sha256(version.encode()).hexdigest()}},
                   "plan": {"envelope": formula["envelope"]},
                   "trial": {"candidate_id": "proof_fixture", "parameters": formula["parameters"]}}
        arguments = [sys.executable, str(clean_project / "check.py"), "proof", "--project", str(clean_project),
                     "--lake", str(args.lake), "--rust", str(args.rust), "--lock", str(lock_path)]
        reports = {}
        envelopes = {}
        for stage in ["proof", "parity"]:
            arguments[2] = stage
            result = subprocess.run(arguments, input=json.dumps(request), text=True, capture_output=True,
                                    timeout=300, check=False)
            if result.returncode:
                raise RuntimeError("clean " + stage + " failed: " + result.stderr[:1000])
            envelopes[stage] = json.loads(result.stdout)
            reports[stage] = envelopes[stage]["payload"]
        if set(reports["proof"]["theorems"]) != set(THEOREMS):
            raise AssertionError("kernel theorem inventory differs")
        malformed_count = sum(case["rust"].get("status") == "PARSE_REJECTED" for case in reports["parity"]["cases"])
        if malformed_count != len(list(malformed_wire_fixtures())):
            raise AssertionError("malformed-input execution evidence is incomplete")
        for mutation in [
            lambda r: r["trial"].update(candidate_id="different_candidate"),
            lambda r: r["trial"]["parameters"].update(stop_atr_multiple="3"),
            lambda r: r["plan"]["envelope"].update(max_stop_atr_multiple="5"),
            lambda r: r["manifest"].update(source_commit="0" * 40),
            lambda r: r["manifest"]["component_hashes"].update(policy="0" * 64),
            lambda r: r.update(parameters_hash="0" * 64),
        ]:
            invalid = deepcopy(request)
            mutation(invalid)
            rejected(lambda: verify_revision(invalid, revision))
            negative_count += 1
        audit = subprocess.check_output([str(args.lake), "env", "lean", "Audit.lean"], cwd=clean_project, text=True)
        first = next(iter(THEOREMS))
        forged = "\n".join("'" + first + "' depends on axioms: [sorryAx]" if line.startswith("'" + first + "'") else line
                           for line in audit.splitlines())
        rejected(lambda: audit_axioms(forged, FOUNDATIONS))
        negative_count += 1
        rejected(lambda: audit_axioms("\n".join(audit.splitlines()[1:]), FOUNDATIONS))
        negative_count += 1
        # Change the copied retention migration alone, keeping every proof
        # source intact. The real helper must reject the old lock before it
        # cleans or rebuilds any of the existing Lean build artifacts.
        migration_name = "migrations/0008_verified_holdings.sql"
        if migration_name not in lock["rust_sources"]:
            raise AssertionError("retention migration is not pinned")
        migration = snapshot / migration_name
        original_migration = migration.read_bytes()
        before_build = build_state(clean_project)
        migration.write_bytes(original_migration + b"\n-- negative regression: changed pinned migration\n")
        arguments[2] = "proof"
        try:
            result = subprocess.run(arguments, input=json.dumps(request), text=True, capture_output=True,
                                    timeout=30, check=False)
            if result.returncode == 0 or "protected source" not in result.stderr:
                raise AssertionError("changed pinned migration did not fail closed")
            if build_state(clean_project) != before_build:
                raise AssertionError("changed pinned migration reached the Lean build")
        finally:
            migration.write_bytes(original_migration)
        negative_count += 1
        # Invoke the real helper again after a protected source change. The old
        # lock must fail before any build, including for a comment-only edit.
        safety = clean_project / "TradyProof/Safety.lean"
        safety.write_text(safety.read_text() + "\n-- negative regression: changed protected source\n")
        arguments[2] = "proof"
        result = subprocess.run(arguments, input=json.dumps(request), text=True, capture_output=True, timeout=30, check=False)
        if result.returncode == 0 or "protected source" not in result.stderr:
            raise AssertionError("changed protected source did not fail closed")
        negative_count += 1
        summary = {"scope": reports["proof"]["scope"], "clean_build": True,
                          "theorem_count": len(THEOREMS), "axioms": reports["proof"]["axioms"],
                          "parity_case_count": len(reports["parity"]["cases"]), "negative_checks": negative_count,
                          "malformed_input_case_count": malformed_count,
                          "source_pin_count": len(lock["sources"]), "rust_source_pin_count": len(lock["rust_sources"]),
                          "migration_source_pin_count": len(migration_sources),
                          "safety_sha256": lock["safety_sha256"], "rust_sha256": lock["rust_sha256"],
                          "toolchain": lock["toolchain"], "revision_sha256": lock["revision_sha256"]}
        if args.output is not None:
            if not args.output.is_absolute() or args.output.exists():
                raise ValueError("new absolute output directory required")
            args.output.mkdir(parents=True)
            for name, value in {"binding": revision, "request": request, "lock": lock,
                                "proof": envelopes["proof"], "parity": envelopes["parity"], "summary": summary}.items():
                (args.output / (name + ".json")).write_text(json.dumps(value, indent=2, sort_keys=True))
        print(json.dumps(summary, sort_keys=True))


if __name__ == "__main__":
    main()

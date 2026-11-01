"""Exact local proof, source, dependency and candidate revision bindings."""
import hashlib
import json
import re

SCOPE = "FORMULA_V1"
FOUNDATIONS = {"propext", "Classical.choice", "Quot.sound"}
THEOREMS = {
    **{f"TradyProof.{name}": "TradyProof/Safety.lean" for name in
       ["cap_le", "cap_nonzero", "floor_le_ceil", "stop_before_target", "non_vacuous"]},
    **{f"TradyProof.CheckedDecimal.{name}": "TradyProof/CheckedDecimal.lean" for name in
       ["representable", "floor_checked", "division_by_zero_rejects", "ties_to_even", "ties_from_odd"]},
    **{f"TradyProof.Formula.{name}": "TradyProof/Formula.lean" for name in
       ["total_calculation", "accepted_checks", "accepted_account_input", "invalid_account_input_cannot_accept",
        "accepted_stop_before_target", "accepted_target_holding_caps"]},
    **{f"TradyProof.Formula.{name}": "TradyProof/Witness.lean" for name in
       ["witness_accepted", "model_non_vacuous", "fx_witness_accepted", "fx_model_non_vacuous"]},
    **{f"TradyProof.Holding.{name}": "TradyProof/Holding.lean" for name in
       ["accepted_update_monotone", "maximum_preserves_left", "trailing_floor_monotone", "latched_exit_preserved"]},
    **{f"TradyProof.Revision.{name}": "TradyProof/Revision.lean" for name in
       ["accepted_revision_bound", "mismatched_source_rejects"]},
}
SAFETY_FILES = ["TradyProof/Safety.lean", "TradyProof/CheckedDecimal.lean", "TradyProof/Formula.lean", "TradyProof/Parsing.lean", "TradyProof/Holding.lean", "TradyProof/Revision.lean"]
POLICY_FIELDS = ["version", "entry_cash_fraction", "confidence_entry_min", "max_target_fraction", "exceptional_targets_enabled",
                 "max_initial_loss_fraction", "min_reward_to_risk", "max_holding_sessions", "leverage", "add_to_position", "rotation_enabled"]
PARAMETER_FIELDS = ["version", "family", "entry_allowance_fraction", "stop_atr_multiple", "target_net_fraction", "max_holding_sessions",
                    "atr_lookback", "bar_seconds"]


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def value_hash(value):
    return hashlib.sha256(encoded(value)).hexdigest()


def rust_struct_hash(value, fields):
    if set(value) != set(fields):
        raise ValueError("revision field inventory differs from production schema")
    ordered = {field: value[field] for field in fields}
    return hashlib.sha256(json.dumps(ordered, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()


def strict_json(text):
    def object_pairs(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate JSON key")
            result[key] = value
        return result
    return json.loads(text, object_pairs_hook=object_pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite JSON number")))


def proof_sources(root):
    return {str(path.relative_to(root)): sha(path) for path in sorted(root.rglob("*"))
            if path.is_file() and not path.is_symlink() and ".lake" not in path.parts
            and (path.suffix in {".lean", ".py"} or path.name == "lean-toolchain")}


def rust_sources(root):
    paths = [root / "Cargo.toml", root / "Cargo.lock"]
    paths.extend(path for path in (root / "crates").rglob("*")
                 if path.is_file() and (path.suffix in {".rs", ".sql"} or path.name == "Cargo.toml"))
    migrations = root / "migrations"
    paths.extend(path for path in migrations.glob("*.sql") if path.is_file())
    if migrations.is_symlink() or any(path.is_symlink() for path in paths):
        raise ValueError("Rust source symlinks are not supported")
    return {str(path.relative_to(root)): sha(path) for path in sorted(paths)}


def adapter_sources(root):
    path = root / "lab/lean/bridge.py"
    if not path.is_file() or path.is_symlink():
        raise ValueError("pinned simulation adapter unavailable")
    return {"lab/lean/bridge.py": sha(path)}


def toolchain_tree(lake):
    root = lake.parent.parent.resolve()
    manifest = {}
    for directory in ["bin", "lib", "include"]:
        for path in sorted((root / directory).rglob("*")):
            if path.is_file():
                if not path.resolve().is_relative_to(root):
                    raise ValueError("toolchain dependency escapes the pinned distribution")
                manifest[str(path.relative_to(root))] = sha(path)
    if not manifest:
        raise ValueError("pinned toolchain distribution is empty")
    return {"file_count": len(manifest), "sha256": value_hash(manifest)}


def safety_hash(sources):
    return value_hash({name: sources[name] for name in SAFETY_FILES})


def theorem_bindings(sources):
    return {name: {"file": path, "file_sha256": sources[path]} for name, path in THEOREMS.items()}


def validate_binding(binding):
    if set(binding) != {"schema_version", "source_commit", "candidate_id", "request"} or binding["schema_version"] != 1:
        raise ValueError("unsupported revision binding schema")
    if not re.fullmatch(r"[a-fA-F0-9]{40}|[a-fA-F0-9]{64}", binding["source_commit"]):
        raise ValueError("source commit binding missing")
    if not re.fullmatch(r"[A-Za-z0-9_.:-]{1,200}", binding["candidate_id"]):
        raise ValueError("candidate identity binding missing")
    formula = binding["request"]
    if set(formula) != {"kind", "policy", "envelope", "parameters", "max_account_risk_fraction", "input", "as_of"} or formula["kind"] != "FORMULA":
        raise ValueError("a concrete formula witness request is required")
    rust_struct_hash(formula["policy"], POLICY_FIELDS)
    rust_struct_hash(formula["parameters"], PARAMETER_FIELDS)


def verify_revision(request, binding):
    validate_binding(binding)
    formula = binding["request"]
    if (request["schema_version"] != 1 or request["stage"] != "verify"
            or not re.fullmatch(r"[a-f0-9]{64}", request["input_hash"])
            or request["trial"]["candidate_id"] != binding["candidate_id"]
            or request["manifest"]["source_commit"] != binding["source_commit"]
            or request["trial"]["parameters"] != formula["parameters"]
            or request["plan"]["envelope"] != formula["envelope"]
            or request["parameters_hash"] != rust_struct_hash(formula["parameters"], PARAMETER_FIELDS)
            or request["manifest"]["component_hashes"]["policy"] != rust_struct_hash(formula["policy"], POLICY_FIELDS)):
        raise ValueError("stale or mismatched formula, policy, source, parameters or candidate binding")

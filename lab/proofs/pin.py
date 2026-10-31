#!/usr/bin/python3
"""Prepare an exact local lock for owner review; does not build or prove anything."""
import argparse
import json
import pathlib
from bindings import (FOUNDATIONS, SCOPE, adapter_sources, proof_sources, rust_sources, safety_hash, sha,
                      strict_json, theorem_bindings, toolchain_tree, validate_binding, value_hash)


def prepare(lake, rust, binding, root):
    validate_binding(binding)
    for path in [lake, lake.parent / "lean", rust]:
        if not path.is_absolute() or not path.is_file() or path.is_symlink():
            raise ValueError("absolute real binary paths required")
    sources = proof_sources(root)
    return {"schema_version": 2, "scope": SCOPE, "lake_sha256": sha(lake), "lean_sha256": sha(lake.parent / "lean"),
            "rust_sha256": sha(rust), "approved_axioms": sorted(FOUNDATIONS), "sources": sources,
            "safety_sha256": safety_hash(sources), "theorem_bindings": theorem_bindings(sources),
            "toolchain": toolchain_tree(lake), "rust_sources": rust_sources(root.parents[1]),
            "adapter_sources": adapter_sources(root.parents[1]),
            "revision": binding, "revision_sha256": value_hash(binding)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--lake", type=pathlib.Path, required=True)
    parser.add_argument("--rust", type=pathlib.Path, required=True)
    parser.add_argument("--binding", type=pathlib.Path, required=True)
    args = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parent
    lock = prepare(args.lake, args.rust, strict_json(args.binding.read_text()), root)
    print(json.dumps(lock, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()

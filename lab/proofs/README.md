# Pinned Lean arithmetic proof project

## Overview

The original integer core is retained. The project now contains the complete
`FORMULA_V1` checked-decimal reference evaluator, acceptance safety certificates,
holding arithmetic/protection lemmas, exact revision binding, and executable
production/simulation-adapter parity. It runs entirely offline.

The frozen [implementation handoff](IMPLEMENTATION.md) records the executed
clean-build/audit/parity results, exact artifact/source pins and trust boundary.

## Interface

- `lean-toolchain` pins `leanprover/lean4:v4.19.0`; `lakefile.lean` has no external
  mathlib dependency. `lake build` builds the library, the original `tradyParity`
  executable, and `formulaParity`.
- [Safety definitions and reference contracts](TradyProof/README.md) describe
  the checked numeric domain, metadata validation and retained theorems.
- `Audit.lean` asks the kernel for each theorem's axiom closure.
- `pin.py --lake ABSOLUTE_BINARY --rust ABSOLUTE_BINARY --binding ABSOLUTE_JSON`
  prints a schema-2 lock for owner review; it does not claim a proof. The binding
  contains `schema_version=1`, the actual `source_commit`, registered
  `candidate_id`, and a concrete accepted `request` for `formula-bridge`.
- `bindings.py:THEOREMS` pins each audited theorem name to its complete source
  file hash. The lock covers every Lean/Python proof source, Rust/SQL source under
  `crates/`, every root `migrations/*.sql` file (including role definitions), Cargo
  manifests/lock, the actual Rust binary, the simulation bridge, and all files in
  the Lean distribution's `bin`, `lib`, and `include` trees. Rust and simulation
  adapter inventories reject their specified symlinks, including a symlinked root
  migrations directory. Proof-source discovery instead excludes symlink files;
  changing an already-pinned inventory fails lock comparison
  (`[bindings.py:rust_sources]`, `[bindings.py:adapter_sources]`,
  `[bindings.py:proof_sources]`, `[check.py:verify_lock]`).
- `check.py proof|parity --project ABSOLUTE_DIR --lake ABSOLUTE_BINARY
  --rust ABSOLUTE_BINARY --lock ABSOLUTE_LOCK` implements the bounded helper
  protocol. Use a prepared repository snapshot outside candidate write access;
  the proof project stays at `lab/proofs` within that snapshot.
- `parity.py --rust ABSOLUTE_BINARY` executes fixed and seeded adversarial
  fixtures against Lean, the production Rust calculator/checked helpers, and
  `lab/lean/bridge.py:RustFormulaBridge.fixture`. Every accepted decimal and
  metadata value and every rejection message must match exactly.
- `[FormulaMain.lean:executeRequest]` dispatches FORMULA and DECIMAL operations.
  Calculation rejections are JSON results; parsing/dispatch errors exit 2 with
  stderr diagnostics (`[FormulaMain.lean:main]`).
- `self_test.py --lake ABSOLUTE_BINARY --rust ABSOLUTE_BINARY` creates a fresh
  temporary snapshot, runs the real proof/parity helper, and tests source/revision
  tampering. It checks complete root SQL membership and mutates a copied pinned
  migration, requiring rejection before any Lean build artifact changes. Optional
  `--output NEW_ABSOLUTE_DIR` preserves fixture evidence.

## Invariants

`[check.py:main]` executes the real pinned toolchain, cleans/builds the project, checks the
actual kernel axiom audit and compares actual Lean/Rust executable results.
Changed sources, toolchain, binary identity, unapproved axioms, missing witnesses,
failed builds and unequal outputs block the helper. Source/binary/dependency pins
are checked both before and after execution. It never installs tools.
`[check.py:MAX]` bounds helper input to 4 MiB; `[check.py:execute]` applies a
120-second subprocess timeout and `[parity.py:execute]` uses thirty seconds per
parity command. `[check.py:audit_axioms]` requires one audit for each of the 24
registered theorems and only the foundations in `[bindings.py:FOUNDATIONS]`.

The declared scope is `FORMULA_V1`: the full typed reference calculation and the
listed acceptance/protection obligations, with finite exact executable parity.
The numeric domain includes every signed 96-bit coefficient and scale 0 through
28. Overflow, division/remainder by zero, precision reduction, half-even arithmetic
rounding and off-grid results have explicit failure semantics. The reference
executes both bounded quantity corrections, the next-lot rejection check, target
correction and all final checks. Metadata includes account/order validation,
causal warmed evidence, score/sentiment gates, currencies/FX, freshness,
timestamp overflow and Chrono's represented leap-second behavior.

`Formula.accepted_checks` is derived from the actual checked branches. It covers
entry bounds, positive/minimum lot-grid quantity, price ordering, acquisition cost
within rounded allocation and raw cash, positive owner/account/position risk caps,
checked loss/fraction definitions, target net wealth and reward/risk, configured
target/holding caps, input identity and remaining validity. The Rust calculator
also explicitly rechecks raw cash before accepting. Representative cash and FX
trades are kernel-evaluated non-vacuity witnesses. Each pinned candidate must
additionally produce an actual accepted witness in all three executable paths;
the candidate/policy/envelope/parameter hashes must match the registered request.

Proofs establish properties of the Lean reference. Finite parity reduces
translation risk; it is **not** a formal refinement proof of Rust, `rust_decimal`,
the Rust compiler, the Lean executable compiler, Python, or QuantConnect LEAN.
The reviewed translation, parsers/transport, host/toolchain integrity and build
provenance remain explicit trust boundaries. The adapter delegates arithmetic to
the pinned Rust binary; its fixtures establish wire/error preservation, not a
historical backtest or fill-model certification. Calendar-resolved deadlines and
truth/provenance of supplied observations remain caller responsibilities.

Holding theorems cover the numeric/version checks in
`[TradyProof/Holding.lean:checkedUpdate]`, the exact
max/subtract trailing update, and the existing-latch early-return rule. Runtime
serialization, durable CAS and broker outcomes remain outside this arithmetic
proof. The original `Safety.lean` definitions are unchanged.

## Side Effects

`[check.py:main]` reads pinned files, replaces local outputs through
`lake clean`/`lake build`, and executes audited Lean/Rust programs and the pinned
simulation bridge. `self_test.py`
writes only its own temporary snapshot and an explicitly requested evidence
directory. No network, database, broker, paid API, scheduling or deployment occurs.

## Verification

With Lean 4.19.0 already prepared, run from the repository root:

```sh
cargo build --offline --locked -p trady-cli --bin trady
/root/.cache/trady-tools/lean-4.19.0-linux/bin/lake -d lab/proofs build
/root/.cache/trady-tools/lean-4.19.0-linux/bin/lake -d lab/proofs env lean lab/proofs/Audit.lean
python3 lab/proofs/parity.py --rust /absolute/pinned/trady
python3 lab/proofs/self_test.py --lake /root/.cache/trady-tools/lean-4.19.0-linux/bin/lake --rust /absolute/pinned/trady
cargo test --offline --locked -p trady-policy --test limits
```

For the helper runner's 64 MiB executable-pin limit, a debug build may need an
`objcopy --strip-debug` copy. Pin and execute that exact copy; stripping debug
information does not authorize substituting another binary. The self-test's
manifest is labelled local fixture evidence and cannot approve a real release.

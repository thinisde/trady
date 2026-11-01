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
- `[Main.lean:main]` exposes `tradyParity NUMERATOR DENOMINATOR QUANTITY MAXIMUM`,
  with four natural-number arguments and a positive denominator. Success prints
  JSON fields `floor`, `ceil` and `cap` and exits 0; invalid arguments print a
  diagnostic to stderr and exit 2.
- [Safety definitions and reference contracts](./TradyProof/README.md) describe
  the checked numeric domain, metadata validation and retained theorems.
- `Audit.lean` asks the kernel for each theorem's axiom closure.
- `pin.py --lake ABSOLUTE_BINARY --rust ABSOLUTE_BINARY --binding ABSOLUTE_JSON`
  prints a schema-2 lock for owner review; it does not claim a proof. The binding
  contains `schema_version=1`, `source_commit`, registered
  `candidate_id`, and a concrete accepted `request` for `formula-bridge`.
  `[pin.py:prepare]` validates and hashes that supplied binding; pin creation
  neither establishes candidate registration nor executes its acceptance witness.
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
  Both CLI stages require stdin `stage:"verify"`
  (`[bindings.py:verify_revision]`). `[check.py:main]` emits
  `{schema_version:1,input_hash,payload}`; `payload.source_hash` is the pinned
  Rust executable hash, and the proof payload reports `witness_count:2`.
- `parity.py --rust ABSOLUTE_BINARY` executes fixed and seeded adversarial
  fixtures against Lean, the production Rust calculator/checked helpers, and
  `lab/lean/bridge.py:RustFormulaBridge.fixture`/`fixture_wire`. Every accepted decimal and
  metadata value and every model rejection message must match exactly.
  `[parity.py:run_malformed_fixtures]` separately requires parser rejection in
  Rust, Lean and the simulation bridge CLI after a successful witness in all
  three paths. It checks exit status, empty stdout and expected diagnostics;
  the adapter must exit 2 with the exact `LEAN formula fixture PARSE_REJECTED: `
  line and the expected Rust parse/schema diagnostic. Its generic `BLOCKED`
  wrapper, missing executable, timeout, integrity failure and wrapped inner
  process signal cannot supply parser evidence. Calculation rejections do not
  count. Parser
  diagnostic strings differ between implementations; `PARSE_REJECTED` records
  the observed rejection outcome rather than numeric-result equality.
- [Parsing.lean](./TradyProof/Parsing.lean) checks nested field allowlists,
  identifiers, currencies, enum tags, unsigned field widths and encoded timestamp
  bounds before decoding formula inputs. Before numeric normalization, `parse`
  requires plain unsigned integer lexemes for production unsigned fields;
  decimal points, exponents and minus signs (including `-0`) are rejected.
  Its string decoder combines high/low surrogate escapes into one Unicode scalar
  and rejects isolated or malformed pairs for keys and values. Its JSON parser
  rejects duplicate decoded keys, including escaped supplementary spellings,
  and bounds nesting to 128 levels.
- `[FormulaMain.lean:main]` reads the complete JSON document (at most 4 MiB) and
  uses the private
  `[FormulaMain.lean:executeRequest]` dispatcher for FORMULA and DECIMAL operations.
  Multiline documents are accepted; trailing documents are rejected.
  Calculation rejections are JSON results; parsing/dispatch errors exit 2 with
  stderr diagnostics (`[FormulaMain.lean:main]`).
- `self_test.py --lake ABSOLUTE_BINARY --rust ABSOLUTE_BINARY` creates a fresh
  temporary snapshot, runs the real proof/parity helper, and tests source/revision
  tampering. It checks complete root SQL membership and mutates a copied pinned
  migration, requiring rejection before any Lean build artifact changes. Optional
  `--output NEW_ABSOLUTE_DIR` preserves fixture evidence. Each offline proof/parity
  stage has a 300-second outer timeout to cover its clean build, inventory hashing
  and corpus execution. Per-request and production helper budgets are unchanged.
- `[parity.py:timestamp]` validates Gregorian calendar dates, clock and offset
  ranges and UTC representability before transport encoding. It retains
  represented leap seconds, including the final date's second 60 through
  nanosecond 1,999,999,999, and Chrono's truncation after nine fractional digits.
  UTC seconds/fraction representability is separate from formula duration-overflow
  checks (`[TradyProof/Formula.lean:validTimestamp]`, `maxOrdinaryTimestamp`).
  [test_inputs.py](./test_inputs.py) provides quick offline transport and
  rejection-evidence checks without building binaries or running the full corpus.

`[fixtures.py:fixtures]` declares 671 exact-output cases. The separate
`[parity.py:malformed_wire_fixtures]` inventory declares 265 parser-rejection cases,
including 96 original exponent/signed-zero lexemes across all twelve unsigned
fields, 16 malformed-surrogate cases and nine raw document/key/syntax cases.
Both escaped and direct UTF-8 supplementary metadata have accepted three-way
cases. The helper adds one concrete revision witness: 937 total cases. These are
corpus inventories, not a statement that any particular source snapshot passed.

## Invariants

`[check.py:main]` executes the real pinned toolchain and cleans/builds the project.
The proof stage checks the actual kernel axiom audit and candidate witness;
the parity stage compares actual Lean/Rust/adapter results without rerunning
`Audit.lean`.
Changed sources, toolchain, binary identity, unapproved axioms, missing witnesses,
failed builds and unequal outputs block the helper. Source/binary/dependency pins
are checked both before and after execution. It never installs tools.
`[bindings.py:validate_binding]` checks the supplied `source_commit` spelling;
`[bindings.py:verify_revision]` checks its equality to the request, without Git
authentication. The helper checks `input_hash` syntax rather than recomputing it.
Candidate identity and envelope are compared directly; policy and parameter
hashes use their declared field order (`[bindings.py:verify_revision]`,
`[bindings.py:rust_struct_hash]`). `[bindings.py:proof_sources]` includes qualifying
Python test files in its source inventory but excludes README and other
documentation; documentation edits alone do not change that inventory.
`[check.py:MAX]` bounds helper input to 4 MiB; `[check.py:execute]` applies a
120-second subprocess timeout and `[parity.py:execute]` uses thirty seconds per
parity command. `[check.py:audit_axioms]` requires one audit for each of the 26
registered theorems and only the foundations in `[bindings.py:FOUNDATIONS]`.
Both subprocess helpers replace the environment with `PATH=/usr/bin:/bin` and
`LANG=C.UTF-8`. Their output limits apply after capture:
`[check.py:execute]` counts characters and `[parity.py:invoke]` counts
encoded bytes.

The declared scope is `FORMULA_V1`: the full typed reference calculation and the
listed acceptance/protection obligations, with finite exact executable parity.
The numeric domain includes every signed 96-bit coefficient and scale 0 through
28. Overflow, division/remainder by zero, precision reduction, half-even arithmetic
rounding and off-grid results have explicit failure semantics. The reference
executes both bounded quantity corrections, the next-lot rejection check, target
correction and all final checks. Metadata includes the reference's account/order
identity and quantity checks, causal warmed evidence, score/sentiment gates,
currencies/FX, freshness,
timestamp overflow and Chrono's represented leap-second behavior.

`[TradyProof/Formula.lean:validAccountInput]` checks the production ASCII identifier
grammar (1–96 characters) for account IDs, position/order instrument IDs and
optional client intent IDs, three-letter uppercase account currency, account modes
`paper`/`shadow`/`live`, sides `BUY`/`SELL`, and the six production order statuses
including `UNKNOWN`. `validateAccount` additionally retains snapshot identity,
cash, positive unique positions and order identity/quantity consistency checks.
`accepted_account_input` and `invalid_account_input_cannot_accept` tie these
account-input guards to successful reference evaluation. Entry readiness and
release authority remain separate production checks.

`Formula.accepted_checks` is derived from the actual checked branches. It covers
validated account inputs, entry bounds, positive/minimum lot-grid quantity,
price ordering, acquisition cost
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

Run `cargo test --workspace --locked` from the repository root for workspace verification.

With Lean 4.19.0 already prepared, run from the repository root:

```sh
cargo build --offline --locked -p trady-cli --bin trady
/root/.cache/trady-tools/lean-4.19.0-linux/bin/lake -d lab/proofs build
/root/.cache/trady-tools/lean-4.19.0-linux/bin/lake -d lab/proofs env lean lab/proofs/Audit.lean
python3 lab/proofs/parity.py --rust /absolute/pinned/trady
python3 lab/proofs/self_test.py --lake /root/.cache/trady-tools/lean-4.19.0-linux/bin/lake --rust /absolute/pinned/trady
cargo test --offline --locked -p trady-policy --test limits
python3 -B -m unittest discover -s lab/proofs -p 'test_inputs.py'
```

For the helper runner's 64 MiB executable-pin limit, a debug build may need an
`objcopy --strip-debug` copy. Pin and execute that exact copy; stripping debug
information does not authorize substituting another binary. The self-test's
manifest is labelled local fixture evidence and cannot approve a real release.
Changed proof/parser sources, Rust/SQL sources or theorem inventories require a
fresh lock after all workspace changes are complete. Root migration discovery
includes newly added migrations, including `0011_fail_closed_storage.sql`; the
self-test requires every discovered migration to appear in the source inventory.

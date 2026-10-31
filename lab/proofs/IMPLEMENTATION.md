# FORMULA_V1 implementation and verification handoff

Checkpoint: 2026-10-31. The implementation is frozen after the successful clean
24-theorem/647-case/10-negative run with the corrected SQL inventory. Its scope
is `FORMULA_V1`: the complete typed checked-decimal
reference calculation, acceptance/protection proofs, exact revision binding and
finite executable production/adapter parity. This meets the reference/proof and
parity boundary in [the weekly design](../../docs/v1/weekly-optimization.md#lean-4-proof-boundary).
The translation and compiler assumptions below remain explicit.

## Retained artifacts and independent verification

The exact Rust verification artifact is
`/tmp/trady-formula-proof-stable.SEReAo/trady` (60,287,408 bytes). It was created
after the final workspace formatter, retention migration, repeatable-read
regression and adapter comment updates:

```sh
cargo build --offline --locked -p trady-cli --bin trady
cargo fmt --all -- --check
objcopy --strip-debug target/debug/trady /tmp/trady-formula-proof-stable.SEReAo/trady
```

The copy fits the helper runner's 64 MiB executable-pin limit. Its exact bytes,
rather than the mutable `target/debug/trady` path, were pinned and executed.

Final evidence is retained in
`/tmp/trady-formula-proof-stable.SEReAo/sql-bound-evidence/`:

This evidence supersedes `final-evidence/` and `root-evidence/`, whose source
inventories omitted root SQL files. Those earlier directories remain unchanged.
The correction changes only binding/test Python sources and documentation;
the same final Rust executable, SQL, Lean definitions and adapter are retained.

| File | Content |
|---|---|
| `binding.json` | Concrete formula request, candidate identity and source commit |
| `request.json` | Helper protocol request with policy/parameter/envelope bindings |
| `lock.json` | All source, theorem, safety, toolchain, adapter and executable pins |
| `proof.json` | Actual kernel audit, two kernel witnesses and revision-bound scope |
| `parity.json` | Exact Lean/Rust results for all 647 cases; adapter equality checked during execution |
| `summary.json` | Clean-build counts and final digests |

From `/thinisde/trady`, the independent end-to-end command is:

```sh
python3 lab/proofs/self_test.py \
  --lake /root/.cache/trady-tools/lean-4.19.0-linux/bin/lake \
  --rust /tmp/trady-formula-proof-stable.SEReAo/trady \
  --output /tmp/trady-formula-proof-stable.SEReAo/root-sql-bound-evidence
```

The output directory must not already exist. The self-test copies the pinned
source inventory into a fresh temporary repository-shaped snapshot. Both helper
stages run `lake clean`, `lake build` and the real axiom audit there; the parity
stage executes the full corpus plus the concrete bound candidate witness.
Sources, executable identity and dependencies are rechecked after execution.
The self-test then exercises ten rejection checks: changed candidate,
parameters, envelope, commit, policy hash and parameter hash; custom-axiom and
missing-theorem audit-parser negatives; an actual protected-source edit; and a
separate change to copied `migrations/0008_verified_holdings.sql`. The migration
case invokes the real helper with the existing lock and verifies protected-source
rejection while all existing Lean build artifact identities, sizes and timestamps
remain unchanged, establishing failure before clean/build. It restores the
copied migration before the independent protected-proof-source negative.

The direct build/audit/parity commands, also checked individually, are:

```sh
/root/.cache/trady-tools/lean-4.19.0-linux/bin/lake -d lab/proofs build
/root/.cache/trady-tools/lean-4.19.0-linux/bin/lake -d lab/proofs env lean lab/proofs/Audit.lean
python3 lab/proofs/parity.py --rust /tmp/trady-formula-proof-stable.SEReAo/trady
cargo test --offline --locked -p trady-policy --test limits
```

Lake's `-d` selects the package without changing the subprocess working
directory; the explicit `lab/proofs/Audit.lean` path is intentional. The direct
parity command has 646 cases; the helper adds one bound witness for 647.

## Actual results

| Check | Result |
|---|---|
| Fresh proof/parity snapshots | Clean builds passed |
| Kernel theorem inventory | 24 audited theorems |
| Transitive axiom union | `Classical.choice`, `Quot.sound`, `propext` |
| Representative kernel witnesses | Full cash/fees request and full FX request, both accepted using `by decide` |
| Exact executable parity | 647 cases across Lean, production Rust and the actual simulation adapter |
| Outcomes | 19 accepted formulas, 288 successful decimal operations, 340 rejections |
| Distinct observed rejection messages | 59 |
| Verifier negative checks | 10 passed, including actual SQL tampering rejected before build |
| Root SQL source membership | All 11 files: nine migrations and two role definitions |
| Policy limits regression suite | 5 tests passed |
| Formatting after final Rust changes | `cargo fmt --all -- --check` passed |

The retained corpus has 118 fixed boundary fixtures, 400 seeded extreme Decimal
vectors and 128 seeded formula vectors. It covers fees/FX, both formula
families, fractional lots, exceptional targets, policy/parameter/metadata gates,
account/order consistency, freshness and lifetime boundaries, UTC offsets,
represented leap seconds, timestamp extrema, checked arithmetic overflow and
zero denominators, half-even ties, precision exhaustion and off-grid rejection.
The final six fixed cases explicitly reach rounded acquisition overspend,
loss-fraction underflow, target-denominator underflow, target ordering with
different FX rates, insufficient bounded target correction, and lot-grid loss
after bounded quantity correction. Each retains its exact expected rejection.

## Immutable pins

Inventory digests use `bindings.value_hash`: SHA-256 of sorted, compact JSON.
Individual source and executable hashes are SHA-256 of exact file bytes.
The broad inventory includes all `.rs` and `.sql` files under `crates/`, every
root `migrations/*.sql` file, Cargo manifests and `Cargo.lock`, including the
final retention/RR changes. The 11 root SQL files are checked explicitly in the
self-test; source-file symlinks and a symlinked migrations directory are rejected.

| Binding | SHA-256 |
|---|---|
| Exact stripped Rust executable / report `source_hash` | `fad2645ec6df1c9007de305d8e839c59a3ce0bb18daa0f8c843a6a07bbc9cd08` |
| Rust/SQL inventory, 172 files, including 11 root SQL files | `1bef9927de0d3b0968c5b54054f934ad78bc845ca3ab134d1d92f7f172503771` |
| Proof/tool/helper inventory, 18 files | `c68809b5a3ff101416ae17a06057e03258a106aead1e66dac016e8267c574b18` |
| Combined protected safety definitions | `508278ec6aa71c0fec2acc0f9f680894624a3c14030de8bf86e31082e4d1b396` |
| Exact theorem-to-source binding inventory | `c1dc907770ed2fdd75e3a57b7faa7de7bdfbfe5f17ff79b9fe9b0ac42ff18021` |
| Lean distribution, 3019 files in `bin`, `lib`, `include` | `dde4425feb261ac42b3ee1b9b7371c0a79953e29bee790a347be28b01e11e95f` |
| `lean` executable | `92c3d35b5bfaa5e0fea413a775d504cf46cd95e1345df61c2274f76779e7e023` |
| `lake` executable | `5111aee5c9434ab05d4265606475ef22e8bda69aaf3a0657b61a7eec6e550bed` |
| Report/manifest toolchain hash, exact `lean --version` stdout | `5362a246ce2545424db10a77de8fd4694f4cf8b0170dfd43fa727acf7f7f259b` |
| `lab/lean/bridge.py` | `d59ac4dab9e1310b93e21b017bfd14043b264bfda98c2a35c7bfb82fc8df9619` |
| Complete final `lock.json` file | `54cc21951464252b29c5f3eb98418a583b50f60d2ee7ba9b917482beb82bfb3e` |
| Concrete fixture revision content | `e4852e4a500442db676a4ff19a61abf9ba6ced58294c593ba3b3db5818267127` |

The combined safety digest covers `Safety.lean`, `CheckedDecimal.lean`,
`Formula.lean`, `Holding.lean` and `Revision.lean`. It replaces an arithmetic-core
digest when configuring `protected_safety_hash`. The full theorem inventory is
[bindings.py:THEOREMS](bindings.py); use that reviewed inventory for
`required_theorems`. Witness source/statement hashes are additionally protected
by the full proof inventory and theorem bindings. All 172 Rust/SQL pins, 18 proof
pins and the adapter pin were compared with the shared workspace again after
the final run and matched.

The retained fixture uses candidate `proof_fixture` and source commit
`470fa28c9e7d934545a97cce61f32da3983a76ab`. The commit is the checkout base;
the separate content inventories pin the uncommitted implementation. No commit
was created. Its policy hash is
`c1b0ddff433573c21ca87ac1679d4a8e9dbad97c56f718d67aa8cbbe2a49bb77`,
parameter hash is
`31dda867fa226af822325acd9eedd6661ac0f4cc9d5980c4d5a7c1351a0b1755`,
and canonical envelope hash is
`b294aaeb22509701ba1044927282c33b42c574aadb83ed78cd06dd9b063b8eec`.
Policy and parameter hashes use the exact Rust struct field serialization order.
The helper compares all bound values and executes the candidate witness; a caller
pass flag cannot establish proof/parity success.

These artifacts are local fixture evidence. A real registered candidate needs
its own exact accepted request and source/candidate binding through `pin.py
--binding`, followed by the same real verification. Changes to any pinned Rust
comment, test, SQL migration, proof/helper source or adapter require new pins and
verification. Documentation files are outside these semantic source inventories.

## Definition and theorem coverage

The original integer safety core is retained. The reference domain includes the
full signed 96-bit Decimal coefficient range and scales 0 through 28. Checked
packing, overflow, division/remainder by zero, half-even precision reduction,
grid rounding and canonical decimal output are executable definitions.

The formula reference implements the actual validators, ordered fee/FX
arithmetic, both sizing calls, stop checks, two bounded quantity corrections,
short-circuit next-lot check, one target correction, final risk/grid checks and
expiry calculation. Accepted safety certificates are constructed from those
executed guards. They establish positive/minimum lot quantity, entry bounds and
price ordering, valid ticks/lots, allocation and raw-cash affordability, modeled
account/position risk limits, exact checked loss/fraction/reward definitions,
required net proceeds, reward/risk, target/holding caps, snapshot identity and
remaining validity. Rust now explicitly checks both allocation and acquisition
cost against raw spendable cash before acceptance; the retained Rust regression
also checks maximum magnitude, 28-place and smallest-unit cash boundaries.

| Source | Audited theorem suffixes | Count |
|---|---|---:|
| `TradyProof/Safety.lean` | `cap_le`, `cap_nonzero`, `floor_le_ceil`, `stop_before_target`, `non_vacuous` | 5 |
| `TradyProof/CheckedDecimal.lean` | `representable`, `floor_checked`, `division_by_zero_rejects`, `ties_to_even`, `ties_from_odd` | 5 |
| `TradyProof/Formula.lean` | `total_calculation`, `accepted_checks`, `accepted_stop_before_target`, `accepted_target_holding_caps` | 4 |
| `TradyProof/Witness.lean` | `witness_accepted`, `model_non_vacuous`, `fx_witness_accepted`, `fx_model_non_vacuous` | 4 |
| `TradyProof/Holding.lean` | `accepted_update_monotone`, `maximum_preserves_left`, `trailing_floor_monotone`, `latched_exit_preserved` | 4 |
| `TradyProof/Revision.lean` | `accepted_revision_bound`, `mismatched_source_rejects` | 2 |

Holding results cover the numeric/version boundary of `checked_update`, monotone
stop/deadline/activation/distance protections, the exact max/subtract trailing
update and preservation of an existing exit latch. Revision results compare
expected/supplied identity and the full policy/envelope/parameter values before
calculation, retain original inputs and reject mismatched source identity.

## Remaining trust assumptions

- The Lean kernel and the reviewed standard logical foundations above are trusted.
  No `sorry`, admitted/custom axiom, `unsafe`, `partial` or `native_decide`
  shortcut is used. Lean 4.19.0 is pinned; no mathlib package is required.
- The reviewed translation from Rust to the Lean reference, JSON/decimal/time
  parsers, Python runtime, executable compilers, host integrity and source-to-build
  provenance remain trusted. Hashes bind artifact identity; they do not prove
  compiler correctness or that an arbitrary binary was built from the sources.
  Finite exact parity reduces translation risk and is not universal Rust,
  `rust_decimal` limb, compiler or QuantConnect LEAN refinement. Such universal
  refinement is outside the documented completion gate.
- The simulation adapter executes the same pinned Rust calculator. Its fixtures
  establish exact wire/rounding/error preservation, not a second independent
  economic algorithm or historical fill/backtest certification.
- Supplied observations, fee/FX assumptions, evidence flags and the
  calendar-resolved holding deadline require their external provenance. The
  reference checks their represented values and relationships. It does not prove
  market-data truth, future return, broker fill availability or realized gap loss.
- Durable runtime CAS/serialization, storage and broker outcomes remain outside
  the holding arithmetic proof. Owner review of the lock/theorem statements and
  candidate isolation remain part of the deployment-owned verifier boundary.

There is no remaining implementation/proof gap within the stated reference and
finite-parity scope. Live activation and empirical strategy evaluation are
separate workflows; this checkpoint performs neither.

## Changed paths and integration

- `lab/proofs/TradyProof/{CheckedDecimal,Formula,Holding,Revision,Witness}.lean`,
  `TradyProof.lean`, `FormulaMain.lean`, `Audit.lean`, `lakefile.lean`: reference,
  safety proofs, witnesses, executable evaluator and kernel audit inventory.
- `lab/proofs/{bindings,pin,check,fixtures,parity,self_test}.py`: exact bindings,
  clean trusted verification and retained adversarial executable fixtures.
- `lab/proofs/README.md`, `lab/proofs/TradyProof/README.md`, this file: contracts,
  commands, pins and trust boundary.
- `crates/trady-policy/src/limits/{mod,proof_bridge}.rs`, its `README.md`, and
  `crates/trady-policy/tests/limits.rs`: pure production bridge, raw-cash safety
  checks and cash-rounding regression.
- The coordinated weekly worker owns `trady weekly formula-bridge --file FILE|-`
  in the CLI. Parity calls that real hook and the pinned
  `lab/lean/bridge.py:RustFormulaBridge.fixture` adapter directly.

No git staging/commit/push, network service, live/paid API, deployment or scheduler
was used by this verification work.

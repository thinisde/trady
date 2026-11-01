# Protected integer safety definitions

## Overview

This library retains the original integer definitions and adds the complete
checked-decimal `FORMULA_V1` reference and explicit safety certificates.

## Interface

`Safety.lean` defines `floorTicks`, `ceilTicks`, `cap` and the theorems `cap_le`,
`cap_nonzero`, `floor_le_ceil`, `stop_before_target`, `non_vacuous`.

- [CheckedDecimal.lean](./CheckedDecimal.lean): bounded signed coefficient/scale,
  checked add/subtract/multiply/divide/remainder, half-even precision reduction,
  exact grid rounding, canonical decimal rendering and rounding/overflow lemmas.
- [Formula.lean](./Formula.lean): typed account/quote/fee/FX/evidence
  validators, bounded solver/target corrections, total `calculate`/`evaluate`, and
  `AcceptedChecks` derived from executed guards. `accepted_checks`,
  `total_calculation`, `accepted_stop_before_target` and
  `accepted_target_holding_caps` are the corresponding public theorems.
  `validIdentifier`, `validCurrency` and `validAccountInput` model production's
  account input constraints; `accepted_account_input` and
  `invalid_account_input_cannot_accept` certify their relationship to acceptance.
  `validTimestamp` covers Chrono's represented seconds/fraction domain;
  `maxOrdinaryTimestamp` separately bounds positive duration-addition results.
- [Parsing.lean](./Parsing.lean): bounded JSON parsing without duplicate decoded
  object keys, nested field allowlists, identifier/currency/enum validation,
  unsigned production integer lexemes/widths and encoded timestamp checks.
  `parse` combines Unicode surrogate pairs and rejects isolated or invalid pairs
  for keys and values. `formula`
  decodes a checked request; `decimal` validates the arithmetic request envelope.
- [Witness.lean](./Witness.lean): actual full cash and FX requests with
  kernel-evaluated `witness_accepted`/`fx_witness_accepted` and existential
  non-vacuity theorems. No native-evaluation shortcut is used.
- [Holding.lean](./Holding.lean): checked/versioned monotone plan updates,
  nondecreasing trailing peak/floor, and preservation of an existing exit latch.
- [Revision.lean](./Revision.lean): exact expected/supplied source identity and
  typed policy/envelope/parameter comparison, revision-bound accepted results,
  and explicit rejection of a mismatched Rust source identity.

## Invariants

`[Safety.lean:floorTicks]` uses total natural-number arithmetic;
`[../Main.lean:main]` requires a positive denominator.
The original non-vacuity theorem contains an actual witness. The new cash/FX
witnesses evaluate the real reference program. `Decimal` carries checked bounds;
accepted formula/holding results carry proofs constructed from the actual branch
checks, rather than assumed output properties. There are no admitted proofs,
custom axioms or unchecked native theorem-evaluation dependencies
(`[CheckedDecimal.lean:Decimal]`, `[Formula.lean:AcceptedChecks]`,
`[Witness.lean:witness_accepted]`).

`[Formula.lean:sizeEntry]`, `[Formula.lean:fxRate]` and `[Formula.lean:calculate]`
bound quote freshness to ten seconds and account/FX freshness to sixty seconds.
The calculation permits at most two quantity-correction iterations, and
`[Formula.lean:targetPrice]` permits one tick of target correction.
`[Revision.lean:verifyCandidate]` compares supplied identity digests and typed
objects; it does not compute those hashes or prove their provenance.

`[Formula.lean:validateAccount]` checks account and nested identifier grammar,
account currency, exact account mode/side/status tags, snapshot identity, cash,
positive unique positions and broker-order identity/quantity consistency.
`[Formula.lean:AcceptedChecks]` carries the `validAccountInput` guard proof from
that executed validation. `UNKNOWN` remains a valid parsed order status;
production entry readiness applies its separate unresolved-order restrictions.
`[Parsing.lean:formula]` enforces transport-level constraints before calculation;
`[Parsing.lean:parse]` limits JSON nesting to 128 and rejects trailing data and
duplicate keys, including equivalent escaped supplementary spellings. It checks
production unsigned field tokens before normalization, rejecting fractional,
exponent and signed forms, including negative zero. `formula` retains the
separate unsigned width checks after parsing. The parent executable
limits the complete document to 4 MiB (`[../FormulaMain.lean:main]`).

`[Formula.lean:validTimestamp]` checks epoch-second bounds and the permitted
nanosecond fraction, including the final date's represented leap second
`+262142-12-31T23:59:60.999999999Z`. `[Formula.lean:calculate]` separately checks
each positive duration result against `maxOrdinaryTimestamp`, preserving both
ordinary and FX validity-overflow rejection. These transport guards do not add
a parser-equivalence or production-refinement theorem to the 26 declarations.

`[../bindings.py:theorem_bindings]` binds definitions and theorem statements by
complete source hashes. Its broad source inventory also binds Cargo inputs,
Rust/SQL under `crates/`, and root `migrations/*.sql`, including role definitions.
Translation to production Rust and simulation transport
is checked by exact executable fixtures (`[../parity.py:run_fixtures]`), not claimed as a universal refinement
theorem. No theorem asserts market-data truth, profitability or guaranteed fills.

## Side Effects

None in the pure definitions (`[Formula.lean:evaluate]`,
`[Holding.lean:checkedUpdate]`, `[Revision.lean:verifyCandidate]`). Kernel checking
has no market or database effects; the parent build produces local outputs
(`[../lakefile.lean:TradyProof]`).

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `lake build` and `lake env lean Audit.lean` from the pinned parent project.
The exact 26-theorem inventory is `../bindings.py:THEOREMS`; the only approved
axioms are `propext`, `Quot.sound`, and `Classical.choice`. Run the parent
`self_test.py` for clean compilation, three-way executable parity and fail-closed
source/revision checks. The SQL tamper regression invokes the real helper after
changing only a copied pinned migration and verifies that Lean build artifacts
remain unchanged.

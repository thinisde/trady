# Formula limit calculator

## Overview

[mod.rs](./mod.rs) calculates limits from supplied account, market, volatility and
research observations. This is a separate pure policy surface; returned records
do not certify readiness or grant permission to place orders.

## Interface

- `[mod.rs:FormulaFamily]`, `FormulaParameters`, `FormulaEnvelope`: trend/reversal
  family and owner-bounded numeric parameter records with validation.
- `[mod.rs:FormulaEvidence]`, `LimitsInput`: causal completed-bar/ATR inputs,
  supplied evidence flags and account/quote/cost/rule snapshots.
- `[mod.rs:CalculatedLimits]`, `calculate_limits`: checked quantity, entry/stop/
  target prices, modeled account risk and correlated source snapshot identifiers.
- `[proof_bridge.rs:Request]`, `Operation`, `Reply`, `evaluate`: offline execution
  of the real formula and checked decimal helpers, returning full accepted limits
  or the actual rejection message. Successful decimals use exact normalized
  base-ten strings.
  Requests use `kind` tags `FORMULA`/`DECIMAL`; replies use `status` tags
  `ACCEPTED`/`DECIMAL`/`REJECTED`. `[proof_bridge.rs:Operation]` supports
  `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Floor` and `Ceil`.
- [Parent policy contract](../README.md) documents the shared economics helpers.

## Invariants

`[mod.rs:FormulaParameters::validate]` requires version 1, matching approved
parameter envelope, 1 through the policy's maximum holding sessions, ATR lookback
2–252 and bar duration 60–86,400 seconds. `[mod.rs:FormulaEnvelope::validate]`
checks policy validity, an entry allowance at most 1%, positive ordered stop-ATR
bounds up to 10 and positive target bounds within the policy ceiling.

`[mod.rs:calculate_limits]` checks fresh account/quote/FX and unexpired inputs,
matching causal completed-bar evidence and supplied sentiment/score constraints.
The latest completed bar must be no older than one configured bar interval;
the conservative baseline may reject carried daily bars across a closure.
It checks a marketable entry tick inside the entry range, a stop outside supplied
noise/minimum distances, downward quantity sizing under both cash and modeled
account-risk budgets, and the shared net target/reward-risk checks. Arithmetic
uses instrument-currency prices and account-currency costs. The planned
account-risk cap uses reconciled spendable cash. Final tick/lot remainders and
the original allocation/downside/reward inequalities are checked again, including
explicit `budget <= cash` and `acquisition_cost <= cash` checks after rounding. The
quantity correction tries at most two lot reductions and checks that the next
lot would violate a quantity, allocation or risk bound
(`[mod.rs:calculate_limits]`). The
calendar-resolved holding deadline is carried unchanged in the result. Arithmetic
failure rejects the calculation. Evidence flags and supported fee/calendaring/
protection assumptions must be established by callers; this function does not
verify their provenance, execute a calendar or establish live broker capability.
Returned validity is capped by input, quote, account, FX and bar deadlines.

`[mod.rs:CalculatedLimits::authority]` is the returned `FORMULA_V1` string tag.
It is not a permission, durable execution acceptance or owner policy relaxation.

## Side Effects

None. `[mod.rs:calculate_limits]` returns values and performs no I/O or mutation
of input policy, application state or orders. The proof bridge is also pure;
CLI serialization and process execution belong to its callers.

## Verification

Run `cargo test -p trady-policy --locked` and
`cargo clippy -p trady-policy --all-targets --locked -- -D warnings` from the
repository root. Formula-specific evidence/provenance and runtime activation are
separate integration responsibilities.

`tests/limits.rs:allocation_rounding_at_decimal_boundaries_never_spends_past_cash`
retains maximum-magnitude, 28-place and smallest-unit allocation regressions.
The offline `lab/proofs` project proves the reference acceptance checks and runs
exact Lean/production/simulation-adapter parity under its explicit translation
and compiler trust boundary.

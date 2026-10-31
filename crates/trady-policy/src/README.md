# Financial policy

## Overview

[lib.rs](./lib.rs) implements checked arithmetic and deterministic policy checks.
Its feature inputs require verification by the application; they are not model authority.

## Interface

- `[lib.rs:ceil_to]`: checked upward tick rounding. `[lib.rs:floor_to]` and
  `ceil_to` recheck exact remainders and reject exhausted decimal precision.
- `[economics.rs:AcquisitionFill]`, `CostBasis::{validate}`, `cost_basis`,
  `ExitCosts`, `liquidation_proceeds`, `planned_loss_fraction`, `NetReturn`,
  `validate_executable_quote`, `net_executable_return`, `target_price`,
  `StopConstraints`, `StopProposal`, `ProtectiveStop`, `validate_protective_stop`:
  checked linear account-currency acquisition/liquidation/stop economics.
  Current cross-currency FX is positive, directed and at most 60 seconds old.
  Actual fills require deduplication by the caller; a stop is a planned trigger,
  rather than a guarantee of an executable fill price.
- `[limits/mod.rs:calculate_limits]`: approved parameter envelope, checked
  prices/lots and financial constraints; [component contract](./limits/README.md).
- `[prediction/mod.rs:PredictionFusionEngine]`, `expected_utility`,
  `select_scenario`, `score_matured_predictions`, `ChronologicalSplit`:
  calibrated baseline, abstention/cash selection and original-outcome evaluation;
  [component contract](./prediction/README.md). All of these boundaries are pure;
  successful calculations grant no order or deployment authority.

- `[sentiment/mod.rs:sentiment_thresholds]`, `SentimentThresholds`,
  `validate_probabilities`: conservative FinBERT overlay with current-time checks;
  [component contract](./sentiment/README.md).

- `[lib.rs:add]`, `sub`, `mul`, `div`, `floor_to`: checked decimal operations.
- `[lib.rs:Policy]`: versioned defaults and hard-limit validation.
- `[lib.rs:ConfidenceInputs]`, `Catalyst`, `ConfidenceScore`, `confidence`:
  reproducible score components and total. Score components are private.
- `[lib.rs:target_ceiling]`, `validate_risk`: confidence bands and reward/risk checks.
- `[lib.rs:EntryRange]`, `FxObservation`, `EntryCosts`, `InstrumentRules`,
  `SizedEntry`, `size_entry`, `fresh`: conservative linear-cost entry sizing.
- `[lib.rs:validate_account_entry]`: partial account/capability readiness only.
- `[lib.rs:SessionLossGuard]`: deposit-adjusted session pause latch.
- `[lib.rs:Policy::default]` and `[lib.rs:Policy::validate]` provide initial values
  and explicit validation. `[lib.rs:ConfidenceInputs]` and `[lib.rs:Catalyst]` also
  have Rust defaults. `[lib.rs:ConfidenceScore::total]` sums the six private
  components; scores support serialization without deserialization.

## Invariants

- `[sentiment/mod.rs:sentiment_thresholds]` requires an assessment no older than 900
  seconds, unexpired observations no older than 86,400 seconds and distinct
  item/lineage IDs. It validates decimal probability distributions, subtracts 10
  points at maximum negative probability 0.60, vetoes entry at 0.80 and recomputes
  the existing target ceiling. Positive tone adds no points. Review-required
  results disallow target increases; returned flags grant no execution authority.
  Exact configured revision correlation belongs to the provider/application;
  this policy checks the model ID and revision shape, not an approval registry.

- `[lib.rs:Policy::validate]` bounds allocation to 98%, entry confidence to 90–100,
  target ceiling to 10%, initial loss to 2%, reward/risk to at least 1.5, and holding
  duration to 1–5 sessions. Leverage, additions and rotation are disabled.
  Version and decimal limits must be positive. Policy fields are public and
  deserialization does not invoke validation; the holding-session bound does not
  count elapsed sessions (`[lib.rs:Policy]`).
- `[lib.rs:confidence]` generates component maxima 25/20/15/20/10/10. Missing
  observations add no points; other available price signals can still score.
  `[lib.rs:target_ceiling]` enforces confidence bands
  and the exceptional-evidence/policy gate for targets above 3%.
- `[lib.rs:confidence]` awards 25 for verified primary evidence, 20/10/0 for
  direct/indirect/generic catalysts, and 0/8/15 for zero/one/at least two independent
  lineages. Three price signals (positive return, above VWAP, relative volume at
  least 1.5) earn 20; two earn 10. Economics earns 10 at spread/cost ceilings
  0.0020/0.0040, or 5 at 0.0035/0.0060, with both observations present and
  nonnegative. Freshness earns 10 within 24 hours with current sources, otherwise
  5 within 72 hours with continuing relevance. These features are supplied inputs.
- `[lib.rs:target_ceiling]` maps scores 0–69/70–79/80–89 to 0%/1%/2%; remaining
  scores through 100 receive 3%, except 95–100 can receive 10% when both exceptional
  gates are true. The result is capped by policy. `[lib.rs:validate_risk]` separately
  requires the entry-confidence floor, positive target/loss within their caps and
  the minimum target/loss ratio. Checked arithmetic failures return typed errors
  (`[lib.rs:add]`, `[lib.rs:sub]`, `[lib.rs:mul]`, `[lib.rs:div]`).
- `[lib.rs:size_entry]` requires fresh tradable bid/ask within 10 seconds,
  correctly directed FX within 60 seconds when currencies differ, nonnegative
  account-currency costs, valid increments and an ask inside the inclusive range.
  It rounds the buy price upward to a valid tick, floors quantity and budget,
  and rechecks total estimated cost against the budget.
  Both observed and received quote times must be fresh, with observation no later
  than receipt. `[lib.rs:fresh]` rejects future times and accepts the exact maximum
  age. Equal quote/cash currencies use FX 1 and ignore a supplied FX observation.
  `currency_unit` rounds the budget, without rounding estimated cost or remaining
  cash. All four instrument-rule values must be positive; costs follow the supplied
  fixed-plus-proportional model (`[lib.rs:InstrumentRules]`, `[lib.rs:EntryCosts]`).
- `[lib.rs:validate_account_entry]` requires a fresh flat account, terminal orders
  and usable cash-account/instrument/protection/quote capabilities. It grants no authority.
  It does not inspect `fractional_orders`. `[lib.rs:size_entry]` does not invoke
  confidence scoring, risk validation, account readiness or the session loss guard;
  these remain separate calls.
- `[lib.rs:SessionLossGuard::observe]` uses opening equity as denominator and never
  clears a pause after a deposit or rebound.
  Opening equity must be positive; `(current_equity - net_deposits - opening_equity)
  / opening_equity <= -0.02` sets `paused`, including exactly a 2% loss. The method
  neither inspects nor resets `session_id`. Both fields remain caller-owned public
  fields (`[lib.rs:SessionLossGuard]`).

## Side Effects

Only the caller-owned pause flag can change. There is no I/O.

## Verification

Run `cargo test --workspace` from the repository root for workspace verification.

Run `cargo test -p trady-policy --locked` from the repository root.

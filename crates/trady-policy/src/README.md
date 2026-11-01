# Financial policy

## Overview

[lib.rs](./lib.rs) implements checked arithmetic and deterministic policy checks.
Its feature inputs require verification by the application; they are not model authority.

## Interface

- [Optional intelligence](./intelligence/README.md): `[intelligence/mod.rs:assess]`
  dispatches ten deterministic research calculations. These functions have no
  order authority, I/O or promotion path into prediction fusion.

- [Prediction fusion and performance](./prediction/README.md): conservative typed
  forecast fusion, uncertainty/cost-aware scenario utility and matured forecast scoring.

- `[sentiment/mod.rs:sentiment_thresholds]`, `SentimentThresholds`,
  `validate_probabilities`: conservative FinBERT overlay with current-time checks;
  [component contract](./sentiment/README.md).

- `[lib.rs:add]`, `sub`, `mul`, `div`, `floor_to`, `ceil_to`: checked decimal operations.
- `[lib.rs:Policy]`: versioned defaults and hard-limit validation.
- `[lib.rs:ConfidenceInputs]`, `Catalyst`, `ConfidenceScore`, `confidence`:
  reproducible score components and total. Score components are private.
- `[lib.rs:target_ceiling]`, `validate_risk`: confidence bands and reward/risk checks.
- `[lib.rs:EntryRange]`, `FxObservation`, `EntryCosts`, `InstrumentRules`,
  `SizedEntry`, `size_entry`, `fresh`: conservative linear-cost entry sizing.
- `[lib.rs:validate_account_entry]`: partial account/capability readiness only.
- `[lib.rs:SessionLossGuard]`: deposit-adjusted session pause latch.
- `[economics.rs:AcquisitionFill]`, `CostBasis`, `cost_basis`: actual fills,
  attributable charges and historical FX combined into a filled cost basis.
  `[economics.rs:CostBasis::validate]` checks positive quantity and acquisition cost.
- `[economics.rs:ExitCosts]`, `NetReturn`, `liquidation_proceeds`,
  `planned_loss_fraction`, `net_executable_return`, `validate_executable_quote`,
  `target_price`, `StopConstraints`, `StopProposal`, `ProtectiveStop`,
  `validate_protective_stop`: linear liquidation math, trigger prices and initial
  stop validation. Prices use instrument currency; amounts use account currency.
- `[holding.rs:HoldingPlan]`, `HoldingState`, `HoldingContext`, `SessionWindow`,
  `TrailingSettings`, `TrailingState`, `ExitReason`, `ExitLatch`, `HoldingAction`,
  `HoldingDegradation`, `HoldingEvaluation`, `PlanUpdate`, `PlanActivation`,
  `evaluate_holding`, `activate_plan`, `holding_target_ceiling`: pure transitions
  and serde snapshots. `HoldingState::new`, `plan`, `trailing`, `exit_latch`,
  `latest_quote`, `original_deadline` construct/read private holding state.
  `[holding.rs:SessionWindow]` exposes `validate`, `permits_entry`,
  `entry_expires_at` and `validate_entry_time`.
- `[capital.rs:SessionPerformance]`, `session_performance`,
  `account_return_contribution`: cash-flow-adjusted operational loss and separate
  reporting denominators.
- `[limits/mod.rs:calculate_limits]`: versioned formula calculator; its
  [component contract](./limits/README.md) lists its supplied-input constraints.
  It also exports `FormulaFamily`, `FormulaParameters`, `FormulaEnvelope`,
  `FormulaEvidence`, `LimitsInput` and `CalculatedLimits`, with
  `[limits/proof_bridge.rs:Request]`, `Operation`, `Reply` and `evaluate` for
  offline formula/decimal evaluation.
- `[prediction/mod.rs:PredictionFusionEngine]`, `expected_utility`,
  `select_scenario`, `score_matured_predictions`, `ChronologicalSplit`: calibrated
  baseline, explicit abstention/cash selection and original-outcome evaluation;
  see the [prediction contract](./prediction/README.md).
  `[prediction/mod.rs:FusionConfig]`, `FusionContext`, `FusionStatus`,
  `FusedPrediction`, `OutcomeEstimates`, `ReturnNormalization`, `UtilityConfig`,
  `ScenarioUtility`, `UtilitySelection` and `[prediction/history.rs:PredictionPerformance]`
  provide configuration and read-only results. The engine exposes `new`, `config`,
  `fuse` and `fuse_ablation`; configuration validators remain explicit.
- `[lib.rs:Policy::default]` and `[lib.rs:Policy::validate]` provide initial values
  and explicit validation. `[lib.rs:ConfidenceInputs]` and `[lib.rs:Catalyst]` also
  have Rust defaults. `[lib.rs:ConfidenceScore::total]` sums the six private
  components. Deserialization goes through `[lib.rs:ConfidenceComponents]` and
  `ConfidenceScore::try_from`, which reject unknown fields and component values
  outside the exact scoring sets; a raw total is not a valid score snapshot.
  This validates numeric shape, not evidence provenance or readiness authority.
  `[lib.rs:Catalyst]` serializes variants as `SCREAMING_SNAKE_CASE`.

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
  deserialization does not invoke validation. `[holding.rs:evaluate_holding]`
  consumes a caller-verified exchange-session number; `[lib.rs:Policy]` itself
  does not derive a calendar or count elapsed sessions.
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
  `[lib.rs:floor_to]` and `ceil_to` use checked remainder and reject increments
  that cannot be represented exactly at the result's magnitude. Successful
  serialization/deserialization of these public records does not validate costs
  or instrument rules; their calculation entry points perform those checks.
- `[lib.rs:validate_account_entry]` requires a fresh flat account, terminal orders
  and usable cash-account/instrument/protection/quote capabilities. It grants no authority.
  It does not inspect `fractional_orders`. `[lib.rs:size_entry]` does not invoke
  confidence scoring, risk validation, account readiness or the session loss guard;
  these remain separate calls.
- `[economics.rs:cost_basis]` sums positive actual fill quantities and converted
  purchase consideration plus nonnegative actual account-currency charges. Fills
  must share instrument and currency pair; callers deduplicate fills. Historical
  FX is checked at each fill time. `[economics.rs:net_executable_return]` uses
  fresh executable bid and current FX; `[economics.rs:liquidation_proceeds]`
  subtracts fixed, proportional and uncertainty charges once. Negative proceeds
  are reported rather than replaced with zero. Nonlinear fee models are unsupported.
  `[economics.rs:ExitCosts::validate]` requires nonnegative fixed/uncertainty
  charges and a rate in `[0,1)`. `[economics.rs:validate_executable_quote]` also
  binds the basis instrument and currency. `[economics.rs:planned_loss_fraction]`
  can report negative loss for profitable stops.
- `[economics.rs:target_price]` solves a positive net target up to 10%, rounds
  upward to a valid tick and recomputes conservative proceeds. It does not check
  confidence/readiness. `[economics.rs:validate_protective_stop]` rounds a long
  stop upward, checks it below bid and outside supplied noise/minimum distances,
  then checks fee/FX-adjusted loss and `[lib.rs:validate_risk]`. It rejects an
  unworkable technical stop instead of searching for a tighter one. A stop is a
  planned trigger, not a fill-price guarantee.
  `[economics.rs:target_price]` enforces its own 10% ceiling independently of a
  stricter policy cap. `[economics.rs:validate_protective_stop]` requires distance
  strictly above zero/noise and at least the supplied minimum.
- `[holding.rs:holding_target_ceiling]` returns the lower of the existing target
  and the current policy cap below 70 while disallowing increases. At 70 and above it applies the existing
  confidence bands without imposing the new-entry confidence floor. Low confidence
  alone creates no exit. `[holding.rs:activate_plan]` validates expected version
  and exactly one version increment, nondecreasing long stop, nonextending
  deadline and nonincreasing trailing activation/distance. Target increases also
  require the caller's checked evidence/permission flag. Available research is
  required for every replacement, including tighter stops and lower targets.
  The policy target cap is checked at construction and replacement; ordinary
  `HoldingState::validate` does not compare an existing target with a tightened
  cap, and evaluation does not automatically lower that existing target.
- `[holding.rs:evaluate_holding]` evaluates the latest fresh accepted quote,
  retaining it against older/conflicting observations. Default trailing activates
  at net return `0.005`, retains peak return and sets a nondecreasing floor to
  peak minus `0.005` (`[holding.rs:TrailingSettings::default]`,
  `[holding.rs:TrailingState::observe]`). Existing latches always win; newly
  observed simultaneous conditions use hard stop, trailing floor, target,
  deadline, maximum sessions, unprotected close, owner close in that order.
  A stale/unusable valuation cannot produce a target/trailing exit. A fresh
  instrument-price stop still works without FX; time/owner exits need no quote.
  An existing latch returns before state validation and current quote/valuation/
  calendar evaluation, with `net_return=None` (`[holding.rs:evaluate_holding]`).
- `[holding.rs:activate_plan]` evaluates old thresholds before replacement. An
  old latch blocks activation; an accepted lower target/tighter stop/trailing
  rule can latch immediately under the new version. Rejections return the
  evaluated old state, including peak/floor changes, for persistence.
  Activation also requires fresh executable valuation, potentially using a retained
  fresh quote, and available session/calendar data. Initial evaluation errors
  propagate as `Err` rather than returned rejection snapshots. Unknown
  outcomes return `HoldingAction::Reconcile`, including when an exit is latched;
  no result grants permission to retry or bypass broker protection coordination.
  Persist snapshots under application serialization/CAS; these functions do not
  make database writes atomic or verify the caller's evidence flags.
- `[holding.rs:SessionWindow::validate_entry_time]` expires proposals at the
  earlier of five minutes and the actual close-minus-30-minute cutoff, exclusive
  at the boundary. `[holding.rs:evaluate_holding]` latches time exits 15 minutes
  before actual close without verified overnight protection or on the final
  permitted holding session; an already-exceeded session count exits immediately.
  Missing calendars block activation. The caller must resolve the original
  deadline and inclusive holding-session count from a verified exchange calendar.
  An unresolved exit at close remains an exit latch, not a closed/flat holding.
- `[capital.rs:session_performance]` uses
  `current_equity - opening_equity - deposits + withdrawals` as P&L and
  `opening_equity + deposits` as denominator. Confirmed gross flows are separate;
  withdrawals never reduce the denominator. Zero denominator returns an unavailable
  fraction. `[lib.rs:SessionLossGuard::observe_cash_flows]` latches at or below
  `-0.02`, on an unavailable fraction or a calculation error. It never clears
  after a deposit/rebound and does not reset `session_id`. Both fields remain
  caller-owned public fields. The legacy `observe` accepts a single signed net
  flow; use `observe_cash_flows` whenever both deposits and withdrawals occurred.
  This operational metric is not a time-weighted investment return.
  `[capital.rs:session_performance]` assumes a common currency and confirmed
  cumulative gross flows; callers establish both. `[capital.rs:account_return_contribution]`
  separately checks matching currencies and positive opening equity.

`[lib.rs:ConfidenceInputs]` defaults omitted fields during deserialization and
rejects unknown fields. `[economics.rs:validate_protective_stop]` requires
`protection_verified=true` and nonnegative minimum/noise distances after quote/FX
valuation. `[holding.rs:HoldingPlan::validate]` requires positive version, stop
and target with target at most 10%; `[holding.rs:TrailingSettings::validate]`
requires nonnegative activation and positive distance. A persisted trailing floor
requires a known peak at least as high (`[holding.rs:TrailingState::validate]`).

`[holding.rs:HoldingState::new]` rounds the initial stop upward and starts with
empty trailing history, exit latch and retained quote. `[holding.rs:evaluate_holding]`
turns research, quote, valuation and calendar failures into degradation flags
while continuing remaining exit checks; invalid state or propagated arithmetic
failures can return `Err`. `[capital.rs:session_performance]` rejects negative
opening/current equity, deposits or withdrawals.

## Side Effects

Only `[lib.rs:SessionLossGuard::observe]` and `observe_cash_flows` change the
caller-owned pause flag. `[holding.rs:evaluate_holding]` and `activate_plan` return
new snapshots. There is no I/O, broker order coordination or durable mutation.

## Application integration

Construct `[economics.rs:CostBasis]` from broker-confirmed filled quantity and
total actual account-currency acquisition cost, or call `cost_basis` with actual
deduplicated fills. Entry prechecks can use conservative sized acquisition cost;
recompute quantity, actual cost and derived limits after each fill.
`net_executable_return(&basis, &quote, &exit_costs, now)` returns
`NetReturn { proceeds, pnl, fraction, fx_rate }`. For an instrument-price stop use
`planned_loss_fraction(&basis, stop_price, &exit_costs, now)`; validate a new entry
with `validate_protective_stop(&basis, &quote, &exit_costs, &constraints, &proposal,
now, &policy)`. The returned `ProtectiveStop` contains `price`,
`estimated_proceeds`, `planned_loss_fraction` and `reward_to_risk`.

`evaluate_holding(&state, &basis, quote.as_ref(), &exit_costs, &context, &policy)`
returns a `HoldingEvaluation`. `activate_plan(&state, &update, &basis,
quote.as_ref(), &exit_costs, &context, &policy)` returns a `PlanActivation` with
`activated`, optional `rejection` and `evaluation`. Persist `evaluation.state` even
on a rejected update. Reconcile unknown outcomes and coordinate native orders
before any software exit. State deserialization requires trusted journal
provenance; it does not establish research, protection or order authority.

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `cargo test -p trady-policy --locked` from the repository root.

Run `cargo test -p trady-policy --test financial --locked` for numeric boundaries,
fee/FX economics, holding-update races, trailing restart and cash-flow accounting.

Run `cargo clippy -p trady-policy --all-targets --locked -- -D warnings` for lints.

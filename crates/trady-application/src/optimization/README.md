# Formula authority and weekly optimization

## Overview

[mod.rs](./mod.rs) binds approved formula parameters and policy hashes to
reproducible limit calculations. [weekly.rs](./weekly.rs), publicly re-exported
by this module, coordinates bounded weekly experiments through persistence and
trusted-lab ports. Research decisions carry no entry or live activation authority.

## Interface

- `[mod.rs:digest]`, `valid_hash`, `valid_id`: serialized SHA-256 and identity
  helpers. Hashes are 64 lowercase hexadecimal characters; IDs contain 1–160
  ASCII alphanumeric characters or `_`, `-`, `.`, `:`.
- `[mod.rs:ApprovedFormula]`: revision/formula/policy hashes, `FormulaParameters`,
  approval/effective/expiry times and proof/parity/evaluation/shadow/owner evidence
  IDs. `[mod.rs:FormulaAuthority]` combines the active approval, `FormulaEnvelope`
  and account-risk ceiling; `validate` and `validate_active` check the binding.
  Its optional `PredictionAuthority` binds `prediction::FusionConfig` and
  `UtilityConfig`. `ScenarioPredictionInput` carries the frozen fusion context,
  typed predictions and approved conditional net payoff estimates.
- `[mod.rs:BoundCalculation]`, `calculate_authoritative_limits(authority, policy,
  input, now)`: schema/calculation/formula/policy/input identities, an
  `authority_hash` binding the complete approved authority, and calculated
  limits. `[mod.rs:FormulaProposal::validate]` replays the stored input and checks
  the current quote, returning `&CalculatedLimits`.
- `[weekly.rs:SessionInterval]`, `RunManifest`: account/mode, source commit and
  cleanliness, champion/dataset, completed and next sessions, cutoff/deadline,
  component hashes and random seed. `RunManifest::{run_id, validate}` expose
  deterministic identity and validation.
- `[weekly.rs:Objective]`, `ExperimentPlan::validate`: campaign/hypothesis,
  envelope/candidates, cumulative trial and wall-clock budgets, objective
  penalties, improvement threshold, outcome minima and development folds.
  `gates_hash` binds the envelope and protected acceptance/budget settings;
  `splits_hash` binds fold intervals and their declared common session count.
- `[weekly.rs:ToolReport]`, `FoldMetrics`, `DevelopmentReport`,
  `development_objective`: bound tool outcomes and scored fold metrics.
- `[weekly.rs:ReconciledWeek]`, `VerificationReport`, `IndependentEvaluation`:
  reconciliation, proof/parity and independent cost/search/causality/risk/economic
  evidence supplied by the lab.
- `[weekly.rs:TrialRecord]`, `WeeklyState`, `WeeklyDecision`: persisted trials,
  inputs/results and reasons/limitations. `[weekly.rs:WeeklyDecisionKind]` has
  `Keep`, `ShadowCandidate`, `ApprovalRequired`, `Blocked`, serialized as
  `SCREAMING_SNAKE_CASE`.
- `[weekly.rs:WeeklyStore]`: mutable `freeze`, `load`, `reserve_trial`,
  `checkpoint` ports. `[weekly.rs:WeeklyLab]`: mutable `reconcile`, `develop`,
  `verify`, `evaluate` ports. This module supplies no concrete store or lab.
- `[weekly.rs:new_weekly_state]` constructs validated state;
  `run_weekly(initial, store, lab, clock) -> Result<WeeklyDecision>` runs/resumes it.
- `[weekly.rs:trial_input_hash]` binds helper evidence to the complete frozen
  manifest, plan and candidate parameters, rather than parameter values alone.
- `[weekly.rs:EquitySubperiod]`, `time_weighted_return`: cash-flow-adjusted chained
  returns over contiguous equity periods.

## Invariants

- `[mod.rs:FormulaAuthority::validate]` checks parameter/policy hashes, evidence
  IDs, `approved_at <= effective_from < valid_until`, and positive account risk
  no greater than policy's maximum initial loss. Active use additionally requires
  `effective_from <= now < valid_until`. Structural validation permits an expired
  approval to remain attached to an open plan; it does not resolve the referenced
  approval evidence or make that approval active again.
- `[mod.rs:calculate_authoritative_limits]` caps calculation expiry at approval
  expiry. `[mod.rs:FormulaProposal::validate]` requires exact original replay and
  matching current-quote recalculation after normalizing calculation time.
  Quote refresh cannot extend original evidence expiry. Save the proposal with
  its final scenario before requesting Perpetual advice.
  It also binds prediction horizon/market/hash to the calculation and requires
  complete calibrated fusion, the verified FinBERT confidence overlay and utility
  strictly above cash. Missing prediction authority or forecasts fail closed.
  Payoffs must identify the input account/snapshot/currency. This validates
  structural bindings; trusted services must resolve actual equity and payoff
  economics independently of model claims.
- `[weekly.rs:RunManifest::validate]` requires schema 1, account/mode agreement,
  a 40/64-character hexadecimal source commit, 1–14 unique chronological completed
  sessions, and `cutoff <= now` with `cutoff < activation_deadline <= next open
  < next close`. Required component hashes cover calendar, features, models,
  calibrators, fusion, labels, policy, costs,
  Rust build, engine, driver, Lean toolchain, mathlib, skill, prompt, objective,
  splits and gates.
- `[weekly.rs:ExperimentPlan::validate]` requires 1–32 distinct candidates,
  cumulative limit 1–32, wall-clock budget 1–86400 seconds and 2–64 chronological
  development folds spanning more than seven days. Equal exchange-session counts
  are supplied calendar facts. Minimum evidence is at least six outer intervals, 100 closed
  trades, four shadow weeks and a positive shadow-trade count.
- `[weekly.rs:development_objective]` uses median mean log return minus maximum
  drawdown, interquartile instability and fixed seven-parameter complexity
  penalties. It rejects empty/count-mismatched reports, differing fold intervals,
  incomplete folds and drawdowns outside `[0,1]` before indexing fold quantiles.
- `[weekly.rs:new_weekly_state]` matches objective/splits/gates/policy hashes
  against the manifest and requires every development fold to close by cutoff.
- `[weekly.rs:run_weekly]` reserves each trial before development. A reserved
  trial lacking a completed result becomes failed on resume. Selection must
  strictly exceed the better champion/cash objective plus minimum improvement.
  Proof/parity, bound independent evaluation and at least 2× cost stress are
  required; outcome counts govern shadow versus approval-required decisions.
  Approval-required also checks clean source and economic feasibility.
- `[weekly.rs:WeeklyStore]` implementations must lock/freeze inputs, enforce
  cumulative campaign counts and fencing. `[weekly.rs:WeeklyLab]` implementations
  must invoke deployment-configured trusted helpers with idempotent sealed
  evaluation; those properties are port requirements, not local implementations.
- `[weekly.rs:within_budget]` checks each clock reading against the original
  persisted `started_at`, elapsed whole seconds strictly below budget and time
  before activation deadline. A nondecreasing clock remains a supplied-clock
  requirement; consecutive readings are not compared. Helper calls must enforce
  their own deadlines; synchronous port calls cannot be interrupted by this runner.
  Resumed development reports
  are revalidated, and cached decisions require the matching `run_id` and cannot
  carry authorization. Generated decisions set
  `entry_authorized` and `live_activation_authorized` to `false`.
- `[weekly.rs:time_weighted_return]` requires 1–10000 contiguous, continuously
  valued periods with positive opening and cash-adjusted closing equity. It
  returns `product((closing - boundary_cash_flow) / opening) - 1`.

## Side Effects

`[weekly.rs:run_weekly]` invokes the supplied clock, store and lab, freezing/loading
state, reserving trials and checkpointing progress/decisions. Existing decisions
return unchanged. Stage errors produce blocked reasons; initialization and storage
errors can propagate. Concrete disk, database, process or network effects depend
on the supplied ports. Formula validation and arithmetic perform no I/O.

Report validation checks metadata, input binding, success and timestamp only.
It does not authenticate tool identity, verify signatures, recompute report
contents or execute Lean/LEAN. The concrete
[PostgreSQL store](../../../trady-storage/src/weekly.rs) enforces frozen inputs,
immutable completed results, first-start preservation and cumulative/fenced
reservations. The [pinned lab adapter](../../../trady-adapters/src/weekly_lab/README.md)
executes configured trusted helpers. This runner optimizes formula parameters;
offline model fitting and calibration have their own
[model-lab contract](../model_lab/README.md).

## Verification

Run `cargo test --workspace --locked` from the repository root. This contract
documents current source; deployment must configure the concrete store, trusted
helpers and reviewed data before running weekly experiments.

# Prediction fusion and performance

## Overview

[mod.rs](./mod.rs) keeps research confidence, text sentiment, quantitative
indicators and calibrated scenario outcomes distinct. Its conservative baseline
uses a complete approved outcome forecast with explicit net payoff estimates;
[history.rs](./history.rs) scores matured original forecasts and checks causal splits.
Prediction advice grants no order or release authority.

## Interface

- `[mod.rs:FusionConfig]`, `FusionContext`, `FusionStatus`: approved source/model,
  calibration, age, uncertainty and scenario/horizon/regime configuration.
  `[mod.rs:FusionConfig::validate]` and `[mod.rs:UtilityConfig::validate]`
  explicitly validate their versioned configuration records.
- `[mod.rs:PredictionFusionEngine]`: `new`, `config`, `fuse` and offline
  `fuse_ablation`. `FusedPrediction` exposes status, context, reason codes,
  duplicated lineages, dependence groups, maximum negative sentiment,
  probabilities and forecast identity through read-only accessors.
- `[mod.rs:OutcomeEstimates]`, `ReturnNormalization`, `UtilityConfig`,
  `ScenarioUtility`, `UtilitySelection`: conditional net account returns,
  account/snapshot/equity basis, frozen penalties and cash comparison.
  `expected_utility(fusion, payoffs, config, now)` returns checked utility;
  `select_scenario(scenarios, config, now)` requires distinct scenario hashes and
  selects the best supported candidate strictly above cash or returns `NoTrade`.
  Equal best utilities retain the first candidate.
  `[mod.rs:ScenarioUtility]` exposes `utility`, `expected_net_account_return`,
  `expected_downside` and `scenario_hash` accessors. `[mod.rs:FusionStatus]`
  serializes as `READY`/`ABSTAIN`; `[mod.rs:UtilitySelection]` uses the `decision`
  tag with `CANDIDATE`/`NO_TRADE` variants.
- `[history.rs:PredictionPerformance]`, `score_matured_predictions`: multiclass
  Brier score, scored/pending-or-censored counts and per-regime sample counts.
- `[history.rs:ChronologicalSplit]`: `validate` checks separated training,
  calibration, validation and test partitions; `training_sample_eligible`
  checks information availability and label-window end against the training cutoff.
- [Domain meanings](../../../trady-domain/src/prediction.rs) own typed prediction
  metadata/payloads, calibration lineage and matured outcome labels.

## Invariants

- `[mod.rs:PredictionFusionEngine::fuse_ablation]` checks bounded, causal, fresh
  compatible predictions, approved revisions, exact horizon/market/scenario and
  regime. A supplied scenario hash must match; research, sentiment and
  quantitative predictions need not supply one. Repeated IDs and shared
  evidence/dependence groups are recorded;
  changed content under an existing prediction ID produces abstention.
- `[mod.rs:PredictionFusionEngine::fuse_ablation]` accepts at most 128 predictions.
  The baseline requires all four source categories and exactly one complete,
  validated outcome distribution with sufficient calibration samples, approved
  calibration report, supported regime and bounded net-return uncertainty.
  Missing quantitative coverage, conflicting research, drift and adverse sentiment
  produce abstention; the negative-sentiment veto is `0.80`. Expiry is capped by
  accepted forecast expiry, horizon and observed-time age limits.
  Offline source ablation cannot produce an entry-ready result.
  Binary target-hit and sentiment probabilities do not fill missing outcome classes.
- `[mod.rs:FusedPrediction]` and `ScenarioUtility` have private fields and no
  unchecked deserialization path. Utility and selection recheck expiry and the
  frozen comparison configuration. Payoff report/cost hashes and normalization
  are required; `[mod.rs:select_scenario]` accepts at most 32 scenarios with the
  same equity basis, decision time and comparison deadline. Individual forecast
  deadlines may differ. Callers supply conditional returns with costs included
  once; validation rejects returns below total account loss
  on the downside. Deployment must resolve actual payoff/calibration provenance.
  `[mod.rs:expected_utility]` subtracts weighted downside, uncertainty and
  complexity penalties from expected net account return using checked arithmetic.
- `[history.rs:score_matured_predictions]` revalidates originals at decision time,
  rejects duplicate identities/mismatched labels and counts unavailable/censored
  outcomes separately. It scores all four outcome classes without inventing wins.
  Both history inputs are bounded to 100,000 entries. `[history.rs:ChronologicalSplit]`
  enforces chronological gaps plus the configured embargo. Training eligibility
  checks `available_at <= label_end_at <= training_end_at`, with no separate
  label-publication-time argument; callers supply actual exchange-calendar label ends.

- `[mod.rs:PredictionFusionEngine::fuse_ablation]` returns `Err` for an invalid
  context/horizon or excessive input count. Prediction-level rejection generally
  returns an `Abstain` result with reason codes.
- `[mod.rs:PredictionFusionEngine::fuse_ablation]` checks duplicate identities
  before excluding an ablated source. Changed content under an excluded source
  can therefore still cause abstention. Every accepted non-drifted prediction
  contributes its expiry and observed-time age deadline to validity, including
  research, sentiment and quantitative inputs.
- `[mod.rs:expected_utility]` requires payoffs available by the original decision
  time, matching forecast identity, positive opening equity, a valid account
  snapshot identifier and a forecast deadline within the comparison deadline.
- `[history.rs:score_matured_predictions]` validates original identities and
  originals before cutoff/payload filtering, and validates all supplied labels,
  including unmatched labels. Future-generated and non-scenario originals enter
  neither output count. Matched label compatibility is checked before label
  availability or censoring, so an unavailable mismatched label still rejects.
  Its Brier score averages the sum of all four squared class errors over scored
  forecasts, without dividing by the number of classes. It returns `None` when
  no forecast is scored; regime sample counts include only scored forecasts.

## Side Effects

None. `[mod.rs:PredictionFusionEngine::fuse]`, `expected_utility`, `select_scenario`
and `[history.rs:score_matured_predictions]` read caller-owned values and supplied
time, return calculated records, and perform no network, filesystem, training,
scheduling or broker I/O.

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `cargo test -p trady-policy --locked` and
`cargo clippy -p trady-policy --all-targets --locked -- -D warnings` from the repository root.
Approved financial forecasts, calibration reports and out-of-sample acceptance
remain deployment inputs rather than products of this module.

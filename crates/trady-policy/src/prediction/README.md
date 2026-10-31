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
- `[mod.rs:PredictionFusionEngine]`: `new`, `config`, `fuse` and offline
  `fuse_ablation`. `FusedPrediction` exposes status, context, reason codes,
  duplicated lineages, dependence groups, maximum negative sentiment,
  probabilities and forecast identity through read-only accessors.
- `[mod.rs:OutcomeEstimates]`, `ReturnNormalization`, `UtilityConfig`,
  `ScenarioUtility`, `UtilitySelection`: conditional net account returns,
  account/snapshot/equity basis, frozen penalties and cash comparison.
  `expected_utility(fusion, payoffs, config, now)` returns checked utility;
  `select_scenario(scenarios, config, now)` selects a unique supported candidate
  strictly above cash or returns `NoTrade`.
- `[history.rs:PredictionPerformance]`, `score_matured_predictions`: multiclass
  Brier score, scored/pending-or-censored counts and per-regime sample counts.
- `[history.rs:ChronologicalSplit]`: `validate` checks separated training,
  calibration, validation and test partitions; `training_sample_eligible`
  checks information and label availability against the training cutoff.
- [Domain meanings](../../../trady-domain/src/prediction.rs) own typed prediction
  metadata/payloads, calibration lineage and matured outcome labels.

## Invariants

- `[mod.rs:PredictionFusionEngine::fuse_ablation]` checks bounded, causal, fresh
  compatible predictions, approved revisions, exact horizon/market/scenario and
  regime. Repeated IDs and shared evidence/dependence groups are recorded;
  changed content under an existing prediction ID produces abstention.
- The baseline requires all four source categories and exactly one complete,
  validated outcome distribution with sufficient calibration samples, approved
  calibration report, supported regime and bounded net-return uncertainty.
  Missing quantitative coverage, conflicting research, drift and adverse sentiment
  produce abstention. Offline source ablation cannot produce an entry-ready result.
  Binary target-hit and sentiment probabilities do not fill missing outcome classes.
- `[mod.rs:FusedPrediction]` and `ScenarioUtility` have private fields and no
  unchecked deserialization path. Utility and selection recheck expiry and the
  frozen comparison configuration. Payoff report/cost hashes and normalization
  are required; multi-scenario comparison requires the same equity basis and horizon.
  Conditional returns include costs once and cannot exceed total account loss
  on the downside. Deployment must resolve actual payoff/calibration provenance.
- `[history.rs:score_matured_predictions]` revalidates originals at decision time,
  rejects duplicate identities/mismatched labels and counts unavailable/censored
  outcomes separately. It scores all four outcome classes without inventing wins.
  `ChronologicalSplit` enforces chronological gaps plus the configured embargo;
  callers supply actual exchange-calendar label ends.

## Side Effects

None. The functions read caller-owned values and supplied time, return calculated
records, and perform no network, filesystem, training, scheduling or broker I/O.

## Verification

Run `cargo test -p trady-policy --locked` and
`cargo clippy -p trady-policy --all-targets --locked -- -D warnings` from the repository root.
Approved financial forecasts, calibration reports and out-of-sample acceptance
remain deployment inputs rather than products of this module.

# Point-in-time model lab

## Overview

This offline baseline fits a distinct four-class softmax model for executable
target-first, stop-first, filled expiry and non-fill outcomes. It fits conditional
net payoffs from supplied reconciled execution/cost records. It does not fit or
modify the native binary Perpetual model. The [operator guide](../../../../services/inputs/README.md)
describes commands, deployment pins and honest approval requirements.

## Interface

- `[mod.rs:ObservationDataset::validate]`, `PointInTimeRow`, `ScenarioFeatures`,
  `PayoffRecord`, `TrainingPlan::validate`, `Window`: strict, chronological supplied
  observations, scenario features, real conditional costs and frozen fit windows.
  `[mod.rs:ScenarioFeatures::validate]` checks target/stop/spread and session bounds.
- `[fit.rs:fit]`: training-only scaling/softmax/ridge fitting, separate temperature
  calibration and validation reporting; returns an unapproved `ModelCandidate`.
- `[mod.rs:ModelCandidate]::{hash,validate}`, `Metrics`, `ReliabilityBin`:
  immutable model identity, schema/support checks and actual measured diagnostics.
- `[fit.rs:evaluate]`, `EvaluationReport`, `[journal.rs:EvaluationJournal]::{open,evaluate}`:
  independent holdout scoring and durable consumption. Command callers use the
  journal, not the pure scoring function, to enforce consumption across runs.
- `[mod.rs:ModelApproval]`, `ApprovedModel::{validate,report_id,estimation_id,calibrator_id}`:
  independently authored deployment approval records, report IDs and evidence pins.
  No fitting/evaluation function creates an approval.
- `[inference.rs:SignalObservation]`, `SupplementalSignals`, `ForecastObservation`,
  `infer_scenario`: original observed/available/generated times and complete
  `ScenarioPredictionInput` bound to an exact `LimitsInput`/`BoundCalculation`.
- `[inference.rs:infer_offline]`, `OfflinePrediction`: explicitly wrapped diagnostic output with
  `deployment_eligible=false`; this wrapper must not be published as a production
  prediction response. TEST ONLY artifacts are permanently production-ineligible.
  It checks candidate validity and evaluation-to-candidate hash, but does not call
  `ApprovedModel::validate`.
- `[journal.rs:read_document]`, `[journal.rs:write_immutable]`, `[mod.rs:FORMAT]`, `MAX_DOCUMENT_BYTES`,
  `MAX_ROWS`, `MAX_FEATURES`: strict duplicate-free bounded file contracts,
  synchronized immutable publication and explicit computational limits.

## Invariants

- `[mod.rs:ObservationDataset::validate]` and `[fit.rs:fit]` synthesize no financial
  observations, outcomes, charges, payoff classes or missing feature values.
  Four classes and complete actual payoff/cost records are required in each fit
  partition. Pending, censored and ambiguous outcomes
  cannot become wins. Actual source/execution hashes remain review obligations.
- `[mod.rs:ObservationDataset::validate]` requires original observation/availability
  cutoffs by decision time. Label end and outcome availability are separate.
  Window-crossing feature/label
  intervals and embargo crossings are purged by `[fit.rs:partition]`. Keep holdout
  data in separate files operationally; the APIs accept in-memory datasets.
  `[fit.rs:fit]` rejects decisions at/after the declared sealed boundary;
  `[fit.rs:evaluate]` checks a different dataset ID plus schema/time compatibility.
- `[mod.rs:TrainingPlan::validate]` bounds fitting to 50–5000 iterations and embargo
  to 604800 seconds. `[fit.rs:fit]` requires every class in each fit partition:
  at least 4 observations per class for TEST ONLY data or 30 otherwise; configured
  regime minima are at least 8 or 100 respectively.
- `[fit.rs:fit]` fits transformations, classifier and conditional payoff regressions
  on training only. Temperature and empirical residual uncertainty fit calibration
  only. Validation
  never refits parameters. The candidate freezes before independent evaluation;
  holdout residuals are reported without changing the calibrated uncertainty.
- `[journal.rs:EvaluationJournal::evaluate]` records an immutable claim before
  scoring, rejects candidate/data revisions and overlapping claimed holdout
  intervals, and returns the original report on identical retries. Interrupted
  claims remain consumed. A crash-created journal lock requires operator
  inspection/recovery. The claimed
  interval begins at the first row's feature-window start, not the minimum across
  all rows; decision ordering does not order feature-window starts. Existing-claim
  scans are bounded to 5000; cached retries return the original report without
  rescoring at the new supplied time.
- `[fit.rs:metrics]` diagnostics include multiclass/per-class Brier, log loss, ten
  reliability bins per class, counts, regime/horizon breakdowns, actual
  account-return/payoff errors
  and a supplied cash-P&L comparator. The selection diagnostic uses a predeclared
  expected-return-minus-calibration-error rule; it is not production performance
  or evidence of causal attribution. Empty selection has no performance number.
- `[inference.rs:build]` rejects missing/unseen regimes, horizons, feature schemas
  and inputs outside training support. It requires finite outputs, economically consistent
  complete conditional payoffs and exact decimal probabilities summing to one.
- `[inference.rs:build]` uses the existing v1 domain/fusion complete-forecast source
  slot `PERPETUAL`. This baseline uses that wire slot with its own exact approved model
  ID, not the native binary artifact/model ID. Native binary advice remains a
  separate readiness operation. No binary probability is expanded to four classes.
- `[inference.rs:infer_scenario]` independently checks the authoritative
  formula/calculation, account normalization, report/model/cost identities, source
  times and all four
  required fusion sources. `[mod.rs:ApprovedModel::validate]` checks supplied
  eligibility flags, digests and chronology; actual source truth, reviewer
  independence and evaluation eligibility require deployment review. Approved
  inputs remain advice, not trading authority.

## Side Effects

`[fit.rs:fit]`, `[fit.rs:evaluate]` and `[inference.rs:build]` compute in memory
and call no provider or broker. `[journal.rs:read_document]` rejects final-path
symlinks, excessive size and duplicate keys; the requested DTO determines unknown
field rejection. `[journal.rs:write_immutable]` publishes synchronized immutable
artifacts, comparing existing outputs by parsed JSON equality. New Unix temporary
files use mode 0600; `[journal.rs:private_directory]` sets newly created final
directories to 0700 and preserves existing-directory permissions.
`[journal.rs:EvaluationJournal::evaluate]` creates a short-lived exclusive lock
with no explicit file mode; it preserves claims on failure. The lock covers claim
examination and publication and is released before reading a cached report or
scoring. `[journal.rs:write_immutable]` publishes through an exclusive temporary
file and hard link; a directory-sync failure can return an error after publication.
Concurrent publication errors propagate without an automatic equality retry.
Caller-supplied source truth, retention rights and independent review are resolved
at the deployment service boundary, rather than inferred from dataset metadata.

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

From the repository root, run
`cargo test -p trady-application model_lab::tests --locked`.
Run `cargo test -p trady-cli --test inputs_model_pipeline --locked`.
Tests use explicitly marked TEST ONLY observations and check learned probabilities,
cost-conditioned returns, leakage/purge gates, immutable holdouts, fusion parity
and permanent rejection of test artifacts from production approval/inference.

# Offline native training executable

## Overview

`train_candidate` fits and evaluates an unapproved native Perpetual binary model
from the [historical research dataset](../../../lab/training/README.md). It is a
separate Cargo example, with no production runtime registration or broker client.

## Interface

- `[train_candidate.rs:run]` accepts `fit|tune|evaluate DATASET FIT_DIRECTORY`. Its Rust
  definitions are private; the command is the component interface.
- `[train_candidate.rs:Dataset]` and `Row` bind research purpose, source-history
  digest, ordered feature schema, dates, partitions and binary labels.
- `[train_candidate.rs:fit_native]` uses the pinned Perpetual 3.0.0-rc.2 LogLoss
  booster, seed 42 and two threads. Ordinary `fit` uses budget 1 and at most 32
  iterations; `tune` selects from a fixed grid. `[train_candidate.rs:fit]` exports
  `native-model.json` and `candidate.json` after native serialization roundtrip.
- `[train_candidate.rs:tune]` tests budgets 0.25, 0.5 and 1, each with iteration
  limits 8 and 32. `[train_candidate.rs:fold_rows]` builds purged expanding folds
  for 2019, 2020 and 2021 inside the original training partition. Each fold has
  separate fitting, six-month calibration and validation windows. Selection
  minimizes equally weighted mean fold log loss, with Brier breaking ties. The
  selected settings are refitted on the original training partition and
  calibrated on its separate calibration partition. `tuning-report.json` records
  every fold, candidate and prevalence baseline and is hashed into `candidate.json`.
- `[train_candidate.rs:native_json]` re-encodes native JSON through ordered maps
  before hashing/exporting, eliminating randomized internal hash-map key order.
  The native roundtrip still verifies identical calibration predictions.
- `[train_candidate.rs:calibrate]` fits bounded monotone sigmoid calibration on
  the calibration partition only. `[train_candidate.rs:evaluate]` uses the frozen
  model and calibration on a separate holdout, producing `holdout-report.json`.
- `[train_candidate.rs:metrics]` reports binary Brier score, log loss, sample
  counts and mean probability against the frozen training-prevalence baseline.

## Invariants

`[train_candidate.rs:validate]` rejects deployment-eligible inputs, unexpected
partitions, changed feature order/schema, nonhex history hashes, nonfinite data,
duplicate instrument/date identities within a partition, future label dates,
insufficient class support and noncausal date ordering. Each required partition
contains 100–20,000 rows and at least 20 observations of each binary class.
The exact schema is `daily_replay_features_v1` with ordered features `return_1`,
`return_5`, `return_20`, `volatility_20`, `range_20`, `volume_ratio_20` and
`distance_sma_20`; the label is `gross_barrier_1pct_5bars_v1`.
`[train_candidate.rs:fit]` requires train labels to end before calibration
features begin; `[train_candidate.rs:evaluate]` requires holdout features to begin
after calibration labels end. Input files are bounded to 16 MiB before reading.

`[train_candidate.rs:tune]` accepts only train/calibration input and selects using
the training partition alone; neither the final calibration partition nor any
holdout is used for selection. A fold needs at least 100 rows and 20 observations
per class in each window. A best-ranked candidate without an average Brier and
log-loss advantage over the prevalence baseline is explicitly reported as
`NO_WALK_FORWARD_ADVANTAGE`; completing tuning does not imply measured advantage.

`[train_candidate.rs:validate_calibration]` checks finite coefficients within
fitting bounds and a training prevalence strictly between zero and one. Evaluation
checks model bytes against the candidate artifact hash and hashes the exact
candidate bytes it used. Both feature and label identities must match.

Reports remain `deployment_eligible=false`, even if both holdout scores improve.
Neither fitting nor evaluation supplies a production manifest, four-outcome
forecasts, reconciled trading economics, prospective evidence or established
profitability. Native model metadata additionally records `trady.research_only`.

`[train_candidate.rs:write]` creates claims and outputs exclusively. Claims block
repetition in the same directory but do not prevent copying datasets elsewhere
or establish an independent reviewer identity. Interrupted runs remain partial.
This workflow trusts local filesystem ownership: pre-read metadata checks are
not atomic against concurrent replacement, and ancestor symlinks/permissions
are not universally validated.

## Side Effects

`[train_candidate.rs:read]` reads local files; fitting performs native CPU work.
`directory` requests 0700 for newly created directories, and `write` requests 0600
on Unix, synchronizing file contents. Existing directory permissions are not
changed. The executable writes local claims/artifacts and stdout/stderr only;
it contains no direct network, broker mutation, model-promotion or release call.
Tuning reserves `tuning-claim.json` before fitting its 18 fold models, then writes
`tuning-report.json` and the final refitted candidate. Its claims and partial-run
behavior follow the same exclusive-write contract as fit/evaluate.

## Verification

From the repository root:

```sh
cargo build --locked -p trady-adapters --example train_candidate
cargo test --locked -p trady-adapters --example train_candidate
cargo clippy --locked -p trady-adapters --example train_candidate -- -D warnings
```

The Python component contract contains the ordered fetch/prepare/fit/evaluate
commands. The already-observed holdout must not be reused to claim independent
validation for revised models.

To tune using only an existing fit partition, choose an unused output directory:

```sh
target/debug/examples/train_candidate tune var/research-training/new-run/dataset/fit.json var/research-training/new-run/tuned-model
```

The tuned candidate still needs a genuinely fresh evaluation dataset and the
separate deployment review evidence. Do not evaluate it on an already exposed
holdout to claim independence.

# Demo training and setup results

The native research model was trained on real historical data and evaluated on
a separate chronological holdout. **It did not outperform the baseline.** The
project's available local setup and skill corrections are complete, but the
autonomous market demo remains blocked by four deployment prerequisites below.
Entry-order API calls remain disabled by `shadow / record_only` operation.

This report supersedes the current-state conclusions in the earlier
[selection report](./demo-setup-selection-2026-11-01.md); historical reports
remain unchanged.

## Completed setup

- Selected and broker-verified the bounded research set: MSFT, JPM, JNJ, XOM
  and PG. This is a test universe, not a portfolio recommendation.
- Implemented the [historical training pipeline](../lab/training/README.md)
  and [native fitter/evaluator](../crates/trady-adapters/examples/README.md).
- Corrected five runtime skills, reviewed the entire 52-file bundle, and
  installed bundle 2.1.1 solely for shadow research instructions. The
  [scope decision](./demo-skill-review-2026-11-01.md) records the review evidence.
- Verified local PostgreSQL and FinBERT services earlier in this setup;
  analyst, executor and gateway single-run role checks passed with
  `entry_authorized=false`. These checks do not establish market readiness.
- Added two leaf README contracts and updated their two parent contracts using
  the EGM bootstrap workflow. The subsequently requested
  [four-contract drift report](./2026-11-01_DRIFT_REPORT.md) records three
  documentation discrepancies; no implementation or trained artifact changed
  during that review.

## Actual training result

Twelve Data returned 13,610 daily OHLCV bars across five stocks, from
2016-01-04 through 2026-10-29. The pipeline produced 6,943 training rows,
2,021 calibration rows and 3,158 holdout rows. Feature and label intervals were
purged across chronological windows; 740 ambiguous observations and 623 rows
outside the windows were excluded.

The pinned Perpetual 3.0.0-rc.2 LogLoss fitter produced 16 trees with seed 42,
a maximum of 32 iterations and separate sigmoid calibration. Fitting did not
read the holdout file. Evaluation used the frozen model and training prevalence
baseline; it did not refit parameters.

| Holdout score (lower is better) | Model | Baseline |
| --- | ---: | ---: |
| Brier | 0.250952619 | 0.249497960 |
| Log loss | 0.695110948 | 0.692142995 |

Result: `NO_MEASURED_HOLDOUT_ADVANTAGE`, `deployment_eligible=false`.
The candidate was not promoted. No tuning or repeated evaluation was performed
after inspecting this holdout. Subsequent source fixes strengthened validation
and provenance checks; they did not alter the fitted artifact or its report.

Local artifacts, intentionally ignored by version control, are under
`var/research-training/2026-11-01/`:

- `history/history.json` and `history/raw/`: source documents and normalized bars.
- `dataset/fit.json`, `dataset/holdout.json`, `dataset/dataset-report.json`:
  separated datasets and exclusion accounting.
- `model/native-model.json`, `model/candidate.json`, `model/holdout-report.json`:
  fitted artifact, calibration metadata and frozen evaluation.
- `proof-harness/`: clean-snapshot proof and parity self-test output.

Native artifact SHA-256:
`457aaccc06956d7e3334561ed9dcb4515b50d6bac5a9d4defd8fe776aba68e44`.

These are gross binary barrier research labels: hypothetical next-open entry,
1% target/stop and five-bar horizon. They omit observed fills, FX, fees and net
payoffs. Historical availability and selection/survivorship bias remain
unresolved. The separate four-outcome model cannot be truthfully trained from
these bars alone; it needs reconciled execution economics and provenance.
Neither this evaluation nor its local claim file establishes independent
production evaluation or profitability.

## Verification

- `cargo test --workspace --locked`: passed earlier in this setup. Opt-in
  database tests remained ignored; this is not a claim that they were rerun.
- Seven Python pipeline tests and four native trainer tests passed.
- Trainer Clippy with `-D warnings` passed. Native export roundtrip passed.
- Negative integration checks rejected a repeated holdout claim and invalid
  calibration/baseline metadata without changing the saved holdout report.
- After skill installation, the native bundle hash-change rejection test and
  CLI missing-prerequisite test each passed.
- `trady skill-manifest skills` exactly matched the approved 52-file manifest.
- Final asset validation passed for 11 canonical JSON assets, 52 skill records,
  205 Markdown files and 1,523 local links. `git diff --check` passed. All 52
  runtime name-to-hash pins matched the catalog and both manifests.
- Lean build and axiom audit passed. The clean-snapshot self-test passed with
  26 theorems, 937 parity cases, 265 malformed-input cases and 10 negative
  checks. Only `Classical.choice`, `Quot.sound` and `propext` were reported.
  The first attempt timed out; retrying with an isolated debug-stripped copy
  of the same executable completed. The main executable was unchanged.
  This uses `proof_fixture` and is harness validation, not formula release
  evidence or prospective trading evidence.

## Final no-submit check

Both commands were run with an empty inherited environment, a minimal PATH,
and an explicitly absent credential-store path:

```sh
target/debug/trady autonomous check --config config/autonomous.demo-no-submit.json
target/debug/trady autonomous run --config config/autonomous.demo-no-submit.json --once
```

The check returned `configuration_ready=false`, `entry_authorized=false`,
`mode=shadow`, `order_submission_policy=record_only`. The run returned
`shadow_blocked`, **cycles=0**, **order_mutations=0**. Its preflight matched the
check and the configuration remained unchanged. The exercised configuration
branch exits before credential loading or network-client construction. No
entry-order API calls were made; network traffic was not independently captured.

Audit file:
`/tmp/trady-demo-no-submit-20261101/shadow-journal/cycle_4b6c879834faca30018b23b6ea3e423c694e5a56f3925268164420bd5d1a3e3b.json`.
Its content SHA-256 matched its filename; phase was `configuration`.

Configuration SHA-256:
`7d3821ca7cefe7bb73a9fbcd73dd53e74307a8bbad306d707540e8f6fe30848a`.
Executable SHA-256:
`863ad68db7b1d4d53130ace5c4a30f13363a8e1afd72a34de56d6214f1f7714a`.

## Remaining deployment prerequisites

| Blocker | What remains |
| --- | --- |
| `approved_model_registry_empty` | A suitable fitted native model, independent evaluation, exact manifest and genuine approval. The current candidate underperformed and remains research-only. |
| `approved_input_service_unconfigured` | Reviewed sources and rights, reviewer identities, account bindings, fresh executable quotes/FX/costs/research, and the deployment HTTPS input service. Existing Massive NBBO access returned HTTP 403. |
| `universe_source_disabled` | Current and historical constituent/classification evidence with mappings and storage rights. Existing FMP access returned HTTP 402. |
| `approved_formula_unavailable` | Compatible formula and prediction authority, genuine evaluation and prospective shadow evidence in addition to proof/parity. A successful fixture self-test is insufficient. |

No subscriptions were purchased and no evidence or release approval was
fabricated to remove these checks. Full autonomous operation has not started.
The next work requires usable feed access, trusted deployment records and
prospective observations; approving placeholders or relabeling the failed
candidate would not complete that work.

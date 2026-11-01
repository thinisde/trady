# Historical research training

## Overview

This component downloads real daily bars and prepares a reproducible binary
barrier-label research experiment for the [native trainer](../../crates/trady-adapters/examples/README.md).
It does not produce execution records, an approved four-outcome model or a release.

## Interface

- `[pipeline.py:main]` exposes `fetch --end DATE --out DIRECTORY` and
  `prepare --history FILE --out DIRECTORY`. Fetch optionally accepts `--selection`
  and `--start` (default `2016-01-01`); selection defaults to
  `config/deployment.demo-selection.json` under the repository root.
- `[pipeline.py:fetch]` requests one to five selected symbols from Twelve Data's
  fixed HTTPS daily time-series endpoint, with split adjustment and at most 5000
  requested bars. `NoRedirect.redirect_request` rejects redirects. The key comes
  first from `TRADY_TWELVE_DATA_API_KEY`, then from the JSON
  `credentials.TRADY_TWELVE_DATA_API_KEY` entry in `TRADY_CREDENTIALS_FILE` or the
  repository's `config/credentials.local.json`.
- `[pipeline.py:validate_bars]` validates provider identity, currency, interval,
  dates and OHLCV consistency. `[pipeline.py:features]` computes seven ordered
  backward-looking features declared by `FEATURES`.
- `[pipeline.py:barrier_label]` computes a hypothetical next-open entry with a 1%
  gross target, 1% stop and five-bar horizon. Open is observed before high/low;
  conflicting same-bar barriers are excluded rather than assigned an order.
- `[pipeline.py:prepare]` revalidates `saved_document_sha256` and normalized bars,
  then writes `fit.json`, separate `holdout.json` and `dataset-report.json`.
  The recorded transport `response_sha256` is not revalidated.
- `[pipeline.py:digest]` returns SHA256; `[pipeline.py:encode]` produces indented,
  newline-terminated JSON bytes with nonfinite values rejected.
- `[pipeline.py:publish]` exclusively creates JSON artifacts without replacing
  prior runs. An interrupted run can leave a partial directory; inspect it and
  use a new output directory rather than deleting prior evidence automatically.

## Invariants

`[pipeline.py:PURPOSE]` and `LIMITATIONS` identify research-only historical replay.
Prepared datasets have `deployment_eligible=false`. Download time is not evidence
that a feature was historically available. Adjusted histories, current-stock
selection and ambiguous-bar exclusions limit the experiment. Labels contain no
actual fills, costs, FX or net returns.

`[pipeline.py:prepare]` uses fixed chronological windows with inclusive starts
and exclusive ends: train `[2016-01-01,2021-12-01)`, calibration
`[2022-02-01,2023-12-01)`, and holdout `[2024-02-01,2100-01-01)`.
Entire feature/label intervals must fit within their window; adjacent partitions'
intervals cannot overlap. Samples within one partition can have overlapping
windows. Every partition needs at least 100 rows and
20 observations of each binary class. Holdout rows are absent from `fit.json`.
Keep the holdout frozen; repeated experiments against the same observed holdout
do not establish independent evaluation.

`[pipeline.py:validate_bars]` requires 100–4999 observations and rejects duplicate
dates, nonfinite or inconsistent bars. `[pipeline.py:fetch]` requires an end date
before today, limits each response to 4 MB and has no paid fallback. Generic
failure output suppresses credential-bearing URLs and provider bodies.

This is a trusted-local-filesystem workflow. Containment and hash checks do not
provide adversarial protection against concurrent path replacement. Do not give
other users write access to the working directory. Operators should keep original
payloads under ignored `var/`; the unrestricted `--out` option does not enforce
that convention. Intended-use and retention rights remain operator obligations.
Fetch requires a nonexistent output directory; prepare permits an existing
directory while requiring each output file to be new (`[pipeline.py:fetch]`,
`[pipeline.py:publish]`).

## Side Effects

`[pipeline.py:fetch]` reads credentials and performs provider GET requests, spacing
them by 13 seconds. It writes parsed original documents and `history.json` and
prints bounded progress. `[pipeline.py:prepare]` is offline. `[pipeline.py:publish]`
requests mode 0700 for its final new parent and 0600 for exclusive files, then
flushes and syncs files; it does not change existing ancestor permissions or
provide atomic publication of the whole multi-file run.

## Verification

From the repository root:

```sh
cargo test --workspace --locked
python3 -B -m unittest discover -s lab/training -v
cargo test --locked -p trady-adapters --example train_candidate
```

For a fresh research run, choose new output directories:

```sh
python3 -B lab/training/pipeline.py fetch --end 2026-10-30 --out var/research-training/new-run/history
python3 -B lab/training/pipeline.py prepare --history var/research-training/new-run/history/history.json --out var/research-training/new-run/dataset
cargo build --locked -p trady-adapters --example train_candidate
target/debug/examples/train_candidate fit var/research-training/new-run/dataset/fit.json var/research-training/new-run/model
target/debug/examples/train_candidate evaluate var/research-training/new-run/dataset/holdout.json var/research-training/new-run/model
```

The date is an explicit example cutoff, not an automatically refreshed market
session. Evaluation is a single frozen-model diagnostic, not deployment approval.

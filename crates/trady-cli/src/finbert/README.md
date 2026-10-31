# FinBERT CLI wiring

## Overview

Private command wiring for explicit sentiment assessment. The public command
remains `sentiment`; the [v1 design](../../../../docs/v1/finbert/README.md) documents
input records, threshold previews and remaining automated integration.

## Interface

- `[mod.rs:HELP]`: command syntax and preview limits.
- `[mod.rs:run]`: `sentiment status --config FILE` and
  `sentiment score --config FILE --input FILE`.
- `[mod.rs:Input]`: normalized `SourceItem` items and optional confidence inputs.

## Invariants

Configuration reads are bounded to 16 KiB and scoring input to 512 KiB. Status
validates configuration and reports `network_checked: false` before constructing
a client. Scoring requires an explicitly enabled provider.

Thresholds are calculated only when confidence inputs are supplied, using the
default policy and no exceptional gate. Output always declares
`evidence_verified: false` and `entry_authorized: false`.

## Side Effects

Status reads a configuration file. Explicit scoring also reads input, samples
time and calls the provider through the application boundary. Root CLI wiring
prints the returned JSON. No database or broker writes occur.

## Verification

Run from the repository root:

```sh
cargo test -p trady-cli --locked
cargo run --locked --bin trady -- sentiment status --config config/finbert/config.example.json
```

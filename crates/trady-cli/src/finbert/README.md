# FinBERT CLI wiring

## Overview

Private command wiring for explicit sentiment assessment. The public command
remains `sentiment`; the [v1 design](../../../../docs/v1/finbert/README.md) documents
input records, threshold previews and remaining automated integration.

## Interface

- `[mod.rs:HELP]` and `run` are the module's public items. `Input` and
  `read_bounded` are private implementation details.
- `[mod.rs:HELP]`: command syntax, service enablement and authorization notes.
- `[mod.rs:run]`: `sentiment status --config FILE` and
  `sentiment score --config FILE --input FILE`.
- `[mod.rs:Input]`: normalized `SourceItem` items and optional confidence inputs.

## Invariants

`[mod.rs:read_bounded]` bounds configuration reads to 16 KiB and scoring input
to 512 KiB, reading at most the limit plus one byte before rejecting oversize
input. `[mod.rs:run]` status validates configuration, reports
`network_checked: false` and returns without constructing a client. Scoring
requires an explicitly enabled provider.

`[mod.rs:run]` accepts the exact flag order shown above; extra or reordered
arguments fail. Thresholds are calculated only when confidence inputs are
supplied, using the default policy and no exceptional gate. Scoring returns
`thresholds: null` when confidence inputs are absent. Both commands
declare `entry_authorized: false`; scoring also declares `evidence_verified:
false`, while status omits that field.

## Side Effects

`[mod.rs:run]` status reads a configuration file. Explicit scoring also reads input, samples
time and calls the provider through the application boundary. Root CLI wiring
prints the returned JSON. No database or broker writes occur.

## Verification

Run from the repository root:

```sh
cargo test -p trady-cli --locked
cargo run --locked --bin trady -- sentiment status --config config/finbert/config.example.json
```

Run `cargo test --workspace --locked` for workspace verification.

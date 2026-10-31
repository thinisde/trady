# Trady policy

## Overview

Pure calculations over domain observations and caller-supplied time.

`[src/lib.rs:SessionLossGuard::observe]` is the explicit mutable boundary: it
updates a caller-owned pause flag when the adjusted session loss reaches 2%.

## Subdirectories & Boundaries

- [Source contract](./src/README.md): policy, scoring, sizing and risk checks.

## Interface

- `[src/sentiment/mod.rs:sentiment_thresholds]`, `SentimentThresholds`,
  `validate_probabilities`: pure sentiment adjustment and entry/target constraints;
  [versioned rules](../../docs/v1/finbert/README.md#threshold-calculation-version-1).

- [Cargo target](./Cargo.toml): `trady_policy`, using domain, chrono, decimal and serde.
- `[src/lib.rs:Policy]`, `[src/lib.rs:confidence]`, `[src/lib.rs:target_ceiling]`
  and `[src/lib.rs:validate_risk]`: validated policy limits, scoring and target/loss checks.
- `[src/lib.rs:size_entry]`, `[src/lib.rs:validate_account_entry]` and
  `[src/lib.rs:SessionLossGuard]`: sizing, partial account readiness and the pause latch.

## Invariants

`[Cargo.toml:dependencies]` contains no network client or storage driver. Policy
validation is called by sizing and target/risk validation.

`[Cargo.toml:dependencies]` describes direct dependencies. The source uses checked
decimal arithmetic and validates observations against the supplied `now`.
`[src/lib.rs:size_entry]`, `[src/lib.rs:validate_risk]`,
`[src/lib.rs:validate_account_entry]` and `[src/lib.rs:SessionLossGuard::observe]`
are separate calls; successful sizing alone does not imply the other checks passed.
`[Cargo.toml:package]` and `[Cargo.toml:lints]` inherit edition 2024, Rust 1.93 and
the workspace unsafe-code prohibition.

## Side Effects

`[src/lib.rs:SessionLossGuard::observe]` can latch its caller-owned pause flag.

## Verification

Run `cargo test --workspace` from the repository root for workspace verification.

Run `cargo test -p trady-policy --locked` from the repository root.

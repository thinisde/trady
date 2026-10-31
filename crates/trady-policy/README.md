# Trady policy

## Overview

Pure calculations over domain observations and caller-supplied time.

`[src/lib.rs:SessionLossGuard::observe]` and `SessionLossGuard::observe_cash_flows`
are the explicit mutable boundary: they update a caller-owned pause flag when
the adjusted session loss reaches 2% or the metric is unavailable. Holding
evaluation returns a new caller-owned snapshot for serialized persistence.

## Subdirectories & Boundaries

- [Source contract](./src/README.md): policy, scoring, sizing, acquisition/exit
  economics, holding transitions, session accounting and risk checks.

## Interface

- `[src/sentiment/mod.rs:sentiment_thresholds]`, `SentimentThresholds`,
  `validate_probabilities`: pure sentiment adjustment and entry/target constraints;
  [versioned rules](../../docs/v1/finbert/README.md#threshold-calculation-version-1).

- [Cargo target](./Cargo.toml): `trady_policy`, using domain, chrono, decimal and serde.
- `[src/lib.rs:Policy]`, `[src/lib.rs:confidence]`, `[src/lib.rs:target_ceiling]`
  and `[src/lib.rs:validate_risk]`: validated policy limits, scoring and target/loss checks.
- `[src/lib.rs:size_entry]`, `[src/lib.rs:validate_account_entry]` and
  `[src/lib.rs:SessionLossGuard]`: sizing, partial account readiness and the pause latch.
- `[src/economics.rs:cost_basis]`, `net_executable_return`, `liquidation_proceeds`,
  `planned_loss_fraction`, `target_price` and `validate_protective_stop`: checked
  account-currency economics with actual fill costs and linear exit charges.
- `[src/holding.rs:evaluate_holding]`, `activate_plan`, `holding_target_ceiling`
  and `SessionWindow`: old-threshold-first holding decisions, monotonic protection,
  exclusive entry expiry and actual-session close checks.
- `[src/capital.rs:session_performance]`, `account_return_contribution`: gross-flow
  session loss denominator and separate position/account reporting denominators.
- [Formula contract](./src/limits/README.md): the separate `limits::calculate_limits`
  calculator. Its returned `authority` tag is data, not an execution permission.
- [Prediction contract](./src/prediction/README.md): versioned source semantics,
  calibrated fusion, net utility, `NO_TRADE` and chronological historical scoring.

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

`[src/economics.rs:cost_basis]` requires actual deduplicated fills from callers;
`[src/capital.rs:session_performance]` assumes common currencies and confirmed
cumulative flows. `[src/holding.rs:HoldingContext]` requires verified calendar,
session, evidence and protection inputs. Callers persist returned snapshots under
serialization/CAS; decoding state grants no authority.

## Side Effects

`[src/lib.rs:SessionLossGuard::observe]` and `SessionLossGuard::observe_cash_flows`
can latch their caller-owned pause flag. `[src/holding.rs:evaluate_holding]` and
`activate_plan` return snapshots; they do not persist state or submit orders.
`[src/holding.rs:evaluate_holding]` preserves exit latches and returns `Reconcile`
for unknown order outcomes even when an exit is latched.

## Verification

Run `cargo test --workspace` from the repository root for workspace verification.

Run `cargo test -p trady-policy --locked` from the repository root.

Run `cargo clippy -p trady-policy --all-targets --locked -- -D warnings` for lints.

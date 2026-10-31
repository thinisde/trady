# Trady domain

## Overview

Provider-neutral records and validated identifiers, with no network or storage driver.

The research module also supplies content/rights metadata and a caller-owned
redaction operation (`[src/research.rs:SourceItem::redact]`).

## Subdirectories & Boundaries

- [Source contract](./src/README.md): exported domain types and validation methods.

## Interface

- `[src/sentiment/mod.rs:SentimentAssessment]` and related DTOs/constants: decimal
  probabilities, exact text/model identity and source lineage/expiry observations.

- [Cargo target](./Cargo.toml): `trady_domain`, using chrono, rust_decimal and serde.
- `[src/lib.rs:AccountSnapshot]`, `OrderIntent`, `Capability` and their explicit
  validation/classification methods define execution observations.
- `[src/lib.rs:research]` exports research records and collection state, including
  `[src/research.rs:SourceItem]`, `SourceRights` and `SourceState`.

## Invariants

`[Cargo.toml:dependencies]` contains no provider DTO, HTTP client or database driver.
Rust 1.93, edition 2024 and the unsafe-code prohibition come from the workspace.

The dependency statement describes direct dependencies (`[Cargo.toml:dependencies]`).
`[src/lib.rs:identifier]` validates generated identifier types, while public records
and derived deserialization require explicit observation validation. Research
redaction is also an explicit call; supplied rights and expiry metadata are not
authenticated by the domain (`[src/research.rs:SourceItem::redact]`).

## Side Effects

Domain validation returns values or typed errors. `[src/research.rs:SourceItem::redact]`
mutates caller-owned content/status. The package performs no I/O.

## Verification

Run `cargo test --workspace` from the repository root for workspace verification.

Run `cargo test -p trady-domain --locked` from the repository root.

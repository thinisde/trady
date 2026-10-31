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
- `[src/financial.rs:FinancialObservation]`, `FinancialPayload`, `DataRights`,
  `Capability`, `FinancialSnapshot`, `FinancialCoverage` and `FinancialSignals`
  define normalized financial context, consumer rights, currentness and quality
  metadata; explicit checks include `DataRights::permits` and `Capability::current`.
- `[src/lib.rs:research]` exports research records and collection state, including
  `[src/research.rs:SourceItem]`, `SourceRights` and `SourceState`.
- `[src/perpetual.rs:ModelManifest]`, `AssessmentContext`, `Assessment`,
  `AssessmentRecord`, `PerpetualReview`: scenario/feature/model provenance,
  explicit approval/drift/shape validation and advisory reviews.
- `[src/screening.rs:screen_universe]`, `UniverseSnapshot`, `ScreeningObservations`,
  `ScreeningPolicy`, `ScreeningResult`: bounded ISIN/MIC/currency identity joins,
  evidence-based research shortlists, peer controls and rejection reasons.
- `[src/prediction.rs:Prediction]`, `PredictionMetadata`, `PredictionPayload`,
  `ForecastHorizon`, `CalibrationRecord`, `MaturedOutcome`: distinct typed research,
  sentiment, event/scenario and quantitative observations with explicit causal/
  calibration/payload validation; construction alone grants no authority.

## Invariants

`[Cargo.toml:dependencies]` contains no provider DTO, HTTP client or database driver.
Rust 1.93, edition 2024 and the unsafe-code prohibition come from the workspace.

The dependency statement describes direct dependencies (`[Cargo.toml:dependencies]`).
`[src/lib.rs:identifier]` validates generated identifier types, while public records
and derived deserialization require explicit observation validation. Research
redaction is also an explicit call; supplied rights and expiry metadata are not
authenticated by the domain (`[src/research.rs:SourceItem::redact]`).
`[src/perpetual.rs:ModelManifest::validate]` requires explicit current approval
validation; `[src/perpetual.rs:Assessment::validate]` has no wall-clock check.
`[src/screening.rs:screen_universe]` preserves missing data and
requires declared source rights, current observations and owner allowlisting;
its `ResearchOnly` finalist grants no order authority.
`[src/screening.rs:BrokerStockSupport]` and `TimingObservation` require trusted
broker-verifier/calendar inputs; metadata checks do not establish their truth.

## Side Effects

Domain validation returns values or typed errors. `[src/research.rs:SourceItem::redact]`
mutates caller-owned content/status. The package performs no I/O.
Persistent content purging remains a caller responsibility
(`[src/research.rs:SourceItem::redact]`).

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `cargo test -p trady-domain --locked` from the repository root.

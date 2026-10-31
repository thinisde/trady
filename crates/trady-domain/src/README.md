# Domain records

## Overview

[lib.rs](./lib.rs) defines the shared execution vocabulary. Deserialization does
not replace the explicit observation validation methods.

[research.rs](./research.rs), exported as `[lib.rs:research]`, adds provider-neutral
research observations, collection state and an explicit content-redaction method.

## Interface

- `[prediction.rs:PredictionSource]`, `PredictionTarget`, `CalibrationStatus`,
  `ForecastHorizon`, `PredictionUncertainty`, `UncertaintyUnit`, `CalibrationRecord`,
  `PredictionMetadata`, `PredictionPayload`, `Prediction`: typed versioned input
  records with explicit `validate(now)` and closed source/payload meanings.
- `[prediction.rs:OutcomeProbabilities::{values, validate}]`, `QuantitativeIndicators`,
  `ScenarioOutcome::index`, `MaturedOutcome`: four-way probabilities, indicator
  missingness and original-scenario labels bound to their convention version.
- `[prediction.rs:valid_prediction_id]`, `valid_prediction_hash`, `probability`:
  identifier/hash/probability checks. Metadata validation requires causal
  observation/availability/generation times, current expiry, bounded lineages,
  exact probability partitions and chronological out-of-sample calibration claims.
  These checks do not authenticate provenance or establish forecast accuracy.

- `[sentiment/mod.rs:SentimentProbabilities]`, `SentimentDocument`, `SentimentPrediction`,
  `SentimentBatch`, `SentimentEvidence`, `SentimentAssessment`: provider-neutral
  sentiment records. Constants expose schema version, shipped model/revision and
  batch/text bounds; see the [component contract](./sentiment/README.md).

- `[lib.rs:Error]`, `Result<T>` and `require`: common fallible validation.
- `[lib.rs:identifier]`: internal macro generating public `AccountId`, `InstrumentId`,
  `IntentId` and `WorkerId`;
  `[lib.rs:Currency]` and `[lib.rs:Money]` represent currency-aware amounts.
- `[lib.rs:Mode]`, `PositionState`, `Side`, `OrderStatus`, `Support`, `IntentState`:
  closed lifecycle and capability enums.
- `[lib.rs:Position]`, `BrokerOrder`, `AccountSnapshot`, `Quote`: broker/market observations.
- `[lib.rs:Capability]`, `Capabilities`, `OrderIntent`, `Fence`, `IntentResolution`,
  `Reconciliation`: capability and execution records.
- `[lib.rs:identifier]` and `[lib.rs:Currency]` expose `new`, `as_str`,
  `TryFrom<String>` and conversion back into `String`; identifiers also implement
  `Display`. `[lib.rs:Error]` implements `Display` and `std::error::Error`.
- `[lib.rs:AccountSnapshot::validate]`, `[lib.rs:OrderIntent::validate]`,
  `[lib.rs:Capability::usable]` and `[lib.rs:OrderStatus::terminal]` are explicit
  validation and classification calls.
- `[research.rs:SourceKind]`, `SourceRights`, `ContentStatus`, `SocialAction`,
  `SocialObservation`, `ReferencedPost`, `SourceItem`, `CollectionStatus` and
  `SourceState` describe research content, rights metadata and collection state.
  `[research.rs:SourceItem::redact]` takes caller-supplied time and a `for_model` flag.
  `[research.rs:SourceState]` includes revision/registry/rights metadata, daily request
  date/count, collection windows, continuation/HTTP-cache metadata and retry/status gaps.

## Invariants

- `[sentiment/mod.rs:SentimentAssessment]` supports serialization only, with public
  fields; DTO shape alone grants no authority. Other sentiment DTOs deny unknown
  fields when deserialized. Application correlation and policy validation enforce
  semantic bounds, permissions, model matching and freshness.

- Identifier construction/deserialization allows 1–96 ASCII letters, digits,
  underscores or hyphens; currency requires three uppercase letters
  (`[lib.rs:identifier]`, `[lib.rs:Currency::new]`).
- Records deny unknown fields; financial decimals serialize as strings
  (`[lib.rs:Money]`, workspace `rust_decimal` features).
- `[lib.rs:AccountSnapshot::validate]` checks nonnegative cash, unique positive
  holdings, unique order IDs, positive order quantities and consistent filled
  quantities, including positive partial fills with a remaining quantity.
  Snapshot IDs and broker order IDs must be nonempty. All orders require
  `0 <= filled_quantity <= quantity`; only `Filled` and `PartiallyFilled` receive
  additional status-specific fill checks.
- `[lib.rs:OrderIntent::validate]` requires positive quantity/price and a session ID.
- `[lib.rs:Capability::usable]` requires verified support, nonempty evidence and
  future expiry. Defaults remain unknown. `[lib.rs:OrderStatus::terminal]` recognizes
  only filled, cancelled and rejected orders.
- `[lib.rs:Currency::new]` checks currency syntax only. Snapshot IDs, session IDs
  and capability evidence use nonempty string checks without trimming or the
  identifier macro's character restrictions (`[lib.rs:AccountSnapshot::validate]`,
  `[lib.rs:OrderIntent::validate]`, `[lib.rs:Capability::usable]`). Expiry equal to
  the supplied `now` is unusable. Capability defaults are Rust `Default` values;
  the records do not declare `serde(default)`.
- `[lib.rs:AccountSnapshot]` and `[lib.rs:OrderIntent]` expose public fields;
  construction, mutation and derived deserialization do not call `validate`.
  Snapshot validation does not check freshness. Intent validation does not check
  timestamps or state/broker-order consistency. `[lib.rs:Quote]`, `Money`, `Fence`,
  `IntentResolution` and `Reconciliation` have no validation methods; reconciliation
  does not validate its nested snapshot automatically. `[lib.rs:PositionState]`
  and `[lib.rs:IntentState]` supply enum values without transition rules.
- `[research.rs:SourceItem]`, `SourceRights`, `SocialObservation`, `ReferencedPost`
  and `SourceState` deny unknown fields during deserialization; research enums use
  `SCREAMING_SNAKE_CASE`. Their public metadata fields have no general validation
  method (`[research.rs:SourceRights]`, `[research.rs:SourceState]`).
  `SourceState` derives Rust `Default` without declaring `serde(default)`; rights,
  retention, URL/hash/ID syntax and collection-state consistency are not authenticated
  or generally validated here.
- `[research.rs:SourceItem::redact]` clears title, summary, text and expanded social
  context text when storage is forbidden, expiry is at or before `now`, model use
  is forbidden for a model read, or content is superseded/unavailable. It preserves
  superseded/unavailable status and otherwise sets `MetadataOnly`. This explicit
  call does not authenticate rights metadata or derive expiry from `retention_hours`;
  deserialization does not invoke it.
  `MetadataOnly` or `Snippet` status alone does not trigger redaction; the method
  leaves fields other than the named content/status fields unchanged.

## Side Effects

None for the execution constructors and validators. `[research.rs:SourceItem::redact]`
mutates caller-owned content and status in memory. Neither source file performs I/O.

## Verification

Run `cargo test --workspace` from the repository root for workspace verification.

Run `cargo test -p trady-domain --locked` from the repository root.

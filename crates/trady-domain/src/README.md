# Domain records

## Overview

[lib.rs](./lib.rs) defines the shared execution vocabulary. Deserialization does
not replace the explicit observation validation methods.

[research.rs](./research.rs), exported as `[lib.rs:research]`, adds provider-neutral
research observations, collection state and an explicit content-redaction method.
[perpetual.rs](./perpetual.rs) defines advisory model/scenario provenance;
[screening.rs](./screening.rs) defines and evaluates research-only universe screens.
[prediction.rs](./prediction.rs) keeps research confidence, text sentiment,
target-hit/scenario probabilities and quantitative indicators as distinct payloads.

## Interface

- [Optional intelligence](./intelligence/README.md): `[intelligence/mod.rs:FeatureKind]`,
  `FeatureInput`, `FeatureAssessment`, `IntelligenceScope`, `SourceEvidence`, `FeatureObservation` and
  `AssessmentRecord` define ten feature kinds with stable tool mappings and typed
  scope, chronology, provenance, audience and missingness envelopes.
  The public record modules are `accounting`, `attention`, `coordination`,
  `cross_asset`, `event`, `insider`, `microstructure`, `options`, `short_interest`
  and `tax` (`[intelligence/mod.rs:FeatureKind]`).

- `[financial.rs:Provider]`, `DataRights`, `Capability`, `FinancialPayload`,
  `FinancialObservation`, `FinancialSnapshot` and `FinancialSignals` define
  attributed research context, consumer permissions, version/receipt metadata
  and quality-aware numerical signals.
  `[financial.rs:Consumer]`, `AccessStatus`, `Adjustment`, `DataStatus` and
  `FinancialCoverage` provide consumer/availability/adjustment/coverage metadata.
  `[financial.rs:Provider::credential_name]`, `Provider::budget_key`,
  `Capability::current` and `valid_currency` expose credential-key, provider-wide
  allowance, current-rights and currency-syntax helpers.

- `[research.rs:SourceCoverage]`, `XCountBucket` and `XCountsMeasurement` preserve
  versioned coverage and preflight intervals/buckets/completeness. `SourceKind`
  includes `XRecentSearch`; `SocialAction::Unknown` represents absent action metadata.
  `SourceState` carries optional coverage/counts and a coverage revision.
  `[research.rs:XCountBucket]` accepts legacy `tweet_count` as an alias for
  `post_count`. `[research.rs:SourceState]` requires callers to retain the original
  collection interval while continuing pagination.

- `[sentiment/mod.rs:SentimentProbabilities]`, `SentimentDocument`, `SentimentPrediction`,
  `SentimentBatch`, `SentimentEvidence`, `SentimentAssessment`: provider-neutral
  sentiment records. Constants expose schema version, shipped model/revision and
  batch/text bounds; see the [component contract](./sentiment/README.md).
  These are `[sentiment/mod.rs:SENTIMENT_VERSION]`, `FINBERT_MODEL`,
  `FINBERT_REVISION`, `MAX_SENTIMENT_ITEMS` and `MAX_SENTIMENT_CHARS`.

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
- `[perpetual.rs:TARGET_DEFINITION]`, `valid_reference`, `AssessArguments::validate`:
  target label and scenario reference syntax. `ScenarioBinding`, `MarketObservation`,
  `ScenarioSnapshot`, `FeatureLineage`, `FeatureValue`, `FeatureSnapshot`,
  `AssessmentContext` bind scenario prices/costs/deadline, market and feature lineage.
  Market/scenario/context records expose explicit `validate(now)` methods.
- `[perpetual.rs:FeatureSpec]`, `FeatureSchema`, `TrainingLineage`,
  `ProbabilityCalibration`, `ScenarioSupport`, `ModelManifest`: model artifact,
  schema/order, training/calibration/support and approval provenance.
  `ModelManifest::{validate, supports, check_features}` validate current approval,
  scenario support and ordered feature drift.
- `[perpetual.rs:ModelApproval]`, `AssessmentStatus`, `CalibrationStatus`,
  `DriftStatus`, `LimitsAuthority`, `AssessmentInfluence`: closed advisory enums.
  `Contribution`, `ReturnInterval`, `Assessment`, `AssessmentRecord`,
  `PerpetualReview` carry predictions, input/model provenance and review reasons.
  `Assessment::{unavailable, validate}` constructs unavailability or validates shape.
- `[screening.rs:MAX_UNIVERSE_ROWS]`, `MAX_CANDIDATES`, `MAX_PEER_CONTROLS`,
  `SCREENING_METHOD`: `2048`, `5`, `2`, `sp500-research-screen-v1`.
- `[screening.rs:EffectiveInterval]` exposes `valid` and `contains`;
  `Classification`, `UniverseMember`, `UniverseProvenance`, `UniverseSnapshot`
  describe effective membership/classification and source rights/times.
- `[screening.rs:SupportStatus]`, `BrokerStockSupport`, `TimingObservation`,
  `MarketObservation`, `PriorityObservation`, `ScreeningObservations` carry
  broker identity/support, market/session facts and five optional priority scores.
  `[screening.rs:PriorityObservation]` names the rubric: catalyst, breadth,
  economic transmission, tradability and counterevidence/coverage. Broker minimum
  notionals and market notionals use instrument currency; market notionals require
  a common provider basis (`[screening.rs:BrokerStockSupport]`, `MarketObservation`).
- `[screening.rs:ScreeningPolicy::validate]`, `RejectionReason`, `RejectedMember`,
  `ScreenedCandidate`, `ScreeningPurpose`, `ScreeningResult`: policy, auditable
  exclusions, candidate/peer/finalist output. Purpose has only `ResearchOnly`.
- `[screening.rs:valid_reference]`, `valid_isin`, `valid_gics_pair` validate syntax/
  checksum/classification pairs; `screen_universe(universe, owner_allowlist,
  observations, policy, now) -> Result<ScreeningResult>` evaluates supplied data.
- `[prediction.rs:PredictionSource]`, `PredictionTarget`, `CalibrationStatus`,
  `UncertaintyUnit`: source, semantic target, calibration state and uncertainty
  units. `ForecastHorizon::validate` covers decision/deadline/exchange sessions;
  `PredictionUncertainty`, `CalibrationRecord`, `PredictionMetadata` carry
  model/instrument/market/scenario, causal timestamps, lineage and regime metadata.
- `[prediction.rs:PredictionPayload]`: `Research { subjective_confidence,
  material_conflict }`, `Sentiment { positive, negative, neutral }`,
  `BinaryTargetHit { probability }`, `ScenarioOutcomes { probabilities }`,
  `Quantitative { indicators }`. `OutcomeProbabilities::{values, validate}`
  covers target-first/stop-first/expiry/non-fill; `QuantitativeIndicators` exposes
  optional momentum/ATR/volatility/volume/VWAP/strength/spread observations.
- `[prediction.rs:Prediction::validate]` takes `now` and validates metadata/payload together;
  `valid_prediction_id`, `valid_prediction_hash`, `probability` are public helpers.
  `ScenarioOutcome::{TargetFirst, StopFirst, Expiry, NonFill}` maps to indices
  `0..=3` through `index`; `MaturedOutcome` stores prediction/scenario identities,
  label convention version, horizon/availability times and optional outcome.

## Invariants

- `[financial.rs:FinancialObservation::validate]` checks digest/identity shapes,
  prior-day bars and temporal ordering. Provenance requires an `https://` prefix
  without `?`, `#` or `@`; these syntax checks do not comprehensively validate URLs
  or guarantee absence of credentials.
  `[financial.rs:FinancialPayload::validate]` preserves null metrics and validates
  OHLCV/currency. `[financial.rs:DataRights::permits]` requires normalized-storage
  permission and the requested consumer permission. These records carry no order
  authority and perform no I/O.
  `[financial.rs:DataRights]` defaults omitted permission flags to false, while
  retention and terms remain required. `[financial.rs:Capability::current]`
  additionally checks review/effective times, revision, evidence and positive
  account-reviewed credit cost. These checks do not authenticate terms or accounts.
  `[financial.rs:FinancialObservation]` describes next-UTC-midnight bar publication
  as a producer convention. `[financial.rs:FinancialSnapshot]`, `FinancialCoverage`
  and `FinancialSignals` have no validators; `entry_authorized` is unconstrained
  metadata, and signal records do not compute indicators.

- `[sentiment/mod.rs:SentimentAssessment]` supports serialization only, with public
  fields; DTO shape alone grants no authority. Other sentiment DTOs deny unknown
  fields when deserialized. Callers must establish semantic bounds, permissions,
  model matching and freshness through application correlation and policy validation.

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
  `SourceState` derives Rust `Default`; its added coverage/count fields have field-level
  deserialization defaults for existing stored records. The other fields do not.
  Rights,
  retention, URL/hash/ID syntax and collection-state consistency are not authenticated
  or generally validated here.
- `[research.rs:SourceItem::redact]` clears title, feed title, summary, text and expanded social
  context text when storage is forbidden, expiry is at or before `now`, model use
  is forbidden for a model read, or content is superseded/unavailable. It preserves
  superseded/unavailable status and otherwise sets `MetadataOnly`. This explicit
  call does not authenticate rights metadata or derive expiry from `retention_hours`;
  deserialization does not invoke it.
  `MetadataOnly` or `Snippet` status alone does not trigger redaction; the method
  leaves fields other than the named content/status fields unchanged.
- `[perpetual.rs:valid_reference]` permits 1–128 bytes, with an ASCII alphanumeric
  first character and only ASCII alphanumerics or `_.:-` thereafter. This differs
  from screening's 1–96-character alphanumeric/underscore/hyphen references.
- `[perpetual.rs:ScenarioSnapshot::validate]` requires current matching market/
  scenario identities, positive entry/stop below target, cost in `[0,1)`, horizon
  1–31536000 seconds and deadline exactly calculation time plus horizon. Entry
  scenarios also require `stop < entry`; explicit position references allow a
  break-even or profitable holding stop after changes in cost-derived entry/FX.
  `[perpetual.rs:AssessmentContext::validate]` requires matching feature/scenario
  times, 1–256 unique finite features and 1–32 lineage entries per feature with
  `observed_at <= available_at <= as_of`.
- `[perpetual.rs:ModelManifest::validate]` requires current `Approved` provenance,
  a lowercase SHA-256 artifact hash, exact target definition, chronological
  training/calibration/validation with sufficient purge and calibration quality,
  bounded support/tolerances and valid ordered feature schema/drift thresholds.
  It does not validate runtime-version/source-revision/model-format strings.
  `supports` and `check_features` do not call manifest validation themselves.
  `supports` does not validate context freshness; `check_features` calls
  `AssessmentContext::validate(now)` before checking feature order/drift.
  `ScenarioSupport` stop bounds are signed `(entry - stop) / entry` distances.
  Zero/negative bounds require explicit `supports_positions`; existing positive
  ranges never acquire profitable-stop support by taking an absolute value.
  Holding scenarios must fall within the manifest's actual declared signed range.
- `[perpetual.rs:ModelManifest::check_features]` requires exact feature count/order
  and current lineage; range/rejection thresholds yield `OutOfRange`, warning
  thresholds yield `Warning`. `[perpetual.rs:Assessment::validate]` requires
  version 1 and `AdvisoryOnly`, prediction shape, bounded contribution count and
  limitations;
  OK requires provenance, validated calibration and in-range drift. Non-OK advice
  prohibits predictions/contributions and requires limitations. It takes no
  current clock argument, so shape validation alone establishes no freshness.
  Contribution fields and expected-return magnitude are unchecked; interval
  endpoints require ordering. `[perpetual.rs:Assessment::unavailable]` does not
  validate supplied IDs/reasons, so its result can still fail `validate`.
- `[screening.rs:ScreeningPolicy::validate]` bounds ages/liquidity/cost/spread/
  participation, 1–5 candidates and 0–2 peers. `screen_universe` bounds membership
  and allowlist to 2048 and each observation vector to 4096. Universe failures
  produce blockers/member rejection reasons; invalid policy or size bounds error.
- `[screening.rs:screen_universe]` requires complete current `SP500` membership,
  effective classifications, declared storage permission/retention/provenance,
  valid ISIN checksum/MIC/currency and no duplicate active share class. It joins
  verified broker `STOCK` support by ISIN, MIC and currency rather than ticker,
  and requires explicit owner allowlisting. Metadata checks do not authenticate
  provider data or broker evidence.
  Broker instrument IDs must also be globally unique. `[screening.rs:EffectiveInterval::contains]`
  includes `from` and excludes `until`; invalid intervals contain no time.
- `[screening.rs:evaluate_member]` requires one current market/priority observation,
  positive bid/ask and notionals, bounded spread/cost/participation, sufficient
  liquidity, a current clear exchange window and five known priority components
  in `0..=4`. It also requires `material_exposure=true`; false yields
  `NoMaterialExposure` regardless of the scores. Zero is an observed score;
  missing is rejected. Notional checks do
  not establish account cash affordability or whole-share quantity feasibility.
- `[screening.rs:screen_universe]` ranks by total score descending, fifth component
  descending, spread ascending, daily notional descending and instrument ID.
  Peers are remaining eligible members in the leader's industry group; the first
  candidate is the finalist. Screening nominates research only and grants no
  execution authority. Public construction/deserialization does not run it.
- `[prediction.rs:Prediction::validate]` requires version 1, valid IDs, bounded
  unique evidence/regime lists and causal current timestamps:
  `observed <= available <= generated <= decision <= now < deadline` and
  `generated <= now < valid_until`. Horizons have 1–5 exchange sessions and
  increasing decision/deadline. Optional hashes are lowercase SHA-256; uncertainty
  radius is nonnegative and at most one for class-probability units.
  `[prediction.rs:valid_prediction_id]` permits 1–160 ASCII alphanumerics or
  `_.:-`. Research confidence is at most 100; supplied quantitative ATR is positive,
  volatility/volume are nonnegative and spread fraction is in `[0,1]`.
- `[prediction.rs:Prediction::validate]` binds LLM research to `ResearchEvidence`,
  FinBERT to `FinancialTextSentiment`, Perpetual binary/scenario predictions to
  `TargetBeforeStop`/`ScenarioOutcomes` with a scenario hash, and quantitative
  data to `MarketIndicators`. Sentiment/outcome distributions sum exactly to one.
  Validated calibration requires positive out-of-sample counts, valid report/
  calibrator IDs and training < validation start < validation end < generation.
  It checks supplied metadata without retrieving reports or granting authority.
- `[prediction.rs:Prediction]` requires explicit validation after construction/
  deserialization. `[prediction.rs:MaturedOutcome]` has no validator; its `None`
  outcome denotes censored/ambiguous evidence by documented convention, rather
  than an enforced winning/losing classification.

`[research.rs:SourceCoverage]`, `XCountBucket` and `XCountsMeasurement` reject
unknown fields but have no local validator for approval, interval chronology,
hashes or agreement between buckets and totals. `[prediction.rs:Prediction::validate]`
checks calibration provenance and training/validation chronology only for
`Validated` calibration. It does not require the current regime in
`supported_regimes` or match uncertainty units to payload variants. Quantitative
payloads may contain no indicators; momentum, VWAP distance and relative strength
receive no additional bounds here.

`[screening.rs:valid_gics_pair]` uses a fixed 11-sector/25-industry-group table;
`taxonomy_edition` syntax does not select another table. `[screening.rs:screen_universe]`
checks provenance digest shape and an HTTPS URL prefix without recomputing the
digest or fully validating the URL. `[screening.rs:evaluate_member]` accepts
1–64 syntactically valid priority evidence IDs without requiring uniqueness,
computes spread from the bid/ask midpoint and requires round-trip cost to cover
that spread. Remaining session time must strictly exceed the close buffer.

`[lib.rs:Mode]` uses snake-case serialization; `PositionState`, `Side`,
`OrderStatus`, `Support` and `IntentState` use `SCREAMING_SNAKE_CASE`.
`[financial.rs:FinancialPayload]` uses the `kind` tag with uppercase variants.
`[financial.rs:FinancialObservation::validate]` has no current-time argument:
temporal ordering neither establishes present freshness nor recomputes hashes.
`[perpetual.rs:Assessment::validate]` permits an OK assessment without an expected
return or return interval, but requires target-hit probability; any supplied
interval has nominal coverage in `[0,1]`. `[perpetual.rs:AssessmentRecord]` and
`PerpetualReview` expose deserializable fields without local validators for
nested-record correspondence, review references, immutability or retention.

## Side Effects

None for the execution constructors and validators. `[research.rs:SourceItem::redact]`
mutates caller-owned content and status in memory. Domain source performs no I/O.
Perpetual validation and screening also perform no I/O; screening borrows supplied
observations and constructs result records without fetching or persisting data.
Prediction records and validation perform no I/O or caller-owned mutation.

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `cargo test -p trady-domain --locked` from the repository root.
`perpetual::tests::only_position_scenarios_admit_break_even_or_profitable_stops`
covers holding geometry while retaining strict entry and stop/target ordering.

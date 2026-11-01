# Intelligence documentation drift report — 2026-10-31

Scope: the ten feature specifications and the intelligence-related content in the 17 other documentation contracts updated during implementation. Existing unrelated product contracts and external vendor/legal claims are outside this audit. Explicitly deferred subscriptions, calibration and live promotion are not counted as missing implemented features. This report changes no runtime code.

## docs/v1/features/README.md

- **P2 — Rejected-input audit coverage is narrower than the shared baseline.** The shared contract requires a deterministic record of rejected inputs, but [application `ingest`](../../../crates/trady-application/src/intelligence.rs) records only selected validation/calculation/invalidation failures. An immutable-ID collision or missing predecessor returns before `rejection` is called, even when audit-marker rights permit storage. CLI replay separately records these failures; ordinary import has no equivalent retained event. Narrow the claim to the actual eligible failures or route every eligible failure through the licensed rejection path.
- **P3 — Replay ingestion time is simulated.** The statement that import and replay do not backdate ingestion conflates PostgreSQL imports with offline replay. [CLI `replay`](../../../crates/trady-cli/src/intelligence.rs) calls `ingest` with each supplied historical event time; [memory `insert`](../../../crates/trady-adapters/src/intelligence/mod.rs) stores that time as `ingested_at`. Document that replay simulates the supplied archive clock and does not establish server-attested historical receipt.

## docs/v1/features/alternative-attention-intelligence.md

- **P2 — The normalized JSON example is not the implemented wire contract.** Its `source`, `topic_id`, `value_semantics`, `completeness` and `rights_id` fields do not describe [the strict `AttentionInput` / `AttentionSeries` / `AttentionObservation` types](../../../crates/trady-domain/src/intelligence/attention.rs). The actual input nests observations under series and requires audited resolution, scale, language, bucket, sampling, coverage and origin metadata. Replace the example with the shipped [fixture](../../../schemas/fixtures/intelligence/attention.json), or label it as a conceptual source record requiring an unimplemented translator.
- **P3 — Service names and confidence contract are stale.** `AlternativeAttentionSource` and `AttentionFeatureService` are absent; implementation uses the [shared source port](../../../crates/trady-adapters/src/intelligence/mod.rs) and [shared application functions](../../../crates/trady-application/src/intelligence.rs). `AttentionAssessment` exposes status, robust scores and sample variance, without the separately described confidence field. Document these actual names and distinguish statistical support from any future calibrated confidence.

## docs/v1/features/cross-asset-capital-flow-intelligence.md

- **P2 — The example and named assessment differ from the typed contract.** The flat `series[].value`, string `period`, lowercase `kind` and top-level `rights_id` example cannot represent [the implemented `CrossAssetInput`](../../../crates/trady-domain/src/intelligence/cross_asset.rs), which requires typed per-series observation arrays and explicit horizon, pairs, flows, stress and breadth inputs. The returned type is `CrossAssetAssessment`, not `CrossAssetRegimeAssessment`. Replace the example with the [shipped fixture](../../../schemas/fixtures/intelligence/cross_asset.json).
- **P3 — Module ownership differs.** `trady-application::cross_asset` and `trady-policy::cross_asset_features` do not exist. Joining/calculation is implemented in [policy `intelligence::cross_asset::assess`](../../../crates/trady-policy/src/intelligence/cross_asset.rs), behind the shared application gates. Update the architecture names and ownership.

## docs/v1/features/financial-statement-anomaly-detector.md

- **P2 — The typed-contract example describes an unimplemented output shape.** `fiscal_period`, `features[].quality`, top-level `flag` and `rights_id` are not [the shipped `AccountingInput` / `AccountingAssessment` contract](../../../crates/trady-domain/src/intelligence/accounting.rs). The implementation accepts comparable fact/period/cohort inputs and returns `signals` with metric-specific reasons and optional empirical ranks. `EarningsQualityAssessment` is absent. Align the example and name with the [fixture](../../../schemas/fixtures/intelligence/accounting.json) and actual result type.
- **P2 — Fact revisions do not automatically invalidate stored dependents.** [Accounting `assess`](../../../crates/trady-policy/src/intelligence/accounting.rs) returns descriptive `FinancialFactRevision` records and an invalidation-required reason. [Application `ingest`](../../../crates/trady-application/src/intelligence.rs) only creates a correction invalidation from explicit observation-level `supersedes`; it does not consume those fact revision records. The documented correction workflow needs an explicit caller-supplied invalidation/predecessor precondition or an implemented fact-dependency bridge.
- **P3 — Period/cohort selection is an input responsibility.** The described `trady-application::accounting` service is absent. The shared service accepts an already selected `AccountingInput`; [policy](../../../crates/trady-policy/src/intelligence/accounting.rs) validates its supplied periods, cohort and taxonomy crosswalk. Document that upstream normalization/selection responsibility.

## docs/v1/features/insider-transaction-intelligence.md

- **P2 — Two research transformations remain unimplemented and unmarked as deferred.** The historical owner-size normalization and issuer-fundamentals/price-response join in transformations 4–5 have no corresponding inputs or outputs in [the insider types](../../../crates/trady-domain/src/intelligence/insider.rs) or [policy `assess`](../../../crates/trady-policy/src/intelligence/insider.rs). Current results contain reviewed purchase/sale notionals, classifications, role breadth and distinct-buyer clusters. Mark the additional analyses as future research or implement their point-in-time contracts.
- **P3 — The sample and service/type names are conceptual.** The flat transaction example is neither `OwnershipFiling` nor `InsiderInput`; `InsiderActivityAssessment` and `InsiderPatternService` are absent. The parser returns `OwnershipFiling`, and the policy returns `InsiderAssessment` through the shared application service. Link the [actual fixture](../../../schemas/fixtures/intelligence/insider.json) and identify the example as conceptual if retained.

## docs/v1/features/market-expectations-vs-reality.md

- **P2 — Independent artifact persistence is not implemented.** The architecture specifies independently versioned expectation, actual and reaction artifacts. [Migration 10](../../../migrations/0010_optional_intelligence.sql) and [the repository](../../../crates/trady-storage/src/intelligence.rs) instead persist a complete `FeatureObservation` containing one `EventInput`; there are no independently addressed expectation/reaction persistence ports. Document snapshot-level versioning and required caller-managed component lineage, or implement the independent store.
- **P3 — The example and service boundary are stale.** The flat estimated/reported-value example is not [the strict `EventInput`](../../../crates/trady-domain/src/intelligence/event.rs); it omits frozen-estimate selection, clock verification, nested estimates/actuals and other required fields. `EventStudyService` is absent; [policy `intelligence::event::assess`](../../../crates/trady-policy/src/intelligence/event.rs) and the shared service implement the calculation. Use the [shipped fixture](../../../schemas/fixtures/intelligence/event.json) and actual service boundary.

## docs/v1/features/market-manipulation-detector.md

- **P2 — Supplied price/volume deviation is not exposed as the specified feature.** [Policy `assess`](../../../crates/trady-policy/src/intelligence/coordination.rs) validates `CoordinationMarketDeviation` but retains only a generic correlation caveat. Its market/sector return and turnover values are absent from `CoordinationAssessment` and do not enter the rule contributions. The described optional price/volume-divergence attachment therefore needs an explicit deferred label or a returned descriptive feature.
- **P3 — Record/schema/module names differ from implementation.** `CoordinationSignal`, `ManipulationAssessment`, `trady-application::coordination` and `trady-policy::evidence_quality` are absent. Actual records are [the nested `CoordinationInput` and `CoordinationAssessment`](../../../crates/trady-domain/src/intelligence/coordination.rs), with `risk_of_unreliable_attention` and an optional multiplier, calculated by `intelligence::coordination::assess`. The flat example's `status`, `features`, `collection_completeness` and `rights_id` are not that wire shape. Document the [shipped fixture](../../../schemas/fixtures/intelligence/coordination.json) and shared service.

## docs/v1/features/options-market-intelligence.md

- **P3 — Proposed public names and example have not been reconciled with the shipped interface.** `OptionsContractKey`, `OptionsQuoteObservation` and `ImpliedExpectationAssessment` are absent. [The actual types](../../../crates/trady-domain/src/intelligence/options.rs) are `OptionsQuote`, `OptionsInput` and `OptionsAssessment`; exercise type, full expiry timestamps, price ticks, per-quote chronology and assumptions are explicit. The example uses lowercase rights, one top-level expiry and `rights_id`, rather than the strict typed input plus shared evidence envelope. Link the [shipped fixture](../../../schemas/fixtures/intelligence/options.json) and distinguish the proposed vendor boundary from the implemented normalized source port.

## docs/v1/features/order-book-microstructure-intelligence.md

- **P3 — The example and policy module name differ from the implemented boundaries.** The example's `size`, `depth_levels`, `book_state` and `rights_id` do not form a valid [policy `MicrostructureInput` / `MicrostructureBook`](../../../crates/trady-domain/src/intelligence/microstructure.rs) or [adapter `BookSnapshot`](../../../crates/trady-adapters/src/intelligence/book.rs). The policy uses `size_shares`, explicit feed kind, availability, epoch and source reference; the adapter tracks generation and exchange time. `trady-policy::microstructure` is absent; the current policy boundary is `trady_policy::intelligence::microstructure`. Link the [shipped fixture](../../../schemas/fixtures/intelligence/microstructure.json) and distinguish adapter snapshots from policy inputs.
- **P3 — Zero-size update semantics are broader than the rejection rule.** The acceptance section says zero-size quote updates must be rejected or labeled as an auction/halt. [The sequenced builder](../../../crates/trady-adapters/src/intelligence/book.rs) deliberately accepts zero-size deltas as level deletions, while rejecting zero-size snapshot levels. State these two cases separately.

## docs/v1/features/short-squeeze-intelligence.md

- **P3 — Proposed records and the sample are not the shipped schema.** `ShortCrowdingObservation` and `ShortCrowdingEvidence` are absent. [The implemented contract](../../../crates/trady-domain/src/intelligence/short_interest.rs) uses `ShortInterestInput`, typed report arrays, float and frozen volume observations, adjustment/freshness settings and `ShortInterestAssessment`. The flat example mixes source fields with derived `features` and omits required inputs. Link the [shipped fixture](../../../schemas/fixtures/intelligence/short_interest.json) and identify the proposed provider ports as future boundaries rather than current symbols.

## docs/v1/features/tax-aware-trading-decisions.md

- **P2 — The architecture body contradicts the ephemeral implementation status.** It assigns approved-context storage and broker-event reconciliation to `trady-application::tax` and describes an owner model tool. That module and durable context store do not exist. [The shared application service](../../../crates/trady-application/src/intelligence.rs) rejects tax ingestion before repository access; [model tools](../../../crates/trady-adapters/src/intelligence/tools.rs) exclude tax. The implemented owner path is ephemeral CLI comparison of supplied normalized inputs. Reconcile the body with that boundary and mark any future context store, statement importer and separately permissioned owner tool as deferred.
- **P2 — Timing value of deferred tax is not calculated.** Calculation stage 4 includes timing value, but [policy](../../../crates/trady-policy/src/intelligence/tax.rs) explicitly states that no timing/deferral value is invented. [The input](../../../crates/trady-domain/src/intelligence/tax.rs) has no discount-rate or settlement-timing model; it compares supplied same-horizon values and immediate cash against annual liability ranges. Document this limitation or specify a separately reviewed timing model.
- **P3 — The sample and module paths do not describe the current schema.** The example's `tax_context.country`, `position_lots`, `estimates` and `MISSING_APPROVED_CONTEXT` differ from strict `TaxInput`, `CostBasisLot` and `TaxAssessmentStatus::EstimateUnavailable`. Current modules are under `intelligence::tax`; the supported rule model is explicitly approved linear rate ranges. Link the [shipped fixture](../../../schemas/fixtures/intelligence/tax.json) and document that narrower model.

## docs/v1/features/implementation-report.md

No drift detected.

## crates/trady-domain/src/intelligence/README.md

No drift detected.

## crates/trady-policy/src/intelligence/README.md

No drift detected.

## crates/trady-adapters/src/intelligence/README.md

- **P3 — Book size validation is misstated.** The invariant describes positive grid-aligned prices/sizes. [The builder](../../../crates/trady-adapters/src/intelligence/book.rs) grid-checks prices; sizes have no lot-grid check, and zero-size deltas delete levels. Document positive snapshot sizes and nonnegative delta sizes separately from price tick validation.
- **P3 — The memory repository has no cumulative capacity bound.** The side-effects section calls both book and repository state bounded. [The repository](../../../crates/trady-adapters/src/intelligence/mod.rs) has growing maps/vectors for observations, erased identities, registries, invalidations, rejections and assessments, without a total entry/byte limit. The 1,024-event cap belongs to [CLI replay](../../../crates/trady-cli/src/intelligence.rs), not the repository port. Qualify the claim with the caller's replay limit and distinguish it from the book builder's enforced capacities.

## schemas/fixtures/intelligence/README.md

No drift detected.

## README.md

No drift detected.

## docs/README.md

No drift detected.

## docs/v1/README.md

No drift detected.

## crates/trady-domain/src/README.md

No drift detected.

## crates/trady-policy/src/README.md

No drift detected.

## crates/trady-application/src/README.md

No drift detected.

## crates/trady-adapters/src/README.md

No drift detected.

## crates/trady-cli/src/README.md

No drift detected.

## crates/trady-storage/src/README.md

No drift detected.

## migrations/README.md

- **P3 — The migration-runner description stops at version 9.** The invariants still say `migrate` applies compiled migrations 1 through 9. [The current runner](../../../crates/trady-storage/src/lib.rs) includes `(10, OPTIONAL_INTELLIGENCE_SQL)` and applies versions 1 through 10. Update the range to include the newly documented intelligence migration.

## config/README.md

No drift detected.

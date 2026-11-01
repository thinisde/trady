# Optional intelligence records

## Overview

Strict, serializable research inputs and descriptive assessments for ten optional features. [Application gates](../../../trady-application/src/intelligence.rs) check permissions and evidence; [policy calculations](../../../trady-policy/src/intelligence/README.md) interpret the supplied facts.

## Interface

`[mod.rs:FeatureKind]` maps ten features to stable research tool names. `FeatureInput` uses the JSON envelope `{"feature":"OPTIONS","input":{...}}`; `FeatureAssessment` uses `feature` and `assessment`. `SCHEMA_VERSION` is 1 and `FEATURE_VERSION` is `trady-intelligence-v1`.

| Module | Input and assessment | Supporting public records |
|---|---|---|
| [options.rs](./options.rs) | `OptionsInput`, `OptionsAssessment` | `OptionsQuote`, `OptionsExpiryAssessment`, `OptionsTermStructure` |
| [coordination.rs](./coordination.rs) | `CoordinationInput`, `CoordinationAssessment` | `PromotionEvent`, `PostingBaseline`, `CoordinationTier` |
| [event.rs](./event.rs) | `EventInput`, `EventAssessment` | `EventExpectation`, `MarketReaction` |
| [short_interest.rs](./short_interest.rs) | `ShortInterestInput`, `ShortInterestAssessment` | `ShortPositionMetric`, `ShortInterestStatus` |
| [insider.rs](./insider.rs) | `InsiderInput`, `InsiderAssessment` | `OwnershipFiling`, `InsiderTransaction` |
| [cross_asset.rs](./cross_asset.rs) | `CrossAssetInput`, `CrossAssetAssessment` | `CrossAssetFlow`, `CrossAssetPairAssessment` |
| [attention.rs](./attention.rs) | `AttentionInput`, `AttentionAssessment` | `AttentionSeries`, `AttentionObservation`, `AttentionResolution` |
| [accounting.rs](./accounting.rs) | `AccountingInput`, `AccountingAssessment` | `FinancialFactRevision`, `AccountingComparabilityIssue` |
| [microstructure.rs](./microstructure.rs) | `MicrostructureInput`, `MicrostructureAssessment` | `MicrostructureBook`, `MicrostructureMetrics`, `MicrostructureDynamics` |
| [tax.rs](./tax.rs) | `TaxInput`, `TaxAssessment` | `TaxContext`, `EffectiveRuleSet`, `CostBasisLot`, `TaxAssumptionSnapshot` |

`[mod.rs:IntelligenceScope]`, `SourceEvidence`, `FeatureObservation` and `AssessmentRecord` supply owner/account identity, attributed facts, recorded input identity, computed status and evidence. `Audience` distinguishes owner, external LLM, local inference, training and export. `AssessmentStatus` includes available, partial, unavailable, abstained, invalidated and rejected.

`[mod.rs:FeatureKind::ALL]`, `FeatureKind::tool` and `FeatureKind::from_tool`
enumerate and map the ten tool names. `[mod.rs:FeatureInput::kind]` identifies
the variant; `valid_key`, `valid_hash` and the explicit scope/evidence/observation
`validate` methods check envelope syntax. `[insider.rs:OwnershipForm::is_amendment]`
and `OwnershipForm::sec_name` expose the filing-form helpers.

Additional public record and enum families complement the principal types above:

- `[accounting.rs:FinancialUnit]`, `FinancialConcept`, `FinancialPeriodKind`,
  `FinancialPeriod`, `ShareCountAdjustment`, `ComparableFinancialFact`,
  `AccountingMetric`, `AccountingCohort`, `AccountingPeerObservation` and
  `AnomalySignal` describe comparable facts and empirical ranks.
- `[attention.rs:AttentionSemantics]`, `AttentionEntityKind`,
  `AttentionAgentCoverage`, `VerifiedAttentionEvent`, `AttentionThresholds`,
  `AttentionStatus`, `AttentionPattern`, `AttentionEventTiming`,
  `AttentionEventComparison`, `AttentionQueryAssessment` and
  `AttentionConcordance` describe query scales, events and pattern agreement.
- `[coordination.rs:PromotionKind]`, `CoordinationCoverage`,
  `CoordinationThresholds`, `CoordinationMarketDeviation` and `SourceBurst`
  describe promotion provenance, coverage and suppression inputs.
- `[cross_asset.rs:CrossAssetKind]`, `CrossAssetUnit`, `CrossAssetObservation`,
  `CrossAssetSeries`, `CrossAssetPair`, `CrossAssetStressMethod`,
  `CrossAssetStressDirection`, `CrossAssetStress`, `CrossAssetBreadthMember`,
  `CrossAssetBreadth`, `CrossAssetStatus`, `CrossAssetAlignedValue`,
  `CrossAssetInterval`, `CrossAssetFlowAssessment`, `CrossAssetStressAssessment`
  and `CrossAssetBreadthAssessment` describe aligned flows, stress and breadth.
- `[event.rs:EstimateDispersionDefinition]`, `EventUnit`, `ActualObservation`,
  `EventPrice`, `EventPricingProxy`, `EventConfounder`, `SurpriseNormalization`
  and `EventOutcome` retain release, pricing and interpretation distinctions.
- `[insider.rs:OwnershipTable]`, `FilingCoverage`, `FilingFootnote`,
  `ReportingOwner`, `InsiderLineRef`, `InsiderSplitAdjustment`, `InsiderHolding`,
  `InsiderTransactionKind` and `InsiderRowAssessment` preserve filing lineage and
  classified rows.
- `[microstructure.rs:MicrostructureBookState]`, `MicrostructureFeedKind`,
  `MicrostructureLevel`, `MicrostructureRouting`, `MicrostructureAuction`,
  `MicrostructureMarkoutSide`, `MicrostructureMarkout` and `MicrostructureStatus`
  preserve feed, book and elapsed-window observations.
- `[options.rs:OptionsRight]`, `OptionsExercise`, `OptionsUnderlying`,
  `OptionsPricingAssumptions`, `OptionsVolatilityObservation`, `OptionsEvent`,
  `OptionsStatus`, `OptionsIssue`, `OptionsIvInterval` and `OptionsSkew` preserve
  contract conventions and available or missing model context.
- `[short_interest.rs:ShortPositionReport]`, `ShortFloatObservation`,
  `ShortVolumeScope`, `ShortAverageVolume`, `ShortBorrowObservation`,
  `ShortMarketContext`, `ShortCrowdingThresholds`, `ShortCrowdingFlag` and
  `ShortInterestIssue` preserve denominator and market-context distinctions.
- `[tax.rs:TaxLotMethod]`, `TaxLossTreatment`, `TaxAllowanceAllocation`,
  `TaxRateRange`, `TaxFxRate`, `TaxSplit`, `TaxWithholding`, `AnnualTaxState`,
  `TaxCashObservation`, `TaxDisposalScenario`, `TaxSafetySnapshot`,
  `TaxAssessmentStatus`, `TaxScenarioKind`, `TaxAmountRange`, `TaxLotAllocation`
  and `TaxScenarioEstimate` describe reviewed scenario assumptions and outputs.

## Invariants

- `[mod.rs:SourceEvidence::validate]` requires bounded identifiers, lowercase SHA-256 syntax, explicit rights/units/basis/lineage and observation <= publication <= availability <= receipt < expiry.
- `[mod.rs:FeatureObservation::validate]` requires a version-1 envelope, 1–64 validated evidence entries and a distinct valid predecessor ID when supplied. Hash integrity and source permissions require application validation.
- `[mod.rs:FeatureInput::subject]` exposes the instrument, issuer, region or optional event subject for reviewed mapping. Serde rejects unknown input fields; deserialization alone does not authorize use.
- Financial values use `Decimal`. Options pricing, cross-asset statistics and attention statistics use explicitly checked finite floating-point calculations in policy. Input records preserve currency, time, adjustment and semantic distinctions.
- `[mod.rs:AssessmentRecord]` has a separate calibration label and entry-authority field; the application always emits `RESEARCH_ONLY_UNCALIBRATED` and `false`. These records do not convert to return forecasts or execution commands.
- `[tax.rs:TaxAssessment]` preserves the supplied safety snapshot; personal tax inputs belong only in the owner’s ephemeral comparator.

Validation is explicit: `[mod.rs:FeatureObservation::validate]` does not validate
the feature payload, compare its subject with envelope/evidence identities,
recompute hashes or resolve predecessors. Public fields do not enforce immutable
identity. `[mod.rs:SourceEvidence::validate]` does not validate currency, venue,
completeness/gap consistency or expiry against a decision-time clock.
`[mod.rs:AssessmentRecord]` has no local validator for its public calibration
string or authority flag; the application-emitted values above are an application
contract. Feature records describe semantic requirements without enforcing them
during construction or deserialization.

The field comments below describe producer contracts; the records do not enforce
them locally. `[accounting.rs:FinancialUnit::scale]` converts as-filed amounts to
base units, and `[accounting.rs:AnomalySignal::empirical_percentile]` is a
comparable-cohort midrank. `[attention.rs:AttentionSeries::scale_available_at]`
retains the scale's actual availability; independent samples belong to the same
query/scale/bucket, and `[attention.rs:AttentionConcordance]` compares patterns
without summing incompatible magnitudes.

`[coordination.rs:PromotionEvent]` carries opaque source-scoped author IDs,
terminal origin ancestry and verifier-linked disclosures.
`[coordination.rs:CoordinationInput::invalidated_origin_ids]` describes cascading
lineage invalidation; these DTOs perform no
cascade. `[cross_asset.rs:CrossAssetKind]` distinguishes primary net flow from
secondary trading volume, and `[cross_asset.rs:CrossAssetSeries::entity_id]`
binds flow/AUM to the same fund. `[event.rs:MarketReaction]` relies on
caller-verified exchange sessions; `[event.rs:EventAssessment::abnormal_return]`
describes stock total return minus matched benchmark return without establishing
causality.

`[options.rs:OptionsQuote]` uses premiums per underlying share and preserves an
unknown deliverable basis; `[options.rs:OptionsMoveScale]` is a model scale.
`[microstructure.rs:MicrostructureBook]` carries materialized snapshots rather
than deltas, with currency/share prices and share sizes. Depth persistence and
quote markouts do not establish individual-order survival or confirmed fills.
`[short_interest.rs:ShortPositionMetric]` separates outstanding positions from
daily short-sale volume; days-to-cover remains a volume heuristic.
`[insider.rs:PlanAnnotation]` preserves reviewed versus unknown plan status and
incomplete coverage cannot establish zero activity. `[tax.rs:TaxCashBasis]`
separates gross and net cash; replacement terminal values are unrealized marks
without subsequent sale tax (`[tax.rs:TaxReplacementScenario]`).

Further producer contracts remain explicit metadata requirements:
`[insider.rs:InsiderTransaction]` separates content-derived line identity from
economic transaction identity and explicit amendment targets.
`[microstructure.rs:MicrostructureInput]` requires preceding history in the same
feed/epoch with monotonic sequences. `[options.rs:OptionsPricingAssumptions]`
uses continuously annualized ACT/365 carry without implicit zero rates or
dividends; `[options.rs:OptionsQuote]` preserves the contractual expiry instant.
`[short_interest.rs:ShortInterestAssessment]` measures publication lag by calendar
dates and borrow-fee range within one provider methodology.

`[tax.rs:EffectiveRuleSet]` supports explicitly approved linear scenarios;
`[tax.rs:TaxFxRate]` distinguishes documented dated FX from future illustrative
assumptions. `[tax.rs:CostBasisLot]` retains reviewed valuation dates and quantities
before separately supplied confirmed splits. `[tax.rs:AnnualTaxState]` excludes
proposed disposals; `[tax.rs:TaxWithholding]` retains one deduction identity for
deduplication, and `[tax.rs:TaxDisposalScenario]` declares exact gross/net broker
cash reconciliation. These records do not perform those validations.

## Side Effects

None. These definitions and validators perform no I/O, persistence, network access or order operations.

## Verification

Run `cargo test -p trady-domain -p trady-policy --locked` and `cargo test -p trady-adapters --test intelligence --locked` from the repository root.
Run `cargo test --workspace --locked` for workspace verification.

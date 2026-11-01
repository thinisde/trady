# Deterministic intelligence calculations

## Overview

Descriptive research calculations over [typed supplied facts](../../../trady-domain/src/intelligence/README.md) and caller-supplied time. Source access, consumer rights, subject mapping, payload integrity and durable retention are enforced by the application.

## Interface

`[mod.rs:assess]` accepts `&FeatureInput` and `DateTime<Utc>` and returns `Result<FeatureAssessment>`. Each module exposes `assess(&ModuleInput, at) -> Result<ModuleAssessment>`:

| Module | Calculation |
|---|---|
| [options.rs](./options.rs) | European Black-Scholes bid/mid/ask IV, move scales, matched skew, term structure, historical/sector/realized comparisons and baseline-based event variance |
| [coordination.rs](./coordination.rs) | Provenance-aware activity bursts and suppression-only unreliable-attention tiers |
| [event.rs](./event.rs) | Frozen pre-release estimates, compatible actuals, normalized/standardized surprise, expectation drift, pre-release proxies and observed/confounded reactions |
| [short_interest.rs](./short_interest.rs) | Published short-interest ratios and changes, frozen-volume days-to-cover, borrow-fee comparisons, market context and crowding flags |
| [insider.rs](./insider.rs) | Transaction classification, amendment/deduplication handling and distinct-owner purchase clusters |
| [cross_asset.rs](./cross_asset.rs) | Sparse publication-aware context, primary flows, aligned beta/correlation and uncertainty, configured stress and historical constituent breadth |
| [attention.rs](./attention.rs) | Compatible query scales, robust median/MAD surprises and availability-aware event timing |
| [accounting.rs](./accounting.rs) | Comparable cash/earnings, accrual, receivables/inventory growth and dilution signals with empirical cohort ranks, plus restatement and one-off disclosure metadata |
| [microstructure.rs](./microstructure.rs) | Displayed spread/depth/imbalance, microprice, sequenced dynamics and elapsed-window markouts |
| [tax.rs](./tax.rs) | Dated owner-reviewed lot/FX/rule illustrations for no-action, target and permitted replacement scenarios |

## Invariants

- `[options.rs:assess]` uses supplied carry/rates and ACT/365 with bounded numerical inversion. Malformed configuration/facts return errors; missing carry or unsupported exercise, quotes or expiry produce unavailable metrics or issues. IV implies neither direction nor a return probability.
- `[short_interest.rs:assess]` rejects daily short-sale volume as a position substitute. Publication vintages and denominator methodology remain explicit; absent borrow data is not imputed.
- `[cross_asset.rs:assess]` aligns availability, calendars, currency and adjustment basis. Primary creation/redemption flow and performance-driven AUM changes remain distinct. Missing required series abstain.
- `[microstructure.rs:assess]` withholds continuous metrics for stale or invalid current-book metadata and for auction/halt states. Malformed continuous-book levels, including locked/crossed prices, return `Err`; malformed otherwise-usable historical levels also propagate errors. Regressive history invalidates the assessment; invalid historical states or changed epochs/feed kinds can suppress dynamics while retaining current metrics. Historical age is not checked. Displayed depth does not imply hidden liquidity, queue position, executable consolidated quotes or individual order survival.
- `[insider.rs:assess]` uses filing availability and exact amendment targets. Cash purchases/sales remain separate from awards, exercises, withholding, derivatives and ambiguous plan activity. Partial coverage cannot establish zero activity.
- `[accounting.rs:assess]` requires comparable periods, units, taxonomy, basis and scope. Unsupported denominators and selected revisions with missing originals produce metric-specific abstention; invalid known-target revision chronology/concept/period returns an error. Empirical ranks do not establish fraud.
- `[event.rs:assess]` requires expectations frozen before release and compatible observed reactions. Confounded windows and revised actuals cannot establish an original-release causal surprise.
- `[coordination.rs:assess]` collapses dependent origins and keeps every evidence multiplier within [0,1]. Missing coverage never establishes normal attention or misconduct.
- `[attention.rs:assess]` excludes incompatible, invalidated, censored and bot observations. Zero MAD has no robust z-score; relative indices are not absolute counts or sentiment.
- `[tax.rs:assess]` requires explicit approved rules, dates and context. Withholding is reconciled once; sale cash and annual liability remain separate. Safety constraints are copied unchanged and can block replacement illustrations.
- None of the ten functions emits order authority, updates an execution policy or promotes itself into prediction fusion.

`[mod.rs:assess]` propagates errors from its selected calculation. Validation
errors remain distinct from returned partial, unavailable, review or abstention
statuses. `[options.rs:assess]` can retain valid ATM IV with `Partial` status;
missing comparison/event context can cause that status. Skew uses no interpolation,
move scales cannot exceed expiry, and event variance requires an explicit
available non-event baseline plus a wholly nonnegative uncertainty interval.

`[cross_asset.rs:known]` selects publication/availability/ingestion revisions known
at decision time. `[cross_asset.rs:latest]` does not replace a missing newest value
with older data; pair calculations join exact intervals without forward filling.
`[cross_asset.rs:pair_assessment]` uses checked OLS/Fisher uncertainty with an IID
approximation. Flow calculations require positive preceding opening AUM and
matching performance intervals (`[cross_asset.rs:flow_assessment]`).

`[event.rs:assess]` withholds surprise when reaction validation fails; confounding
can retain numeric surprise while forcing `Inconclusive`. `[insider.rs:assess]`
withholds all totals on unresolved amendment lineage and leaves totals unavailable
when no qualifying cash rows exist. Purchase totals and distinct single-owner
cluster counts have different inclusion rules. `[accounting.rs:rank]` uses
distinct comparable peer issuers and half-weight ties; rank unavailability can
coexist with an available ratio. Restatement invalidation is a returned warning,
not an operation performed by this module.

`[attention.rs:assess_series]` distinguishes whole-query unavailability from
filtering unusable historical values; zero MAD retains partial statistics without
a z-score. `[attention.rs:assess]` is `Unknown` when no query has a robust z-score.
`[coordination.rs:assess]` retracts derived aggregate metrics and its multiplier
on expiration, missing calibration or inadequate coverage/lineage; optional
feature gaps can remain local. Origin groups merge transitively and verified
disclosures are excluded from behavioral concentration/synchronization statistics
(`[coordination.rs:independent_groups]`).

`[short_interest.rs:assess]` selects only published, available and ingested report
revisions. Missing/stale primary reports abstain; incompatible share bases can
require review. `[microstructure.rs:assess]` checks exact registered markout
horizons and retains the supplied confirmed-fill flag; capacity covers only the
selected depth. `[tax.rs:assess]` supports explicit linear, single-tax-year
illustrations; validation errors, `EstimateUnavailable`, `Partial` and
`Illustrative` remain distinct. Lot allocation conserves basis, uses permitted
FIFO/specific identification and dated actual/illustrative FX, and keeps
withholding credit/refund rules explicit. Replacement terminal values are supplied
unrealized marks; their subsequent sale tax is unmodeled.

`[attention.rs:assess]` returns pattern concordance, deduplicated origins and the
supplied coordination-warning reference. Its publicity-manipulation flag records
that reference's presence. Concordance requires compatible region, entity kind
and observation window. `[attention.rs:assess_series]` caps historical buckets
before filtering unusable values; an available robust score with fewer than two
current independent samples remains `Partial`.

`[accounting.rs:Calculation::comparable_growth_periods]` requires matching period
kinds and known, equal fiscal-week counts. `[accounting.rs:rank]` requires current
fiscal duration and formula revision `earnings-quality-v1`, and excludes
conflicting eligible observations for one peer issuer. With coverage and lineage
gates satisfied, `[coordination.rs:assess]` gates behavioral flags on the minimum
post count: zero or one flag yields `Normal`/weight 1, two yield `Elevated`, and
three or four yield `ReviewRequired`. Disclosure exemptions retain aggregate
post/burst counts and origin participation.

`[cross_asset.rs:validate]` requires nonoverlapping, positive-duration return/flow
intervals. Breadth constituents are unique; positive weights sum exactly to one
and must be effective and available by the period start. `[event.rs:normalize]`
requires explicit permission to use the declared floor for near-zero consensus;
standardized surprise also requires at least two analysts and a positive,
explicitly defined sample/population deviation (`[event.rs:assess]`).
`[insider.rs:assess]` applies both filing-availability and transaction-date windows.
The cluster floor applies to each qualifying single-owner purchase.

`[microstructure.rs:assess]` bounds selected depth to 1–1024 levels and history to
512 snapshots; `[microstructure.rs:validate_levels]` bounds each side to 1024
levels. `[options.rs:quote_unavailable]` admits expiries from 60 seconds through
ten 365-day years. `[options.rs:solve]` searches IV from 0.000001 to 5 in at most
128 iterations, requiring residual at most `max(tick * 1e-6, spot * 1e-10)`.
`[options.rs:comparisons]` matches the earliest valid expiry covering the requested
horizon and its remaining tenor: historical IV rank is an unclipped range rank,
while sector percentile is an inclusive empirical CDF. Unavailable forward
variance alone does not force `Partial` (`[options.rs:assess]`).

When finalization is reached, `[short_interest.rs:assess]` makes any remaining
issue `Partial`; a market-context adjustment mismatch suppresses that context
without producing `ReviewRequired`. `[tax.rs:reason]` requires nonblank missing
reasons but returns fixed labels instead of owner prose. `[tax.rs:difference]`
subtracts ranges as lower-minus-upper, central-minus-central and upper-minus-lower;
`[tax.rs:sale_estimate]` reverses settlement bounds for after-tax values.
`[tax.rs:prepare_lots]` preserves total basis across confirmed splits, ignores
identical split duplicates and rejects conflicting or inexact quantities.
`[tax.rs:assess]` bounds replacement acquisition plus costs by immediate sale cash
after withholding.

## Side Effects

None. The dispatcher and all ten `assess` functions calculate and allocate results without I/O, wall-clock reads, storage, model calls or caller-owned policy mutation.

## Verification

Run `cargo test -p trady-policy --lib intelligence:: --locked` and `cargo test -p trady-adapters --test intelligence --locked` from the repository root.
Run `cargo test --workspace --locked` for workspace verification.

# Optional market intelligence (10 independent features)

Status: **implemented deterministic research services**, 2026-10-31. All ten are enabled by default for research and remain outside the approved live acceptance baseline. The implementation includes typed calculations, approved normalized-file ingestion, SEC ownership XML parsing, bounded books, historical replay, optional analyst storage and nine read-only model tools. Tax comparison is an ephemeral owner CLI operation. Source subscriptions, historical alpha validation and live promotion remain separate gates. See the [implementation report and operating guide](./implementation-report.md).

## What is new versus existing plans?

Trady v1 already plans news/X, FinBERT, Perpetual, quantitative indicators, FRED/SEC integrations, an audited Rust trading policy, Trading 212, Telegram, a weekly LEAN/Lean 4 laboratory and shadow-to-live release. Do not reimplement those foundations in this folder. The differentiators below are a **particular new analytical product** layered on the pre-existing adapters and research interfaces.

| Feature specification | New capability beyond current roadmap | Suggested relative priority |
|---|---|---|
| [1. Options market intelligence](./options-market-intelligence.md) | Read-only implied-volatility/term-structure/skew expectations from licensed option chains | High if affordable |
| [2. Suspected manipulation detector](./market-manipulation-detector.md) | Behavioral-coordination and promotional-anomaly risk, beyond simple news deduplication | High |
| [3. Expectations vs reality](./market-expectations-vs-reality.md) | Freeze consensus and model surprise vs pre-release pricing proxies and event reaction | High |
| [4. Short-squeeze intelligence](./short-squeeze-intelligence.md) | Delayed-position-aware crowding and borrow-risk research | Medium |
| [5. Insider transaction intelligence](./insider-transaction-intelligence.md) | Form 4 parsing, cash-vs-award classification and insider purchase clustering; generic SEC already planned | Medium |
| [6. Cross-asset capital flows](./cross-asset-capital-flow-intelligence.md) | Primary ETF flows vs price changes, cross-market funding/stress state and bounded inference | Medium |
| [7. Alternative attention](./alternative-attention-intelligence.md) | Google Trends/Wikimedia/authorized Reddit attention, outside planned X and news | Lower until rights clear |
| [8. Accounting anomaly detector](./financial-statement-anomaly-detector.md) | Cash-earnings divergence, accruals, receivables, restatement and dilution anomaly service; companyfacts already planned | Medium |
| [9. Order-book microstructure](./order-book-microstructure-intelligence.md) | L2 displayed-depth signals, entry fill/adverse-selection assessment, venue-aware confidence | Lower until market data/route validated |
| [10. Tax-aware comparison](./tax-aware-trading-decisions.md) | Explicitly configured *hypothetical after-tax* hold/rotation comparison beyond withholding reports | Medium; reporting-only first |

Priority describes **research order**, not permission to trade. Live activation priorities remain broker protection, real feeds and order-state recovery. For a small-capital single-stock bot, high-fee options/L2 inputs may have poor economic value.

## Common architecture

~~~text
Approved source adapters / original research observations
  -> immutable source-vintage, rights and availability registry
  -> optional standalone feature extractor (Rust analyst)
  -> source-specific typed, provenance-bound assessment
  -> optional analyst storage, historical replay and ephemeral research chat
  -> RESEARCH_ONLY_UNCALIBRATED assessment; entry_authorized=false
~~~

Reuse [free financial data contracts](../free-financial-data.md), [prediction fusion](../prediction-fusion.md), [news provenance](../research.md), [weekly optimization](../weekly-optimization.md), [operations](../operations.md) and [trading policy](../strategy.md). Promotion into fusion requires independent offline validation and release approval; the current implementation has no such promotion path. Multiple feeds can reflect one disclosure, price reaction or campaign and do not establish independent news confirmation.

## Interface

- [Domain contracts](../../../crates/trady-domain/src/intelligence/README.md): `[../../../crates/trady-domain/src/intelligence/mod.rs:FeatureInput]`, `FeatureAssessment`, `SourceEvidence`, `FeatureObservation` and `AssessmentRecord` cover all ten typed features.
- [Pure calculations](../../../crates/trady-policy/src/intelligence/README.md): `[../../../crates/trady-policy/src/intelligence/mod.rs:assess]` dispatches each feature to its deterministic calculation.
- [Adapter contracts](../../../crates/trady-adapters/src/intelligence/README.md): bounded JSON imports, SEC ownership parser, sequence book builder, memory repository and optional model tools.
- `[../../../crates/trady-application/src/intelligence.rs:IntelligenceConfig]`, `IntelligenceRepository`, `ingest`, `assess_ephemeral`, `ReadRequest` and `read` enforce reviewed scope, sources, permissions, vintages and invalidations.
- `[../../../crates/trady-storage/src/intelligence.rs:PgIntelligenceRepository]` and [migration 10](../../../migrations/0010_optional_intelligence.sql) provide analyst-only immutable research persistence.
- `[../../../crates/trady-cli/src/intelligence.rs:run]` provides status, ephemeral assessment, replay, import, stored reads, invalidation and purge. [Configuration example](../../../config/intelligence.example.json) enables no feature or source.

## Invariants

`[../../../crates/trady-application/src/intelligence.rs:check]` binds subjects and source/reference pairs, verifies payload hashes and exact source revisions, and checks capabilities at both decision and computation times. Source evidence chronology is observation <= publication <= availability <= receipt < expiry. Stored reads exclude later ingestion; erasure also blocks earlier replay. Missing and uncalibrated observations never authorize entry.

`[../../../crates/trady-adapters/src/intelligence/tools.rs:IntelligenceTools]` requires ephemeral sessions and revalidates retained context before each model request. External-model permission remains necessary. `[../../../crates/trady-application/src/intelligence.rs:ingest]` rejects tax before any repository operation, and licensed rejection markers expire and follow source/lineage erasure.

### Shared contract baseline

Every proposed new source/assessment must carry:
- schema version, originating provider and exact entitlement/rights revision;
- stable issuer/instrument identification, currency/unit/venue and adjustment basis if relevant;
- observation/event time, source publication time, first independently available time and ingestion time;
- raw source reference/content hash, model/feature version and source-lineage relationships;
- allowed consumers/retention, coverage/partial collection flags, calibration/support and uncertainty;
- explicit unavailable/invalidated/abstained states and a deterministic record of rejected inputs.

Use Rust decimal/checked math for financial state; use explicitly justified numeric representation for statistical calculations. A model score, news sentiment, option IV, tax estimate or crowding measure **cannot become expected return** by type coercion. Raw user tax context and licensed content must not leak into general LLM prompts or public experiment data.

## Cross-feature interactions

| Interaction | Required safeguard |
|---|---|
| Options chain + earnings surprise | Same upcoming event is correlated information; avoid double counting; compare matched horizons |
| Coordination detector + alternative attention | Attention spikes and copied posts may share origin; treat suppression separately from direction |
| Insider filings + accounting flags | Both use SEC but distinct event/filing facts; don't award independent source confirmation for one underlying disclosure |
| Short interest + L2 liquidity | Reporting lag, venue fragmentation and different timescales; do not turn a snapshot into a real-time short position |
| Cross-asset state + individual stock research | Sector and issuer returns share market risk; maintain baseline/ablation and factor exposure |
| Tax comparison + execution | Tax scenarios do not reprice or veto already-latched safety exits, and cannot grant order authority |

## Delivery pathway and measurable gates

1. **Research selection:** all ten research features are enabled by default; the owner selects reviewed inputs for an exact use case and budget. An explicit `enabled` list restricts the feature set, and `enabled: []` opts out. Missing sources and disabled features return explicit missingness rather than a zero-valued signal.
2. **Contract and rights gate:** audit terms, sample access, retention, source timestamps, source quality and alignment to existing Rust adapters. Avoid buying data solely on hypothetical alpha.
3. **Offline deterministic fixtures:** parse and validate source records; prove correction/availability semantics, missingness handling, idempotency, rights invalidation and no side effects on execution.
4. **Historical point-in-time experiment:** register version/threshold and sample cuts before evaluating; use original publication/availability, not retrospective scraping or today's survivorship-filtered universe. Compare against both simple controls and the exact current Trady baseline.
5. **Ablation and cost:** evaluate each feature independently and conditionally, with overlap/correlation and the actual incremental fee/API/VPS spend. Compute uncertainty intervals, calibration, false alarms, drawdown, after-cost return, coverage and performance across regimes. No requirement that all features improve returns.
6. **Prospective paper and shadow:** collect actual scheduled features and record whether they would have changed abstention, selection or execution. Require no degradation of protection/recovery. Insufficient sample support -> remain research-only.
7. **Independent promotion:** follow [weekly approval lifecycle](../weekly-optimization.md) and [live release gates](../implementation.md). The coding agent, LLM, provider feed and feature assessment have no authority to deploy changes themselves.

## Side Effects

`ingest` can store configuration revisions, normalized observations, corrections and licensed rejection markers. `read` purges expired content and stores assessments only when all contributing sources permit derived storage. Invalidation removes linked payloads, assessments and rejection markers. PostgreSQL assigns real ingestion time. Offline replay simulates the supplied historical event clock in process memory; it does not establish server-attested historical receipt.

Pure feature calculations and the ownership XML parser perform no I/O. The JSON importer reads a bounded local file; explicitly enabled intelligence chat can use the configured external LLM under source permissions. This work installs no subscription, schedule or live feed. All ten research flags ship enabled; source rights, subscriptions and owner tax inputs require explicit configuration. Existing exits and protection remain independent of research and LLMs.

Non-goals include options/ETF/bond/short trading, accusation of insider misconduct or fraud, identifying social-media users, legally definitive personal-tax advice, guaranteed daily income, claims of causal price impact without defensible evidence, or making profit forecasts from observed data with future leakage.

## Suggested first experiments

The cheapest candidate is **event expectations vs. reality** when historical consensus entitlements are available; next, **anomaly-based social attention suppression** using the already-authorized X/news pipeline. Higher-cost derivatives and L2 data should pass a *provider sample and total-cost feasibility check* before ingestion work. Tax-aware analysis can begin as a standalone statement-backed reporting experiment without touching trading decisions. Any of these can be rejected if support, rights or benefit is insufficient.

## Verification

Run `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check`, `python3 scripts/test-postgres.py` and `python3 scripts/verify-assets.py` from the repository root. The [implementation report](./implementation-report.md) records results and the remaining empirical/data-access gates. Synthetic fixtures verify deterministic behavior; they do not establish predictive value.

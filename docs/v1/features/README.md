# Proposed market intelligence extensions (10 independent features)

Status: **new optional product proposals**, documented 2026-10-31. These are not part of the approved Trady v1 acceptance baseline and have not been coded, provisioned, backtested or activated. This folder documents ten **distinct analytical services**, not ten mandatory signals or ten new trading strategies. Implementation requires owner selection, rights review, independent evaluation and the existing [v1 release gates](../implementation.md).

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
  -> experimental feature registry and frozen historical snapshots
  -> offline weekly ablation / costs / chronological validation
  -> independently approved compatible fusion input (or ABSTAIN)
  -> existing Rust policy + executor (never modified by an LLM)
~~~

Reuse [free financial data contracts](../free-financial-data.md), [prediction fusion](../prediction-fusion.md), [news provenance](../research.md), [weekly optimization](../weekly-optimization.md), [operations](../operations.md) and [trading policy](../strategy.md). The proposed new inputs are *not* extra independent corroborating news merely because they originate from a different vendor: multiple feeds may reflect the same underlying disclosure, price reaction or promotional campaign.

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

1. **Proposal acceptance:** owner chooses one optional feature, its exact use case and ceiling for API/data/inference costs. Unchosen features remain absent, not "off" signals falsely treated as zero.
2. **Contract and rights gate:** audit terms, sample access, retention, source timestamps, source quality and alignment to existing Rust adapters. Avoid buying data solely on hypothetical alpha.
3. **Offline deterministic fixtures:** parse and validate source records; prove correction/availability semantics, missingness handling, idempotency, rights invalidation and no side effects on execution.
4. **Historical point-in-time experiment:** register version/threshold and sample cuts before evaluating; use original publication/availability, not retrospective scraping or today's survivorship-filtered universe. Compare against both simple controls and the exact current Trady baseline.
5. **Ablation and cost:** evaluate each feature independently and conditionally, with overlap/correlation and the actual incremental fee/API/VPS spend. Compute uncertainty intervals, calibration, false alarms, drawdown, after-cost return, coverage and performance across regimes. No requirement that all features improve returns.
6. **Prospective paper and shadow:** collect actual scheduled features and record whether they would have changed abstention, selection or execution. Require no degradation of protection/recovery. Insufficient sample support -> remain research-only.
7. **Independent promotion:** follow [weekly approval lifecycle](../weekly-optimization.md) and [live release gates](../implementation.md). The coding agent, LLM, provider feed and feature assessment have no authority to deploy changes themselves.

## Side effects, non-goals and operating defaults

**As documented:** zero runtime side effects. No API account, order endpoint, schedule, migration, model, Telegram notification or live policy is changed by these Markdown files.

**When implemented:** isolated rights-approved reads, optional feature storage and offline evaluations only unless independently approved. All ten should ship disabled. Any unavailable source is a missing feature with an explicit reason; the absence of a proprietary expensive feed does not block the current safe policy unless a *separately reviewed* dependency makes it required. Existing exits/protection always remain independent of research and LLMs.

Non-goals include options/ETF/bond/short trading, accusation of insider misconduct or fraud, identifying social-media users, legally definitive personal-tax advice, guaranteed daily income, claims of causal price impact without defensible evidence, or making profit forecasts from observed data with future leakage.

## Suggested first experiments

The cheapest candidate is **event expectations vs. reality** when historical consensus entitlements are available; next, **anomaly-based social attention suppression** using the already-authorized X/news pipeline. Higher-cost derivatives and L2 data should pass a *provider sample and total-cost feasibility check* before ingestion work. Tax-aware analysis can begin as a standalone statement-backed reporting experiment without touching trading decisions. Any of these can be rejected if support, rights or benefit is insufficient.

## Verification of this documentation

Documentation review should check all ten linked files, internal parent-doc paths, explicit planned-versus-implemented status and provider rights caveats. Repository source/tests are unchanged. Use the existing root asset/link checks; no success claim is made without actually running them. An eventual implementation adds integration tests and source-specific acceptance cases from each file, not a declaration that their existence satisfies the v1 test suite.

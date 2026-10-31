# Trady v1 specification

Status: implementation specification. Product name: **Trady**. Application language:
**Rust**. An initial [offline foundation](../foundation.md) now implements a subset
of these contracts. The linked specification describes the full product; the
foundation record identifies working interfaces and validation evidence.

## Purpose and operating model

Trady screens effective S&P 500 membership within owner/broker permissions, selects a sector then an industry-group subsector, researches a finalist and maintains an evidence-backed plan. It combines X, news APIs, Google News RSS, disclosures and configured collectors. PostgreSQL holds profiles, public statements and directed relationships. The owner receives Telegram research/execution/watchdog updates and can submit X URLs.

The LLM researches stocks and proposes evidence-backed scenario constraints.
The target [prediction-fusion architecture](./prediction-fusion.md) combines
distinct LLM research, FinBERT sentiment, calibrated Perpetual forecasts and
quantitative observations. Approved mathematics is the primary numerical
authority for buy/stop/target limits, quantity and horizon; Rust independently
validates risk/readiness and controls execution. Unsupported forecasts abstain
and the selector may return `NO_TRADE`. This supersedes LLM-selected final prices;
deployment remains gated. Holding protection runs independently of inference.

The ordinary objective is approximately 1% daily growth, with position profit targets of 1% normally, 2–3% for stronger opportunities, and up to 10% for an exceptional verified catalyst. These are objectives, not a requirement to trade every day. Returns, losses, costs, and idle cash must be reported accurately. No configured profit target may exceed 10%.

## Reading map

| Document | Scope |
|---|---|
| [Architecture](./architecture.md) | Rust processes, module ownership, ports, scheduling, configuration, and event flows |
| [Research and sources](./research.md) | X, news APIs, RSS, website collectors, evidence quality, and submitted URLs |
| [Free financial data integration](./free-financial-data.md) | Shared Rust data layer, point-in-time snapshots, LLM/ML consumers, quotas, safeguards and delivery gates |
| [Free financial API catalog](./free-financial-api-catalog.md) | Provider endpoints, verified public limits, access caveats, authentication and primary references |
| [Proposed market intelligence extensions](./features/README.md) | Ten new optional feature specifications with data contracts, Rust boundaries, evaluation and safeguards; no implementation or deployment |
| [Sector and subsector funnel](./sector-workflow.md) | S&P 500 eligibility, all-sector comparison, group/branch filtering and finalist selection |
| [Company intelligence](./company-intelligence.md) | Profiles, CEO/counterparty statements, directional mentions and PostgreSQL memory |
| [X accounts and watchdogs](./watchdogs-and-x.md) | Owner's Trump/Musk/Gill/Ackman/Icahn list, listener coverage, holding focus and alternatives |
| [Five-account X pilot and costs](./x-api-monitoring.md) | Proposed Ackman/Musk/Bessent/Burry/Altman pilot, identity evidence, upstream filters, delivery choices, budgets and offline evaluation; no replacement or activation of the owner's existing watchlist |
| [Perpetual advisor](./perpetual.md) | Native Rust inference, feature/label lineage, advisory influence, limit ownership and tool schema |
| [FinBERT sentiment](./finbert/README.md) | Self-hosted Docker inference for X/news/text, source permissions, wire adapters, threshold effects and dormant deployment |
| [Research artifact contracts](./research-contracts.md) | Exact bounded screening/profile/relationship/watchdog/comparison writes and calculators |
| [Research guide sources](./research-sources.md) | Researched primary measurements, taxonomy, attribution and limitations |
| [Trading policy](./strategy.md) | Allocation, confidence points, targets, stops, adaptive holding, and execution state machines |
| [Prediction fusion](./prediction-fusion.md) | Typed model semantics, calibrated baseline, scenario utility, historical evaluation and mathematical limit authority |
| [Prediction implementation record](./prediction-implementation.md) | Tested Rust subset, source contracts, trusted-service obligations and unimplemented proof/backtest/deployment work |
| [Weekly optimization](./weekly-optimization.md) | Local Codex skill, cumulative predictive/formula calibration, offline LEAN, Lean 4 proofs and independent release gates; no installed schedule or validated deployment |
| [Telegram](./telegram.md) | Owner authorization, commands, notifications, and operational control |
| [Memory and data](./memory-and-data.md) | LLM sessions, memory files, knowledge retrieval, PostgreSQL schema, and concurrency |
| [LLM conversations and tools](./llm-and-tools.md) | JSON responses, repeated tool rounds, news/X/T212 information tools, and conversation finalization |
| [Exact JSON contracts](./llm-contracts.md) | Canonical schemas, research artifacts, complete proposal bodies, application envelopes, and worked final responses |
| [Runtime skills](./runtime-skills.md) | Scenario-specific SKILL.md files, loading/versioning, research workflow, and completion records |
| [Operations](./operations.md) | Reusing `paperless-db-1`, provisioning design, deployment, recovery, budgets, and reporting |
| [Implementation and acceptance](./implementation.md) | Delivery phases, meaningful tests, requirements traceability, and launch criteria |
| [References](./references.md) | Current documentation checks and provider capabilities still requiring verification |
| [Original v1.1](./v1.1.md) | Preserved historical design input |

## Decisions that replace the original design

The newer requirements take precedence wherever the original v1.1 conflicts with them.

| Original v1.1 choice | Trady v1 decision | Reason |
|---|---|---|
| Unnamed bot | Trady | Requested product name |
| Python | Rust for all application processes | Requested runtime |
| SQLite | Dedicated `trady` PostgreSQL database | Durable shared state, sessions, and memory |
| Fixed €50 trading capital | Dynamic broker-confirmed cash; €50 is only an example | Owner may deposit more funds |
| €45 maximum entry and €5 fixed reserve | Entry cost budget up to 98% of available cash, preserving at least 2% | Requested allocation policy |
| One buy **or** sell submission per session | At most one entry attempt per session; exits and necessary exit replacements are separate | Enables same-day automatic stop/target exits |
| X as the main scheduled collection source | Multiple source types, including Google News RSS | Broader market research |
| Model output can include `SELL` | Research proposals and plan updates; no model-originated order command | Separates analysis from execution |
| Two scheduled research calls | Scheduled research plus bounded event-driven updates during a holding | Tracks changing evidence and thresholds |
| Single-response model recommendation | Persistent dialogue, repeated JSON tool rounds and approved scenario skills | Completes research and source comparisons before an entry decision |
| Fixed €5 loss-review threshold | Proposed percentage-based loss controls and deposit-adjusted reporting | Capital is variable |
| Limit orders for every order | Limit entries/profit exits; a separately verified native protective-stop profile for loss exits | A limit-only loss exit can remain unfilled after a gap |

There is no longer a unique daily slot shared by buys and sells. Its replacement is an entry-only slot plus position-specific exit intents. Implementing the old `daily_trade_slots` rule unchanged would violate this specification.

## Requirements and initial defaults

Requirements below originate from the requested product behavior. Defaults provide concrete starting values for implementation and paper evaluation; they can be changed by the owner through versioned policy. An LLM cannot change policy.

| Setting | v1 requirement or proposed default |
|---|---|
| Brokerage account | One dedicated cash account; no borrowing, margin, short selling, or automatic deposits |
| Stocks held | One distinct ordinary stock at a time; fractional shares when verified supported |
| Initial universe | Effective S&P 500 members intersected with owner permissions and broker support; at most five shortlisted candidates |
| Broker | Trading 212 Invest is a candidate, subject to API and exit-order capability verification |
| Entry allocation | Up to 98% of synchronized, broker-confirmed available cash, including estimated entry costs |
| Top-ups | Detect owner deposits; apply the same allocation rule to later eligible entries |
| Existing holding | No additional purchase or forced sale merely to reach 98% |
| Entry frequency | Proposed default: one entry submission attempt per exchange session |
| Same-day exit | Allowed and required when an active exit condition is met |
| Normal profit target | 1% estimated net return on position entry cost |
| Strong profit targets | 2% or 3%, according to validated confidence and evidence |
| Exceptional target | Up to 10%; requires verified exceptional-catalyst conditions |
| Exceptional live targets | Proposed default: targets above 3% disabled until the owner enables an evaluated policy version |
| Maximum profit target | 10%, enforced in code and owner configuration validation |
| Entry confidence floor | Proposed high-confidence default: 90/100, plus a completed research/readiness checklist |
| Multi-round LLM | Persistent owner dialogue and repeated tool-result/model rounds; JSON-only normalized messages |
| Model tools | Scoped news, X, RSS, source comparison, quotes, T212 information, calculations and knowledge reads |
| Runtime skills | Owner-approved, versioned scenario instructions; required research skills must finish before entry |
| Topic guides | Separate files for 11 sectors and 25 industry groups used as subsectors, plus operating branches |
| Company/people memory | Revisioned profiles and attributed statements/relationships with provenance and corrections |
| Global X people | Owner-selected Trump, Musk, Gill, Ackman and Icahn; scoped qualitative priorities |
| Perpetual | Approved Rust advisor; compatible assessment required for entry; binary probability alone cannot enable expected-utility selection |
| FinBERT | Self-hosted Docker text-sentiment service; versioned Rust overlay affects entry/target thresholds; source rights and freshness apply; shipped disabled |
| Holding attention | Personalized watchdog; 80% holding / 10% global / at most 10% alternative research, separate safety capacity |
| Opportunity rotation | Initially disabled; comparative/execution gate required, default entry slot retained; bounded two-attempt policy is opt-in |
| Numerical limits | Approved formula selects buy/stop/target, quantity and horizon within verified constraints; LLM hints confer no authority |
| Initial position stop | Formula derives a meaningful technical stop; at most 2% planned net downside and minimum reward/risk remain policy |
| Daily loss guard | Proposed default: -2% deposit-adjusted session return blocks entries; exits continue |
| Holding duration | Intraday preferred; overnight allowed with verified protection; proposed maximum five exchange sessions |
| Trading hours | Regular exchange sessions initially; use exchange calendars and early closes |
| Entry window | Fresh five-minute proposal; no new entries in the last 30 minutes of the actual session |
| Operating mode | Paper by default; read-only, shadow, and explicit live modes |
| Control channel | Telegram private chat with an owner user-ID and chat-ID allowlist |
| Persistence | PostgreSQL is authoritative; versioned memory files provide readable projections |

Using 98% of cash is an entry sizing rule. It does not compel a purchase without a valid setup, a daily rebalance, or an addition to an existing position. When already invested, Trady continues managing the holding. Unsettled proceeds, pending deposits, and reserved cash are not available balance.

The 1% daily account objective and a 1% position target use different denominators. A 1% net position gain on a 98% allocation produces approximately 0.98% account growth before external operating costs. Trady reports both figures and does not inflate its target or trade frequency to manufacture a daily win.

## Interface

The [FinBERT contract](./finbert/README.md) adds implemented domain records,
`SentimentProvider`, `FinBertClient`, `assess_sentiment`, `sentiment_thresholds`
and explicit CLI scoring. Its Compose service is unlaunched; durable production
readiness and plan activation remain integration work. All trading processes stay
Rust; the Python model-serving sidecar is an isolated inference dependency.

The planned user interface is [Telegram](./telegram.md). The full internal contracts
include research proposals, versioned trading plans, price observations, validated
order intents and reconciled financial events. Their owners and persistence rules
are defined in the linked documents; implemented subsets are identified in their
implementation records.

## Invariants

- The analyst has no broker trading credential and cannot call an executor order API.
- LLM confidence, sentiment classes and scenario probabilities have different meanings; missing forecasts or outcomes cannot be invented.
- Formula/fusion results, a backtest or a theorem cannot bypass risk, readiness or deployment approval.
- Rust sells under validated price/time rules, authenticated owner close or an enabled deterministic rotation gate; research and ML text cannot submit orders.
- New research can adjust future thresholds only through validation and an atomic plan-version change.
- A triggered exit stays triggered; a newer optimistic recommendation cannot cancel it.
- Confidence never authorizes a target above 10%, leverage, an extra holding, or more cash than allowed.
- Entry requires finalized research, complete required skills/source comparison, no material contradictions and computed confidence at least 90/100.
- Owner policy, research evidence, model memory, and the financial ledger have separate write authority.
- Unknown order state blocks new submissions until reconciled; it never permits a blind retry.

## Side Effects

The complete application will read external data, send LLM requests and Telegram
messages, persist database records and memory files, and submit broker orders in
live mode. The implemented offline database/fixture effects are listed in the
[foundation record](../foundation.md). Existing-server reuse remains a deployment
design and has not been performed.

## Remaining deployment choices

Implementation can proceed against fake adapters and paper trading. Before a live launch, record the actual broker entity and account permissions, a licensed quote provider with suitable venue coverage, news-provider contracts, LLM provider/model/context limits, operating-cost caps, and the existing PostgreSQL version/network/access arrangement. The capability checklist in [operations](./operations.md) makes these deployment choices concrete.

## Verification

Run `python3 scripts/verify-assets.py` and `cargo test --workspace --locked` from
the repository root. The [foundation record](../foundation.md) lists the separate
database checks and partial acceptance evidence. The [implementation plan](./implementation.md)
still defines the full behavioral requirements and launch criteria.

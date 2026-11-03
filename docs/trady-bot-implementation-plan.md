# Trady bot: requested EUR day-trading implementation plan

**Status:** implementation plan derived from the complete repository audit

**Target authority:** one daily, same-day EUR equity trade selected by a research
LLM and executed only through Trading 212 after deterministic Rust validation.

**Current repository authority:** fixed IEX30 US equities, Alpaca IEX market data,
Trading 212 demo execution, and a pre-registered long Donchian/ATR policy. That
scope is not the requested target and must be migrated deliberately.

## 1. Product contract

The bot runs one research and trading cycle per configured European session day.
The research loop must produce at least one ranked candidate and select the top
candidate. “No interesting stock” is not a valid research result. The selector
must keep searching the approved EUR universe, use fallback research sources, and
rank all eligible candidates until it has at least one candidate or the
operational inputs themselves are unavailable. It must never fabricate a ticker,
quote, source, or eligibility result to satisfy that requirement.

The mandatory-selection rule applies to research. It does not authorize an order
when the quote is stale, the Trading 212 instrument cannot be identified exactly,
protection cannot be maintained, the market is closed, the position cannot be
flattened before the deadline, or a hard risk invariant fails. Those are
operational safety halts, not a strategic “pass” choice.

The target behavior is:

- Discover current information from yfinance news, X API monitoring, Google or
  publisher RSS, registered browser/page readers, and user-supplied article or X
  URLs.
- Run a pinned OpenCode Go model (`deepseek-v4.1-flash`) as a research analyst.
  The model may gather and compare evidence, classify sentiment, explain risks,
  rank candidates, and propose bounded entry/exit levels.
- Restrict candidates to EUR-denominated equity instruments that have an exact
  Trading 212 instrument mapping, a verified exchange/session, and a compatible
  quote path.
- Run deterministic Donchian, perpetual, and other registered algorithms on
  completed bars. Combine their snapshots with current bid/ask, volume, spread,
  volatility, and exchange state.
- Produce one selected stock, an executable entry proposal, a protective stop, a
  target, a maximum holding deadline, a quantity, and an immutable evidence
  bundle.
- Aim for approximately 1–3% net profit when the market permits it. A 1–2%
  initial loss budget is the default hard envelope. The phrase “4–5% lower exit”
  is ambiguous and must become a separately approved policy version; sentiment
  may never widen a protective stop after entry.
- Monitor live quotes and news while the position is open. Exit dynamically by
  tightening risk, taking profit, responding to a verified adverse event, or
  flattening before the configured same-day deadline.
- Notify the owner through Telegram about research, selection, evidence, order
  state, fills, position changes, exit decisions, errors, and daily/weekly
  reports.
- Reconcile every day and produce a weekly performance and improvement report.
  A weekly Codex session may propose changes and tests, but it cannot approve or
  activate its own changes.

The LLM is an adviser and researcher. Rust owns instrument eligibility, quote
freshness, sizing, stop distance, target economics, order idempotency, broker
capability enforcement, and the final order decision. No model output may call a
broker mutation directly or change a hard limit.

## 2. Current state and migration boundary

The repository is a six-crate Rust workspace with a large supporting lab and
skills surface. The existing runtime is intentionally focused on IEX30:

- `trady-domain` contains typed IDs, currencies, quotes, orders, instruments,
  sessions, and strategy-related domain types.
- `trady-policy` contains risk/economic limits, intelligence policies, formula
  definitions, and validation rules.
- `trady-application` contains orchestration, autonomous execution, research,
  screening, readiness, watchdogs, perpetual inference, and weekly optimization.
- `trady-storage` contains PostgreSQL persistence, migrations, research jobs,
  evidence, account state, and weekly artifacts.
- `trady-adapters` contains Trading 212, Alpaca, yfinance-rs, news/RSS/X, OpenCode,
  Telegram, financial-data, API health, and provider transports.
- `trady-cli` exposes configuration, research, account, broker, autonomous,
  watchdog, yfinance, Telegram, and lab commands.

The target migration is therefore an authority change rather than a greenfield
build:

1. Replace the active IEX30/Alpaca/USD universe authority with a reviewed EUR
   universe and a broker-compatible instrument registry.
2. Make `yfinance-rs` the live bid/ask and quote adapter for research and signal
   inputs, while proving that its quote represents the same listing and venue that
   Trading 212 will execute.
3. Keep Trading 212 as the only execution broker. Demo/paper and replay adapters
   remain for validation; no other broker may be enabled by production config.
4. Wire the OpenCode Go analyst into autonomous candidate selection and holding
   review using the existing versioned JSON tool protocol.
5. Add the missing same-day coordinator, live monitoring, daily reconciliation,
   user URL ingestion, and weekly self-improvement scheduler.
6. Remove or archive conflicting active paths only after their replacement is
   replay-tested and documented.

## 3. Target architecture

### 3.1 Runtime state machine

Implement one explicit, durable coordinator state machine:

`Idle -> Researching -> CandidatesRanked -> CandidateValidated -> SignalReady -> RiskApproved -> EntryPending -> Open -> ExitManaging -> Closed`

Every state also has `Rejected`, `Expired`, `EmergencyExit`, and `Recovery`
transitions. Persist the state, event ID, account ID, instrument ID, policy
revision, evidence snapshot IDs, and timestamps. A restart resumes only from an
unexpired state after reconciliation with Trading 212.

The coordinator owns sequencing and idempotency. Adapters return typed data and
receipts; they do not decide whether an order is appropriate.

### 3.2 Market data (`yfinance-rs`)

Extend `crates/trady-adapters/src/yfinance.rs` from an optional, one-shot Euronext
quote reader into a bounded market-data service:

- Add exact instrument bindings: internal ID, ISIN, Trading 212 instrument ID,
  exchange, Yahoo symbol, currency, tick/quantity precision, and venue.
- Expand suffix handling only through an explicit reviewed registry. Do not infer
  `.DE`, `.F`, or another suffix from a user ticker without a mapping and
  exchange evidence.
- Persist bid, ask, last, observed time, received time, market state, source,
  venue, and quote quality. Reject missing, crossed, zero, stale, closed, or
  inverted quotes.
- Add a poll scheduler with bounded concurrency, jitter, retry policy, and
  backoff. A polling client is not a stream; every consumer sees the age of the
  exact observation it used.
- Add historical OHLCV retrieval and completed-bar snapshots for strategy
  calculation. Preserve provider and local receive timestamps.
- Record provider-to-broker divergence during paper/shadow operation. A generic
  Yahoo quote must not authorize Trading 212 until crosswalk and tolerances are
  approved.
- Fix the suspicious market-state comparison in the current adapter and add
  tests for regular, pre-market, post-market, closed, halted, and unknown states.
- Expose `market.quote`, `market.bars`, `market.sessions`, and `market.fx` as
  application-owned read tools, not arbitrary HTTP calls.

If yfinance returns only a last price or a delayed/indicative quote for a
candidate, it remains research context and cannot authorize a new entry or a
price-sensitive exit.

### 3.3 EUR universe and candidate eligibility

Create a versioned `eur_equity_universe` configuration and storage snapshot.
Populate it from Trading 212's instrument catalog and owner-reviewed exchange
metadata, then attach yfinance symbols. Each row must include:

- internal ID and stable broker instrument ID;
- ISIN, legal name, ticker, exchange, country, currency, and session timezone;
- Yahoo symbol and provider source;
- minimum quantity, price/quantity precision, and fractional-order support;
- liquidity, average volume, spread, volatility, and halt status thresholds;
- effective/expiry timestamps, rights/provenance, and review owner.

The deterministic screener filters by EUR currency, instrument kind, exchange
session, quote freshness, spread, liquidity, volatility, price bounds, broker
availability, and same-day flatten feasibility. It records every exclusion reason.
The LLM ranks only surviving candidates and cannot expand the universe.

To satisfy the mandatory-selection rule, candidate discovery has ordered fallback
stages: reviewed universe snapshot; broker EUR catalog plus existing mapping;
recent yfinance news; RSS; X monitored accounts; and user-provided URLs. Search
stops once at least one fully eligible candidate is ranked, or records an
operational input failure if providers cannot supply verifiable data.

### 3.4 News, X, RSS, browser, and user URLs

Reuse the existing news, X, RSS, provenance, rights, budget, and deduplication
components. Add the missing sources and one evidence ingestion contract:

```text
EvidenceItem {
  id, canonical_url, source_kind, publisher, author, title, body_or_excerpt,
  symbol_mentions, entity_ids, published_at, fetched_at, content_hash,
  rights_status, freshness, primary_or_secondary, contradictions, raw_ref
}
```

Required work:

- Wire yfinance news into the normalized evidence store.
- Add Google News/publisher RSS collectors with allowlisted hosts and limits.
- Add a registered page/browser reader with bounded size, timeout, content hash,
  rights policy, and no arbitrary tool discovery.
- Add an owner-submitted URL queue. URLs are untrusted evidence; they cannot
  provide credentials, code, tools, or order instructions.
- Normalize publication time separately from fetch time and enforce “not older
  than 30 minutes” for live sentiment decisions.
- Cluster syndicated stories, preserve conflicting claims, and never count a
  failed fetch as evidence that no news exists.
- Store X post IDs, author IDs, listener scope, rights, retention, and budgets.

### 3.5 OpenCode Go analyst

Reuse `docs/v1/llm-and-tools.md`, `schemas/llm-contracts.v2.json`, and the existing
OpenCode adapter. Extend the autonomous worker so it invokes OpenCode Go for
selection and holding-review jobs.

The model contract must return versioned JSON containing:

- run/session/model/prompt/tool manifest hashes;
- all candidates considered, eligibility snapshot IDs, ranking scores, and
  exclusion reasons;
- exactly one selected EUR instrument for a successful selection run;
- source/evidence IDs, publication/fetch ages, sentiment and confidence;
- catalyst, market regime, sector/stock type, liquidity/volatility observations;
- algorithm snapshots requested and their results;
- bounded entry, stop, target, expiry, and rationale proposals;
- uncertainties, contradictions, and required follow-up checks.

The application validates every source ID, price, currency, quote age, schema field,
and model revision. Hallucinated URLs or uncalled tools are rejected. Keep the
existing bounded loop (six turns, four tool rounds, twelve calls, and a deadline)
unless measured workload requires a reviewed change.

The LLM may choose among registered strategy profiles by stock type, but may not
create an unregistered algorithm, widen a stop, increase account risk, or select a
non-EUR instrument.

### 3.6 Strategies and price calculation

Create a `Strategy` trait/registry with deterministic, replayable outputs:

```text
SignalSnapshot {
  formula_id, parameter_revision, instrument_id, bar_snapshot_id,
  direction, signal_at, entry_zone, invalidation, stop_distance,
  expected_target, max_hold_deadline, reward_to_risk, confidence_inputs
}
```

Retain and adapt Donchian/ATR and perpetual implementations. Add algorithms only
through the same registry and evidence process. Run independent parity tests
against the existing Python/Lean lab where applicable.

The application, not the LLM, computes executable entry range, stop, target,
quantity, costs, and net return. The initial policy should be:

- desired net target: 1–3%, bounded by volatility and cost;
- initial maximum loss: 1–2% of allocated risk/notional according to policy;
- no averaging, leverage, or overnight carry;
- same-day maximum hold and deterministic flatten deadline;
- stop may tighten or profit may be taken earlier, but never widen after entry;
- 4–5% loss handling stays disabled until separately approved.

The risk engine includes spread, commission, FX conversion, slippage, quantity
precision, partial fills, and minimum-order constraints. A 1–3% gross move is not
a 1–3% net result.

### 3.7 Trading 212 execution

Keep `crates/trady-adapters/src/trading212/` as the only broker mutation path.
Complete a capability matrix for the exact account and instrument:

- account identity and base currency;
- instrument lookup and venue mapping;
- quote/executable-price availability;
- market/limit order semantics;
- quantity and fractional support;
- attached stop/target or equivalent protection;
- cancel/replace, partial fills, fees, FX, and history;
- rate limits, acknowledgement latency, and restart reconciliation.

Current evidence says public Trading 212 quote support and native attached
protection are unsupported. Until a tested alternative exists, the target runtime
remains paper/shadow-only for entries that cannot be protected and reconciled.
A model or preflight flag must never override this.

Implement an idempotent order intent/ack/fill journal with broker IDs, request
hashes, retry policy, cancellation state, and recovery rules. The broker adapter
must not accept a yfinance symbol without the exact crosswalk and account-scoped
instrument ID.

### 3.8 Dynamic holding monitor and same-day exit

Add a persistent monitor independent of the LLM:

- quote poll loop and age/quality checks;
- strategy recalculation on completed bars and configured intraday intervals;
- fresh-news refresh for configured evidence age;
- bounded LLM holding-review jobs for material new evidence;
- deterministic stop/target/deadline checks every cycle;
- stale-feed, broker-disconnect, halt, abnormal-spread, and data-disagreement
  emergency behavior;
- monotonic exit updates and rate limits to prevent churn;
- hard flatten at the exchange-local deadline, followed by broker reconciliation.

News and the LLM are advisory. Broker-native protection or a deterministic local
emergency exit must continue if the LLM, Telegram, browser, X, or news providers
are unavailable.

### 3.9 Telegram gateway

Connect the existing Telegram transport/spool to coordinator events. Provide
coalesced, rate-limited messages for research, ranked candidates, selected stock,
evidence, prices, order/fill state, dynamic exits, stale data, risk halts,
forced flatten, daily reports, weekly reports, and source-linked company/CEO
facts. Inbound commands, if added, must be authenticated and cannot bypass the
coordinator or risk engine.

### 3.10 Daily and weekly evidence/reconciliation

Add daily jobs and migrations for broker account/order/fill/transaction
reconciliation; realized/unrealized P&L net of fees, FX, spread, and slippage;
quote/news age; signal latency; rejected orders; provider errors; candidate ranks;
strategy/indicator attribution; news-release price windows; and LLM costs/errors.

The weekly report calculates expectancy, hit rate, drawdown, average hold, profit
distribution, stop/target adherence, exposure, stock-type performance,
calibration, and failure modes. It creates an immutable artifact and a Codex
session using the existing weekly optimization skill. Codex may propose code,
policy, or prompt changes and tests, but cannot activate itself.

## 4. Removal and simplification plan

Do not remove safety checks merely because they are called “preflight.” Replace
duplicated entry-time checks with one application-owned order-readiness gate that
runs immediately before each order and is enforced continuously by the state
machine.

### Keep as hard safety invariants

- exact Trading 212 account and instrument identity;
- EUR currency and approved venue/session;
- fresh, two-sided, venue-compatible quote;
- spread, volatility, liquidity, and price bounds;
- buying power and quantity precision;
- maximum account/trade/daily loss and order count;
- protection/recovery capability;
- idempotency, pending-intent recovery, and broker acknowledgement;
- evidence/model/policy validity where the policy requires it;
- same-day flatten deadline and no overnight state;
- credential presence, TLS endpoint, and secret redaction.

### Remove from the active trading path or merge into the gate

- duplicated `autonomous check`, `autonomous_shadow` checks, and readiness probes
  that repeat the same invariant in different layers;
- repository-state, unused-daemon, optional-service, or unrelated-provider checks
  that cannot protect a specific order;
- Alpaca/IEX30 watchdog and stream checks after yfinance EUR monitoring becomes
  the active authority;
- model/fusion/FinBERT checks when DeepSeek is the only configured model;
- provider probes for adapters disabled by production config;
- manual preflight steps that block research, reports, or Telegram when no order
  is pending.

Keep a diagnostic command that explains the gate and its blockers, but make the
runtime gate authoritative and idempotent. Document every removed check and the
safety invariant that replaces it.

### Archive or remove after migration validation

- Alpaca/IEX30 production configs, source/venue assumptions, and IEX watchdog
  wiring;
- conflicting IEX30 README/operator claims and superseded European artifacts;
- FinBERT runtime/config/tests if no approved policy consumes FinBERT outputs;
- AlphaVantage, Finnhub, GDELT, and optional providers not selected for the target
  authority, retaining fixtures only if a test needs them;
- generated top-level review files that are historical rather than runtime input;
- duplicate or unregistered strategy paths and legacy CLI commands;
- service units that are not part of the daily/weekly scheduler.

Deletion follows two steps: mark deprecated, migrate tests/config/docs, run replay
and workspace checks, then remove or move the artifact to explicit `docs/archive`.
Do not delete shared domain, policy, storage, schema, or lab code before proving
that no retained path imports it.

## 5. Phased implementation backlog

### Phase 0 — Freeze and inventory

- Record current main commit and runtime behavior.
- Add target-scope documentation and a migration feature flag; never mix IEX30
  and EUR authorities in one configuration.
- Generate a module/file dependency inventory and mark each path retained,
  refactor, new work, deprecated, or removable.
- Preserve IEX30 replay fixtures until EUR parity is established.

**Exit:** current and target authorities are separately named and testable.

### Phase 1 — Contracts and policy

- Add EUR universe, instrument crosswalk, session calendar, quote-quality,
  candidate-ranking, dynamic-exit, and reconciliation schemas.
- Define the maximum-loss policy and resolve the 4–5% ambiguity.
- Define session windows and local flatten deadlines for each exchange.
- Add migrations for evidence, quote snapshots, strategy snapshots, selection
  runs, order intents, dynamic exits, Telegram events, and report versions.

**Exit:** schemas reject unknown fields, revisions are hashed, and replay IDs are
stable.

### Phase 2 — Market data and broker mapping

- Extend yfinance configuration and polling service.
- Build Trading 212 catalog ingestion and EUR crosswalk.
- Add quote freshness/quality/divergence fixtures and session tests.
- Prove exact symbol/ISIN/venue alignment in paper/shadow.

**Exit:** the screener produces a deterministic EUR eligibility snapshot.

### Phase 3 — Evidence ingestion

- Wire yfinance news, X, RSS, browser/page, and user URL queue.
- Add deduplication, syndication clustering, rights, retention, budgets, and
  publication-age checks.
- Persist raw and normalized evidence with provenance.

**Exit:** replay reconstructs information available at selection time.

### Phase 4 — Candidate selection

- Implement fallback discovery and the mandatory-at-least-one contract.
- Add candidate ranking and deterministic eligibility vetoes.
- Invoke pinned OpenCode in research-only mode first.
- Send candidate, evidence, and selection events to Telegram.

**Exit:** replay produces the same ranked list and one selected candidate.

### Phase 5 — Strategies and risk

- Register Donchian, perpetual, and additional approved algorithms.
- Add intraday/completed-bar data required by each formula.
- Compute executable entry, stop, target, quantity, costs, and deadline in Rust.
- Add property tests for no stop widening, max loss, stop/target conflict, and
  no overnight carry.

**Exit:** strategy/replay outputs are deterministic and cost-aware.

### Phase 6 — Trading 212 execution

- Complete capability evidence on the exact account and EUR instruments.
- Implement idempotent intent, acknowledgement, fill, cancellation, protection,
  and restart reconciliation.
- Keep live disabled while quotes/protection/recovery are unsupported.

**Exit:** paper/shadow tests pass with partial fills and disconnects.

### Phase 7 — Coordinator and holding monitor

- Implement state machine and durable recovery.
- Add quote/news polling, bounded holding-review LLM jobs, monotonic exit updates,
  emergency handling, and hard same-day flatten.
- Remove duplicate preflight paths after the central gate is proven.

**Exit:** a full paper day survives restart, stale data, a news event, and a
flatten deadline.

### Phase 8 — Telegram and operations

- Connect coordinator events to Telegram.
- Add daily scheduler and report storage.
- Add health metrics, operator status, and alert coalescing.

**Exit:** owner receives complete, non-secret lifecycle information.

### Phase 9 — Validation and controlled activation

- Run unit, contract, property, replay, historical, walk-forward, holdout,
  shadow, failure-injection, and broker reconciliation tests.
- Compare yfinance quotes with venue-compatible execution evidence.
- Activate only an explicitly versioned policy with small risk and an owner release
  record.

**Exit:** live activation is an evidence-backed decision, not a config toggle.

### Phase 10 — Daily/weekly learning loop

- Generate immutable daily reconciliation and weekly performance reports.
- Spawn or queue a Codex review using the existing offline skill with report/evidence references. The runtime service must not receive live credentials or self-activate changes; use a separately authorized reviewer/Codex runner and persist the handoff/result.
- Require independent review and activation evidence for every change.

**Exit:** learning improves proposals without self-modifying live execution.

## 6. Acceptance tests

### Selection and research

- Every successful selection run contains at least one ranked candidate; otherwise
  it contains a typed operational-input failure and never an invented candidate.
- The selected instrument is EUR, mapped to a Trading 212 ID, in the approved
  universe snapshot, and supported by current evidence.
- No article, X post, RSS item, browser page, or user URL can inject a tool,
  credential, strategy, or order instruction.
- A stale or failed source remains `STALE`, `ERROR`, or `UNAVAILABLE`; it is not
  converted to an empty successful result.

### Market and execution safety

- Quotes are two-sided, positive, uncrossed, fresh, and crosswalked to the exact
  listing before entry.
- Fees, FX, spread, slippage, quantity precision, and partial fills are included.
- A stop never moves farther from entry after a position opens.
- No order exceeds loss, notional, daily count, or same-day deadline policy.
- A restart reconciles broker state before another mutation.
- A quote/news/LLM outage cannot disable deterministic protection or flattening.
- No position remains open after the exchange-local deadline.

### Reconciliation and learning

- Daily P&L matches broker history after fees and FX or records an explicit gap.
- Every selected/rejected candidate, evidence item, quote, algorithm output, order,
  fill, exit update, and Telegram event is replayable by ID.
- Weekly reports include raw snapshots needed to reproduce metrics.
- Codex proposals cannot activate without a separate reviewed release.

## 7. Questions to resolve before live activation

1. Which Trading 212 account mode is the target first: demo, paper equivalent, or
   live? Current evidence is for demo and does not prove live protection.
2. Which EUR exchanges should be allowed initially, and what is the exact local
   flatten time for each?
3. Is one trade per day a hard limit, or may the bot close and replace a trade in
   the same session? This plan assumes one selected instrument and one entry slot.
4. What exact loss policy replaces “4–5% lower exit”? The default is a hard 1–2%
   risk envelope and no stop widening.
5. Should AlphaVantage, Finnhub, GDELT, and FinBERT be removed from the target
   build or retained as disabled research adapters?


The per-file decisions and line-count inventory are in
[the complete codebase file matrix](./trady-codebase-file-matrix.md), with the
package-level reasoning in [the audit](./trady-codebase-audit.md).

# Trady v1 free financial data integration

Status: implementation specification, researched 2026-10-31. The bounded
[Rust integration](../financial-data.md) now supplies catalog adapters, shared
observations/signals, migration 9, scoped read tools and central credential names.
Sources remain disabled; this broader design includes future work and supplies
neither trained models nor permission to trade. Provider details and primary
references are in the [API catalog](./free-financial-api-catalog.md).

## Purpose and existing implementation

Give research, FinBERT, Perpetual, deterministic calculators, watchdogs and the
local weekly laboratory a common, auditable financial-data layer. More data is a
hypothesis to evaluate, not evidence of better returns. Missing or unlicensed
inputs must remain missing rather than becoming invented observations.

The inspected repository already has [news collection](../news.md), including
optional Alpha Vantage news, bounded HTTP, durable request reservations, cursors,
rights checks and correction records. The [source registry](../../config/research.json)
ships disabled. Extend this work; do not create a second Alpha Vantage collector
or another independent allowance for the same API key.

The [Rust architecture](./architecture.md), [prediction implementation record](./prediction-implementation.md),
[conversation protocol](./llm-and-tools.md), [trading policy](./strategy.md) and
[weekly optimization](./weekly-optimization.md) retain authority. In particular:

- Trading 212 Invest is the current candidate broker, subject to capability
  verification. These integrations do not switch the broker to Interactive Brokers.
- The account remains cash-only, long-only, one ordinary stock at a time, with
  existing universe, entry-slot, confidence, allocation and protection constraints.
  Crypto observations are context only; they do not authorize crypto trading.
- All application services remain Rust. FinBERT remains an isolated inference
  dependency. Optional Python research imports are offline, not a new execution service.
- The executor owns orders and protection. LLMs, provider scores and offline Codex
  cannot place orders, alter owner policy, approve deployment or cancel latched exits.

## Delivery priorities

Start with SEC filings, FRED vintages, one licensed historical-price adapter and
one additional news collector. Implement the shared contracts before adding every
vendor. Redundant vendors are useful for validation only when their underlying
lineages and price definitions really differ.

| Stage | Sources | Intended outcome |
|---|---|---|
| Foundation | Existing Alpha Vantage/news transport and registry | Account-wide quotas, typed capabilities, immutable observations and rights enforcement |
| Primary context | SEC EDGAR and FRED/ALFRED | Attributed financial facts, disclosures and revision-aware macro context |
| Price history | Twelve Data, or reviewed Tiingo/Massive alternative | Completed OHLCV bars and corporate-action-aware features; not live execution certification |
| News | Finnhub and GDELT, plus existing permitted RSS/Alpha Vantage | Issuer/event discovery, corroboration and coverage diagnostics |
| Optional enrichments | FMP and CoinGecko | Statement cross-checks and crypto market context |
| Offline-only exploration | yfinance | Explicitly labeled research imports; no production fallback dependency |

An adapter being implemented is different from an endpoint being entitled, a
payload being usable, a feature being approved, or a strategy being deployable.
Store and display those states independently.

## Architecture and ownership

```text
Reviewed provider account + endpoint capabilities + source rights
                              |
                    durable collection jobs
                              |
             Rust adapters -> validation/quarantine
                              |
           immutable normalized observations + provenance
                              |
             point-in-time snapshots and feature jobs
                 /            |             \
        bounded LLM tools  FinBERT/ML     deterministic calculations
                 \            |             /
                evidence / forecasts / safety observations
                              |
                existing formula and readiness pipeline
                              |
                executor independently revalidates

Offline rights-approved snapshot -> weekly Codex/LEAN/Lean 4 workflow
Execution quotes/account/order reconciliation remain a separate protected path.
```

Use PostgreSQL and the existing durable job/outbox approach. Redis, Kafka and a
new database are not prerequisites. No network call or model inference may occur
while an executor serialization lock or a collection commit transaction is held.

| Workspace owner | Proposed extension | Boundary |
|---|---|---|
| `trady-domain` | Provider capabilities, observation variants, availability/rights metadata, feature and dataset manifests | No provider DTOs or network clients |
| `trady-application` | Collection orchestration, snapshot builder, feature jobs, read tools and data-quality checks | Uses ports; does not deserialize arbitrary vendor JSON into policy |
| `trady-adapters` | FRED, SEC, Finnhub, GDELT and selected price/fundamental adapters | Reuse hardened transport; vendor names/fields stay here |
| `trady-storage` | Durable quotas, append-only revisions, lineage, invalidation and dataset manifests | Restricted analyst writes; no access to order intents or active policy |
| `trady-policy` | Pure indicator/quality/risk calculations and compatible feature validation | No HTTP, hidden fallbacks or model-authored arithmetic |
| `trady-cli` | Explicit diagnostics, collection wiring and offline export commands | The [operator guide](../financial-data.md) lists implemented commands; broader examples here remain design proposals |

Keep source items on the existing `NewsSource` path where appropriate. Introduce
separate conceptual `HistoricalDataSource`, `FundamentalDataSource` and
`MacroDataSource` ports rather than encoding a financial statement as a news
headline. These are proposed names, not claims about existing Rust traits.
A collection result should carry typed records, a continuation, coverage/gaps,
request usage, capability revision and invalidations. Reads must be possible from
fixtures without keys or network access.

Historical OHLCV is not a replacement for `QuotePort`. A last trade, daily close,
aggregate price or broker portfolio valuation cannot be converted into a current
bid/ask by copying the value into both fields. Live readiness still requires the
licensed feed coverage, observed timestamps, tradability and freshness specified
by the executor contract.

## Provider onboarding and rights

Before enabling any endpoint, create a deployment-owned capability record with
provider/account namespace, plan, endpoint, symbols/venues, supported fields,
delay, historical range, pagination, credit weights, reset rules, source links,
review time, review expiry and redacted test evidence. Record whether access is
verified, denied, unknown or suspended. Do not infer global access from one demo
symbol or a successful request to an unrelated endpoint.

The existing rights fields are a starting point, not blanket permission for every
consumer. Propose separate permissions for raw storage, normalized storage,
internal display, redistribution, external LLM inference, local sentiment
inference, model training and derived-data export, plus retention and attribution.
An absent permission is denied. A successful HTTP response proves neither rights
nor truth. Numeric data and derived features also need an applicable rights review.

Cloud LLM submission is a separate use of provider data. Running Codex CLI locally
does not keep its model requests local. Never send prohibited vendor payloads to
an LLM, FinBERT, training job or telemetry system. Source rights must be enforced
by the application before context construction, not by a prompt asking the model
to ignore restricted text. No automatic paid upgrades, overage purchases or key
rotation to evade quotas are allowed.

If retention/revocation requires deletion, delete permitted content projections,
caches, embeddings, exported snapshots and dependent copies as applicable. Keep
only allowable audit metadata/tombstones. Mark affected models and experiment
artifacts for review; do not claim that deleting a source row removes its influence
from an already trained model. Check whether even hashes may be retained.

## Canonical observation and identity contracts

Each normalized record has a typed payload and the following envelope. Names are
a proposed internal contract; canonical LLM schemas are changed separately through
review and fixture updates.

| Field group | Required semantics |
|---|---|
| Identity | Internal observation ID, provider/account/source IDs, dataset/endpoint, provider item key, source revision, parser/schema versions |
| Instrument | Stable internal instrument/issuer ID, provider symbol, exchange/MIC where known, currency, effective symbol-mapping interval; ambiguity is explicit |
| Time | Economic period/event time, provider publication/availability time where known, actual first-seen and receipt times, normalization time, precision and timezone |
| Revision | Vintage/as-of date, accession/revision identifier, superseded observation ID and reason; never rewrite historical knowledge silently |
| Semantics | Units, frequency, fiscal/calendar basis, price type, adjustment basis, volume units, venue coverage and delay |
| Quality | Completeness/window, stale/truncated/ambiguous flags, validation failures, confidence in mapping, and unresolved contradictions |
| Provenance | Source URL without credentials, upstream lineage IDs, payload digest when permitted, fetch audit ID and immutable source references |
| Rights | Rights/capability revision, per-consumer permissions, expiry/retention, attribution and export restrictions |

Use decimal strings/checked decimal values for money, prices, quantities and
accounting amounts. ML tensors may use validated finite floating-point values with
units and missingness masks; converting to a tensor does not erase source metadata.
Never replace null, FRED's missing marker, an unavailable metric or a failed
request with zero. Dates without a known time retain date precision.

Map company CIK, vendor tickers and broker instrument IDs through effective-dated
records. A CIK identifies a filer, not necessarily one security. Different share
classes, ADRs, listing venues, reused tickers and currency denominations must not
collapse into a single instrument. Current aliases are not automatically valid
throughout a historical backtest. Crypto symbols are especially ambiguous; use
provider coin IDs and keep them outside the stock trading universe.

### Availability and point-in-time selection

Distinguish three notions: when an event occurred, when a version became public,
and when this deployment actually received it. A fiscal quarter ending in March
does not make its later filing available in March. A macro observation labeled
January does not become known on January 1. A revised historical value is not the
original release.

For recorded deployment replay at decision time `t`, an observation is eligible
only when both its supported public-availability bound and its recorded ingestion
bound are at or before `t`, and the revision/rights/quality policy permits it.
Derive features only from eligible dependencies. A correction or revocation may
invalidate future use while preserving the allowable record of what was used then.

Historical reconstruction is a distinct mode: an authentic vendor archive or FRED
vintage can reconstruct earlier public information even though downloaded today.
Require evidence of version availability, conservative delivery latency, timestamp
precision and source coverage. Label it reconstructed, not observed production
replay. Never backdate a current snapshot merely because its economic period is old.
When intraday publication timing is unknown, exclude the value until a conservative
availability bound has elapsed; a calendar date is not an exact release instant.

Keep two temporal axes in storage: effective/public time and system-recorded time.
The snapshot builder receives an application-bound `decision_at`; an LLM cannot
request a future cutoff or select a favorable revision after seeing the outcome.
Bar inputs must be completed and available, not still-forming candles. Future
scheduled calendar events are allowed as schedule records published before the
cutoff; their future event time must not be mistaken for a future publication.

## Persistence, deduplication and correction flow

Extend existing storage with migrations after reviewing its actual schema. The
following are logical entities, not SQL to apply unchanged:

| Entity | Key and responsibility |
|---|---|
| `provider_capabilities` / `source_rights` | Versioned deployment approvals; endpoint/consumer permissions and expiry |
| `provider_budget_windows` / `request_reservations` | Shared account/credential budget, endpoint-weighted reservations, reset and retry deadline |
| `financial_observations` | Append-only typed versions; unique provider + dataset + external key + source-version/content identity |
| `collection_windows` | Durable pending intervals, cursors, overlap, successful watermark and complete/partial/error state |
| `observation_dependencies` / invalidations | Directed provenance from inputs to facts, features, contexts, predictions and exports |
| `dataset_manifests` / `feature_snapshots` | Immutable cutoff, row/dependency hashes, feature/model compatibility, rights and coverage |

A worker leases a bounded job, reserves worst-case credits transactionally, commits,
fetches outside the transaction, validates and then atomically stores observations,
coverage, cursor and the successful watermark. A timeout after sending a request
still consumes the reservation. Crash recovery cannot refund unknown usage or
advance the watermark past missing work. Unique keys make duplicate delivery safe.

Provider cursors and `next_url` fields are untrusted. Extract a bounded continuation
and reconstruct an allowlisted route; never follow a URL with an arbitrary host,
credential query or private address. Page caps remain `PARTIAL`. For saturated
news windows, recursively split within bounded depth, retain overlapping windows
and deduplicate; an irreducibly saturated interval stays incomplete, not empty.

When a record changes, preserve the original allowed revision, append its replacement
and publish an invalidation. Recompute dependent research/features; expire affected
unexecuted readiness dossiers. A correction cannot rewrite original predictions,
closed trade outcomes or the account ledger. Existing position protection continues
under executor-owned transition rules; no collector can directly change a plan.

## Shared quotas, caching and scheduling

Every consumer uses the same provider-account ledger: scheduled collection, LLM
refreshes, diagnostics, backfills and weekend jobs. Per-source limits alone are
insufficient when multiple sources share a key. Also enforce aggregate IP limits
where applicable, such as SEC fair access. Reserve weighted symbol/endpoint credits,
not merely HTTP requests; a batched request can cost more than one credit.

Effective allowance is the smallest of verified provider ceilings, endpoint limits,
owner daily/monthly spending caps and the local conservative cap. Honor reset
semantics and `Retry-After`; maintain durable retry deadlines across restarts.
Do not publish speculative provider quotas as guarantees. Unknown entitlement
blocks collection until reviewed; unknown remaining budget never means unlimited.

On 429, back off with jitter and stop until the applicable deadline. Retry bounded
transient transport/5xx failures within the same deadline and budget. Treat 401/403
as credential/entitlement failures requiring review, not an invitation to use a
legacy endpoint. Detect JSON error/quota bodies even with HTTP 200. Do not retry
schema failures indefinitely or log raw error bodies containing credentials.

Cache by provider, dataset, canonical parameters, effective cutoff/vintage,
adjustment basis, capability/rights revision and schema version. Cache freshness
is not the observation's market freshness. Conditional requests consume allowance
unless the provider explicitly documents otherwise. A cache hit rechecks rights,
revocation and consumer suitability. Concurrent identical misses coalesce.

### Initial collection policy

All intervals below are proposed operational defaults, not source-delivery promises.
Use actual exchange sessions and `America/New_York` for US market jobs; owner reports
use `Europe/Berlin`. Never hard-code a UTC offset or assume every Friday is open.

| Data | Proposed cadence and scope | Freshness/coverage rule |
|---|---|---|
| Daily price history | After each actual close and provider publication delay; bounded eligible universe | Require latest completed expected session, adjustment metadata and sufficient warmup |
| Held-company news | Preserve existing five-minute target where the verified plan permits; coalesce duplicates | Record collection age, provider lag and gaps; not a real-time alert guarantee |
| Shortlist news/profiles | Pre-market and on material events; at most the existing five-candidate scope | Hold a bounded cache; new evidence invalidates dependent context |
| SEC submissions | Held issuer every 15 minutes initially; shortlist pre-market and close | Fetch facts/documents when a new accession appears; primary filings can arrive outside sessions |
| FRED | Daily plus bounded checks around selected releases | Use per-series expected release cadence, vintage and actual ingestion; monthly data is not stale after one day |
| GDELT | Four scoped issuer/sector/geopolitical queries every 30 minutes initially | Discovery coverage only; capped results and availability gaps remain visible |
| Optional CoinGecko | One batched context request every 15 minutes | Preserve provider update time; no stock-execution freshness claim |
| Weekend imports | Low-priority resumable backfill after reconciliation | Separate registered history demand; cannot starve collection or executor safety work |

Example sizing, not an entitlement claim: 500 one-credit daily-symbol requests,
12 additional scoped refreshes and 60 reserved retry/backfill credits consume
572 credits/day. A local 700-credit cap fits below Twelve Data's documented
800-credit Basic ceiling only when every requested endpoint/symbol is entitled
and costs one credit. At six credits/minute the 500-symbol sweep alone takes at
least 84 minutes. Actual effective membership need not contain exactly 500
securities. Five symbols polled every minute through a 390-minute session would
consume 1,950 one-credit requests before other work, so it does not fit that tier.
See [credit documentation][td-credits].

Prioritize held-symbol and required decision inputs over broad discovery and lab
backfills. Research capacity is separate from executor quotes, protection and
reconciliation. At 80% of a local budget, defer optional work; at exhaustion,
return explicit unavailable coverage. Never make an order dependent on a queue
of LLM-initiated refreshes.

## Proposed configuration and secret handling

This JSON is an illustrative future registry, **not accepted by the current
`config/research.json` parser**. Implement schema/version migration and validation
before using it. It grants no rights and enables no source.

```json
{
  "design_version": 1,
  "mode": "research_only",
  "automatic_paid_fallback": false,
  "provider_accounts": [
    {
      "id": "fred_primary",
      "provider": "FRED",
      "enabled": false,
      "credential_env": "TRADY_FRED_API_KEY",
      "allowed_hosts": ["api.stlouisfed.org"],
      "capability_review_id": null,
      "local_limits": {"requests_per_minute": 20, "requests_per_day": 200},
      "endpoints": ["series_observations", "series_metadata", "release_dates"],
      "rights": {
        "normalized_storage": false,
        "external_llm": false,
        "local_inference": false,
        "model_training": false,
        "export": false,
        "retention_hours": 24,
        "terms_reference": ""
      }
    }
  ]
}
```

Resolve credentials through the [existing central credential mechanism](../credentials.md).
Registered names are `TRADY_FRED_API_KEY`, `TRADY_FINNHUB_API_KEY`,
`TRADY_TWELVE_DATA_API_KEY`, `TRADY_FMP_API_KEY`, `TRADY_COINGECKO_API_KEY`,
`TRADY_TIINGO_API_KEY` and `TRADY_MASSIVE_API_KEY`. Existing Alpha Vantage uses
`TRADY_ALPHA_VANTAGE_API_KEY`. SEC needs a truthful contact-bearing user agent,
configured as `TRADY_SEC_USER_AGENT`, not a fabricated contact or secret token.
The [implemented adapters](../financial-data.md) resolve these names through the
central controller; endpoint capabilities and consumer rights still require review.

Use provider-supported authentication headers where verified; otherwise inject
query credentials only inside the adapter. Redact query strings, auth headers,
request debug dumps, exception URLs and tracing spans. Secrets are not arguments
to LLM tools, shell-command examples, database observation fields or committed
configuration. Avoid putting keys in command-line arguments during manual tests.

Reuse the hardened HTTP allowlist, DNS/address validation, no authenticated
redirects and bounded parsing. The existing news transport caps bodies at 2 MiB
and rejects compression. SEC company facts or bulk files may exceed that cap:
use narrower company-concept requests first, or implement a separately reviewed
large-document/download profile with strict compressed/uncompressed limits,
streaming, disk quotas and archive-path checks. Do not raise all news limits or
silently truncate a financial statement to make it fit.

## LLM and other service interfaces

Prefer reads from immutable snapshots. The model requests information; the
application chooses reviewed sources, binds the account/universe/cutoff and
constructs the result. No arbitrary URL, SQL, Python, shell, provider key or source
configuration is accepted as a tool argument.

Reuse existing `news.search`, `news.read_item`, `sources.health`, `profiles.get`
and `evidence.compare` where their semantics fit. `market.bars` and `market.fx`
remain planned interfaces.
The table below describes the broader proposed interfaces. Bounded
`fundamentals.snapshot`, `filings.search` and `macro.snapshot` reads are now
registered with `source_id` and a 1–3 result limit, alongside `financial.snapshot`
and `prices.features`; see the [implemented tool contracts](../financial-data.md).
The fuller arguments below and the other listed tools remain proposed:

| Proposed tool | Bounded arguments | Result/use |
|---|---|---|
| `fundamentals.snapshot` | `instrument_id`, approved metric IDs, snapshot ID | Period/vintage/unit-aware facts and missingness for LLM explanations and quant services |
| `filings.search` | `issuer_id`, approved forms, bounded date window, limit | Accession, availability, document references and coverage |
| `filings.read` | Stored filing ID and approved section ID | Rights-approved untrusted excerpt; no arbitrary document URL |
| `macro.snapshot` | Approved series IDs and snapshot ID | Values, units, vintages, release/availability and freshness |
| `events.upcoming` | Instrument IDs and bounded horizon | Known schedules, timestamp precision and uncertainty; not proof of an event-free period |
| `features.snapshot` | Instrument ID and approved feature-set version | Deterministically computed features, dependencies, masks and quality flags |
| `data.quality` | Snapshot ID | Gaps, conflicts, entitlement state, invalidations and consumer suitability |

Use the existing `TOOL_RESULTS` envelope and `OK`, `PARTIAL`, `STALE`, `ERROR`,
`DENIED`, `UNAVAILABLE` and `CANCELLED` statuses. Register strict schemas, aliases,
output bounds, tests and rights checks before advertising a tool. Models cannot
self-report a snapshot or a completed source-verification check into existence.

A context bundle contains the decision cutoff, instrument identity, permitted
facts with evidence IDs, filing/news excerpts, separately labeled sentiment,
feature values, contradictory evidence and missing-data warnings. Keep the
existing 6,000-token total tool-result budget and 1,500-token per-call default;
return references and pagination rather than whole filing dumps. A model cannot
use an omitted article as if it had read it.

Treat source text as untrusted, even when hosted by a regulator. Ignore embedded
requests to change policy, call tools, disclose secrets or override instructions.
The LLM may explain a balance-sheet change, classify an event or propose a
research constraint. Rust computes ratios, volatility, costs and limits; the LLM
must cite the exact stored inputs and must not invent missing estimates or prices.

FinBERT receives only permitted language/text with distinct lineage and its pinned
model revision. Perpetual receives only its approved feature schema and trained
missingness behavior. Neither a sentiment probability nor an LLM's confidence is
a target-hit probability. A new observation cannot manufacture the missing
four-outcome forecasts/payoffs required by the existing fusion/utility contract.

## Deterministic features and calculation semantics

All proposed features require versioned definitions, unit checks, minimum sample
counts and out-of-sample evaluation. Start with a small approved set; do not add
hundreds of indicators because the APIs expose them.

| Family | Example calculation | Important constraints |
|---|---|---|
| Momentum | `log(C_t / C_(t-k))` | Positive prices; completed bars; consistent split/dividend basis and session indexing |
| Realized volatility | Sample standard deviation of daily log returns times `sqrt(252)` | 252 is a declared convention, not a calendar truth; sufficient observations and no silent gap filling |
| ATR | `TR=max(H-L,abs(H-C_prev),abs(L-C_prev))`; versioned Wilder smoothing or declared alternative | All OHLC inputs on the same split basis; seed/lookback pinned; ATR has price units |
| Relative volume | Completed-session volume divided by trailing comparable-session median | Nonzero denominator; intraday comparisons require same elapsed-session bucket |
| Revenue growth | Comparable-period revenue divided by prior comparable-period revenue, minus one | Same currency/concept/period basis; zero/negative bases flagged; no annual-versus-quarter mixing |
| Profitability | Net income divided by comparable revenue | Sector-specific interpretation; missing or incompatible facts abstain |
| Leverage/liquidity | Reviewed debt/cash/equity or current-ratio definitions | Metric mappings and sign conventions pinned; do not apply industrial-company ratios blindly to banks |
| Macro | Latest available vintage, lagged change, or training-window-only standardization | Observation period differs from release time; no revised-future data |
| Events | Time to verified earnings/release, filing type, evidence age | Uncertain event time remains uncertain; consensus surprise needs an independently timestamped expectation |
| News | Distinct-lineage event count, sentiment distribution, contradiction/missingness flags | Duplicate syndication is not corroboration; absence under partial coverage is not zero events |
| Market context | Lagged benchmark return, breadth or correlation | Disclose covered universe and effective membership; no claimed full-market breadth from a shortlist |

For SEC facts retain taxonomy, tag, unit, start/end, fiscal year/period, form,
accession, filed date and supported acceptance/publication evidence. Avoid
combining original and amended facts into a fictional filing. Derive standalone
quarters from year-to-date figures only with compatible contexts, units and
as-of versions. Maintain explicit tag mappings and unmapped facts rather than
assuming every issuer uses the same tag. Cross-provider disagreements stay
visible until the underlying filings/definitions resolve them.

Keep historical raw execution prices and explicitly defined adjusted indicator
series separate. A split-adjusted series is not necessarily total-return
adjusted. Never double-count dividends, treat a split as a crash, execute at a
back-adjusted price, or mix differently adjusted OHLC fields. Backtests model
corporate actions and instrument changes explicitly.

## Safeguards and degraded operation

These checks feed the existing independent readiness gate. They do not confer
order authority or replace broker-native protection. New policy thresholds need
owner approval and evaluation; no invented threshold is automatically active.

| Condition | New entry / target increase | Existing holding |
|---|---|---|
| Required price feed stale, wrong venue, missing bid/ask or unknown tradability | Block; never substitute an EOD/aggregate feed | Keep broker protection; mark quote monitor degraded and alert; do not calculate exits from fabricated quotes |
| Required source missing, incomplete, revoked or unlicensed | Block the dependent decision | Preserve valid protection; disclose the research gap |
| Optional feature unavailable | Only proceed if the approved model explicitly supports that missingness pattern | No automatic strategy substitution |
| Conflicting units, issuer mapping, adjustment or material facts | Quarantine; block affected calculations until resolved | Investigate without changing active thresholds from raw source text |
| Scheduled earnings/macro event inside a reviewed blackout window | Apply approved no-entry rule; unknown time cannot be treated as safe | Existing overnight/event policy governs; do not force an unapproved sale |
| Excessive observed volatility, spread or liquidity risk | Apply approved sizing/rejection logic within unchanged risk ceilings | Never loosen a stop simply because a new source is optimistic |
| Quota exhaustion, provider outage or delayed correction | Explicit degraded coverage; required input failure blocks entry | Protection/reconciliation retains reserved independent resources |
| Model/feature drift or schema mismatch | Abstain or use an independently approved compatible fallback | Existing plan/exit latch remains authoritative |

A delayed-data disagreement is not evidence of fraud or a broken exchange. Compare
like-for-like timestamps, currencies, sessions, adjustments and venues first.
Likewise, an empty earnings calendar is not proof of no impending announcement;
record the calendar's coverage and review status.

Material filing/news events may enqueue bounded reassessment and require additional
verification. They cannot directly produce `SELL`, remove a protective order, reset
an entry slot or raise the 10% target ceiling. An already-triggered exit remains
triggered. No free-source outage should stop a valid broker reconciliation task.

## Weekly optimization and historical evaluation

Extend the [existing weekend workflow](./weekly-optimization.md); the owner still
schedules local Codex CLI. This documentation installs no automation or timer.

Freeze an export manifest containing source commit, cutoff, provider/endpoint and
rights revisions, instrument mappings, actual versus reconstructed availability,
corporate-action policy, missingness/gaps, raw/normalized checksums where permitted,
feature definitions, calendar version and approved model/calibrator versions.
The offline agent sees only permitted data and no broker credentials. Large raw
vendor exports need not and often should not enter a cloud prompt.

Ablate additions in registered stages: price-only baseline; macro context;
filing/fundamental context; distinct-lineage news; sentiment; selected interactions.
Compare against the unchanged champion and cash using chronological walk-forward
splits, training-only preprocessing, label-horizon purge/embargo and a protected
holdout. Account for transaction costs, FX, slippage, missed fills and event gaps.
Save rejected/no-trade decisions as well as trades.

Evaluate incremental calibration, Brier/log loss where appropriate, drawdown,
net return, turnover, coverage, abstention frequency, data/LLM costs and behavior
during provider outages. Score pending/censored outcomes only after valid labels
mature. Adding a source that increases apparent in-sample returns but damages
calibration or robustness is not a successful integration.

Historical current-only statements, revised macro data, today's index membership,
missing delisted securities, newly reconstructed news timestamps and later-trained
LLM knowledge can all contaminate evaluation. Record those limitations. A modern
LLM retrospectively labeling old stories may know later outcomes; use frozen
prospective outputs or explicitly label the experiment as potentially contaminated.
There is no free-data promise of a complete survivorship-free, intraday execution
archive. Insufficient data limits the claim; it must not be filled with synthetic
performance observations.

Lean 4 work may prove conditional properties such as eligible-input cutoffs,
nonnegative budget reservations, policy bounds and rejection of missing required
features. It does not prove a provider is truthful or a strategy profitable.
QuantConnect LEAN uses a reviewed data/normalization bridge and execution model;
passing its backtest does not activate a strategy. Preserve independent promotion,
shadow evaluation, rollback and existing-position protection.

## Implementation sequence and acceptance

| Phase | Concrete deliverable | Completion evidence |
|---|---|---|
| 1. Shared contracts | Capability/rights types, global quota ledger, typed observation envelope and fixture transport | Schema, role-boundary, concurrency and crash/restart tests |
| 2. Primary context | SEC submissions/concepts and FRED observations/vintages; immutable storage and snapshots | Timestamp/revision/period tests and reviewed account-free fixtures |
| 3. Historical prices | One reviewed price adapter, identity/action handling and deterministic indicators | Adjustment, missing-bar, calendar, warmup and unit tests |
| 4. News enrichment | Finnhub/GDELT collectors integrated with existing news lineage and invalidations | Capped-window, replay, correction and source-rights tests |
| 5. Consumers | Scoped tool schemas, context builder, feature contracts and readiness checks | Prompt-injection, entitlement, stale/partial and model-compatibility tests |
| 6. Laboratory | Rights-aware exporter, feature ablations and reproducible data manifest | Temporal-leakage tests, unchanged holdout registry and no self-promotion |
| 7. Deployment review | Operator approval, account-specific capability evidence and paper/shadow observation | No live activation without all existing independent launch gates |

Required failure cases include HTTP 200 quota notices, 401/403/429, retry deadlines,
malformed/oversized/compressed bodies, malicious continuation URLs, duplicate pages,
repeated cursors, invalid/future publication times, legitimate future schedule times,
midnight reset races, requests sharing a key across sources, crash-after-reservation,
expired rights, deletion propagation, ambiguous share classes, mixed currencies,
restated filings, FRED revisions, unclosed candles, early closes and DST changes.

Assert that a failed source never becomes an empty successful result; unavailable
never becomes zero; partial coverage never passes a required completeness gate;
revised data never rewrites earlier features; and an LLM cannot manufacture evidence
IDs, approve a capability, increase a budget or reach broker mutations. Test that
source failures leave independent position-protection and reconciliation paths alive.

Use fixtures by default. Opt-in live smoke tests must use reviewed credentials,
a tiny accounted request budget and no broker order calls. Capture redacted
endpoint/plan/field evidence; do not commit responses that violate data terms.

Existing repository checks to run during implementation are:

```sh
python3 scripts/verify-assets.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Database checks use the repository's isolated PostgreSQL runner separately. New
provider-specific test targets must actually exist before documenting them as
runnable. A documentation change is not evidence that these integrations, API
entitlements, broker capabilities, backtests or proofs have been tested live.

## Operations and observability

Expose provider/endpoint request and credit usage, budget remaining/reset, latency,
retry deadlines, successful watermarks, coverage windows, stale/partial rows,
quarantine causes, parse/schema failures, rights expiry, invalidation backlog,
feature availability and entry blocks caused by data quality. Logs use source and
request IDs, not credentials, full request URLs or unrestricted article text.

On outage, suspend affected collection, retain valid cached context with its real
age, block only decisions that require missing data, and alert with the capability
and last successful watermark. Resume from overlap/cursor after recovery; verify
completeness before clearing the degraded state. On suspected key leakage, disable
the account, rotate through the owner-controlled secret mechanism and inspect
redacted audit records. Do not automatically fail over to a paid or unlicensed feed.

Keep research/backfill concurrency and memory bounded on the 12-core/24-GB VPS.
Run heavy feature/backtest work below executor resource priority; stream or chunk
bulk history rather than loading the full universe into every LLM/model worker.
Retention capacity and inference costs remain real even when API access is free.

[td-credits]: https://support.twelvedata.com/en/articles/5615854-credits

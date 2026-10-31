# Financial-data contract drift report

Scope: the financial-data additions in the 17 documents below, checked against
the current working tree on 2026-10-31. Differences from the broader v1 design
are included even where that design already labels them as future work. This is
not an audit of unrelated, pre-existing subsystems described by these READMEs.

## README.md

No drift detected.

## config/README.md

No drift detected.

## config/credentials/README.md

No drift detected.

## config/credentials/financial-data/README.md

No drift detected.

## crates/trady-domain/src/README.md

No drift detected.

## crates/trady-application/src/README.md

No drift detected.

## crates/trady-adapters/src/README.md

No drift detected.

## crates/trady-adapters/src/financial/README.md

No drift detected.

## crates/trady-adapters/src/news/README.md

No drift detected.

## crates/trady-storage/src/README.md

No drift detected.

## crates/trady-cli/README.md

No drift detected.

## migrations/README.md

No drift detected.

## docs/financial-data.md

No drift detected.

## docs/credentials.md

No drift detected.

## docs/news.md

No drift detected.

## docs/v1/free-financial-data.md

- **Collection and scheduling scope:** the proposed separate historical,
  fundamental and macro ports are represented by one typed `FinancialProvider`
  port. `FinancialPage` has no continuation or request-usage field. The financial
  CLI implements explicit single-pass collection, with per-source cadence and
  leases; it has no durable backfill queue, cursor resumption, recursive saturated
  window splitting, priority scheduler or 80% optional-work deferral. See
  [the application ports](crates/trady-application/src/financial.rs) and
  [CLI collection](crates/trady-cli/src/financial.rs).
- **Capability and identity models:** `Capability` records review evidence,
  revision, expiry and credit weight, while `FinancialSource` contains the
  configured endpoint and identity. The design's plan, venue/field entitlement,
  delay, reset-policy and effective-dated symbol mappings are not separately
  machine-readable records. Extend these types and mapping validation before
  relying on automatic historical identity or entitlement reconstruction. See
  [domain contracts](crates/trady-domain/src/financial.rs) and
  [FinancialSource](crates/trady-application/src/financial.rs).
- **Observation metadata:** the implemented envelope does not represent every
  proposed field, including timestamp precision/timezone, provider-account ID,
  normalization time, effective mapping interval and frequency. SEC concept
  metrics retain period bounds and accession, but do not expose fiscal year,
  filing form or all XBRL contexts as distinct payload fields. See
  [FinancialObservation and FinancialPayload](crates/trady-domain/src/financial.rs)
  and [SEC normalization](crates/trady-adapters/src/financial/parse.rs).
- **Historical reconstruction:** `read_snapshot_inner` uses only the current
  source registry hash. Archived registry documents are not consulted to
  reconstruct older mappings or capabilities. There is no separate reconstructed
  history mode or authenticated vintage-delivery manifest. See
  [snapshot selection](crates/trady-storage/src/financial.rs).
- **Quota dimensions:** `BudgetLimits` and `reserve_credits` enforce UTC calendar
  minute/day/month credits. The design's independent endpoint ceilings, aggregate
  IP limits, sub-minute request spacing, hourly limits, byte allowances and
  unique-symbol counters are absent. Credentials are grouped by their fixed
  credential names, with one account per provider, rather than configurable
  account namespaces. Account changes can lower an existing window's ceiling;
  raising it waits for reset. See [budget contracts](crates/trady-application/src/financial.rs),
  [reservation storage](crates/trady-storage/src/financial.rs) and
  [shared news reservations](crates/trady-storage/src/research.rs).
- **Retry and caching policy:** rate-limit deadlines are durable, but the
  financial client has no jittered retry loop, bounded transient-failure retry
  worker or conditional-response cache. Source leases coalesce active work for a
  source, rather than maintaining the proposed parameter/window/vintage cache.
  See [FinancialClient::fetch](crates/trady-adapters/src/financial/mod.rs) and
  [PgFinancialRepository::reserve](crates/trady-storage/src/financial.rs).
- **Invalidation propagation:** corrections append
  `research.financial_invalidations`, but no consumer of that table propagates
  changes to dependent Perpetual contexts, forecasts, exports or unexecuted
  readiness dossiers. The proposed dependency graph, invalidation outbox and
  recomputation worker remain necessary for that behavior. See
  [correction writes](crates/trady-storage/src/financial.rs) and
  [migration 9](migrations/0009_financial_data.sql).
- **Consumer tool breadth:** implemented financial tools use approved `source_id`
  plus a 1–3 result limit, rather than the broader instrument/metric/form/date and
  stored-snapshot arguments proposed in the table. `filings.read`,
  `events.upcoming`, `features.snapshot` and `data.quality` are absent. See
  [registered schemas](crates/trady-adapters/src/research_jobs/mod.rs) and
  [tool dispatch](crates/trady-storage/src/research_jobs.rs).
- **Feature and calendar semantics:** `signals` provides last close, arithmetic
  window return and mean close. It does not implement the proposed log momentum,
  volatility, ATR, relative volume, accounting ratios, macro surprises or
  event/news/breadth features. Provider-response completeness and increasing bar
  dates do not establish expected-session continuity, corporate-action coverage or
  indicator warmup. Add explicit calendar and feature contracts before using
  those stronger meanings. See [signals](crates/trady-application/src/financial.rs)
  and [price parsers](crates/trady-adapters/src/financial/parse.rs).
- **Model and laboratory composition:** `model_features` exports a compatible
  Perpetual record, but no financial-specific automatic model-training,
  watchdog/readiness adapter, weekly dataset exporter, registered ablation or
  QuantConnect LEAN data bridge is wired to it. The proposed immutable laboratory
  manifest requires additional lineage, calendar, rights and evaluation fields.
  See [model_features](crates/trady-application/src/financial.rs) and
  [the existing lab boundary](lab/README.md).
- **Revocation beyond local storage:** purge removes local observations and
  dependent local manifests. It has no traversal of external exports, embeddings
  or trained-model influence, and it retains registry, collection and invalidation
  metadata. The design's broader deletion and artifact-review propagation needs
  explicit downstream ownership. See [purge](crates/trady-storage/src/financial.rs).
- **Operational diagnostics:** `health` reports source outcomes, budget usage,
  reset times and retry deadlines. The proposed latency, quarantine, successful
  watermark, invalidation-backlog and data-caused-entry-block diagnostics are not
  separately implemented. See [health](crates/trady-storage/src/financial.rs).

## docs/v1/free-financial-api-catalog.md

- **Additional candidate routes:** the catalog also describes SEC company-facts,
  older submission files and filing sections; FRED series/release metadata;
  Twelve Data quote/price; Finnhub general news, profile, earnings calendar and
  quote; and Alpha Vantage overview/daily history. These are not dispatched by
  the implemented financial/news request enums. See
  [financial requests](crates/trady-application/src/financial.rs),
  [fixed routes](crates/trady-adapters/src/financial/mod.rs),
  [Finnhub/GDELT dispatch](crates/trady-adapters/src/news/financial_news.rs) and
  [existing Alpha Vantage](crates/trady-adapters/src/news/alpha.rs).
- **GDELT discovery timing:** the collector validates `seendate` against the
  request and preserves unknown publication, but does not retain that vendor
  timestamp in a distinct stored field. It marks saturation partial without
  implementing recursive window splitting. Add a distinct discovery timestamp
  before consumers need the catalog's original discovery-time lineage. See
  [GDELT normalization](crates/trady-adapters/src/news/financial_news.rs) and
  [SourceItem](crates/trady-domain/src/research.rs).
- **CoinGecko route and batching:** the implementation uses one coin's
  `/coins/markets` response rather than the catalog's batched `/simple/price`
  example. The normalized payload lacks the example's 24-hour change. See
  [CoinGecko request construction](crates/trady-adapters/src/financial/mod.rs)
  and [MarketContext](crates/trady-domain/src/financial.rs).
- **Tiingo corporate actions:** the implementation reads raw daily OHLCV. It does
  not normalize the catalog's dividend/split fields, adjusted series or symbol
  metadata route. See [daily normalization](crates/trady-adapters/src/financial/parse.rs).
- **Provider resource accounting:** the catalog's FMP bandwidth, Tiingo
  hourly/unique-symbol/bandwidth and GDELT minimum-spacing requirements have no
  independent counters in the shared ledger. Conservative call ceilings and the
  response-size cap cannot represent all of those account policies. See
  [BudgetLimits](crates/trady-application/src/financial.rs) and
  [reserve_credits](crates/trady-storage/src/financial.rs).
- **Offline artifact provenance:** yfinance import accepts canonical daily-bar
  JSON and records import-time availability. There is no pinned Python importer
  or artifact envelope containing its revision, original download identity and
  adjustment/session configuration as proposed by the catalog. See
  [offline_page](crates/trady-adapters/src/financial/parse.rs).

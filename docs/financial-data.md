# Financial data and shared signals

Trady implements the catalog's bounded research adapters through a shared Rust
observation layer. [The registry](../config/financial-data.json) ships disabled;
it contains no credentials, entitlement approvals, or trading permission. The
[integration specification](./v1/free-financial-data.md) remains the broader
design; the [catalog](./v1/free-financial-api-catalog.md) explains provider-specific
access and rights reviews.

## Implemented interfaces

| Source | Implemented route/use |
|---|---|
| SEC EDGAR | Company concepts and recent submissions; fiscal periods, accessions and date-precision availability |
| FRED/ALFRED | Bounded series observations with explicit optional vintage, null missing values and actual receipt time |
| Twelve Data | Daily unadjusted OHLCV with reviewed symbol, exchange and currency |
| Finnhub | Company news through existing `SourceItem`, evidence and permitted FinBERT collection paths |
| Alpha Vantage | Existing news collector, with the same shared credential budget ledger |
| GDELT | Article discovery metadata through the news path; seen time remains distinct from publication |
| Financial Modeling Prep | One explicitly configured income-statement metric and reported currency |
| CoinGecko | Coin-ID-specific market context; no crypto trading authority |
| Tiingo / Massive | Alternative unadjusted daily history; no automatic vendor failover |
| yfinance | Canonical JSON daily-bar imports only; no runtime Yahoo client or dependency |

All requests use fixed HTTPS routes and exact destination hosts. The existing
transport pins public DNS, rejects compressed/oversized responses, and limits
bodies to 2 MiB. Authenticated requests do not redirect. SEC uses the central
contact-bearing User-Agent; CoinGecko and Tiingo use headers. Query keys are added
inside adapters, omitted from observation URLs, and excluded from error text.
Large SEC bulk downloads and article bodies are outside these routes.

## Credentials and activation

Use the [central credential controller](./credentials.md) and the
[financial credential guide](../config/credentials/financial-data/README.md).
New installations initialize the existing ignored `config/credentials.local.json`:

```sh
cargo run --locked --bin trady -- credentials init
cargo run --locked --bin trady -- credentials status
cargo run --locked --bin trady -- financial status
```

Initialization refuses to overwrite an existing file. Existing installations add
the required named entries to that same private file, or use the existing
environment overrides. No keys belong in either registry. `status` reads only
configuration and credential availability; it contacts no provider or database.

For each financial source, review its exact endpoint, plan/fields, identity,
currency/units, historical coverage and rights. Set `mapping_reference`, a
versioned `capability` evidence reference, review/expiry times and verified credit
weight. Set `rights.terms_reference`, retention, normalized-storage permission and
each permitted consumer independently. Enable both the source and its account.
The example account limits are conservative local ceilings, not provider promises;
lower them to the smallest verified account, endpoint and owner allowance.

For issuer facts, preserve the distinction between an issuer ID and a stock
instrument. Price mappings must name the reviewed internal instrument, venue and
currency; copying an example symbol does not approve a mapping.

Finnhub and GDELT use [config/research.json](../config/research.json) and existing
news rights. Their shared account limits are also governed by the synchronized
financial registry. Synchronizing the shipped disabled account entries blocks
those accounts until reviewed and enabled. A standalone news collector without a
financial account row uses conservative shared ceilings, additionally constrained
by its existing source limit. Alpha Vantage continues to use its existing key and
collector; no second allowance or news copy is created.

## Collection and storage

Run migrations through the existing restricted migrator, then collect explicitly:

```sh
cargo run --locked --bin trady -- migrate
cargo run --locked --bin trady -- financial collect --once --source fred_treasury_10y
cargo run --locked --bin trady -- financial health
cargo run --locked --bin trady -- news collect --once --source finnhub_company_news
```

Database commands use `TRADY_ANALYST_DSN`. The financial command synchronizes the
complete registry, disabling sources/accounts omitted from it; use one deployment
registry rather than competing partial registries. Migration 9 adds observation
versions, registry revisions, collection leases, account budget windows, durable
retry deadlines, invalidations and snapshot manifests. Runtime executors,
gateways and lab roles have no direct financial-payload access.

Before network I/O, a short transaction reserves weighted credits against the same
provider credential's UTC minute, day and month windows and obtains a bounded
source lease. Crashes and failed requests consume the reservation. Different
source IDs cannot create another allowance for the same key. Provider backoff
survives restarts; denied financial accounts require a registry review/revision.
Network calls occur outside collection transactions and executor locks.
The ledger enforces UTC minute/day/month credits. It has no independent hourly,
bandwidth, unique-symbol or minimum-spacing counters; account reviews must bound
those requirements separately. Lower limits take effect within an existing
window; raising a limit takes effect after that window resets.

The default collection window is the previous 366 calendar days, ending yesterday.
Use `--start YYYY-MM-DD --end YYYY-MM-DD` for a smaller reviewed interval. Sources
keep their configured record cap; saturated responses, uncollected SEC historical
files and provider cursors remain `PARTIAL`. No untrusted continuation URL is
followed. Retry with narrower windows when appropriate; there is no automatic
unbounded backfill or new scheduler daemon.
Cadence applies per source, including when a subsequent request changes dates.

Identical latest content deduplicates; corrections append a new version and an
invalidation. Snapshots select the latest version actually available, received
and database-recorded by the decision cutoff. A newly downloaded vintage cannot
become an earlier production observation. Date-only filing availability is bounded
by the next UTC midnight. Null macro values stay null. Decimal JSON lexemes are
preserved locally without changing other services' JSON behavior.
Historical reads use the current reviewed registry hash. Archived registry
revisions do not restore earlier mappings, permissions or expired payloads.

`financial purge` removes expired/revoked payloads and manifests; collection and
reads also apply purge/current rights. Registry revisions, allowed audit hashes
and invalidation metadata remain. These operations do not delete unrelated
external copies or erase trained-model influence. A deployment whose rights forbid
retaining even audit metadata needs a separate deletion policy before activation.

## Reading and consuming signals

```sh
cargo run --locked --bin trady -- financial snapshot --source fred_treasury_10y
cargo run --locked --bin trady -- financial signals --source twelve_daily
cargo run --locked --bin trady -- financial features --source twelve_daily
cargo run --locked --bin trady -- financial snapshot --source twelve_daily --consumer EXPORT
```

Reads default to at most 500 rows, can accept `--limit`, and bind `--as-of` to a
nonfuture cutoff. Every result carries evidence IDs, source lineage, units,
capability/registry identity, receipt/public times, expiry, manifest hash and the
actual last collection window. Current rights are checked on every read. A capped
read or incomplete collection is partial; an outage is never an empty successful
result. A narrow complete collection does not assert a complete historical archive.
Coverage records provider-response completion for the requested window. It does
not certify exchange-session continuity, corporate actions or indicator warmup;
consumers needing those checks must establish them separately.

`FinancialRepository::read` is the shared application port. `signals` returns the
snapshot and deterministic last close, window return and arithmetic mean close.
Missing, stale, partial, denied or incompatible bars leave those features missing.
They are context features, not target-hit probabilities, forecasts or executable
bid/ask quotes. Broader volatility, calendar-aware indicators and economic-surprise
features remain separate reviewed work.

`features` defaults to `LOCAL_INFERENCE` and returns the existing Perpetual
`FeatureSnapshot` contract through `model_features`. This requires current local
inference or training rights, complete compatible data and at most 32 dependencies
to respect the existing lineage bound. Collect a small complete window for this
export. The model deployment must explicitly accept `trady-financial-context-v1`;
an existing model is not silently given new inputs or promoted. Missing features
are never replaced with zero. Training/export consumers require their separate
permissions even when display or external LLM access is allowed.

Service callers obtain snapshots through the rights-aware repository or source
snapshot builder. The pure signal/feature helpers consume those trusted records;
deserializing a caller-supplied consumer label does not establish permission.

The scoped research runtime registers `financial.snapshot`, `fundamentals.snapshot`,
`filings.search`, `macro.snapshot` and `prices.features`. Tools accept only an
approved source ID and a 1–3 result limit, bind the cutoff themselves, recheck the
job policy's source/instrument scope and require external-LLM rights. They return
bounded references and untrusted data under the existing tool-result budgets.
`prices.features` calculates from the complete bounded snapshot while returning
only a few observation references and the full dependency count. There is no URL,
SQL, key, review approval or broker authority in their arguments. Filing body
retrieval and automatic training/release activation are not registered tools.

The existing research worker supplies these tools through `ResearchTools`;
standalone basic chat retains its existing tool composition. FinBERT receives
Finnhub/GDELT text only through existing permitted and verified news packets.
Numerical inference and weekly jobs must explicitly use the consumer-appropriate
snapshots/features and keep their existing model, evidence and holdout gates.

For offline imports, supply a bounded JSON array of `FinancialPayload::DailyBar`
records whose currency and adjustment match the reviewed offline source:

```sh
cargo run --locked --bin trady -- financial import /path/to/bars.json --source yfinance_offline
```

Imports retain their actual import time and carry an unverified-coverage flag.
They cannot supply production replay history or automatic live fallback.

## Verification

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/verify-assets.py
python3 scripts/test-postgres.py
```

Fixtures exercise missingness, precision, identity/currency, completed bars,
consumer permissions, SEC revisions, error bodies, retry deadlines, shared-credit
races, crash-after-reservation, point-in-time reads, deduplication, revocation and
database role boundaries. The disposable PostgreSQL runner never targets the
deployment database. No paid-provider smoke test or live activation is implied.

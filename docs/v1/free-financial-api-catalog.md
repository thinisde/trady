# Trady v1 free financial API catalog

Status: researched integration reference, checked 2026-10-31. Read the
[integration specification](./free-financial-data.md) for authority, storage,
point-in-time semantics, quotas, model access, safeguards and acceptance tests.
These are candidate adapters and request templates, not account-level entitlement
certificates. No authenticated API smoke tests were performed for this document.

## What free access does and does not establish

A free API key does not imply every endpoint, ticker, historical interval, model
processing use or redistribution use is free. Documentation describes interfaces;
account-specific capability tests and rights review determine usable access.
Keep all new integrations disabled until those reviews pass. Endpoint errors must
not trigger paid upgrades, hidden scraping or attempts to bypass plan restrictions.

The earlier informal provider comparison is not a deployment contract. This catalog
uses primary documentation where readable, records unresolved verification, and
separates provider ceilings from conservative **proposed local caps**. Recheck plan
terms before activation and after changes; store the reviewed source and date.

| Provider | Public access information checked | Proposed local cap after approval | Primary role / limitation |
|---|---|---|---|
| SEC EDGAR | No API key; SEC publishes a 10-request/second maximum and identified user-agent requirement [S1] [S2] | 2 requests/second shared across workers; 2,000/day initially | Filings and XBRL; no market-price feed |
| FRED/ALFRED | API key required; observations support real-time/vintage selection [F1] [F2] [F3] | 20/minute and 200/day; honor actual provider limits | Macro context; revisions and date-level availability need care |
| Twelve Data Basic | 8 API credits/minute and 800/day; symbol/endpoint weights apply [T1] [T2] | 6 credits/minute and 700/day | Historical bars; exact market/field access must be verified |
| Finnhub | Official client confirms routes; dynamic pricing/docs did not expose a readable rate table in this check [H1] [H2] [H3] | 30 requests/minute and 300/day, only if reviewed account limits permit | News/company context; do not treat the previously quoted 60/minute as verified here |
| Alpha Vantage | Standard free limit 25 requests/day; premium functions exist [A1] [A2] | 20/day across all functions and collectors | Existing news adapter; small selective enrichment budget |
| Financial Modeling Prep | Free plan advertises 250 calls/day; pricing also lists free bandwidth constraints [P1] [P2] | 200/day and 400 MB per trailing 30 days, subject to lower account limits | Optional normalized statements; endpoint/ticker/history entitlements vary |
| GDELT DOC | Public DOC interface; article-list maximum 250 documented [G1] | 1 request/10 seconds globally and 200/day | News discovery, not a licensed full-article archive or guaranteed alert service |
| CoinGecko Demo | Pricing currently lists 100 calls/minute, 10,000/month and attribution [C1] | 5/minute and 3,000/month | Optional crypto context, not executable stock prices |
| Nasdaq Data Link | Free access is dataset-specific; `NDAQ/RTAT10` is identified as free, full `NDAQ/RTAT` is premium [N1] | 30/minute and 100/day only after account/table review | Selected context tables, not a universal free equity feed |
| yfinance | Unofficial Python library; research/education purpose and Yahoo data-use restrictions [Y1] | Manual bounded offline imports only | No production SLA, approved live fallback or Rust runtime dependency |
| Tiingo Starter | Pricing lists 50/hour, 1,000/day, 500 unique symbols/month and 1 GB/month [I1] | 40/hour, 800/day, 450 unique symbols/month and 800 MB/month | Alternative EOD history; symbol cap may preclude full-universe coverage |
| Massive Stocks Basic | 5 calls/minute, two years of history and EOD availability [M1] | 4/minute and 1,000/day as an owner cap | Alternative history/corporate-action context; not a real-time free quote feed |

These local numbers are engineering starting points, not promises of enough quota
for every workload. Respect the provider's accounting units and reset windows.
A rate or bandwidth ceiling does not grant access to an endpoint. FRED, GDELT and
Nasdaq limits must be filled from the actual reviewed service/account rather than
assuming the local cap is an advertised provider guarantee. For unknown ceilings,
source enablement remains blocked pending operator review.

## SEC EDGAR: primary filings and financial facts

Use issuer submissions to detect accessions, then retrieve relevant facts or filing
sections. The SEC documents JSON submission and XBRL APIs without API-key
registration, with fair-access controls [S1] [S2]. Start with the held issuer and
shortlist, not a full-market repeated scrape.

```http
GET https://data.sec.gov/submissions/CIK0000320193.json
User-Agent: Trady research REAL_OPERATOR_CONTACT
Accept: application/json

GET https://data.sec.gov/api/xbrl/companyfacts/CIK0000320193.json
User-Agent: Trady research REAL_OPERATOR_CONTACT

GET https://data.sec.gov/api/xbrl/companyconcept/CIK0000320193/us-gaap/Assets.json
User-Agent: Trady research REAL_OPERATOR_CONTACT
```

`REAL_OPERATOR_CONTACT` must be replaced with the operator's truthful contact,
not used literally. CIKs are zero-padded to ten digits in these routes. The example
identifies a filer; it is not approval to trade that company's securities. Historical
submission-file references must be collected when the current response does not
cover the requested interval. A company-facts response is not full filing text [S1].

Normalize submission metadata separately from numerical facts. Record accession,
form, primary document, publication/acceptance evidence and retrieval time. Keep
10-K/10-Q facts distinct from 8-K disclosures, amendments and other forms. Fetch
full documents only through an explicitly allowlisted SEC route and reviewed parser.
Do not assume Form 4/13F information is represented in company-facts XBRL.

Use filings to support fundamental calculations, verified catalysts, dilution/debt
research and contradiction checks. Their presence does not establish a safe entry:
filing meaning, availability and instrument mapping still need validation. The
integration specification defines period/unit/restatement handling. Company facts
may exceed the existing 2-MiB news limit; prefer narrow concepts or a separately
reviewed bounded document transport, never silent truncation.

## FRED and ALFRED: revision-aware macro context

Use FRED v1's explicit series, observation and vintage interfaces [F1] [F2] [F3].
Authenticate with the deployment's own `api_key`; request JSON explicitly.
The following templates omit the key, which the adapter adds privately:

```http
GET https://api.stlouisfed.org/fred/series?series_id=DGS10&file_type=json
GET https://api.stlouisfed.org/fred/series/observations?series_id=DGS10&file_type=json&observation_start=2026-01-01
GET https://api.stlouisfed.org/fred/series/observations?series_id=CPIAUCSL&file_type=json&realtime_start=2025-06-30&realtime_end=2025-06-30
GET https://api.stlouisfed.org/fred/release/dates?release_id=10&file_type=json
```

Treat series identifiers as an operator-reviewed allowlist. Candidate metadata to
review includes policy rates, Treasury yields, consumer prices, unemployment and
employment. Read each series' units, frequency, seasonal adjustment, source and
rights instead of deriving those from the identifier. A percentage-point change
is not the same as a percent change.

FRED normally returns today's knowledge of historical data; explicitly selected
real-time periods or vintages support reconstruction [F2] [F3]. Store missing values
as null. Release dates do not necessarily equal FRED/ALFRED availability and do not
supply a guaranteed intraday publication timestamp [F4]. The runtime collector
must preserve actual ingestion; historical reconstruction needs a conservative
availability rule. Do not use a newly revised value in an earlier decision.

Feed macro values to regime features and LLM context. Event blackout rules need a
separately verified schedule and owner-approved policy, not a guessed release time.
Do not calculate an economic surprise without a pre-release, independently
recorded expectation. Cache low-frequency series and collect around known releases
rather than polling every second.

## Twelve Data: primary candidate for historical OHLCV

The Basic allowance is credit-based. A multi-symbol call does not automatically
cost one credit; the documented daily Basic counter resets at midnight UTC
[T1] [T2]. Choose a bounded symbol universe and one-credit routes only after checking
actual account coverage and endpoint weights.

```http
GET https://api.twelvedata.com/time_series?symbol=AAPL&interval=1day&outputsize=100&format=JSON
GET https://api.twelvedata.com/quote?symbol=AAPL
GET https://api.twelvedata.com/price?symbol=AAPL
```

These routes are documented in the provider's API reference [T3]. Add `apikey`
inside the adapter and redact it everywhere else. The price/quote routes are
research candidates, not approval for Trady's execution feed. Explicitly review
adjustment behavior, exchange timezone, bar completion, sessions and price fields.
Request an approved adjustment setting where supported; retain it in the manifest.

Normalize bars into instrument, resolution, start/end/session, OHLCV, currency,
adjustment basis and source availability. Sort/deduplicate provider output before
feature calculation. Persist requested and returned windows; bounded output and
history restrictions must not masquerade as a complete backfill. A new bar version
may invalidate indicators computed from the old one.

Use locally calculated indicators so the lookback, smoothing and missing-data
rules are deterministic and do not spend an extra provider request per indicator.
The daily-universe budget example in the integration specification explains why a
free plan is plausible for EOD research but not unrestricted intraday polling.

## Finnhub: company news and event discovery

Candidate routes below are present in the official client [H3]. Its current base
URL uses `api.finnhub.io`; explicitly allowlist that exact host when choosing this
route. Do not rely on redirects from another hostname for authenticated requests.
The client uses the `token` parameter; keep it adapter-private and redacted.

```http
GET https://api.finnhub.io/api/v1/company-news?symbol=AAPL&from=2026-10-01&to=2026-10-30
GET https://api.finnhub.io/api/v1/news?category=general
GET https://api.finnhub.io/api/v1/stock/profile2?symbol=AAPL
GET https://api.finnhub.io/api/v1/calendar/earnings?from=2026-10-01&to=2026-10-30&symbol=AAPL
GET https://api.finnhub.io/api/v1/quote?symbol=AAPL
```

The route list does not establish free entitlement to calendars, financials,
historical candles or any particular exchange. The official pricing/docs pages
were reachable but their dynamic tables were not readable during this check
[H1] [H2]. Verify limits and each required endpoint in the account before enabling;
leave unsupported capabilities unavailable rather than assuming them from a free key.

Map news into existing `SourceItem` semantics: vendor item ID, URL, issuer mapping,
publication/first-seen time, source, permitted title/summary and coverage. A syndicated
story appearing through Finnhub, RSS and GDELT remains one reporting lineage.
Calendar dates/times may be uncertain or revised; store schedule versions.
A quote observation is not automatically a usable executable bid/ask.

Use news for source discovery, FinBERT input where permitted and LLM event research.
Do not use a vendor's sentiment or recommendation directly as a calibrated return
forecast. Cross-check material claims against primary issuer/regulatory material.

## Alpha Vantage: reuse the existing news adapter

Trady already has disabled `ALPHA_VANTAGE` news configuration and an adapter with
quota-body detection, bounded windows and continuation handling; see
[implemented news behavior](../news.md). Keep one shared account allowance for
this collector and any future fundamental/history calls.

```http
GET https://www.alphavantage.co/query?function=NEWS_SENTIMENT&tickers=AAPL&sort=LATEST&limit=50
GET https://www.alphavantage.co/query?function=OVERVIEW&symbol=AAPL
GET https://www.alphavantage.co/query?function=TIME_SERIES_DAILY&symbol=AAPL&outputsize=compact
```

Functions and parameters are documented by the provider [A2]; add `apikey`
privately. The standard free budget is 25 requests/day [A1], so use targeted jobs,
not per-minute or all-universe polling. Premium functions/history/real-time
entitlements must be reviewed separately. A visible function in documentation is
not proof it is included for this key.

Vendor article sentiment is a separately labeled observation, not a second
independent confirmation of the same story and not a replacement for the approved
FinBERT contract. Keep the original text lineage and scoring origin. Any additional
functions need their own typed DTOs, period/price semantics, fixtures and rights
checks; the existing news parser does not already implement them.

## Financial Modeling Prep: optional normalized statements

Prefer documented `/stable` routes for new work rather than copying older `/api/v3`
examples. The public free plan allowance does not guarantee all statement endpoints
or arbitrary ticker/history access [P1] [P2]. A documented income-statement route is
[P3]:

```http
GET https://financialmodelingprep.com/stable/income-statement?symbol=AAPL
```

Inject `apikey` privately. Enable balance-sheet/cash-flow/ratio routes only after
checking their current stable documentation and account entitlement; do not guess
a bundle from a successful income-statement example.

Treat normalized financials as secondary convenience data. Bind reporting period,
currency, filing references and supported availability; resolve material differences
against issuer/SEC filings. Unknown point-in-time history stays research-only for
historical decisions. A current company profile or ratio must not be backfilled as
if known years earlier.

Use this adapter to simplify reviewed metric extraction and identify discrepancies,
not to manufacture independent corroboration when it copies the same filing.
Track both call count and bandwidth, including retries and backfills. When required
statement access is denied, use a separately approved SEC workflow or return
unavailable; never silently purchase a plan.

## GDELT: broad news discovery, not full-text entitlement

The DOC API supports article-list queries and a bounded result count [G1]. Start
with tightly scoped issuer aliases and sector/geopolitical topics; avoid interpreting
all matches to an ambiguous word as the company.

```http
GET https://api.gdeltproject.org/api/v2/doc/doc?query=%22Apple%20Inc%22&mode=artlist&format=json&timespan=24h&maxrecords=100&sort=datedesc
```

No API key is used for this public route. URL-encode queries as parameters, not
string-concatenated instructions. The documented article-list maximum is 250; a
full response can be a truncated result set rather than complete coverage [G1].
Use overlapping bounded windows and deduplication; preserve unresolved saturation.
Do not invent a pagination cursor the endpoint does not provide.

Store discovery metadata and source URLs only under reviewed rights. Searchable
full text does not mean returned articles include full bodies or that publisher
copyright/LLM permissions are transferred. Fetching a publisher page is a separate
collector, allowlist and rights decision. Preserve reported seen-time semantics;
GDELT discovery time is not automatically the publisher's exact publication time.

Use the data to broaden research and identify risk topics. GDELT tone or mention
volume is not a stock-return forecast. Missed queries, multilingual coverage,
duplicate syndication and source selection must appear in feature-quality masks.
The debut documentation is an older interface reference: verify present query
windows and runtime behavior before using it as an archive or availability promise.

## CoinGecko: optional crypto market context

Use the Demo host and Demo header, not the Pro host/key combination [C2]:

```http
GET https://api.coingecko.com/api/v3/simple/price?ids=bitcoin,ethereum&vs_currencies=usd&include_24hr_change=true&include_last_updated_at=true
x-cg-demo-api-key: SECRET_INJECTED_BY_ADAPTER
```

Use stable coin IDs rather than symbol guessing. Retain `last_updated_at`; a fresh
HTTP fetch does not prove the underlying aggregate was just updated [C2].
The checked pricing page currently lists 100 calls/minute and 10,000 calls/month
for Demo, with attribution [C1]. Do not copy older 30/minute numbers as timeless
facts; the actual account and current terms govern activation.

A single batched request every 15 minutes consumes 2,880 calls in a 30-day month
before retries. A 3,000-call local cap leaves little reserve and a 31-day month
needs 2,976 scheduled calls, so the scheduler must budget by the actual month and
skip optional polls before exhausting its cap. No automated paid fallback.

Use permitted crypto observations only as optional contextual features for relevant
stock research. They do not expand the stock-only universe, supply broker FX,
provide executable prices or authorize cryptocurrency orders. Review retention,
model processing and attribution separately; do not infer commercial rights from
a free Demo key.

## Nasdaq Data Link: select a specific free table

This is a marketplace/access platform, not one uniform free dataset. One concrete
candidate is `NDAQ/RTAT10`, the daily top-ten retail-activity table; the full-universe
`NDAQ/RTAT` product is premium. Its documentation describes a reporting lag and
historical coverage issues [N1]. Do not represent the top-ten subset as market-wide
coverage or a same-day live signal.

```http
GET https://data.nasdaq.com/api/v3/datatables/NDAQ/RTAT10/metadata?api_key=SECRET_INJECTED_BY_ADAPTER
GET https://data.nasdaq.com/api/v3/datatables/NDAQ/RTAT10.json?api_key=SECRET_INJECTED_BY_ADAPTER
```

Review table metadata, filters, column types, pagination and actual account limits
before enabling row downloads. Keys are redacted from every logged URL. Persist
the table code, metadata revision, historical symbol interpretation and supported
availability. Follow only allowlisted, reconstructed continuation requests.

Research use is a contextual retail-activity feature, subject to incremental
validation. Absence from the top ten is not zero activity. Free sample rows in a
premium table do not establish a complete free backtest dataset. Do not default to
old WIKI examples or assume SHARADAR fundamentals are free current data.

## yfinance: explicitly offline and unofficial

The maintainers describe yfinance as an unofficial Python tool for research and
education; Yahoo data-use terms still apply [Y1]. This is not a native Rust REST
integration with a guaranteed free service contract.

Allow only a separate optional importer producing a reviewed, provenance-rich
artifact for the offline laboratory. Pin the importer revision and explicitly set
adjustment/session behavior rather than relying on changing library defaults.
Keep raw and adjusted semantics distinct and record the import time, source,
missingness and limitations. No live execution fallback, anti-bot bypass, proxy
rotation or runtime dependency is introduced.

Do not use current constituents and current company metadata to claim a complete
point-in-time backtest. An offline import can help prototype feature code; it does
not demonstrate licensing, survivorship completeness, true historical availability
or live strategy readiness. All production application processes remain Rust.

## Tiingo: alternative EOD history

The Starter pricing lists separate hourly, daily, unique-symbol and bandwidth caps
[I1]. In particular, an hourly cap matters even when daily allowance remains, and
the 500-symbol/month ceiling is not a promise that the full effective S&P universe
plus benchmarks fits. Fundamentals API access is separately described on pricing.

```http
GET https://api.tiingo.com/tiingo/daily/aapl
GET https://api.tiingo.com/tiingo/daily/aapl/prices?startDate=2026-01-01&endDate=2026-10-30
```

Resolve the token through the adapter using the provider's reviewed authentication
method. The EOD reference describes raw and adjusted prices, dividend/split fields
and possible later corrections [I2]. Preserve their semantics; do not confuse an
EOD update with a streaming quote or a dividend-adjusted return with a fill price.

Use Tiingo as an alternative to the primary history provider, not an automatic
second download of everything. Verify field/adjustment parity before switching a
feature pipeline. Revisions append versions and invalidate dependent features.
An apparent discrepancy caused by differing dividend adjustments is not evidence
that either vendor is wrong.

## Massive: alternative EOD history and reference data

Stocks Basic currently advertises five calls/minute, two years of history and EOD
availability, even though minute aggregates are listed [M1]. Bar resolution is not
delivery timeliness: free minute history must not be described as a real-time feed.

```http
GET https://api.massive.com/v2/aggs/ticker/AAPL/range/1/day/2026-01-01/2026-10-30?adjusted=false&sort=asc&limit=5000
```

Add the reviewed API key privately. The aggregate reference documents this route,
its timestamp semantics, split-adjustment flag, limit and optional next-page URL
[M2]. Its adjustment option concerns splits; do not label it a dividend-total-return
series. No eligible trades can mean no aggregate bar, so missing intervals need
coverage-aware handling rather than automatically invented flat candles.

Treat `next_url` as untrusted continuation data and enforce the shared transport
boundary. Store each response window and account-plan history limit. Use this
provider for a deliberately selected EOD/history adapter or cross-check, never to
silently rescue an execution quote failure. Live bid/ask/venue entitlements remain
a separate deployment requirement.

## Capability smoke-test record

Each adapter needs a redacted, deployment-owned record before enablement. Record
reviewed plan/terms, effective date, source URLs, actual request units, auth method,
endpoint/symbol/window tested, observed fields/timezones, response/error semantics,
known delay, pagination and retention/model permissions. Test at least one
permitted ordinary response and representative unsupported/missing cases without
exhausting or deliberately stress-testing the provider's rate limit.

A successful smoke test is not an availability SLA. Revalidate on account changes,
API version changes, persistent 403/429 responses, field drift or expiring rights.
Only approve the tested scope. No source here completes the broker, native-stop,
exchange-calendar, historical-membership or live quote requirements by itself.

## Primary references

References were checked for interface/access information on 2026-10-31. Dynamic
pages and endpoint-level access still need the operator checks described above.
Historical documentation publication dates are not claims of a recent API change.

[S1]: https://www.sec.gov/search-filings/edgar-application-programming-interfaces
[S2]: https://www.sec.gov/search-filings/edgar-search-assistance/accessing-edgar-data
[F1]: https://fred.stlouisfed.org/docs/api/api_key.html
[F2]: https://fred.stlouisfed.org/docs/api/fred/series_observations.html
[F3]: https://fred.stlouisfed.org/docs/api/fred/realtime_period.html
[F4]: https://fred.stlouisfed.org/docs/api/fred/release_dates.html
[T1]: https://twelvedata.com/pricing
[T2]: https://support.twelvedata.com/en/articles/5615854-credits
[T3]: https://twelvedata.com/docs
[H1]: https://finnhub.io/pricing
[H2]: https://finnhub.io/docs/api
[H3]: https://raw.githubusercontent.com/Finnhub-Stock-API/finnhub-python/master/finnhub/client.py
[A1]: https://www.alphavantage.co/support/
[A2]: https://www.alphavantage.co/documentation/
[P1]: https://site.financialmodelingprep.com/pricing-plans?direct=true
[P2]: https://site.financialmodelingprep.com/faqs
[P3]: https://site.financialmodelingprep.com/developer/docs/stable/income-statement
[G1]: https://blog.gdeltproject.org/gdelt-doc-2-0-api-debuts/
[C1]: https://www.coingecko.com/en/api/pricing
[C2]: https://docs.coingecko.com/demo/reference/simple-price
[N1]: https://data.nasdaq.com/databases/RTAT/documentation
[Y1]: https://ranaroussi.github.io/yfinance/
[I1]: https://www.tiingo.com/pricing
[I2]: https://www.tiingo.com/documentation/end-of-day
[M1]: https://massive.com/pricing?product=stocks
[M2]: https://massive.com/docs/rest/stocks/aggregates/custom-bars

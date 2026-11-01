# Provider API research — 2026-11-01

This record captures the provider documentation reviewed on 1 November 2026 and
the read-only probes made with the configured free credentials. It is an input
selection record, not an entitlement grant. A documented feature is not treated
as available to Trady until the account, source rights, freshness and exact
instrument binding are independently recorded by the input service.

The T3 preview browser was requested for this research, but its environment
reported `available=false` and no tab could be opened. The documentation links
below were therefore fetched through the web and Context7 documentation
retrieval paths. No credential value was printed or written to this document.

## Trading 212 Public API

Primary documentation: [API limitations and rate limiting](https://docs.trading212.com/api/section/general-information/api-limitations),
[market orders](https://docs.trading212.com/api/orders/placemarketorder),
[limit orders](https://docs.trading212.com/api/orders/placelimitorder),
[cancel order](https://docs.trading212.com/api/orders/cancelorder), and
[historical events and CSV exports](https://docs.trading212.com/api/historical-events/requestreport).

The API is beta and is available for Invest and Stocks ISA accounts. The demo
base URL is `https://demo.trading212.com/api/v0`; the live base URL is
`https://live.trading212.com/api/v0`. Order execution is restricted to the
primary account currency and the API does not expose multi-currency account
execution. Limits are applied per account, with response headers exposing the
limit, period, remaining calls and reset time.

The relevant documented endpoints and limits are:

| Capability | Documented feature | Limit or constraint |
| --- | --- | --- |
| Account and cash | Account summary/cash and positions | Read-only account reads; the observed `account/summary` response advertised 1 request per 5 seconds; response values use the primary currency |
| Instrument identity | Exchanges and instruments, including ticker, ISIN, currency and trading hours | The observed `metadata/instruments` response advertised 1 request per 50 seconds; metadata is identity evidence, not executable quote evidence |
| Market order | Demo and live order endpoint | 50 requests per minute; the live environment currently supports market orders only |
| Limit order | Quantity, limit price, `DAY` or `GOOD_TILL_CANCEL` | 1 request per 2 seconds; order placement is quantity-based |
| Stop/stop-limit | Stop and stop-limit order resources are documented for the API | Must be proved on the account before runtime use; no documentation alone promotes `UNKNOWN` |
| Cancellation | `DELETE /equity/orders/{id}` | 50 requests per minute; cancellation is not guaranteed once filling starts |
| Order state | Pending and historical orders include quantity, filled quantity, value, status, limit/stop fields and instrument identity | Statuses include `PARTIALLY_FILLED` and `FILLED`; this is a schema promise, not an account-bound observation |
| History | Orders, transactions, dividends and asynchronous CSV reports | History lists: 6 requests per minute; export list: 1 per minute; export request: 1 per 30 seconds; list pages max 50 |

The configured demo account returned HTTP 200 for account and metadata reads.
The catalog contained the exact Procter & Gamble row
`PG_US_EQ` / ticker `PG` / ISIN `US7427181091` / USD. The similarly prefixed
`PGC_US_EQ` row is Peapack-Gladstone Financial Corp with ISIN
`US7046991078`, so it is rejected by the crosswalk. The raw catalog is retained
privately at `var/identity/t212-demo-metadata-20261101.json` with SHA-256
`03ae2dc6133d2b47f93d511b99e4a313d79c65420f66d146c1d031825f887665`.
The reviewed account-bound capability record was subsequently imported at
`var/cold-start/capability-evidence/capability-review-20261101-1931.json` with
hash `f6b0438415b53a47eca3725a4e2fff5097c5fa45f6c7e9ff64f3bc8e74e4d780`; it
promotes only cash-account and exact-instrument identity.
For the selected volatility trade, a separate XOM-bound record was imported at
`var/cold-start/capability-evidence/capability-review-xom-20261101.json` with
hash `80615d26cfacbb0ae952992d006a02c54edaa12faa07c8d163367574940e6d8e`.

The public API has no pre-trade NBBO endpoint. The broker metadata read proves
identity and availability only. The cold-start account has EUR 5,000 available,
but no historical orders or fills, so fee, FX, spread, slippage, cancellation,
partial-fill and protection support remain unverified.

## Trading 212 Invest profile selected for the first shadow/paper cycle

The selected profile is **Invest, cash account, EUR primary currency, demo
environment, no leverage, no CFD, no shorting, one supervised equity order at a
time, and mandatory attached protection once the broker proves it**. The first
research allocation remains bounded by the repository policy (2% of available
cash and no more than EUR 100), with `PG_US_EQ` as the exact broker identity.
Fractional quantity remains disabled until account-bound fractional-fill
evidence is imported. This profile is recorded for the paper configuration; it
does not change the broker account or authorize a live order.

Trading 212's current Invest fee page states that trading commission and custody
are free and that its FX fee is 0.15% when an order is converted between account
and instrument currencies. The [FX explanation](https://helpcentre.trading212.com/hc/en-us/articles/360018909758-What-is-the-FX-fee-Invest-Stocks-ISA)
and [fee table](https://helpcentre.trading212.com/hc/en-us/articles/11471996799517-What-are-the-fees-in-the-Invest-ISAs-and-SIPP)
also list exchange or tax charges that can apply to particular venues. The
runtime must use the actual broker fill wallet impact for the cost record; the
published fee page is not a substitute for a genuine fill.

## Twelve Data Basic

Primary sources: [Basic pricing](https://twelvedata.com/pricing),
[credits](https://support.twelvedata.com/en/articles/5615854-credits), and
[API documentation](https://twelvedata.com/docs).

The free Basic tier publishes 8 API credits per minute and 800 per day, with 8
trial WebSocket credits. Endpoint weights are per symbol; `/price`, `/quote`,
`/eod` and daily `/time_series` are listed as 1 credit per symbol. The plan
offers reference and market data for a limited set of markets and the API
returns historical bars, latest-price responses, and technical indicators where
the endpoint is included. The documentation also says that Basic data is for
internal non-display use and that null values are valid when a metric is not
available.

The configured key returned HTTP 200 for `PG` and daily history, but the quote
response contained a last/close value with `bid=null` and `ask=null`. It is
therefore suitable for research freshness checks only, not for Trady's
10-second executable quote gate. The local input-service cap remains below the
provider limit so that retries cannot consume the full free quota. A EUR/USD
exchange-rate probe returned a timestamp/date inconsistent with the current
clock (future-dated), so it was rejected rather than used for the €50 trade.

## Finnhub free tier

Primary documentation: [rate-limit](https://finnhub.io/docs/api/rate-limit),
[quote](https://finnhub.io/docs/api/quote),
[stock candles](https://finnhub.io/docs/api/stock-candles), and
[company profile](https://finnhub.io/docs/api/company-profile2).

The free plan is commonly documented as 60 REST calls per minute, with a global
30-calls-per-second ceiling. The REST surface includes quote snapshots, OHLCV
candles, symbol/profile data, company news and fundamentals; a WebSocket
surface exists for streaming trades where the account is entitled. Endpoint and
market entitlement can still reduce what a free key returns.

The configured `PG` quote probe returned HTTP 200 and a current/last price, but
bid and ask were null. No Finnhub response was accepted as an executable quote,
fee, FX or fill record.

## Massive (formerly Polygon)

Primary sources: [Stocks product page](https://www.massive.com/stocks),
[REST quickstart](https://massive.com/docs/rest), and the
[authorization/entitlement explanation](https://massive.com/knowledge-base/article/why-am-i-not-authorized-when-attempting-to-connect-to-massives-websockets).

The free Basic tier publishes 5 requests per minute, end-of-day stock bars and
reference/corporate-action data. Trades, quotes, snapshots, WebSockets and flat
files are not included on Basic; delayed or real-time feeds require a paid tier
and the relevant product/exchange agreements. The configured NBBO request
returned HTTP 403, so the key has no entitlement for Trady's quote requirement.

## Financial Modeling Prep

Primary sources: [quickstart](https://site.financialmodelingprep.com/developer/docs/quickstart),
[pricing](https://site.financialmodelingprep.com/developer/docs/pricing), and
[free-plan FAQ](https://site.financialmodelingprep.com/faqs?code=platform).

The current Basic/free documentation publishes 250 requests per day, US data,
personal use, no premium endpoints and a five-year stock-data window. The
configured S&P 500 membership request returned HTTP 402, so membership and
effective-date data are not available under the current entitlement. No FMP
response is used as a deployment input.

## Alpaca Basic

Primary sources: [market-data subscriptions](https://docs.alpaca.markets/us/docs/about-market-data-api),
[latest stock quotes](https://docs.alpaca.markets/reference/stocklatestquotes-1), and
[historical stock quotes](https://docs.alpaca.markets/reference/stockquotes-1).

The configured Alpaca paper credentials are valid: the paper account returned
HTTP 200 and `ACTIVE`. Basic market data is free, with real-time equities limited
to the IEX feed, 30 WebSocket symbols and 200 historical API calls per minute.
The current market clock reports the NYSE closed until
`2026-11-02T09:30:00-05:00`; consequently the latest IEX quote response for
`PG`/the candidate set returned no current bid/ask. A historical SIP XOM quote
was readable, but it is not fresh execution evidence.

Using 64 daily IEX bars from 3 August through 30 October 2026, annualized
population standard deviation of close-to-close returns ranked `XOM` highest
at 23.998906%. The selected bounded instrument is therefore
`XOM_US_EQ` / ISIN `US30233Q1085` / USD. The private raw responses and selection
hash are retained under `var/research-training/2026-11-01/alpaca/` and the
selection is explicitly `RESEARCH_ONLY_OUT_OF_SESSION`.

Alpaca is now a candidate quote source for the input service, but it cannot make
the Trading 212 first-order gate pass while the market is closed. At the next
open session, a fresh IEX bid/ask must be captured within the 10-second runtime
window and independently imported/reviewed. Basic IEX is also a single-venue
feed, so it must not be described as consolidated NBBO.

## Decision

The currently usable free sources are read-only Trading 212 identity/account
metadata, Alpaca historical/IEX research data, and Twelve Data/Finnhub
last-price/bar responses. Alpaca supplies a future executable-quote path during
an open IEX session, but not a current quote now. None supplies the Trading 212
fee, FX, protection, cancellation or fill evidence required by `first-order`;
the input-service deployment remains blocked and the shadow config remains
record-only.


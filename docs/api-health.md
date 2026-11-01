# API health checks

`trady api-health` reports credential availability without making requests. Use
`trady api-health --online` for explicit diagnostics, or select one integration:

```sh
cargo run --locked --bin trady -- api-health
cargo run --locked --bin trady -- api-health --online --provider fred
```

Provider names are `sec`, `fred`, `twelve_data`, `finnhub`, `alpha_vantage`, `fmp`,
`coingecko`, `tiingo`, `massive`, `gdelt`, `google_news`, `publisher_rss`, `telegram`,
`x` and `yfinance`. Credentials use the [central store](./credentials.md).

Online checks use fixed HTTPS routes and at most one GET per selected provider.
Financial routes reuse the shipped adapter with a small recent window and a
one-record limit where supported. SEC submissions, Tiingo and RSS responses can
contain multiple entries. Telegram uses
`getMe`, which does not send messages or acknowledge updates. Authenticated
requests follow no redirects; the shared transport bounds DNS, connection,
response time and response size. There are no retries or automatic fallbacks.

Each result includes `provider`, credential availability, `status`, `http_status`,
`latency_ms`, `response_records`, numeric `retry_after_seconds` when supplied, a
whitelisted `failure_code` for transport failures or malformed GDELT payloads, and
the number of logical requests. `healthy` requires a successful HTTP response and
the expected payload shape. Empty results, denied access, exhausted credits,
rate limits, invalid payloads and transport failures remain distinct. HTTP 200
with a provider error does not count as healthy. A healthy probe establishes
only that diagnostic endpoint's behavior, not every endpoint or source's rights.

GDELT HTTP 200 responses without an article array remain `invalid_response` with
`failure_code: gdelt_response_invalid`. They receive a 60-second suggested backoff
when no numeric Retry-After was supplied, as do GDELT rate-limit responses. A
valid `articles: []` response remains distinct as `empty_data`. These diagnostics
make one request and leave retry scheduling to the caller.

Output contains no tokens, request URLs, provider error text or response bodies.
No raw response is retained. Online checks do not synchronize a registry, write
database records, enable collection, authorize entries or call a broker. An
ordinary provider request may consume the existing account's request allowance.
X reports `requires_existing_cost_gate` without a request; use the reviewed
[X workflow](./news.md#x-pilot-and-resource-budget) for metered X tests.
Yfinance reports `offline_import_only`.

The command exits successfully when it can produce the diagnostic report;
automation should inspect each check's `status`, rather than treating process
success as proof that every API passed.

Verification: `cargo test -p trady-adapters api_health --lib --locked`.


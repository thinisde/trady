# Financial adapter blueprint

## Overview

Bounded read-only financial provider adapters for the shared Rust observation
layer. The [operator guide](../../../../docs/financial-data.md) covers current
routes, rights, collection and consumer signals. This scoped package has no child
subsystems; news discovery stays in the existing
[news package](../news/README.md).

## Interface

- `[mod.rs:FinancialClient]` implements the application `FinancialProvider` port.
  `FinancialClient::new` injects a transport, credential lookup and receipt clock;
  the default uses `SafeHttp`, the central credential controller and `Utc::now`.
  The public `transport` must support `HttpTransport::get_with_headers` for
  SEC, CoinGecko and Tiingo; its default rejects nonempty custom headers.
- `[mod.rs:request]` constructs fixed SEC/FRED/Twelve Data/FMP/CoinGecko/Tiingo/
  Massive routes and adapter-private authentication. It rejects yfinance
  network access. `PARSER_VERSION` records the normalizer revision.
- `[parse.rs:offline_page]` accepts bounded canonical daily-bar JSON for explicitly
  reviewed offline yfinance imports.

## Invariants

- `[mod.rs:FinancialClient::fetch]` checks source approval, window bounds, HTTP
  status and the 2 MiB body limit. Provider error bodies cannot become successful
  empty observations. Errors expose stable codes rather than raw URLs or content.
  HTTP 429/503 returns `Unavailable`; retry time uses integer/RFC2822
  `Retry-After`, then rate-reset epoch, then 15 minutes, at least one second ahead.
- `[mod.rs:request]` chooses credential names through domain
  `Provider::credential_name`, uses fixed HTTPS hosts and disables redirects.
  SEC requires the operator's contact-bearing User-Agent. The shared
  `[../news/http.rs:SafeHttp]` boundary pins public DNS and bounds header values.
- `[parse.rs:financial_json]` preserves decimal numeric lexemes locally.
  `[parse.rs:observation]` records source/entity/lineage identity, registry/content
  hashes, actual receipt, availability and expiry, omitting credential queries.
- `[parse.rs:page]` validates requested identities, periods, units/currencies and
  daily price basis. FRED missing markers remain null; SEC revisions retain
  fiscal periods/accessions. Completeness rules differ by provider: equality to
  the cap can remain complete for FRED, Tiingo and SEC concepts, while
  Twelve Data/FMP/Massive require fewer rows. Tiingo inherits identity/currency
  from the reviewed source rather than checking equivalent response fields;
  CoinGecko market context does not filter by the requested date window.
- `[parse.rs:offline_page]` validates the source and daily payload currency/basis,
  records import-time availability and always marks coverage unverified.
- `[mod.rs:FinancialClient::fetch]` implements provider access; collection,
  snapshot suitability and weighted budgets belong to the application/storage ports. An implemented route alone establishes no entitlement,
  executable quote, calibrated forecast or model deployment approval.

## Side Effects

`[mod.rs:FinancialClient::fetch]` calls its injected credential resolver,
transport and receipt clock. Defaults read private credentials and perform bounded
HTTPS through `SafeHttp`. `[mod.rs:request]` puts FRED/Twelve Data/FMP/Massive
credentials in queries and SEC/CoinGecko/Tiingo authentication in headers;
`fetch` removes queries from observation URLs. Receipt time is the later of the
injected clock and caller time. Neither parsing nor offline imports make network calls.
This package writes no database, execution ledger or broker order.

## Verification

```sh
cargo test --locked -p trady-adapters financial::tests
cargo test --locked -p trady-cli --test financial_cli
python3 scripts/test-postgres.py
```

Fixtures check precision, missingness, revisions, OHLCV identity, rights and retry
handling. The isolated database runner verifies reservations, corrections, replay,
retention and service-role boundaries separately.

# Trading 212 API implementation

The HTTP adapter supports explicit demo and live account bindings. It can read
account state and provides an executor-only order transport. Automated live trading
is still blocked: durable live intent dispatch, protection coordination, complete
readiness checks and the release gates are not implemented by this adapter.

## Read an account

Start with [demo configuration](../config/trading212.demo.example.json) or
[live configuration](../config/trading212.live.example.json). Copy the example to
a local deployment file and replace `broker_account_id` with the numeric account
number shown in Trading 212 and `base_currency` with its primary currency. Use a
`paper_` local account ID for demo and a `live_` ID for live. Environment selection
is explicit; there is no fallback between them.

The configuration's `environment` selects the matching endpoint and read-only
credentials from your secret manager, private process environment or
[central store](./credentials.md):

- `"demo"` uses `TRADY_T212_DEMO_READ_API_KEY` and `TRADY_T212_DEMO_READ_API_SECRET`.
- `"live"` uses `TRADY_T212_LIVE_READ_API_KEY` and `TRADY_T212_LIVE_READ_API_SECRET`.

Fill the appropriate pair in `config/credentials.local.json`. Environment values
override the matching entries. Both values must belong to that account and mode;
missing credentials fail before a request, without falling back to the other
mode or the former shared credential names. Use a key with only information
permissions. Credential values stay out of broker configuration, arguments and output.

```sh
cargo run --locked --bin trady -- t212 snapshot --config config/trading212.demo.example.json
cargo run --locked --bin trady -- t212 snapshot --config config/trading212.live.example.json
```

Replace the illustrative account values before running either command.
The command performs GETs for account summary, positions and pending orders and
prints a normalized snapshot with `entry_authorized=false`. It does not contact
PostgreSQL or submit/cancel an order. Successful snapshot collection is tested at
the adapter level against local HTTP fixtures. CLI tests cover help, missing
credentials and configuration/command rejection; successful CLI collection has not
been tested with Trading 212. No broker account or credential has been used.

## Implemented boundaries

The [adapter contract](../crates/trady-adapters/src/trading212/README.md) lists the
actual interfaces. `Trading212Reader` implements the application information port
and exposes instrument metadata, pending-order lookup and cursor-based order
history. It validates the actual account number and primary currency when reading
summary/snapshot data. Direct order/history/metadata reads require that identity
check and cache it for at most 60 seconds. Every executor mutation repeats the
summary binding check without using that identity cache.

`Trading212Executor` implements
`trady_application::broker::BrokerExecutionPort`: market, limit, stop and stop-limit
submission, plus cancellation by numeric broker order ID. Quantities are positive
in application requests; sell quantities become negative on the wire. Prices use
the instrument currency and metadata must identify a stock with the expected
currency. The API performs execution and account reporting in the primary account
currency; FX/cost policy verification remains the caller's responsibility.

The port is a low-level transport, not the validated entry use case. Callers must
persist an intent before sending, verify current fencing/readiness/policy and
available sell quantities, and coordinate existing protection. No command or model
tool exposes this mutation port. Existing `FoundationConfig`, paper intent SQL and
paper reconciliation restrictions have not been extended to live accounts.

## Failure and recovery behavior

- The client uses fixed HTTPS origins, Basic key/secret authentication, bounded
  timeouts, no redirects, no proxies, no automatic retries and an 8 MiB response
  limit. Decimal number tokens are parsed and emitted without binary floating point.
- Account snapshots use `cash.availableToTrade`, total owned position quantities
  including shares in pies, and all returned pending orders. Negative holdings,
  duplicate identities and malformed/unsupported quantity observations fail closed.
  Responses have no shared atomic snapshot timestamp: `observed_at` is local
  collection start, and collection taking over 60 seconds is rejected. Snapshot
  orders do not include an automatic history scan.
- Trading 212 does not document a client intent ID or idempotency key for these
  requests. `find_order(intent_id)` therefore fails explicitly. Store the returned
  numeric broker ID; a matching ticker/quantity/time is not proof of ownership.
- Each mutation is attempted once. Connection loss, timeouts, server errors,
  redirects and malformed/mismatched successful order responses report
  `outcome_unknown=true`. Reconcile before another mutation. Explicit rejection
  statuses are retained without echoing provider bodies or credentials.
- A cancellation receipt means the cancellation request was accepted. It does not
  mean the order is cancelled or that no fill can follow. A pending-order GET returning
  404 means only that the order is absent from the pending endpoint. Consult history,
  fills and positions; an unresolved outcome must remain unresolved.
- History is fetched one bounded page at a time. Continuation paths are validated
  and converted to typed cursors; arbitrary provider URLs are never followed.
  Repeated fill records deduplicate equal order totals rather than adding totals
  together. Conflicting observations fail instead of guessing.
- Per-endpoint pacing and provider `Retry-After`/exhausted `x-ratelimit-*` headers
  produce a retry delay without sleeping or resending. Trading 212 quotas are per
  account across keys and processes; this client's limiter is local to its instance.
  A future executor must share/coalesce account requests. Metadata is cached for
  ten minutes; neither metadata nor a successful request verifies capabilities.

## Capabilities and remaining work

All application capability flags remain `Unknown`. Public stop-order endpoints
do not establish account-specific fractional support, precision, broker-native
brackets/OCO, partial-fill protection, no-oversell coordination, quote freshness or
tradability. Position `currentPrice` is not used as a bid/ask quote feed. New entry
must remain blocked until those provider and application requirements are verified.

Next work includes the durable live intent/acknowledgment/unknown state machine,
broker ID and fill history persistence, restart/takeover recovery, native protection
and cancellation races, account-wide scheduling and rate limits, then account-specific
demo/shadow validation and the [live release gates](./v1/implementation.md).

## Verification and references

```sh
cargo test -p trady-adapters trading212 --locked
cargo test -p trady-cli --test trading212_cli --locked
```

Fixtures cover account binding, decimal precision, all four order bodies and both
sides, partial fills, unknown statuses, ambiguous submissions, rejection/rate limits,
cancellation acknowledgments and lost responses, malicious continuations and CLI
credential/endpoint boundaries. Cancellation racing with a fill remains future work.
The broker verification run on 2026-10-31 passed 16 adapter tests and two CLI tests. They do not verify an
actual Trading 212 account or complete the live execution acceptance cases.

Current behavior was checked on 2026-10-31 using Context7 and the official
[Trading 212 API reference](https://docs.trading212.com/), including its downloadable
[v0 OpenAPI](https://docs.trading212.com/_bundle/api.json?download),
[orders](https://docs.trading212.com/api/orders),
[accounts](https://docs.trading212.com/api/accounts) and
[positions](https://docs.trading212.com/api/positions). Transport configuration was
checked against [reqwest 0.12.28](https://docs.rs/reqwest/0.12.28/reqwest/blocking/struct.ClientBuilder.html)
and its [disabled-retry policy](https://docs.rs/reqwest/0.12.28/reqwest/retry/fn.never.html).

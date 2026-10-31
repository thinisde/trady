# Trading 212 broker adapter Blueprint

## System Overview

Typed synchronous access to the Trading 212 v0 API, with separate information
and executor clients. The adapter does not persist execution state or establish
policy, readiness, fencing or deployment authority. See the
[implementation guide](../../../../docs/trading212.md) for setup and remaining work.

## Interface

- `[mod.rs:Environment]`: `Demo`/`Live`, fixed `base_url()`, paper/live `mode()` and
  `read_credential_names()` for the matching central-store pair.
- `[mod.rs:Trading212Config]`: versioned non-secret account/environment/currency
  binding with `validate()`. `[mod.rs:Credentials]`: validated key/secret pair with
  redacted `Debug`; no serialization or public credential fields.
  `Credentials::read_from_store` resolves both read-only values for the explicitly
  selected environment from one store snapshot, with no shared/other-mode fallback.
- `[mod.rs:BrokerInstrument]`, `HistoryQuery`, `HistoryPage`: metadata and bounded
  history requests/results with normalized order totals and a typed continuation cursor.
- `[mod.rs:Trading212Reader]`: `new`, `with_clock`, `config`, `spendable_cash`,
  `pending_orders`, `pending_order`, `instruments`, `historical_orders`, `read_snapshot`.
  Implements application `BrokerInformationPort` and observation-only `BrokerPort`.
  Capabilities remain unknown; `find_order(intent_id)` explicitly fails.
- `[mod.rs:Trading212Executor]`: `new`, `information`; implements
  `BrokerExecutionPort::submit_order` and `cancel_order` from the
  [application contract](../../../trady-application/src/broker.rs).

## Local Boundaries

| File | Responsibility |
|---|---|
| [mod.rs](./mod.rs) | Public types, account identity, metadata cache, information operations and order transport |
| [dto.rs](./dto.rs) | Private wire DTOs, exact decimal numbers, order normalization and request serialization |
| [http.rs](./http.rs) | Private bounded HTTP client and per-endpoint cooldowns |

## Invariants

- `[mod.rs:Trading212Config::validate]` requires version 1, a positive numeric broker
  account, a 1–30 second timeout, and `paper_`/`live_` local ID matching environment.
  Production origins are fixed HTTPS URLs; arbitrary URLs are not configuration fields.
- `[mod.rs:Credentials::new]` rejects empty/untrimmed/control-containing credentials
  and colons in the key. `[http.rs:Http::build]` disables redirects, proxies and
  automatic retries. `[http.rs:Http::request]` performs one send using Basic auth
  and caps successful response bodies at 8 MiB. Diagnostics never contain provider
  bodies, credentials or request URLs.
- `[mod.rs:Trading212Reader::spendable_cash]` checks actual broker account and primary
  currency and uses only nonnegative `availableToTrade`. Snapshots and every mutation
  force this summary check. Other information reads invoke `ensure_binding`, which
  caches successful identity verification for at most 60 seconds.
- `[dto.rs:Number]` parses/emits decimal JSON tokens without binary floating point;
  unsupported precision/range fails. `[dto.rs:PositionDto::normalize]` uses positive
  total holdings. `[dto.rs:Order::normalize]` requires quantity strategy, recognized
  order types, consistent ticker fields and valid fill bounds. Unknown/transitional
  statuses stay `Unknown`; sell quantities normalize to positive magnitudes.
- `[mod.rs:Trading212Reader::read_snapshot]` stamps collection start, rejects clock
  reversal/collection over 60 seconds, and validates the complete domain snapshot.
  Sequential cash/position/pending-order reads are not atomic. History is separate.
- `[mod.rs:Trading212Reader::pending_order]` accepts canonical positive numeric IDs.
  A 404 means absent from pending orders, without proving nonexecution or cancellation.
  `[mod.rs:Trading212Reader::historical_orders]` rejects conflicting duplicate totals,
  wrong tickers, excessive page sizes and invalid queries. `[mod.rs:next_cursor]`
  accepts only the expected relative history path with unique recognized parameters,
  matching filter/limit and a nonnegative cursor different from the current cursor.
- `[mod.rs:Trading212Executor::submit_order]` validates positive quantities/prices,
  configured/actual account identity and cached stock metadata currency. It maps
  market/limit/stop/stop-limit bodies and validates the returned type, instrument,
  side, quantity and relevant order parameters. Sell wire quantities are negative.
  No client intent ID is transmitted or inferred.
- `[http.rs:Http::request]` and `Http::json` mark uncertain mutation outcomes explicitly.
  Transport/body failures, redirects, unexpected HTTP statuses and malformed or
  mismatched submission responses require reconciliation. Explicit rejection statuses
  are retained; requests are never automatically repeated. `[mod.rs:Trading212Executor::cancel_order]`
  returns request acceptance only, not terminal cancellation or a no-fill guarantee.
- `[http.rs:Bucket::interval]` paces each instance: cash/pending list 5s, positions/order
  detail 1s, instruments 50s, history 3s, cancellation 1.2s, each order type 2s.
  `retry_delay` can extend cooldowns from provider headers. Blocked calls return
  `RateLimited` and a delay without sleeping. Quotas across instances/processes need
  caller coordination. `[mod.rs:Trading212Reader::instruments]` caches metadata for
  ten minutes; it does not verify precision, fractional or protective-order capabilities.
- The executor must separately enforce durable intents, current fencing, readiness,
  policy, available sell quantity and protection. No persistence or live worker is
  implemented here. Acknowledgments do not establish filled/protected positions.

## Side Effects

Information methods issue authenticated GETs. Executor methods additionally issue
POST/DELETE and can create/cancel real orders with live credentials. Calls update
in-memory cooldowns, identity and metadata caches. Snapshot creation reads the clock
and advances a process-global atomic sequence. There are no database or file writes.

## Verification

Run from the repository root:

```sh
cargo test -p trady-adapters trading212 --locked
cargo test -p trady-cli --test trading212_cli --locked
```

All broker interaction tests use an ephemeral loopback HTTP fixture. They do not
require or use broker credentials/accounts.

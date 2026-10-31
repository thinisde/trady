# Trading 212 broker adapter Blueprint

## Overview

Typed synchronous access to the Trading 212 v0 API, with separate information
and executor clients. The adapter does not persist execution state or establish
policy, readiness, fencing or deployment authority. See the
[implementation guide](../../../../docs/trading212.md) for setup and remaining work.

## Interface

- `[mod.rs:Environment]`: `Demo`/`Live`, fixed `base_url()`, paper/live `mode()` and
  `credential_names()` for the matching central-store pair.
- `[mod.rs:Trading212Config]`: versioned non-secret account/environment/currency
  binding with `validate()`. `[mod.rs:Credentials]`: validated key/secret pair with
  redacted `Debug`; no serialization or public credential fields.
  `Credentials::from_store` resolves the API key and secret for the explicitly
  selected environment from the supplied store, with no shared/other-mode fallback.
  Each lookup rechecks its environment override; the pair is not an atomic
  environment snapshot.
- `[mod.rs:BrokerInstrument]`, `HistoryQuery`, `HistoryPage`: metadata and bounded
  history requests/results with normalized order totals and a typed continuation cursor.
- `[mod.rs:Trading212Reader]`: `new`, `with_clock`, `config`, `spendable_cash`,
  `pending_orders`, `pending_order`, `instruments`, `historical_orders`, `read_snapshot`.
  Implements application `BrokerInformationPort` and observation-only `BrokerPort`.
  Capabilities remain unknown; `find_order(intent_id)` explicitly fails.
- `[mod.rs:Trading212Executor]`: `new`, `information`; implements
  `BrokerExecutionPort::submit_order` and `cancel_order` from the
  [application contract](../../../trady-application/src/broker.rs).
- `[autonomous.rs:AutonomousTrading212]`: reexported by `mod.rs`; `new(config,
  credentials)` constructs the paced production wrapper. It implements
  `BrokerExecutionPort::{submit_order, cancel_order}` and the application
  `ExecutionBroker::{observe(account, tracked), submit_order_before(request,
  expires_at)}` port. It exposes no information
  accessor or injected-clock constructor.
- `[autonomous.rs:Trading212Executor::observe]`: `Trading212Executor<C>` also
  implements `ExecutionBroker::{observe, submit_order_before}`. The private `observe_execution` collects an
  `ExecutionObservation` containing `snapshot`, `position_costs`,
  `available_quantities` and `total_equity`.

## Local Boundaries

| File | Responsibility |
|---|---|
| [mod.rs](./mod.rs) | Public types, account identity, metadata cache, information operations and order transport |
| [dto.rs](./dto.rs) | Private wire DTOs, exact decimal numbers, order normalization and request serialization |
| [http.rs](./http.rs) | Private bounded HTTP client and per-endpoint cooldowns |
| [autonomous.rs](./autonomous.rs) | Paced execution observations, exact-ID history recovery and the production wrapper |

## Invariants

- `[mod.rs:Trading212Config::validate]` requires version 1, a positive numeric broker
  account, a 1–30 second timeout, and `paper_`/`live_` local ID matching environment.
  Deserialization rejects unknown fields. Production origins are fixed HTTPS
  URLs; arbitrary URLs are not configuration fields.
- `[mod.rs:Credentials::new]` rejects empty/untrimmed/control-containing credentials
  and colons in the key. `[http.rs:Http::build]` disables redirects, proxies and
  automatic retries. `[http.rs:Http::request]` performs one send using Basic auth
  and caps successful response bodies at 8 MiB. Diagnostics never contain provider
  bodies, credentials or request URLs.
- `[mod.rs:Trading212Reader::spendable_cash]` checks actual broker account and primary
  currency and uses only nonnegative `availableToTrade`. Low-level snapshots,
  submission and cancellation force this summary check. Other information reads
  invoke `ensure_binding`, which caches successful identity verification for at
  most 60 seconds.
- `[autonomous.rs:AutonomousTrading212::observe]` makes a successful observation's
  account binding available for one submission; construction and failed
  observations leave reuse consumed. `submit_order` first paces placement, then
  reuses only an unconsumed binding at most five seconds old, otherwise fetching
  a fresh summary. It consumes reuse before `submit_bound`, including helper
  failures; cancellation also consumes it. The age check precedes metadata
  retrieval and is not repeated immediately before POST. Summary fallback does
  not require a full observation or enforce application recovery/readiness gates.
- `[mod.rs:Trading212Executor::submit_bound_before]` checks an optional expiry
  after metadata validation and before the HTTP call. At or after expiry it
  returns `InvalidRequest` without sending an order. Both `ExecutionBroker`
  implementations supply this expiry through `submit_order_before`; ordinary
  `BrokerExecutionPort::submit_order` supplies none. This is a pre-send check,
  not a cancellation deadline for an in-flight request.
- `[dto.rs:Number]` parses/emits decimal JSON tokens without binary floating point;
  unsupported precision/range fails. `[dto.rs:PositionDto::normalize]` uses positive
  total holdings. `[dto.rs:Order::normalize]` requires quantity strategy, recognized
  order types, consistent ticker fields and valid fill bounds. Unknown/transitional
  statuses stay `Unknown`; sell quantities normalize to positive magnitudes.
- `[mod.rs:Trading212Reader::read_snapshot]` stamps collection start, rejects clock
  reversal/collection over 60 seconds, and validates the complete domain snapshot.
  Sequential cash/position/pending-order reads are not atomic. History is separate.
- `[autonomous.rs:Trading212Executor::observe_execution]` additionally requires
  nonnegative `totalValue`, per-position available quantity in `[0, quantity]`,
  unique tickers and positive `walletImpact.totalCost` in the account currency.
  Missing accounting fields fail; snapshot position/cost-map counts must agree.
  It stamps collection start after summary pacing and rejects reversed time or
  collection over 60 seconds at a final check. This is not a hard operation
  deadline; the reads remain sequential.
- `[autonomous.rs:Trading212Executor::observe_execution]` recovers only exact
  broker IDs from previously observed nonterminal tracked orders missing from
  pending orders. It scans at most ten unfiltered history pages of 50 records and
  rejects repeated cursors. Missing IDs remain unresolved; it cannot discover a
  lost acknowledgement's unknown broker ID from ticker, quantity or time.
  Observation can still return successfully with these IDs absent; the caller
  must handle that recovery state.
- `[mod.rs:Trading212Reader::pending_orders]` rejects duplicate broker IDs;
  `instruments` rejects duplicate tickers.
- `[mod.rs:Trading212Reader::pending_order]` accepts canonical positive numeric IDs.
  A 404 means absent from pending orders, without proving nonexecution or cancellation.
  `[mod.rs:Trading212Reader::historical_orders]` rejects conflicting duplicate totals,
  wrong tickers, excessive page sizes and invalid queries. `[mod.rs:next_cursor]`
  accepts only the expected relative history path with unique recognized parameters,
  matching filter/limit and a nonnegative cursor different from the current cursor.
  `[dto.rs:History]` requires `nextPagePath`; only explicit `null` ends pagination.
- `[mod.rs:Trading212Executor::submit_order]` and private `submit_bound` gate
  submission on positive quantity/applicable prices, configured/actual account
  identity and metadata with matching instrument currency and kind exactly `STOCK`.
  They map market/limit/stop/stop-limit bodies and validate the returned type,
  instrument, side, quantity and relevant order parameters. Sell wire quantities are negative.
  `[dto.rs:order_body]` emits `timeValidity`; response `[dto.rs:Order]` reads
  `timeInForce`.
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
- `[http.rs:Http::pace]` is the explicit blocking path used by autonomous operations:
  it sleeps for cooldowns up to 30 seconds and returns `RateLimited` for longer
  waits. It does not replay mutations. `[autonomous.rs:Trading212Executor::observe]`
  converts broker failures to domain errors, losing their structured retry/outcome fields.
- `[mod.rs:Trading212Executor]` leaves durable intents, current fencing, readiness,
  policy, available sell quantity and protection to its caller. No persistence or live worker is
  implemented here. Acknowledgments do not establish filled/protected positions.

## Side Effects

`[http.rs:Http::request]` issues authenticated GETs for information operations;
`[mod.rs:Trading212Executor::submit_bound_before]` and `cancel_order` additionally
issue guarded POST/DELETE. `[mod.rs:Environment::base_url]` selects the live
origin only for `Live`. Calls update instance-local cooldowns, identity and
metadata caches. `[mod.rs:Trading212Reader::read_snapshot]` and
`[autonomous.rs:uuid_free_id]` read time and advance a process-global sequence.
`[http.rs:Http::pace]` may sleep during autonomous observation/mutation.
`[mod.rs:Trading212Reader::snapshot]` and `[autonomous.rs:Trading212Executor::observe]`
stringify broker failures into domain errors, losing structured retry/outcome fields.
There are no database or file writes.

## Verification

Run from the repository root:

```sh
cargo test -p trady-adapters trading212 --locked
cargo test -p trady-cli --test trading212_cli --locked
```

All broker interaction tests use an ephemeral loopback HTTP fixture. They do not
require or use broker credentials/accounts.
Workspace verification is `cargo test --workspace --locked`; offline tests do not
establish live broker acceptance or deployment readiness.

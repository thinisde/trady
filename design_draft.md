# Trading 212 Same-Day Sequential Protection

*   **Author:** thinisde
*   **Status:** Proposed
*   **Date:** 2026-11-03

--------------------------------------------------------------------------------

## Overview

Trady needs a supervised and eventually autonomous demo execution mode for one
same-day round-trip trade: one buy, one protected holding, and one sell. Trading
212 can execute the stock order and accept a standalone stop order, but its API
does not provide atomic attached bracket or OCO protection. The design therefore
uses a durable sequential workflow that records the buy before dispatch, confirms
the actual fill, places protection for only the filled quantity, and reconciles
broker state before any target or liquidation sell.

The mode remains fail-closed. It does not claim that sequential protection is
atomic or that a same-day deadline guarantees liquidation. It requires a fresh,
venue-appropriate bid/ask and FX/cost observation before entry, applies the
0.15% FX fee on each cross-currency conversion, and retains recovery state when
the broker response or cancellation result is uncertain.

## The Problem

The current Trading 212 capability record proves cash access, stock metadata,
USD-listed market execution through EUR conversion, and standalone stop-order
acceptance. It does not prove atomic attached protection or provide a broker
executable quote feed. Treating those gaps as verified would allow a target sell
to race an active stop, oversell a partially filled position, or size a trade
from a price that is not available at the broker.

The existing application already has durable order roles, account observations,
partial-fill accounting, cancellation requests, and recovery states. The design
must bind those pieces into an explicit same-day lifecycle with these properties:

* a buy uses a marketable limit with a maximum price and never retries an
  ambiguous submission;
* a filled quantity is protected before normal holding decisions continue;
* a target or deadline exit cannot submit while a stop remains non-terminal;
* a failed or uncertain protection operation latches recovery and prevents a
  new entry;
* expected profit is calculated after FX, spread, fees, slippage, and the
  session deadline, rather than from the stock return alone.

## Goals

* Define a durable state machine for one daily buy-to-sell cycle on a single
  account and instrument.
* Use fresh bid/ask, session, liquidity, spread, FX, and cost evidence before
  creating an entry intent.
* Submit one marketable-limit buy, reconcile exact filled quantity and price,
  and place a native stop for only that quantity.
* Handle partial fills without leaving excess protection or creating an
  oversell opportunity.
* Cancel and reconcile protection before a profit-target or software exit;
  preserve protection when an executable exit is unavailable.
* Enter recovery immediately when stop placement, cancellation, fill status, or
  connectivity produces an uncertain outcome.
* Enforce one buy and one sell attempt per session while allowing risk-reducing
  protection and emergency liquidation to proceed.
* Record broker order IDs, fills, FX, fees, cancellation observations, recovery
  reasons, and the final flat reconciliation in the durable journal.
* Keep the capability and release gates honest: sequential protection is a
  distinct supported mode, not evidence of atomic bracket/OCO support.

## Non-Goals

* Claim atomic attached, bracket, OCO, or reduce-only protection from Trading
  212's standalone stop endpoints.
* Guarantee a fill at the target, stop price, or same-day liquidation deadline.
* Add retries for non-idempotent order placement or cancellation requests.
* Infer an executable quote from a position mark, fill price, historical bar, or
  an unrelated market venue.
* Permit overnight holding when protection or final reconciliation is missing.
* Support multiple simultaneous positions, averaging down, pyramiding, shorting,
  leverage, or more than one strategy entry per session.
* Replace the independent capability review, owner release, formula, research,
  input-service, or shadow/paper acceptance gates.

## Technical Plan (Architecture)

The design keeps broker transport and execution policy separate. Trading 212
continues to expose ordinary market, limit, stop, stop-limit, cancellation, and
observation operations. The application owns the sequential protection state
machine and never represents it as atomic protection.

```text
fresh quote/session/costs
          |
          v
durable BUY intent -> one marketable-limit submission
          |
          v
exact broker observation and fill reconciliation
          |
          +-- no fill ------------> cancel/reconcile entry
          |
          +-- partial fill --------> cancel remainder, protect filled quantity
          |
          +-- full fill -----------> place native stop, reconcile terminal state
                                           |
                                +----------+----------+
                                |                     |
                         protected holding      stop failure/unknown
                                |                     |
                    target/deadline/owner exit       recovery latch
                                |
                  cancel stop -> observe terminal
                                |
                       sell available quantity
                                |
                         exact flat reconciliation
```

### Capability and configuration boundary

Add a protection-mode distinction to the reviewed capability/configuration
contract. `SEQUENTIAL_NATIVE_STOP` means the broker accepted a standalone stop
for the tested account and instrument; `ATOMIC_ATTACHED` remains unsupported for
Trading 212. Entry readiness may use the sequential mode only when the runtime
also enables the bounded same-day policy and all quote, cost, account, and
reconciliation evidence is current.

Keep the existing `Capability::usable` semantics for expiry and evidence IDs.
Expose the unsupported list separately from the generic unverified list so an
operator can distinguish missing evidence from a broker limitation. The
capability artifact must bind the exact account, instrument, stop order type,
quote source, fee model, observation window, and independent review hash.

### Quote, session, and cost gate

Extend the quote input contract used by `TradingInputs::quote` so an accepted
observation carries the source, venue or execution-compatibility scope,
instrument currency, bid, ask, observed time, received time, tradability, and
liquidity/spread measurements. The executor rejects a candidate when the feed is
stale, crossed, outside the approved instrument scope, or not proven compatible
with the broker execution path.

Before persisting the buy intent, calculate account-currency economics using the
buy conversion, sell conversion, 0.15% FX fee on each cross-currency conversion,
spread, broker-reported or configured charges, conservative slippage, and the
same-day liquidation deadline. The net target must exceed the configured hurdle
after these costs. A fill price or position mark cannot substitute for a fresh
pre-trade bid/ask.

### Durable sequential order state

Reuse `OrderRole::{Entry, Protection, Exit}` and the fenced `TradingState`, while
adding explicit protection lifecycle data to the active plan or order journal:

* required protection quantity and currently protected quantity;
* protection mode (`SEQUENTIAL_NATIVE_STOP` or `ATOMIC_ATTACHED`);
* cancellation requested, cancellation observed terminal, and cancellation
  outcome;
* recovery reason and the last broker observation used to make the decision.

Every mutation keeps the existing write-before-send sequence. The journal first
records the request and marks it attempted, then sends exactly once, then stores
the broker acknowledgement or sanitized failure. Broker IDs, fill quantities,
fees, FX, stop parameters, and cancellation observations remain immutable audit
facts. Existing daily limits count one strategy buy and one normal strategy sell;
protection and risk-reducing emergency exits do not consume those limits.

### Entry and protection flow

1. Validate flatness, session window, daily slot, current quote/cost packet,
   candidate limits, and account currency before creating the entry intent.
2. Submit one marketable limit buy with the maximum acceptable price. Do not
   replay it after a timeout or ambiguous response.
3. On the next broker observation, reconcile exact filled quantity and price.
   Cancel any unfilled entry remainder before creating protection. An unknown
   cancellation or fill outcome enters recovery.
4. Submit a native stop for the reconciled filled quantity. For partial fills,
   protect only the acquired quantity and add protection only when a later fill
   is observed. Reject protection that exceeds the actual available quantity.
5. Require a broker observation that identifies the stop as open or otherwise
   accepted before marking the plan `OPEN_PROTECTED`. A rejected stop latches an
   exit and attempts a risk-reducing flatten when a valid executable quote is
   available; an unknown stop outcome enters `RECOVERY_REQUIRED`.

### Target, stop, and deadline exits

When a target, stop, deadline, owner close, or session-loss latch occurs, persist
the exit decision before mutation. Cancel any remaining entry first. For a
software target or liquidation sell, request stop cancellation once, then wait
for a fresh observation showing a terminal cancellation or a terminal broker
fill. A cancellation acknowledgement alone never authorizes the sell. If the
stop filled, recompute the remaining position and protect or close only that
quantity; if it is missing from pending orders and history, retain recovery.

After protection is terminal and the available quantity is reconciled, submit one
target limit or market liquidation order. If the quote or session is unavailable,
retain native protection and the durable exit latch rather than sending an
unpriced software sell. Reconcile the exact broker ID, fills, FX, fees, and zero
position before moving the plan to `FLAT`.

### Failure recovery

Recovery is a first-class state, not an error string. It is entered for unknown
submission/cancellation outcomes, missing broker IDs, conflicting fills, stop
placement failure, quantity disagreement, untracked broker orders, or a lost
quote/session while an exit is required. Recovery blocks new entries, keeps any
known native protection active, retries only read observations, and permits an
owner-reviewed exact-ID recovery operation. Automatic flattening is attempted
only when the broker observation and available quantity make it safe; otherwise
the system alerts and remains unresolved.

### Component mapping

* `crates/trady-application/src/autonomous/mod.rs`: protection-mode state,
  entry/partial-fill/protection/exit transitions, same-day limits, and recovery
  decisions.
* `crates/trady-application/src/broker.rs`: document the sequential protection
  boundary and keep atomic protected-order receipts unavailable for Trading 212.
* `crates/trady-adapters/src/trading212/mod.rs`: preserve one-send mutation
  behavior, exact stop/cancel acknowledgements, broker account binding, and
  fill/FX/fee normalization.
* `crates/trady-adapters/src/trading212/autonomous.rs`: expose paced observation,
  exact-ID history recovery, and cancellation re-observation for the sequential
  workflow.
* `crates/trady-storage/src/autonomous.rs` and the fenced execution journal:
  persist the expanded protection lifecycle and immutable broker facts without
  widening analyst or gateway privileges.
* `config/autonomous.iex30.demo.json` and the capability evidence artifact:
  declare sequential protection, quote-source compatibility, fee assumptions,
  validity windows, and explicit unsupported atomic protection.
* `docs/trading212.md`, `docs/autonomous.md`, and `docs/cold-start.md`: document
  the operator-visible workflow, recovery meanings, cost gate, and release
  limitations.

## Alternatives Considered

### Alternative A: Move to a broker with atomic bracket or OCO protection

*   **Pros**: A broker-native bracket could bind the stop and entry or target
    together, reduce the cancellation race, and provide stronger protection
    semantics for unattended execution.
*   **Cons**: It requires a new account/profile, credential and permission
    review, instrument crosswalk, quote/venue contract, adapter implementation,
    and migration of the existing demo evidence. It also does not remove the
    need for fill reconciliation, idempotency controls, or same-day liquidation
    handling.
*   **Reason for Rejection**: Rejected by author for the initial scope because
    the existing Trading 212 demo account already supports the intended test
    order path and the same-day mode can be bounded around sequential native
    stops. Keep this as the preferred escalation path if sequential protection
    fails its acceptance tests or the risk policy later requires atomic loss
    protection.

### Alternative B: Keep Trading 212 as manual management-only execution

*   **Pros**: It avoids unattended protection races and quote-authority claims;
    an operator can inspect the account and place or cancel orders directly.
    The broker adapter can remain read-heavy and the runtime can retain all
    entries disabled.
*   **Cons**: It does not deliver the requested one-trade-per-day automation,
    does not provide deterministic recovery after an operator or connection
    failure, and cannot validate the executor's same-day state transitions.
*   **Reason for Rejection**: Rejected by author as the primary mode because the
    goal is supervised demo automation that can later be evaluated for bounded
    autonomy. Retain it as the fallback when quote, protection, or recovery
    evidence is stale or unavailable.

### Alternative C: Add only an external executable quote feed

*   **Pros**: It addresses the missing pre-trade bid/ask, spread, session, and
    liquidity observations without changing the Trading 212 order transport.
    It can also support fee-aware target calculations and same-day deadline
    checks.
*   **Cons**: It does not establish that the quote is executable on Trading
    212's actual venue, does not create native protection, and cannot resolve
    order cancellation races or ambiguous fills. A feed such as an IEX quote
    source may differ from the broker's OTC execution path.
*   **Reason for Rejection**: Rejected by author as a complete execution design;
    retained as a required supporting component of the sequential mode. The
    feed must be independently scoped to the broker instrument and execution
    path before it can authorize an entry.

## Non-Functional Requirements

* **Safety**: Every mutation is persisted before send, sent once, and followed
  by an exact-ID observation. Ambiguous outcomes remain unresolved. No target
  sell may race a non-terminal stop.
* **Accounting**: Use checked decimal arithmetic. Store instrument-currency
  fills, account-currency wallet impact, FX rates, conversion fees, spread, and
  slippage assumptions separately so realized results can be recomputed.
* **Recovery**: Restart must rebuild the protection and exit lifecycle from the
  fenced journal and broker observations. A missing or conflicting observation
  must produce `RECOVERY_REQUIRED`, not a flat or cancelled guess.
* **Latency**: Protection placement follows the first fresh fill observation and
  is paced within Trading 212's limits. The design does not promise a maximum
  stop-placement latency or same-day liquidation fill.
* **Security**: Broker secrets remain in the central credential store. Analyst,
  model, and gateway processes receive no mutation port. Audit records contain
  sanitized broker facts only.
* **Rollout**: The mode starts disabled and entry-paused. Supervised demo runs
  require a bounded daily order limit, verified quote/cost evidence, and an
  independently reviewed capability artifact. Rollback means pausing entries;
  existing native protection and recovery continue.

## Detailed Implementation Plan

### Target Files to Create/Modify

#### 1. `crates/trady-domain/src/lib.rs` (Modify)

* Add a serialized `ProtectionMode` value distinguishing
  `SEQUENTIAL_NATIVE_STOP` from `ATOMIC_ATTACHED`.
* Extend capability/configuration records with a defaulted protection mode so
  existing fixtures remain readable while new reviewed evidence can state the
  exact protection semantics.
* Keep `Capability::usable` strict about support, evidence ID, and expiry.

#### 2. `crates/trady-application/src/broker.rs` (Modify)

* Document that `ProtectedOrderRequest` and `ProtectedOrderReceipt` represent
  atomic protection and must remain unsupported by Trading 212.
* Add sanitized request/observation types for sequential protection only if the
  existing `BrokerOrderRequest`, `BrokerOrder`, and `CancellationReceipt` do
  not carry enough fields for the application journal.
* Preserve `outcome_unknown` and broker order IDs on failed submissions and
  cancellations; never add automatic mutation retries.

#### 3. `crates/trady-application/src/autonomous/mod.rs` (Modify)

* Add explicit protection lifecycle state to `ActivePlan` or `TradingState`,
  including required quantity, protected quantity, mode, cancellation outcome,
  and recovery reason.
* Refactor entry management into the ordered transitions: persist buy intent,
  submit once, observe fills, cancel the unfilled remainder, place protection,
  observe accepted protection, and only then mark `OPEN_PROTECTED`.
* Handle partial fills by protecting only acquired quantity and recalculating
  the uncovered amount after every observation.
* Require terminal stop cancellation observation before a target or software
  liquidation order. A cancellation acknowledgement alone must return a
  waiting action.
* Latch recovery for unknown stop placement/cancellation, conflicting fills,
  missing IDs, untracked orders, unavailable quantities, and broker holdings
  that disagree with the journal.
* Enforce one strategy buy and one ordinary strategy sell per account/session;
  exclude protection and emergency liquidation from those counts.
* Keep native protection active when quotes or the session are unavailable.

#### 4. `crates/trady-application/src/autonomous/readiness.rs` (Modify)

* Require an approved quote source, instrument/venue mapping, fresh session,
  liquidity and spread evidence, and a complete account-currency cost packet.
* Reject candidates whose expected net return does not clear FX conversion on
  both legs, spread, fees, slippage, and the configured same-day hurdle.
* Reject an atomic protection claim when the selected mode is sequential.

#### 5. `crates/trady-adapters/src/trading212/mod.rs` (Modify)

* Keep marketable-limit, market, stop, and stop-limit requests as one-send
  operations with exact signed quantities.
* Ensure normalized fills expose account-currency wallet impact, FX rate, tax or
  conversion-fee rows, and exact filled quantity for the journal.
* Keep account binding and instrument-currency validation before every mutation.
* Preserve the distinction between cancellation acceptance and terminal
  cancellation; expose exact pending/history observations for the application.

#### 6. `crates/trady-adapters/src/trading212/autonomous.rs` (Modify)

* Pace the sequential observation and mutation sequence within Trading 212
  limits.
* Recover tracked stop and exit IDs from pending orders and bounded exact-ID
  history scans after restart.
* Return unknown outcomes without replaying a mutation, including a lost stop
  placement or cancellation response.

#### 7. `crates/trady-adapters/src/trading212/dto.rs` (Modify)

* Preserve stop/stop-limit parameters, signed quantities, `timeInForce`, fill
  values, wallet-impact FX, tax names, and conversion fees during normalization.
* Add fixture-safe validation for the stop order's quantity and price matching
  the sequential protection request.

#### 8. `crates/trady-adapters/src/autonomous/market.rs` and the active quote adapter (Modify)

* Attach quote source, venue compatibility, observed/received timestamps,
  tradability, spread, and liquidity evidence to the quote result.
* Reject stale, crossed, future-dated, or venue-incompatible observations.
* Carry the explicit 0.15% cross-currency conversion assumption into the
  account-currency cost calculation and keep provider-observed fees separate.

#### 9. `crates/trady-storage/src/autonomous.rs` (Modify)

* Encode and decode the expanded protection lifecycle and broker observations
  through the fenced journal.
* Add journal methods for immutable protection/cancellation/recovery events if
  JSON state alone cannot preserve event history without overwriting evidence.
* Keep all writes behind the executor role and existing account fence.

#### 10. `migrations/0015_sequential_protection.sql` (Create)

* Extend the executor journal validation for protection mode, protected and
  uncovered quantities, cancellation observations, recovery reasons, and
  broker fill/fee/FX event records.
* Add security-definer functions or append-only event storage where the current
  JSON document cannot prove the order of mutation, acknowledgement, and
  reconciliation events.
* Preserve daily buy/sell trigger limits and reject direct writes from analyst,
  gateway, or public roles.

#### 11. `crates/trady-application/tests/autonomous.rs` (Modify)

* Add acceptance cases for marketable-limit entry, no-fill cancellation, partial
  fill protection, full fill protection, stop rejection, unknown stop response,
  cancellation acknowledgement without terminal observation, late stop fills,
  target exit after terminal stop cancellation, and same-day deadline exit.
* Assert exact quantity, no oversell, one ordinary buy/one ordinary sell, durable
  recovery state, and restart behavior.
* Add fee-aware target tests showing that a gross 0.25% move does not pass a
  roughly 0.30% two-leg FX fee burden.

#### 12. `crates/trady-adapters/src/trading212/tests.rs` (Modify)

* Extend loopback fixtures for accepted and rejected stop/stop-limit orders,
  cancellation responses, terminal history fills, wallet-impact FX, and
  conversion-fee rows.
* Verify malformed or mismatched stop acknowledgements fail closed and are never
  retried.

#### 13. `crates/trady-adapters/src/trading212/autonomous_tests.rs` (Modify)

* Test exact-ID recovery for missing pending stops and exits, ambiguous
  cancellation, partial fills, repeated history cursors, and account quantity
  disagreement.

#### 14. `crates/trady-cli/tests/autonomous_cli.rs` and `crates/trady-cli/tests/trading212_cli.rs` (Modify)

* Assert configuration output distinguishes unsupported from unverified
  capabilities and keeps entry authorization false.
* Add offline checks for sequential protection mode, fee-aware quote gates,
  daily limits, and the absence of any unrestricted mutation command.

#### 15. `config/autonomous.iex30.demo.json` and capability evidence artifacts (Modify)

* Add the sequential protection mode, quote-source compatibility scope, fee
  model, same-day deadline, and explicit validity windows.
* Keep atomic protection unsupported and retain entry pause until independent
  review, quote evidence, and shadow/paper gates pass.

#### 16. `docs/trading212.md`, `docs/autonomous.md`, `docs/cold-start.md`, and
`docs/v1/strategy.md` (Modify)

* Document the state machine, stop-cancellation rule, recovery behavior,
  partial-fill handling, FX fee treatment, quote-source limitation, and the
  difference between sequential and atomic protection.
* Add operator procedures for recovery, exact broker-ID reconciliation, and
  pausing entries without cancelling existing protection.

### Verification Plan

* **Compilation**: `cargo check --workspace --locked`.
* **Formatting and linting**: `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings`.
* **Unit and acceptance tests**: `cargo test --workspace --locked`, with focused
  runs for `trady-application` autonomous tests and Trading 212 adapter tests.
* **Persistence tests**: run the PostgreSQL migration and journal tests with
  `python3 scripts/test-postgres.py`; verify account fencing, append-only event
  order, daily limits, and restart recovery.
* **Provider fixtures**: replay accepted/rejected stop, cancellation, fill,
  FX, fee, and history responses through loopback fixtures. No live account is
  used by automated tests.
* **Demo acceptance**: on the demo account only, run a bounded one-share
  marketable-limit round trip, verify a native stop for the actual fill,
  exercise target-stop cancellation reconciliation, and confirm zero position
  and zero pending orders afterward.
* **Evidence checks**: verify capability artifact hashes, independent review,
  quote-source compatibility, cost assumptions, and expiry before enabling any
  supervised or autonomous entry gate.
* **Integrity**: run `git diff --check`, JSON validation, and the deployment
  integrity verifier after every configuration or authority change.

*Drafted with EGM*

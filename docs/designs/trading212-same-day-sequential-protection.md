# Trading 212 Same-Day Sequential Protection

*   **Author:** thinisde
*   **Status:** Proposed
*   **Date:** 2026-11-03

--------------------------------------------------------------------------------

## Overview

This design defines a bounded shadow and paper proof for one same-day round trip
through the existing Trady execution state machine. The proof models one buy,
one filled holding, sequential native stop protection, and one target or
liquidation sell. It does not authorize broker mutation or live release.

Trading 212 accepts standalone stop orders but does not provide atomic attached
bracket or OCO protection. The design therefore treats sequential protection as
an explicitly weaker mode: a buy is observed, the acquired quantity is
protected, and a target sell is allowed only after the stop's exact broker ID is
terminally reconciled. Unknown outcomes remain recovery. Atomic protection is
not a selectable Trading 212 runtime capability.

## The Problem

Trading 212 can report cash, stock instruments, market and limit fills, native
stops, cancellations, FX, and fees. Its public API does not provide an
executable bid/ask feed or atomic attached protection. A same-day intention does
not remove the race between a fill and stop placement, a stop cancellation and
a target sell, or a cancellation request and a late fill.

The existing application already contains `PositionState`, `OrderRole`,
`TrackedOrder`, fenced `TradingState`, write-before-send dispatch, exact broker
ID recovery, available-quantity checks, and daily limits. The missing design is
the small set of invariants that connect these existing contracts for a bounded
shadow/paper proof without inventing a second journal or a new broker API.

## Goals

* Prove the sequential lifecycle in shadow and paper fixtures without broker
  mutation.
* Keep one strategy buy and one ordinary strategy sell per exchange session;
  risk-reducing protection and emergency handling are tracked separately.
* Use existing quote, exchange-calendar, account, cost, and capability gates.
* Reconcile exact pending and history observations before every replacement
  mutation in the simulated workflow.
* Handle no fills, partial fills, late fills, stop partial fills, ambiguous
  cancellations, and restart recovery without overselling.
* Calculate expected account-currency return after both cross-currency FX fees,
  spread, configured costs, and conservative slippage.
* Produce measurable evidence for a later supervised-demo release decision.

## Non-Goals

* No live or demo broker mutation is enabled by this design.
* No atomic attached, bracket, OCO, or reduce-only protection is claimed.
* No new broker, quote, recovery, or migration subsystem is introduced.
* No overnight holding is authorized by this design.
* No guarantee is made that a target, stop, or same-day liquidation fills.
* No retry is allowed for a non-idempotent broker mutation or an ambiguous
  outcome.
* No change is made to the existing owner, formula, research, input-service,
  shadow, paper, or release gates.

## Open Questions and Release Prerequisites

These are prerequisites for a later supervised-demo release and remain outside
the shadow/paper proof:

* A quote compatibility artifact must identify the approved feed, instrument
  mapping, execution-venue scope, source owner, maximum quote age, spread and
  liquidity limits, divergence test, hash, reviewer, and expiry. Trading 212's
  OTC fills cannot be declared equivalent to IEX merely because symbols match.
* A versioned execution-cost artifact must state the FX rate policy, 0.15% fee
  per cross-currency conversion, spread/slippage bounds, precision, rounding,
  and conflict behavior when broker-reported costs differ.
* An independent capability review must approve sequential native-stop evidence;
  it must not promote atomic protection.
* A later release decision must define whether an unprotected live holding can
  be carried into the next session. This design does not permit that state.

## Critical User Journeys

### CUJ 1: Paper entry and partial fill protection

* **Given**: A flat paper account, a valid exchange session, a quote no older
  than 10 seconds, spread no greater than 20 basis points, round-trip modeled
  cost no greater than 50 basis points, and an approved cost artifact.
* **When**: The executor records one marketable-limit buy intent and the paper
  broker returns a partial fill followed by a late fill after cancellation was
  requested.
* **Then**: The entry remainder is not retried; exact pending and history rows
  are reconciled; the acquired quantity is recalculated; protection covers only
  the actual quantity; the state remains `RECOVERY_REQUIRED` until the late fill
  is resolved; no sell exceeds the reconciled remaining quantity.

### CUJ 2: Target exit with a live stop

* **Given**: A reconciled paper position with a working native stop and an
  approved target quote.
* **When**: The target condition latches an exit.
* **Then**: The entry remainder is cancelled first; the stop cancellation is
  requested once; no target sell is submitted until exact pending plus history
  observation shows terminal cancellation or a stop fill; the remaining
  quantity is recalculated; the target sell is submitted only for available
  quantity; the position reaches `FLAT` only after zero position and no
  unresolved order are observed.

### CUJ 3: Protection failure at the session boundary

* **Given**: A paper fill whose stop submission is rejected or has an unknown
  outcome.
* **When**: The session approaches its exchange-calendar close cutoff.
* **Then**: The state latches `RECOVERY_REQUIRED`, no new entry is allowed, the
  broker ID is reconciled by read-only observation, the owner-control recovery
  path is recorded, and the paper run fails unless the exact quantity and order
  outcome are resolved before close. No overnight path is created.

## Technical Plan (Architecture)

### Scope and existing boundaries

The proof reuses the current `AutonomousTrader`, `ExecutionBroker`,
`ExecutionJournal`, `TradingState`, `PositionState`, `TrackedOrder`,
`TradingInputs`, `Quote`, `EntryCosts`, exchange-calendar, owner-control, and
`recover-order` contracts. It does not add a second order journal, a new broker
mutation port, or a runtime-selectable atomic protection mode.

The first phase is shadow/paper only. The current runtime remains disabled and
entry-paused. A later supervised-demo design may use the evidence produced here
but must revalidate all release gates.

### Canonical states and transitions

The existing `PositionState` values are authoritative:

| State | Entry condition | Allowed next state |
|---|---|---|
| `FLAT` | No position and no unresolved order | `ENTRY_PENDING` |
| `ENTRY_PENDING` | Buy intent persisted or entry order unresolved | `OPEN_UNPROTECTED`, `FLAT`, `RECOVERY_REQUIRED` |
| `OPEN_UNPROTECTED` | Position exists with uncovered quantity | `OPEN_PROTECTED`, `EXIT_LATCHED`, `RECOVERY_REQUIRED` |
| `OPEN_PROTECTED` | Every reconciled share has a working native stop | `EXIT_LATCHED`, `RECOVERY_REQUIRED` |
| `EXIT_LATCHED` | Target, stop, deadline, loss, or owner exit is durable | `EXIT_PENDING`, `RECOVERY_REQUIRED` |
| `EXIT_PENDING` | Stop is terminal and exit order is unresolved | `CLOSED`, `RECOVERY_REQUIRED` |
| `CLOSED` | Zero position and no unresolved order | `FLAT` |
| `RECOVERY_REQUIRED` | Any unknown, conflicting, or unsafe observation | `RECOVERY_REQUIRED`, `FLAT` after exact owner/read-only resolution |

The journal remains authoritative for intent and attempt order. A broker
observation is authoritative for actual position quantity, filled quantity, and
broker order status. The executor uses the union of pending orders and bounded
exact-ID history; absence from pending alone never proves cancellation or
non-execution.

### Order status and quantity rules

* A protection order is working only when normalized status is `Open` or
  `PartiallyFilled` and its broker quantity is positive, its side is sell, its
  instrument and TIF match the request, and its remaining quantity is no more
  than the reconciled position.
* `Cancelled`, `Filled`, and `Rejected` are terminal. A missing pending row is
  terminal only when exact-ID history returns one of those terminal statuses.
  A 404 with no terminal history remains unresolved.
* `actual_position = observed_position.quantity` is the authority for acquired
  quantity. `uncovered = actual_position - sum(remaining working protection)`.
  Negative uncovered quantity is a journal error and enters recovery.
* A fill observed after an entry cancellation request is not discarded. The
  executor reconciles the exact entry ID and history, updates actual position,
  and creates or adjusts protection for the uncovered quantity. If the entry
  outcome cannot be settled, no replacement mutation is allowed.
* A partial stop fill reduces actual position. The remaining position is
  protected again only after exact pending/history reconciliation; no sell may
  exceed broker-reported available quantity.

### Entry and sequential protection

The paper proof uses the existing marketable-limit request fields, exchange
calendar, and account observation. It persists the entry before dispatch and
does not retry an ambiguous request. After each observation it cancels any
unfilled remainder, waits for exact terminal settlement, calculates `uncovered`,
and submits a native sell stop for that quantity. The stop's trigger side, TIF,
price tick, and quantity step come from the reviewed instrument rules; missing
rules reject the candidate.

Sequential protection is an application workflow, not an atomic broker feature.
The runtime configuration must not accept `ATOMIC_ATTACHED` for Trading 212.

### Exit and cancellation ordering

On target, deadline, loss, or owner latch, persist the exit reason first. Cancel
any entry remainder. Request stop cancellation once and observe until exact-ID
history or pending state proves terminal cancellation or a stop fill. If the
stop fills, recompute actual position and either finish flat or protect the
remaining quantity. If cancellation remains unknown, remain in
`RECOVERY_REQUIRED`; do not submit a target sell.

After terminal protection resolution, the target limit or market exit follows
the existing policy and instrument rounding. A partial target fill consumes the
ordinary sell attempt, updates actual position, and leaves the residual under
the same recovery/protection rules. Risk-reducing emergency actions are
recorded separately and cannot be used to create another strategy entry.

### Quote and cost gates

The existing quote contract supplies bid, ask, currency, timestamps, and
tradability. The proof accepts a quote only when observed within 10 seconds,
the account observation is within 60 seconds, the exchange session is active,
the spread is at most 20 basis points, and modeled round-trip cost is at most
50 basis points. These are the existing IEX30 policy bounds; a later release
must add the separate venue-compatibility artifact before they authorize a
broker entry.

Account-currency economics use checked decimal arithmetic:

```text
entry_cost = quantity * ask * buy_fx * (1 + fx_fee)
           + entry_fees + entry_slippage
exit_value  = quantity * bid * sell_fx * (1 - fx_fee)
           - exit_fees - exit_slippage
net_return  = (exit_value - entry_cost) / entry_cost
```

For cross-currency trades, `fx_fee = 0.0015` is a versioned cost-artifact input,
not an adapter constant. Provider-observed fees and FX are stored separately;
missing, conflicting, or higher-than-bound values reject entry. Currency and
quantity rounding follows existing `EntryCosts` and instrument rules.

### Recovery and close boundary

The proof uses existing `recover-order` and owner-control paths. Recovery binds
only an exact broker ID returned by a fresh observation; it cannot guess from
ticker, quantity, or time. Recovery actions are idempotent, fenced, audited, and
read-first. A recovery condition blocks new entries and leaves any known native
stop active. Since this design is shadow/paper only, an unresolved state fails
the run rather than defining a live overnight exception.

## Alternatives Considered

### Atomic-capable broker

An atomic bracket/OCO broker would reduce the stop-cancellation race, but it
requires a new account, instrument crosswalk, quote contract, permissions,
adapter, and capability review. It is deferred until sequential proof fails or
the release policy requires atomic loss protection.

### Manual management-only mode

Manual execution avoids unattended races but cannot validate the executor's
same-day lifecycle. It remains the fallback when quote, cost, protection, or
recovery evidence is unavailable.

### External quote feed only

An external feed can supply pre-trade bid/ask and cost inputs but cannot prove
Trading 212 OTC executability or solve protection/cancellation races. It is a
release prerequisite, not a complete execution architecture.

## Non-Functional Requirements

* **Safety**: no replacement mutation while an exact broker ID is unresolved;
  no target sell while protection is non-terminal; no oversell beyond observed
  available quantity.
* **Accounting**: checked decimals, versioned cost artifact, provider-observed
  FX/fees, and immutable order/fill/recovery facts.
* **Recovery**: restart reads the fenced journal and exact broker IDs before any
  mutation; unresolved states remain recovery.
* **Security**: only the executor role can mutate or write the journal; analyst
  and gateway processes receive no broker mutation port.
* **Rollout**: shadow/paper only in this design; live or demo mutation requires
  a later reviewed release artifact.

## Detailed Implementation Plan

### Phase 1: Shadow/paper contract proof

1. **Modify** `crates/trady-application/src/autonomous/mod.rs` only where
   current `manage`, `dispatch`, cancellation, and reconciliation behavior does
   not satisfy the canonical state and quantity rules above. Reuse existing
   `PositionState`, `OrderRole`, `TrackedOrder`, and journal CAS.
2. **Modify** `crates/trady-application/src/autonomous/readiness.rs` to bind the
   existing quote age, account age, spread, liquidity, session, and versioned
   cost-artifact gates. Do not add a venue-compatibility claim without its
   prerequisite artifact.
3. **Modify** `crates/trady-application/tests/autonomous.rs` with exact expected
   states for no-fill, partial-fill, late-fill, stop partial-fill, ambiguous
   cancellation, target cancellation, restart, and no-position recovery.
4. **Modify** `crates/trady-adapters/src/trading212/tests.rs` and
   `crates/trady-adapters/src/trading212/autonomous_tests.rs` only for existing
   DTO/status/fee/FX fixtures needed by those paper tests. Do not add a new
   mutation API.

### Phase 2: Evidence and operator boundary

5. **Modify** the existing capability evidence artifact under
   `docs/evidence/` to include the sequential-stop scope, versioned cost input,
   quote prerequisite status, reviewer hash, and expiry. Keep atomic protection
   unsupported and keep runtime entry paused.
6. **Modify** `docs/trading212.md`, `docs/autonomous.md`, `docs/cold-start.md`,
   and `docs/v1/strategy.md` to link the existing state, recovery, calendar,
   cost, and owner-control contracts and document the shadow/paper boundary.
7. **Modify** `config/autonomous.iex30.demo.json` only to preserve disabled and
   paused controls and to reference the reviewed artifacts. Do not promote
   unsupported quote or atomic protection fields.

### Deferred work

No migration, new event store, new atomic protection type, live/demo mutation,
or new recovery CLI is part of this design. Those require a separate design
after the Phase 1 proof and external quote/cost/capability prerequisites pass.

### Verification Plan

* `cargo fmt --all -- --check`
* `cargo check --workspace --locked`
* Focused autonomous application and Trading 212 adapter tests with exact state,
  quantity, status, and recovery assertions.
* `cargo test --workspace --locked` after focused proof passes.
* Existing PostgreSQL journal tests only; no new migration is run for Phase 1.
* Validate quote age (10s), account age (60s), spread (20bp), modeled cost
  (50bp), session cutoff, and versioned fee artifact in deterministic fixtures.
* Run `git diff --check`, JSON validation, and the existing deployment integrity
  verifier. No broker mutation is part of automated acceptance.

*Resolved with EGM*

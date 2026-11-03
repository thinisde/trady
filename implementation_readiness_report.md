# Stage 9: Implementation Readiness Test

Target: `docs/designs/trading212-same-day-sequential-protection.md`

## Blockers

### Blocker: The plan delegates schema and migration decisions

The plan says to add types only if existing fields are insufficient and to add
event storage if JSON cannot preserve ordering. Existing broker request/order
types and the fenced TradingState/journal already exist. The design must name
the exact missing invariant, exact fields, authoritative state versus event log,
migration SQL, compatibility/backfill behavior, and rollback, or defer migration
work.

### Blocker: No executable state contract

The plan names lifecycle data without enum values, field types/defaults, legal
transitions, broker status taxonomy, cancellation terminal definition, timeout,
observation window, late-fill rule, stop partial-fill handling, or emergency
sell counting. An implementer cannot know when `OPEN_PROTECTED` or `ExitPending`
is legal.

### Blocker: Quote and evidence dependency is unbounded

No concrete quote adapter, owner, venue crosswalk, compatibility measurement or
threshold, artifact schema/hash/expiry, or account/instrument mapping is named.
Adding fields to `Quote` cannot prove Trading 212 OTC compatibility. Entry
authorization remains false until a prerequisite artifact and rollout gate are
defined.

### Blocker: File boundaries and sequencing are not buildable

“Capability evidence artifacts” and “active quote adapter” are not exact paths.
Sixteen files span domain, runtime, adapter, storage, migration, config, docs,
and live demo without phases or stable checkpoints. Split shadow/paper proof
from later live mutation and give exact paths and per-phase tests.

## Major findings

1. `ATOMIC_ATTACHED` conflicts with the current capability model and must be
   registry-only/non-selectable or removed for Trading 212, with explicit serde
   backward compatibility.
2. Entry/protection requests lack exact stop type, trigger side, TIF, tick/lot
   rounding, pacing, observation cadence, and accepted status values. The plan
   must say how it reuses the current `manage` path without duplicate stops.
3. Exit cancellation lacks terminal status sets, history settlement sequence,
   grace/timeout, in-flight fill handling, and ambiguous-cancellation behavior.
4. Required/protected/uncovered quantity and daily-limit semantics are not
   deterministic across late fills, stop partial fills, restart, emergency
   sells, target partials, and replacement orders.
5. Economics lacks a versioned policy artifact, exact formula, rounding and
   precision rules, FX direction, provider-fee conflict behavior, and explicit
   spread/slippage/hurdle thresholds. It should bind to existing `EntryCosts`
   and refreshed exit-cost validation.
6. Adapter normalization claims exceed current DTOs: wallet impact has optional
   FX/net/P&L/taxes but no explicit conversion-fee field, and stop/signed-sell
   fixture validation is not specified.
7. Storage rollout omits schema version, old JSON behavior, defaults/backfill,
   in-flight journal compatibility, deployment ordering, and rollback. Existing
   migrations and fenced CAS/audit must be reused or the precise gap must be
   demonstrated.
8. Recovery/operator authority lacks owner role/API/CLI schema, allowed
   idempotent actions, audit fields, alert/exposure SLA, and close/overnight
   behavior. Reuse `recover-order` and owner controls with an exact handoff
   invariant.
9. Verification commands lack fixture names, expected states/quantities/statuses,
   freshness/pacing/deadline/flatness thresholds, migration rollback tests, and
   demo pass/fail criteria. Live mutation conflicts with the current shadow and
   paused-entry boundary and must be deferred or gated by a named release
   artifact.

## Code-versus-plan discrepancies

* `Quote` currently has only instrument, currency, bid, ask, timestamps, and
  tradable; it does not carry source, venue, liquidity, or spread evidence.
* `BrokerOrder` does not carry stop price, TIF, or a terminal cancellation
  observation. A new observation contract and plumbing through execution state
  would be required before the proposed checks can be implemented.
* `BrokerFillWalletImpact` has optional FX, net/P&L, and taxes but no explicit
  conversion-fee row; the wire schema and journal destination must be specified.
* `AutonomousTrader::manage` currently submits protection whenever reserved
  quantity is below position quantity and cancels it immediately before exit;
  the plan must specify the exact replacement behavior for terminal cancellation
  waiting.
* The current config marks protective orders and quotes unsupported and uses a
  shadow execution stage, so the proposed demo acceptance cannot run until a
  named evidence/release transition changes those gates.

## Nits

Use exact function-level changes instead of “document” or “keep” wording. Define
fresh, bounded, exact, safe, configured hurdle, and quote compatibility with
measurable values. Link existing contracts such as `PositionState`,
`TrackedOrder`, `ExecutionJournal`, and `recover-order`. Separate Modify from
Create paths. Add explicit no-position, late-fill, and restart assertions.

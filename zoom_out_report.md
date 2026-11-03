# Stage 5: Initial Zoom-Out Review

Target: `docs/designs/trading212-same-day-sequential-protection.md`

## Context alignment

The objective and motivation are clear: define a bounded one-trade same-day
lifecycle around Trading 212's non-atomic stop orders. The non-goals and
alternatives acknowledge the main uncertainty. The document should state more
explicitly that implementation remains behind the existing shadow and paused
entry gates. The demo acceptance wording should name the reviewed release
artifact or transition that permits any mutation.

## Findings

### Major: The design is too close to an implementation backlog

The Technical Plan, Component Mapping, and Detailed Implementation Plan list 16
file edits, a new migration, many state fields, fixture cases, CLI assertions,
and complete build commands. This is below architectural altitude and makes the
review and sequencing harder. Existing code already implements much of the
described flow: entry/protection/exit roles, fenced state, write-before-send
dispatch, unknown outcomes, partial uncovered protection, cancellation before
exit, daily limits, and recovery. Reframe around the few missing invariants and
reuse those paths instead of prescribing a parallel lifecycle.

### Major: ProtectionMode is speculative

Adding `ATOMIC_ATTACHED` while explicitly declaring it unsupported creates a
second semantic axis beside `Capability::usable` and risks allowing a
configuration value the broker cannot implement. Keep the existing unsupported
capability representation and add only concrete sequential evidence/configuration
until a second protection implementation is approved.

### Major: The new migration is not justified

The proposal conditionally adds append-only event storage and security-definer
machinery without showing that the current fenced JSON journal and audit events
cannot preserve the required ordering. Existing migrations already provide
revisioned state, audit records, and daily limits. State the exact missing
invariant first; otherwise extend existing journal/audit functions or defer a
new migration.

### Major: Quote compatibility is an unresolved dependency

The design requires venue compatibility, liquidity, spread, and fee evidence,
but does not name an authoritative source, crosswalk, owner, or acceptance
artifact. Trading 212 has no quote endpoint and its OTC execution may differ
from Alpaca IEX. Define the source and evidence gate first, or keep entries
blocked; do not treat field extensions as a solution.

### Major: The FX rate is not scoped as policy or evidence

The 0.15% conversion rate is repeated across the overview, cost gate, config,
adapter, and tests. The document does not say whether it is a reviewed provider
fact, a configured upper bound, or a fallback. Put it in the existing reviewed
cost/evidence model and fail closed when absent instead of scattering a magic
economic policy.

### Major: Partial-fill races need one authoritative rule

The proposal says to cancel the remainder, protect the filled quantity, and add
protection when later fills appear, but does not define fills that arrive after
the cancellation request or acknowledgement. Specify one reconciliation rule
for actual position quantity, available quantity, and protected quantity, then
reuse existing remaining/reserved calculations.

### Major: The scope mixes rollout stages

Supervised demo, eventual autonomy, and live-like release/storage/security work
are mixed together. Split a narrow shadow/paper acceptance slice from later live
release work. Otherwise the design commits to speculative deployment machinery
before the sequential workflow is proven.

### Nit: Alternatives repeat constraints

The alternatives are useful, but they repeat earlier constraints and lack
measurable decision criteria such as quote compatibility and tolerated
protection race behavior. Condense them into decision rules and keep escalation
paths in release documentation.

### Nit: Verification is too checklist-heavy

Full workspace tests, PostgreSQL migration work, deployment integrity checks,
and live demo exercise are verification tasks rather than architecture. Move
them to the implementation/acceptance plan and leave architectural proof
obligations in the design.

## Strengths

The document states the root problem clearly, keeps the fail-closed stance,
does not claim sequential protection is atomic, rejects retries and overnight
exposure, emphasizes exact-ID reconciliation, preserves native protection when
quotes are unavailable, and recognizes that cancellation acceptance is not
terminal cancellation. The codebase already contains substantial reusable
infrastructure in the autonomous application, broker port, Trading 212 adapter,
fenced storage migration, and daily-limit migration.

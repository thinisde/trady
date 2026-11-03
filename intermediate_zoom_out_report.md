# Stage 8: Intermediate Zoom-Out Review

Target: `docs/designs/trading212-same-day-sequential-protection.md`

## Residual findings

### Blocker: Scope crosses the current rollout boundary

The repository is shadow/record-only for this path; entry authorization remains
false, executable broker quotes are unsupported, and atomic protection is absent.
The design still commits to production mutation, migration, adapter
normalization, CLI work, live demo acceptance, and eventual autonomy in one
change. Narrow the design to a shadow/paper proof of the sequential lifecycle
and its evidence gates. Defer live mutation and release work until quote
compatibility, protection race handling, and independent release artifacts pass.

### Major: ProtectionMode remains an unsupported semantic axis

`ATOMIC_ATTACHED` is explicitly unsupported for Trading 212 but remains a
serialized/selectable mode. Existing capability support, expiry, evidence, and
unsupported lists already express this. Keep only concrete sequential evidence
for this broker, or make atomic capability registry-only and non-selectable.

### Major: New migration/event machinery is speculative

Existing `TradingState`, position states, fenced journal, audit facts, first-order
dispatch/reconciliation, and daily-limit migration cover the current invariants.
The proposed append-only event/security-definer machinery does not identify a
precise missing invariant, compatibility/backfill plan, or rollback path. Require
that gap before adding storage; otherwise extend existing journal APIs or defer
schema work.

### Major: Quote compatibility remains an external prerequisite

The design correctly rejects marks and fills as executable quotes, but names no
venue crosswalk, authoritative source owner, divergence test, expiry, or
acceptance artifact. Extending quote fields cannot prove compatibility. Keep
entries blocked until this external artifact exists.

### Major: Same-day/overnight rules conflict with established policy

The design disallows overnight holding without protection/reconciliation but
allows unresolved recovery past the deadline without defining close or next
session behavior. Reuse the existing exchange-calendar, liquidation-before-close,
alert, and reconcile policies rather than creating a second same-day policy.

### Major: Recovery owner boundary is unspecified

Owner-reviewed exact-ID recovery and optional flattening are not defined as an
interface or policy. Existing `recover-order`, owner controls, incident response,
and protection-retention rules should be reused. Add only the handoff invariant,
maximum alert/exposure SLA, and close handling needed by this mode.

### Major: Partial-fill and stop/exit races still lack one rule

Late fills after cancellation, ambiguous stop status, cancellation versus
terminal state, stop partial fills, and in-flight exit fills remain unresolved.
Reuse existing entry/open/exit/recovery states and remaining/reserved quantity
rules, then define one exact-ID pending/history reconciliation snapshot before
another mutation. Clarify emergency-sell counting separately from strategy sells.

### Major: Cost and FX policy is scattered

The 0.15% per-leg FX fee, slippage, and hurdle appear across multiple layers
without policy-versus-observation distinction, version, rounding, or conflict
handling. Bind reviewed assumptions through the existing execution-cost artifact
and hash; persist provider fees separately; do not bake economic policy into
adapters or fixtures.

### Nit: Alternatives and verification remain too detailed

The alternatives need measurable decision rules rather than author attribution.
The long file/test/command checklist belongs in a later implementation plan.
Architectural proof obligations should focus on state and quantity authority,
release gates, and recovery boundaries.

## Reuse evidence

`docs/v1/architecture.md` defines executor-only broker authority,
quote/calendar/execution boundaries, and analyst/gateway restrictions.
`docs/v1/strategy.md` defines lifecycle states, exact-ID reconciliation,
partial-fill protection, cancellation ordering, and overnight rules.
`docs/autonomous.md` exposes `recover-order`, shadow configuration, and false
entry authorization. Existing migrations 0004, 0012, and 0013 encode state,
dispatch/recovery, and daily-limit invariants.

## Recommendation

Reduce the design to a bounded shadow/paper sequential-protection experiment
with explicit prerequisites: a quote compatibility artifact, versioned cost
artifact, exchange-calendar policy, and owner recovery path. Defer unsupported
atomic mode, a new migration, live demo mutation, and the adapter/CLI backlog
until those prerequisites are independently proven.

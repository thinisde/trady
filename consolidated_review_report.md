# Consolidated EGM Review Report

## Target and execution

* **Target:** `docs/designs/trading212-same-day-sequential-protection.md`
* **Preset:** Consolidated report with all review rubrics
* **Feature range:** Lines 1–302
* **Detailed implementation range:** Lines 303–478
* **Additional context:** None; the target has no internal Markdown links
  requiring inclusion.

## Review result

The design is not implementation-ready. Its central problem is understandable,
but the proposed scope crosses the repository's current shadow/paused-entry
boundary and prescribes a broad live mutation, storage, adapter, and release
system before its external prerequisites exist. The implementation plan also
leaves several decisions to the implementer, especially the state contract,
quote evidence, migration strategy, recovery authority, and fee formula.

The design should be narrowed to a shadow/paper sequential-protection proof and
should reuse existing autonomous state, journal, recovery, calendar, cost, and
daily-limit contracts. Live mutation, new storage, and autonomous release work
should wait for independent quote compatibility and capability evidence.

## Blockers

### 1. Rollout scope exceeds current authority

The repository currently keeps entry authorization false and entries paused. The
design nevertheless includes live mutation, a new migration, adapter changes,
CLI work, and demo round trips in one implementation. Define a first phase that
only exercises shadow/paper state transitions and evidence gates. Defer broker
mutation and release promotion behind named artifacts and explicit gates.

### 2. The plan delegates architecture decisions

Conditional language such as “add types only if” and “add event storage if” is
not an implementation contract. The plan must either name exact fields, enums,
storage authority, migration/backfill/rollback, and compatibility behavior, or
defer the work until a concrete storage gap is demonstrated.

### 3. The sequential lifecycle has no executable state contract

The design lacks exact state values, fields, legal transitions, broker status
sets, terminal cancellation definition, observation cadence, timeouts, late-fill
handling, stop partial-fill handling, and emergency-sell counting. Without these,
an implementer cannot determine when protection or a target exit is authorized.

### 4. Quote compatibility is an external prerequisite, not a field addition

The document requires a venue-compatible executable quote but does not define a
source owner, instrument/venue crosswalk, divergence test, threshold, expiry,
artifact schema, or behavior when Trading 212 OTC execution differs from IEX.
Current quote fields cannot prove this compatibility. Entry must remain blocked
until a concrete evidence artifact exists.

### 5. The unprotected-holding and recovery boundary is unsafe

The plan says to flatten when safe or alert and remain unresolved, but does not
define an operator authority, recovery command contract, alert/exposure SLA,
manual procedure, kill switch, market-close behavior, or overnight rule. Reuse
the existing owner controls and exact-ID recovery path and define the minimum
handoff invariant.

### 6. Partial-fill and cancellation races are not closed

The flow cancels entry remainder, places a stop, cancels protection before a
target sell, and allows later fills, but has no single authoritative pending plus
history reconciliation sequence. It needs explicit rules for late fills after a
cancellation request, ambiguous cancellation, stop partial fills, in-flight exit
fills, and residual quantity before any new mutation.

### 7. Atomic protection remains an unsupported selectable concept

The design names `ATOMIC_ATTACHED` even while declaring it unsupported for
Trading 212. Make it registry-only and non-selectable for this broker, or remove
it until another implementation exists. Sequential evidence must not imply
atomic protection.

## Major findings

* Same-day values are undefined: timezone, calendar, session boundary, deadline,
  cutoff, pacing, quote age, flatness SLA, and alert SLA need values.
* Stop semantics are incomplete: stop versus stop-limit, trigger side, price
  derivation, tick/lot rounding, working statuses, and partial stop fills need
  exact rules.
* Exit cancellation lacks a terminal-status set, history settlement/grace
  sequence, timeout, and ambiguous-result behavior.
* Daily limits do not classify emergency liquidation, stop-triggered sells,
  target partials, and replacement orders deterministically.
* The 0.15% FX fee, slippage, spread, and hurdle lack a versioned cost artifact,
  formula, rounding/precision, FX direction, and provider-fee conflict policy.
* The current `Quote` type lacks source, venue, liquidity, and spread evidence.
  `BrokerOrder` lacks stop/TIF and terminal cancellation facts. Wallet impact has
  optional FX/net/P&L/taxes but no explicit conversion-fee row. The plan must
  specify new wire fields and journal destinations or reuse existing contracts.
* Journal schema expansion lacks versioning, old-record defaults, backfill,
  in-flight compatibility, deployment ordering, rollback, and restart behavior.
* The demo acceptance lacks exact account/instrument, stop price, deadline,
  expected observations, thresholds, and pass/fail assertions.
* The alternatives use “Rejected by author” without linked evidence. Use
  measurable decision rules or remove unsupported attribution.
* The 16-file implementation backlog is below architectural altitude and should
  be reduced to missing invariants and phased proof obligations.

## Nits

Define uncalibrated terms such as fresh, bounded, exact, safe, and configured
hurdle. Normalize list style. Link existing contracts (`PositionState`,
`TrackedOrder`, `ExecutionJournal`, `recover-order`) and distinguish Modify from
Create. Avoid claims that current capability records “prove” behavior without
artifact references.

## Traceability matrix

| Consolidated action | Stage 5 | Stage 6 | Stage 7 | Stage 8 | Stage 9 |
|---|---:|---:|---:|---:|---:|
| Narrow to shadow/paper before live mutation | ✓ | — | ✓ | ✓ | ✓ |
| Reuse existing state/journal; remove speculative migration | ✓ | ✓ | ✓ | ✓ | ✓ |
| Define exact lifecycle, statuses, timeouts, and quantity rules | ✓ | ✓ | ✓ | ✓ | ✓ |
| Define quote compatibility artifact and gate | ✓ | ✓ | ✓ | ✓ | ✓ |
| Define owner recovery authority and unprotected exposure handling | — | ✓ | ✓ | ✓ | ✓ |
| Bind FX/slippage economics to a versioned cost artifact | ✓ | ✓ | ✓ | ✓ | ✓ |
| Remove or restrict unsupported atomic mode | ✓ | — | ✓ | ✓ | ✓ |
| Make demo acceptance thresholds and assertions executable | — | ✓ | ✓ | — | ✓ |
| Replace unsupported alternative attribution | ✓ | — | ✓ | — | — |

## Required revision sequence

1. Rewrite the scope as a shadow/paper experiment with no live mutation and a
   named later release gate.
2. Bind the design to existing lifecycle, journal, calendar, cost, owner-control,
   and exact-ID recovery contracts.
3. Write the canonical state/quantity/order-status contract, including late-fill,
   cancellation, stop-fill, emergency-sell, timeout, and close behavior.
4. Define the quote compatibility artifact and the independent review owner,
   schema, hash, expiry, and pass/fail rule.
5. Define the versioned cost formula and provider-fee conflict behavior.
6. Remove the speculative migration and unsupported atomic runtime mode unless a
   concrete gap and implementation are approved.
7. Replace the broad file backlog with phased proof obligations and exact
   acceptance assertions.

## Stage artifacts

* [Initial zoom-out report](/thinisde/trady/zoom_out_report.md)
* [Comprehension report](/thinisde/trady/comprehension_report.md)
* [Critic report](/thinisde/trady/critic_report.md)
* [Intermediate zoom-out report](/thinisde/trady/intermediate_zoom_out_report.md)
* [Implementation readiness report](/thinisde/trady/implementation_readiness_report.md)

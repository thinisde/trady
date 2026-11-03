# Goldfish Annotated Review Report

**Status:** resolved

**Target:** `docs/designs/trading212-same-day-sequential-protection.md` (`full file`)

The findings below combine the five Goldfish stages recorded in
`consolidated_review_report.md`. Suggestions are classified against the current
design document and repository context. No target edits have been applied.

## Prioritized Findings & AI Suggestions

### Issue 1: Scope crosses the current rollout boundary [`egm-zoom-out`, `egm-design`] - Severity: Blocker

*   **Affected Scope:** Lines 9-23, 281-301, 303-476 (`Overview`, `Non-Functional Requirements`, `Detailed Implementation Plan`)
*   **Discrepancy Description:** The design combines shadow/paper proof, live mutation, a new migration, adapter and CLI work, demo round trips, and eventual autonomous release even though the current runtime keeps entry authorization false and entries paused.
*   **Rationale:** A single change cannot be reviewed or safely rolled out while quote compatibility, protection semantics, and release artifacts remain unresolved.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Narrow the design to a shadow/paper experiment. Defer broker mutation, live demo acceptance, new schema work, and autonomous release to a later design gated by named evidence.
*   ***Proposed Edit:*** Replace the rollout goal and implementation scope with two phases: (1) shadow/paper lifecycle and evidence validation with zero broker mutation; (2) a separate supervised-demo/live design after quote compatibility, cost, protection, and owner-release artifacts pass. Remove live mutation and demo-order work from this document's target files and verification plan.

### Issue 2: The implementation plan delegates architectural decisions [`egm-implementation-readiness`] - Severity: Blocker

*   **Affected Scope:** Lines 316-324, 388-405 (`Target Files`, broker and storage sections)
*   **Discrepancy Description:** The plan says to add types only if existing fields are insufficient and to add event storage if JSON cannot preserve ordering.
*   **Rationale:** An implementer cannot choose an authoritative state model, event model, migration, or compatibility strategy from conditional instructions.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Existing `BrokerOrderRequest`, `BrokerOrder`, `CancellationReceipt`, fenced `TradingState`, and journal APIs are concrete reuse points. The design must either prove a missing invariant or defer schema work.
*   ***Proposed Edit:*** Replace conditional bullets with an explicit decision: reuse existing `TradingState`/`TrackedOrder` and fenced audit facts for the shadow/paper phase; do not create `migrations/0015` in this design. Add a separate migration only after a documented storage-gap test fails.

### Issue 3: No executable lifecycle/state contract [`egm-comprehension`, `egm-design`, `egm-implementation-readiness`] - Severity: Blocker

*   **Affected Scope:** Lines 148-208 and 326-343 (`Durable sequential order state`, entry/protection flow, autonomous application plan)
*   **Discrepancy Description:** The design names protection lifecycle data but does not define fields, enum values, legal transitions, broker status taxonomy, terminal cancellation, observation windows, late-fill rules, or emergency-sell counting.
*   **Rationale:** A fresh implementer cannot determine when `OPEN_PROTECTED`, `ExitPending`, or recovery is legal.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Existing `PositionState`, `OrderRole`, `TrackedOrder`, and `AutonomousTrader::manage` provide the baseline. The document must state one canonical contract before proposing changes.
*   ***Proposed Edit:*** Add a state table with `FLAT`, `ENTRY_PENDING`, `OPEN_UNPROTECTED`, `OPEN_PROTECTED`, `EXIT_LATCHED`, `EXIT_PENDING`, `CLOSED`, and `RECOVERY_REQUIRED`; define required fields, legal transitions, broker working/terminal statuses, observation cadence, timeout, and quantity invariants. State that exact pending-plus-history observation is authoritative before any replacement mutation.

### Issue 4: Quote compatibility is an unbounded external dependency [`egm-zoom-out`, `egm-comprehension`, `egm-design`, `egm-implementation-readiness`] - Severity: Blocker

*   **Affected Scope:** Lines 19-23, 132-146, 266-279, 346-352, 439-444 (`Overview`, quote/cost gate, alternatives, implementation files)
*   **Discrepancy Description:** The design requires venue-compatible executable quotes but names no source owner, instrument/venue crosswalk, divergence test, threshold, expiry, evidence schema, or behavior when Trading 212 OTC execution differs from IEX.
*   **Rationale:** Adding source and venue fields cannot prove compatibility; without an artifact, entry remains blocked.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** The design needs a prerequisite contract, not a promise to extend `Quote`. The existing `Quote` has only instrument, currency, bid/ask, timestamps, and tradable.
*   ***Proposed Edit:*** Add a “Quote compatibility prerequisite” section defining the authoritative feed, account/instrument mapping, venue scope, freshness threshold, spread/liquidity limits, divergence test, evidence owner/hash/expiry, and fail-closed behavior. Move the adapter field expansion to a later design and state that no entry phase proceeds without the artifact.

### Issue 5: Unprotected holding recovery lacks an operational owner boundary [`egm-design`, `egm-comprehension`, `egm-zoom-out`] - Severity: Blocker

*   **Affected Scope:** Lines 183-208, 298-301, 452-453 (`Exit`, `Failure recovery`, rollout and verification)
*   **Discrepancy Description:** If stop placement is rejected or unknown and no quote exists, the design says to flatten when safe or alert and remain unresolved, but defines no owner role, recovery command schema, alert/exposure SLA, manual procedure, kill switch, market-close, or overnight rule.
*   **Rationale:** A live unprotected holding needs a bounded operational handoff; “owner-reviewed recovery” is not an interface.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Existing `recover-order`, owner controls, exchange calendar, and management-only paths should be reused rather than inventing a new subsystem.
*   ***Proposed Edit:*** Add the exact existing recovery command/API, authorized owner identity, allowed exact-ID actions, idempotency/audit requirements, alert deadline, maximum exposure window, market-close behavior, and explicit rule that unresolved recovery blocks new entries and may remain management-only.

### Issue 6: Partial-fill and cancellation races are not closed [`egm-comprehension`, `egm-design`, `egm-zoom-out`, `egm-implementation-readiness`] - Severity: Blocker

*   **Affected Scope:** Lines 166-197 and 331-343 (`Entry and protection flow`, exit flow, detailed state plan)
*   **Discrepancy Description:** The plan cancels entry remainder, protects filled quantity, allows later fills, cancels protection before exits, and relies on terminal observations without defining late-fill, ambiguous-cancel, stop-partial-fill, or in-flight-exit rules.
*   **Rationale:** These races determine uncovered quantity and oversell safety.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Existing exact-ID pending/history recovery and remaining/reserved quantity calculations can provide one authoritative rule.
*   ***Proposed Edit:*** Specify this sequence: cancel remainder; require exact-ID pending/history reconciliation; recompute actual position minus terminal sold quantity; protect only uncovered quantity; never submit a replacement while the prior ID is unresolved. Define late fills after cancel as `RECOVERY_REQUIRED` until reconciled, and define stop partial fills as a new uncovered-quantity calculation.

### Issue 7: Atomic protection remains an unsupported selectable mode [`egm-zoom-out`, `egm-design`, `egm-implementation-readiness`] - Severity: Major

*   **Affected Scope:** Lines 117-130 and 307-316 (`Capability boundary`, domain plan)
*   **Discrepancy Description:** `ATOMIC_ATTACHED` is named as a serialized mode while the document says Trading 212 does not support it.
*   **Rationale:** A configuration could appear valid while selecting an unimplemented broker behavior.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Sequential evidence is sufficient for this broker; atomic support belongs in a future capability registry only.
*   ***Proposed Edit:*** Remove `ATOMIC_ATTACHED` from runtime-selectable configuration. Record it only as an unsupported registry value or future extension, with explicit rejection of legacy/unknown values.

### Issue 8: Same-day and overnight behavior is contradictory [`egm-design`, `egm-zoom-out`] - Severity: Major

*   **Affected Scope:** Lines 68-77 and 183-208 (`Non-Goals`, deadline/recovery flow)
*   **Discrepancy Description:** The design disallows overnight holding without protection/reconciliation but allows unresolved recovery past the deadline without specifying next-session behavior.
*   **Rationale:** Timezone, holidays, early close, and unresolved close handling are safety policy, not implementation detail.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Existing exchange-calendar and liquidation-before-close policies should be the authority.
*   ***Proposed Edit:*** Define the exchange calendar/timezone and close cutoff through existing contracts; state that shadow/paper recovery may remain unresolved for observation, while any live promotion requires pre-close liquidation or an independently reviewed overnight protection path. Add alert and next-session reconciliation behavior.

### Issue 9: Fee and FX economics are scattered and non-normative [`egm-zoom-out`, `egm-comprehension`, `egm-design`, `egm-implementation-readiness`] - Severity: Major

*   **Affected Scope:** Lines 19-23, 132-146, 281-301, 385-386, 415-416 (`Overview`, cost gate, NFR, implementation plan)
*   **Discrepancy Description:** The 0.15% per-leg FX fee, slippage, spread, and hurdle appear across several layers without version, source, rounding, precision, FX direction, or conflict handling.
*   **Rationale:** Different layers could calculate different net targets.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Existing execution-cost artifacts and `EntryCosts` provide the correct policy/evidence boundary.
*   ***Proposed Edit:*** Replace scattered constants with a versioned cost artifact containing source, effective time, FX direction, rounding, precision, spread/slippage bounds, and conflict behavior. Require the hash before entry and persist provider-observed FX/fees separately.

### Issue 10: Storage and migration rollout is speculative [`egm-zoom-out`, `egm-implementation-readiness`] - Severity: Major

*   **Affected Scope:** Lines 148-164, 388-405 (`Durable state`, storage and migration plan)
*   **Discrepancy Description:** The plan proposes a migration/event model without a demonstrated storage gap, schema version, old-record defaults, backfill, in-flight compatibility, deployment order, or rollback.
*   **Rationale:** A second journal could bypass existing fenced CAS/audit boundaries and break restart safety.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Existing migrations 0004, 0012, and 0013 already cover state, dispatch/recovery, audit, and daily limits.
*   ***Proposed Edit:*** Remove `migrations/0015_sequential_protection.sql` from this design. Add a storage-gap acceptance test and require a separate migration design only if it fails; document backward-compatible defaults and rollback in that future design.

### Issue 11: Recovery mutation is security-sensitive but underspecified [`egm-comprehension`, `egm-design`, `egm-implementation-readiness`] - Severity: Major

*   **Affected Scope:** Lines 199-208 and 388-405 (`Failure recovery`, storage plan)
*   **Discrepancy Description:** Owner-reviewed exact-ID recovery lacks role, authorization, allowed actions, idempotency, audit fields, dual control, and exposure limits.
*   **Rationale:** Recovery can itself create or cancel broker mutations.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** The repository already exposes an owner control and `recover-order` path that can be constrained rather than inventing a new API.
*   ***Proposed Edit:*** Reference the existing command and define its permitted exact-ID binding, no-guess rule, authenticated owner scope, audit record, replay behavior, and escalation when the position is unprotected.

### Issue 12: Stop and exit semantics lack deterministic details [`egm-comprehension`, `egm-design`, `egm-implementation-readiness`] - Severity: Major

*   **Affected Scope:** Lines 166-197 and 331-343 (`Entry/protection`, exits, implementation plan)
*   **Discrepancy Description:** Stop type, trigger side, price derivation, TIF, tick/lot rounding, working statuses, cancellation terminal set, target order selection, partial target fill, and replacement behavior are not specified.
*   **Rationale:** These values determine whether protection is live and whether a target sell is safe.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Trading 212 already exposes stop/stop-limit fields; the design must bind to exact supported statuses and existing price/quantity rules.
*   ***Proposed Edit:*** Add a parameter table for stop-market versus stop-limit, sell trigger, TIF, rounding, working/terminal status sets, target limit/market selection, partial target handling, and replacement restrictions.

### Issue 13: Demo verification is not executable [`egm-comprehension`, `egm-design`, `egm-implementation-readiness`] - Severity: Major

*   **Affected Scope:** Lines 455-476 (`Verification Plan`)
*   **Discrepancy Description:** The demo plan lacks exact account/instrument, stop price, deadline, expected observations, thresholds, migration rollback tests, and pass/fail assertions.
*   **Rationale:** A fresh implementer cannot run or judge the acceptance test.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Acceptance evidence should be bounded, deterministic, and compatible with the current shadow/paused-entry state.
*   ***Proposed Edit:*** Replace the live round-trip step with a shadow/paper fixture matrix naming each input, expected state, quantity, broker status, timeout, recovery result, zero-oversell assertion, and evidence artifact. Defer demo mutation to a named release gate.

### Issue 14: Alternatives lack evidence-based rejection rationale [`egm-zoom-out`, `egm-design`] - Severity: Major

*   **Affected Scope:** Lines 233-279 (`Alternatives Considered`)
*   **Discrepancy Description:** The alternatives use “Rejected by author” without linked evidence or measurable criteria.
*   **Rationale:** The review standard requires reasons that a reader can verify.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** The rationale can be retained without unsupported attribution.
*   ***Proposed Edit:*** Replace “Rejected by author” with decision rules: atomic broker requires migration and new capability evidence; management-only omits automation; quote-only does not solve protection. Link the repository/API evidence where available.

### Issue 15: Verification and file list are below architectural altitude [`egm-zoom-out`, `egm-implementation-readiness`] - Severity: Nit

*   **Affected Scope:** Lines 303-476 (`Detailed Implementation Plan`)
*   **Discrepancy Description:** The 16-file backlog and full test command list obscure the few architectural invariants.
*   **Rationale:** The document should establish boundaries and proof obligations; a later implementation plan can enumerate files after the scope is resolved.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** The findings are useful but belong in a narrower implementation handoff.
*   ***Proposed Edit:*** Condense this document's implementation section to phases, invariants, prerequisites, and acceptance gates. Move exact file lists and command checklists to a follow-up implementation plan.

### Issue 16: Terminology and current-code claims need calibration [`egm-comprehension`, `egm-design`] - Severity: Nit

*   **Affected Scope:** Lines 25-46, 82-231 (`Problem`, architecture)
*   **Discrepancy Description:** Terms such as fresh, bounded, exact, safe, fail-closed, and “proves” are not operationally defined, and current interfaces are named without links or contract summaries.
*   **Rationale:** A fresh implementer needs measurable terms and references to distinguish existing guarantees from proposed behavior.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** Small definitions and links improve precision without changing the architecture.
*   ***Proposed Edit:*** Add a short terminology table and link `PositionState`, `TrackedOrder`, `ExecutionJournal`, `recover-order`, `Quote`, `EntryCosts`, and the capability artifact. Replace “proves” with “observed” unless an artifact establishes the claim.

### Issue 17: Additional nits [`egm-design`, `egm-implementation-readiness`] - Severity: Nit

*   **Affected Scope:** Lines 233-279, 303-476
*   **Discrepancy Description:** List marker style varies, some bullets say “keep” or “document” without function-level scope, and tests omit explicit no-position, late-fill, and restart assertions.
*   **Rationale:** These reduce implementation precision but do not change the architecture.
*   ***AI Suggestion:*** DECLINE
*   ***Justification:*** Fold these into the larger structural revisions rather than creating separate design decisions.
*   ***Proposed Edit:*** None as a standalone fix; normalize style during the next document rewrite.

## Findings needing author input

None. All findings can be resolved by revising the design around existing
repository contracts and the review evidence; no external verbal decision is
required at this stage.

## Resolution record

* **PROPOSE FIX accepted:** 16
* **CONTEST:** 0
* **ESCALATE:** 0
* **DECLINE:** 1
* **Target edits applied:** The design was narrowed to shadow/paper proof,
  existing lifecycle and recovery contracts, explicit state/quantity rules,
  quote and cost prerequisites, and deferred live/migration/atomic work.

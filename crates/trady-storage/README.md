# Trady PostgreSQL storage

## Overview

Synchronous PostgreSQL persistence behind the application
execution, research and model-session repository ports, including autonomous
account journals and immutable screening/dossier/Perpetual records.

## Subdirectories & Boundaries

- [Source contract](./src/README.md): migration runner, runtime role checks,
  `PgExecutionRepository`, `PgResearchRepository`, `PgSessionRepository`,
  `PgIntelligenceRepository`, `PgTradingJournal` and `PgTradingResearch`.

## Interface

- `[src/lib.rs:PgExecutionRepository]` and `[src/sessions.rs:PgSessionRepository]`:
  execution fixtures and owner/account/policy-scoped model sessions.
- `[src/financial.rs:PgFinancialRepository]`: shared provider-credit reservations,
  temporal/rights-checked snapshots, source health and explicit content purge.
- `[src/x_monitoring.rs:PgXCostRepository]`: durable resource reservations,
  settlement, billing reconciliation and status across shared credential scope.
- `[src/research.rs:PgResearchRepository]`: analyst-only source item/cursor storage,
  atomic revisions/events, retention purge and rights-aware reading.
- `[src/intelligence.rs:PgIntelligenceRepository]`: analyst-only observation,
  registry, invalidation, rejection and descriptive-assessment persistence with
  database ingestion times, immutable replay and erasure tombstones.
- `[src/autonomous.rs:PgTradingJournal]`: executor account leases and atomic
  revision/fence-checked autonomous state.
- `[src/trading_research.rs:PgTradingResearch]`: owner/account-scoped immutable
  dossier/evidence, screening, context, assessment/review writes and expiry purge.
  `connect_retention` supplies a separate connection with statement/lock timeouts.
- `[src/research_jobs.rs:PgResearchJobs]`: scoped durable research/sentiment jobs,
  gateway receipts/outbox and owner control previews.
- `[src/weekly.rs:PgWeeklyStore]`, `PgForecastLedger`, `PgReleaseStore` and
  `PgActiveReleaseReader`: fenced experiments and independent release evidence.
  `[src/weekly.rs:PgHoldoutLedger]` claims/finishes sealed evaluation;
  `[src/weekly.rs:ReleaseCandidate]` and `[src/weekly.rs:ReconciledExport]`
  validate scoped release and reconciled export documents.
  Structural/hash checks do not establish independent provenance;
  `[src/weekly.rs:evidence_envelope]` does not call candidate validation.
- `[src/holding_research.rs:VerifiedHolding]`: current executor-attested acquisition
  costs, holding advice validation and queued target updates.
- [Migration contract](../../migrations/README.md): exact tables, functions and role grants.
- [Cargo target](./Cargo.toml): `trady_storage`. Its database scenario is in `tests/postgres.rs`.

## Invariants

`[src/lib.rs:PgExecutionRepository::connect]` requires the restricted executor role.
Runtime execution-state mutations use narrow SQL functions with database-time leases and
fencing checks; the executor receives no direct financial-table mutation grants.

`[src/research.rs:PgResearchRepository::connect]` and
`[src/sessions.rs:PgSessionRepository::connect]` require `trady_analyst`.
Research cursors, content, collection events and model-session JSON are analyst
metadata; writing them does not confer executor authority. Research page commits
check durable cursor revisions. Model-session saves check revision, worker,
generation and database-time expiry, then renew the lease for its stored TTL.

`[src/lib.rs:migrate]` transactionally applies versions 1–11 and rejects changes to
previously applied migration text. `[src/transport.rs:uses_tls]` permits plaintext
TCP only for exact `127.0.0.1` or `::1` hosts; explicit Unix sockets also qualify
and plaintext eligibility requires every hostaddr override to be loopback. Remote
connections require `sslmode=require`, platform CA trust and hostname verification.

`[src/intelligence.rs:insert]` rejects Tax payloads and erased-ID reuse.
`[src/intelligence.rs:save_assessment]` requires an unerased, unexpired observation
and research-only calibration/authority fields. It clamps stored retention to the
earlier of the requested and observation deadlines.
Matching erasing invalidations reject storage; non-erasing invalidations reject
when their time is at or before the assessment's `decision_at`.
`find` and `candidates` exclude erased/expired documents; invalidation checks
remain separate from those reads.
`register` atomically activates a reviewed registry and retires its predecessor;
stale callers cannot reactivate a retired hash. A new explicit review needs a
fresh revision/hash. Erased lineage rejects new observation IDs and rejection
markers even with a fresh registry; newly sourced lineage remains admissible.
Migration 11 enforces scoped typed-content/hash/dependency
bindings and research-only assessments, applies direct SQL invalidations/erasure,
and filters expired/unlicensed payloads with row policies. Every content operation
reconciles current database-time retention, including historical reads.
Intelligence insert/erasure/invalidation/activation/refresh boundaries require
`READ COMMITTED`, as normal repository transactions/autocommit use. Unsupported
isolation raises SQLSTATE `0A000` / `STORAGE_REQUIRES_READ_COMMITTED`; roll back
and retry at `READ COMMITTED`. This covers snapshots established before any
observation/tombstone or dependent publication, which existing-row locks cannot
make current.

`[src/autonomous.rs:PgTradingJournal]` requires `trady_executor` and migration 4;
`[src/trading_research.rs:PgTradingResearch]` requires `trady_analyst`. Trading
records bind owner/account/kind/ID, immutable payload, nonextendable retention and actual persisted
references; expired records are excluded. Autonomous SQL preserves account scope,
order attempts, consumed sessions and same-proposal exit latches across fenced
revisions. Analyst writes grant no financial authority.
`[src/financial.rs:reserve_credits]` serializes shared credential budgets.
Migration 11 bounds consumed credits by stored ceilings and prevents refunds,
window rebinding or ceiling increases; a tightened cap below spent usage denies
further reservations while preserving the charges. Weekly/release SQL rejects
null/missing guard inputs, and both active-release readers recheck renewable,
scoped verifier readiness and owner expiry at lookup time. Release transitions
and `release.current_active` require `READ COMMITTED` and raise the same isolation
error otherwise, preventing reuse of old readiness or an old active pointer.
Lookup checks committed state during the call; later commits require a new lookup.
Intelligence SQL checks input keys and envelope/hash/dependency bindings without
deserializing all nested Rust types or establishing source truth. Its SELECT
policies enforce retention/rights using row scope, while end-user owner/account
authentication remains a caller responsibility
(`[../../migrations/0011_fail_closed_storage.sql:research.guard_intelligence_observation]`).
`[src/autonomous.rs:validate_entry_release]` resolves current persisted ACTIVE
authority against the deployment binding; release-bound holding updates require
matching model revision/artifact hashes (`[src/autonomous.rs:pending_holding_update]`).
Verified holding observations and durable owner control latches require the current
executor fence. Holding advice resolves immutable assessment/review/research rows;
the executor evaluates active exits before considering a replacement target.
New live plans and target revisions check persisted sentiment dependencies in
their acceptance transaction. Queued updates retain copied research only until
the earliest underlying retention deadline or their own expiry; hash identities
remain immutable after payload cleanup.

## Side Effects

Database connections and transactional schema/grant changes. Execution SQL
functions persist account/intent/snapshot state and associated audit/outbox records.
Research persistence stores cursors, items, revisions, invalidations and collection
events, including retention/rights redaction. Session persistence stores complete
session documents and renews generation-fenced leases
(`[src/lib.rs:migrate]`, `[src/lib.rs:PgExecutionRepository]`,
`[src/research.rs:PgResearchRepository]`, `[src/sessions.rs:PgSessionRepository]`).
Autonomous state functions write `execution.autonomous_accounts`, audit and
notification-outbox records; trading research writes/purges `research.trading_records`
and `research.holding_updates` and retains hash-only update identities.
Dossier/evidence writes are separate statements and can persist partially.
`[src/intelligence.rs:PgIntelligenceRepository]` writes migrations 10/11's research
tables. Erasure nulls observation JSON/canonical text and removes derived/rejection
rows. Registry activation records permanent hash retirements; reads can commit
cleanup before returning or failing. `purge` cleans expired/unlicensed payloads
across scopes while retaining observation metadata. Physical cleanup occurs on
repository operations or explicit SQL sweeps; row policies deny expired content
while idle and acquire shared locks on checked registry/input rows. Atomic
dependent cleanup is supported at `READ COMMITTED`; fixed-snapshot row mutations
and refresh are rejected before they can erase content.
`[src/financial.rs:read]` purges expired/revoked content before its snapshot read;
the shared `[src/financial.rs:read_snapshot]` helper can persist manifest references
without purging. Research reads can perform a global content purge
(`[src/research.rs:purge]`). Scheduling can
persist partially across separate enqueue/update operations
(`[src/research_jobs.rs:schedule]`). X status acquires a transaction advisory
lock in addition to reading accounting state (`[src/x_monitoring.rs:status]`).

## Verification

Run `cargo test -p trady-storage --locked` for local unit checks. Run
`cargo test --workspace --locked` for the workspace and
`python3 scripts/test-postgres.py` for explicitly ignored database scenarios and
CLI checks against a disposable PostgreSQL container. Run commands from the
repository root.

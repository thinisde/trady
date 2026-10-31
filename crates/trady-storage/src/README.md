# PostgreSQL repository

## Overview

[lib.rs](./lib.rs) implements the execution repository and migration runner with
the synchronous postgres client. [research.rs](./research.rs) persists research
collection state and content; [sessions.rs](./sessions.rs) persists scoped model
sessions. `[transport.rs:connect]` supports explicit local plaintext and verified
remote TLS connections.
[autonomous.rs](./autonomous.rs) persists fenced trading state;
[trading_research.rs](./trading_research.rs) stores immutable owner/account-bound
dossiers, screening snapshots and Perpetual contexts/assessments/reviews.

## Interface

- `[financial.rs:PgFinancialRepository]` implements `FinancialRepository` with
  synchronized source/account records, append-only observation revisions,
  manifests, bounded collection leases and shared credential budget windows.
  Migration 9 is compiled as `[lib.rs:FINANCIAL_DATA_SQL]`.

- [research_jobs.rs](./research_jobs.rs): `PgResearchJobs::{connect_gateway,
  connect_analyst, connect_executor}`, durable `ResearchJobRepository` and
  `ResearchInformation`, `publish_verified_packet`, immutable sentiment/readiness
  reads, schedules, `accept_gateway` and outbox delivery. Executor integration uses
  `publish_control_context`, `pending_owner_controls`, `record_control_result` and
  the transaction-local SQL predicate `research.sentiment_reference_current`.
  `publish_report` accepts attributed bounded owner/account reports for gateway
  display. See the [research automation contract](../../trady-adapters/src/research_jobs/README.md).

`[weekly.rs:PgWeeklyStore]` implements fenced `WeeklyStore` persistence;
`PgHoldoutLedger` shares its database session/fence. `PgForecastLedger` records
immutable prospective forecasts, matured labels and independently reconciled
exports. `PgReleaseStore` resolves content-addressed candidate/evidence documents
and advances deployment-owned release states. `PgActiveReleaseReader` uses only
the executor DSN to resolve current ACTIVE authority through a read-only function.
Weekly worker TTLs are 1–3,600 seconds and canonical documents are bounded to
4 MiB. Checkpoint persistence and lease renewal are separate calls
(`[weekly.rs:PgWeeklyStore::connect]`, `[weekly.rs:canonical]`,
`[weekly.rs:PgWeeklyStore::checkpoint]`).

- `[x_monitoring.rs:PgXCostRepository]`: `connect`, `status`, `reconcile` and
  `XCostRepository::{reserve,settle}` persist shared resource accounting;
  `[x_monitoring.rs:XReconciliation]` carries a provider total/time/evidence reference.
- `[lib.rs:X_API_MONITORING_SQL]` adds migration 5. `[research.rs:export_x_evidence]`
  exports permitted current X evidence using durable ingestion cutoffs and stored coverage.

- `[lib.rs:ROLES_SQL]`, `FOUNDATION_SQL`, `SESSIONS_SQL`, `RESEARCH_SOURCES_SQL`,
  `AUTONOMOUS_SQL`, `X_API_MONITORING_SQL`, `WEEKLY_LAB_SQL`, `RESEARCH_JOBS_SQL`,
  `VERIFIED_HOLDINGS_SQL`, `FINANCIAL_DATA_SQL`: embedded SQL. `[lib.rs:migrate]` applies versions 1–9 transactionally,
  protected by advisory lock `861204733` and exact source comparison;
  `ROLES_SQL` is exposed separately and is not executed by `migrate`.
  Migration 3 covers source state, observations, revisions, collection events,
  invalidations and content redaction.
  Migration 4 adds `execution.autonomous_accounts`, `research.trading_records`
  and narrowly granted autonomous/trading-research functions.
- `[lib.rs:check_service_database]`: verifies an expected runtime role on `trady`.
- `[lib.rs:PgExecutionRepository]`: `connect`, `register_paper_account` and repository
  methods for leases, intents, snapshots and reconciliation.
- `[research.rs:PgResearchRepository]`: `connect`, `purge_content`, `read_item`,
  `mark_x_unavailable` and
  `ResearchRepository::{reserve_shared_requests, state, commit_page, read_items}` for durable source cursors,
  normalized items, content revisions and collection status.
- `[sessions.rs:PgSessionRepository]`: `connect` and
  `SessionRepository::{create, load, list, acquire, save, release}` for owner,
  account and policy-scoped model-session documents. Leases carry the session,
  worker, generation and `ttl_seconds`.
- `[autonomous.rs:PgTradingJournal]`: `connect` and
  `ExecutionJournal::{acquire, renew, load, save, release}`. Save accepts a fence,
  expected `u64` revision and `TradingState`; acquisition accepts `TradingConfig`,
  worker and TTL.
  Holding observation/queued-update/control methods use the current fence;
  `bind_live_release` pins the deployment candidate and `validate_entry_release`
  resolves current ACTIVE evidence immediately before live entry.
- `[trading_research.rs:PgTradingResearch]`: `connect(dsn, owner_id, account_id)`,
  `connect_retention`, `purge_expired`; `TradingResearchStore::{store_calculation, store_dossier, store_context}`;
  `ScreeningRepository::store_screening_snapshot`;
  `PerpetualRepository::{load_context, save_assessment, load_assessment, save_review}`.
  Dossier/context writes take explicit retention deadlines; optional assessment
  reads return `Result<Option<AssessmentRecord>>`.
- `[holding_research.rs:VerifiedHolding]`: `basis`, `calculation_id` and
  `validate_context` bind actual executor-observed economics and active thresholds.
  `PgTradingResearch::{load_verified_holding,queue_holding_update}` resolve native
  advice/review/research before queueing an update.

## Invariants

- `[financial.rs:reserve_credits]` serializes weighted reservations by provider
  credential key across minute/day/month windows, retaining failed/crashed usage.
  `[financial.rs:check_lease]` rechecks source, account, window and approval.
  `[financial.rs:read_snapshot]` checks current consumer rights, receipt/public/
  database recording cutoffs and coverage. `[financial.rs:finish]` verifies
  observation hashes and appends correction invalidations without rewriting inputs.
  It marks availability after the payload transaction commits; an interrupted
  collection without that marker remains excluded from snapshots
  (`[financial.rs:finish]`). `[financial.rs:health]` reports local stored source
  health with `entry_authorized=false`.

- Migration 7 keeps gateway acceptance separate from analyst sessions and
  financial mutation. The SQL enqueue function enforces source/instrument scope,
  coalescing, idempotent payloads and durable per-owner queue/day caps. Lease
  generation and server time fence worker completion/failure. Claim-time failures
  and watchdog cancellations use their own scope/state guards
  (`[research_jobs.rs:check_lease]`, `[research_jobs.rs:claim]`,
  `[research_jobs.rs:cancel_closed_watchdogs]`). Packet and assessment
  rows are insert-only; current dependency/readiness checks retain ambiguity.
- Gateway owner controls bind authenticated bot/user/chat/account/mode/policy and
  exact plan/holding/quantity. Resume/close require hashed single-use expiring
  confirmations; pause may request reduced activity immediately. The executor
  independently rechecks binding and records its durable result. Status changes
  enqueue result notifications atomically; acknowledgements do not prove broker
  submission or a fill.

`[../../../migrations/0006_weekly_lab.sql:weekly.freeze]` retains database start
time and immutable inputs; `[../../../migrations/0006_weekly_lab.sql:weekly.reserve_trial]` reservations
consume cumulative dataset budgets before helpers run. Failed/interrupted trials
count. A sealed dataset can be consumed once per account/mode; a crash does not
silently replay it. Checkpoints cannot remove trials or rewrite completed
evidence. Release transitions require independent verification, twenty distinct
prospective real calendar sessions and scoped owner approval; test/demo evidence
cannot verify a release. Stored content hashes and the active pointer resolve at
runtime. Lab, verifier, shadow, owner and releaser roles have distinct grants.

- `[x_monitoring.rs:reserve]` takes a transaction advisory lock per billing scope,
  checks database time, rejects overlapping/reset policies and commits the resource
  bound before network work. Scope-wide totals include every source and rule.
- `[x_monitoring.rs:settle]` is idempotent for identical results; uncertain calls
  retain their bound and resource overruns block new reservations. Status remains
  available; successful reconciliation clears the block
  (`[x_monitoring.rs:status]`, `[x_monitoring.rs:reconcile]`). `[x_monitoring.rs:reconcile]`
  cannot regress provider totals/time. Its floor includes later and uncertain work.
- `[research.rs:export_x_evidence]` excludes records ingested or fetched after
  the cutoff and excludes denied/expired/unavailable/superseded evidence. It sets
  training permission false and does not fabricate verification/feature-ready times.
  `[research.rs:commit_page]` records availability only after the content transaction
  commits, guarded by its page revision. Missing markers stay unavailable to export.
  The SQL row limit is applied before permission filtering, so export can return
  fewer eligible records than requested (`[research.rs:export_x_evidence]`).

- `[transport.rs:uses_tls]` permits plaintext TCP only for exact `127.0.0.1`
  or `::1` hosts; explicit Unix socket hosts also qualify. Every `hostaddr`
  override must be loopback. Remote connections require
  `sslmode=require`, TLS 1.2 or later, platform CA trust and hostname verification.
- `[lib.rs:verify_role]` checks session/current role, database, elevated attributes
  and owner membership. `[lib.rs:migrate]` requires `trady_migrator` on `trady`.
- `[lib.rs:save_reconciliation]` and `reserve_paper_intent` validate domain inputs
  before calling SQL. Stored intents are deserialized into the domain type.
- `[../../../migrations/0001_foundation.sql:execution.assert_fence]` locks the account
  lease and rejects expired, mismatched or null ownership/generation arguments.
  Reconciliation and reservations use that common guard.
- The migration functions require fresh flat reconciliation before reserving a new
  paper fixture, preserve one intent per account/session and deduplicate stable IDs.
- `[research.rs:PgResearchRepository::connect]` and
  `[sessions.rs:PgSessionRepository::connect]` require the restricted
  `trady_analyst` role. Analyst research/session metadata does not authorize
  financial mutations; executor operations remain behind the execution port and
  its narrowly granted database functions.
- `[research.rs:commit_page]` locks source state and compares the durable revision
  with `expected_revision` before incrementing it. A page contains at most 100
  items, its cursor is bounded, and each item's rights must match the source-state
  rights. Item checks cover source identity, hash shape, timestamps and bounded
  content; storage does not recompute content hashes or establish evidence
  provenance (`[research.rs:validate_item]`).
- `[research.rs:commit_page]` takes sorted transaction advisory locks for X edit
  roots before resolving versions across sources. Concurrent lookup/timeline
  ingestion cannot leave an older known version available after its newer edit.
- `[research.rs:read_items]` accepts limits 1–100; it and `read_item` intersect item content/model
  permissions with current source permissions when present, and applies domain
  redaction. It performs a global content purge before the optional source-filtered
  read. `[research.rs:purge]` redacts expired content and content whose source has
  disabled storage, including historical revisions. Shortened source retention
  reduces existing current/history deadlines using original first-seen time.
- `[research.rs:reserve_shared_requests]` reserves shared provider credits by
  credential account. Initial minute/day ceilings are 5/25 for Alpha Vantage,
  20/200 for Finnhub and 5/200 for GDELT; source/account limits may tighten them.
  Disabled accounts reject reservations. `[research.rs:commit_page]` persists
  provider retry deadlines alongside applicable collection results.
- `[sessions.rs:create]` validates scope/limits and requires schema version 1 and
  revision 0. `[sessions.rs:decode]` rejects a mismatched document scope/version;
  `load`, `list` and `acquire` select owner/account/policy. `list` returns at most
  100 documents.
- `[sessions.rs:acquire]` accepts TTLs 1–300 seconds and only takes expired leases,
  using `clock_timestamp()` and incrementing the generation. `[sessions.rs:save]`
  checks session/owner/account, revision, worker, generation and unexpired database
  lease, writes the next revision, and renews expiry by `lease.ttl_seconds`.
  `[sessions.rs:release]` expires the matching session/worker/generation lease.
  These are model-session fences, separate from account execution fences.
  `[sessions.rs:PgSessionRepository::connect]` sets statement and lock timeouts
  to five seconds. `[transport.rs:connect]` supplies a ten-second connection
  timeout only when the DSN does not already specify one.
- `[autonomous.rs:PgTradingJournal::connect]` requires `trady_executor`;
  `[trading_research.rs:PgTradingResearch::connect]` requires `trady_analyst`.
  All use the same checked transport and restricted-role checks.
- `[../../../migrations/0004_autonomous_execution.sql:execution.acquire_trader]`
  binds paper/shadow/live accounts to owner/mode/currency and blocks governing
  configuration changes during an active plan. `execution.save_trader` requires
  a current fence and next revision, nonshrinking orders/session sets, immutable
  existing requests/roles, attempted flags that never revert, matching plan scope and irreversible
  same-proposal exit latch. Shadow attempts are rejected.
- `[trading_research.rs:PgTradingResearch]` uses owner/account/kind/ID keys and
  excludes expired records. `research.put_trading_record` requires object JSON
  at most 8 MiB, future retention and immutable payload on replay. Identical retries
  may shorten retention, and cannot extend it.
  Calculations check their content/input hashes and account/market/deadline
  binding. Entry contexts join the actual calculation and require matching
  entry/stop/target/currency/deadline/FX, a successful screening candidate for that
  instrument/market, a current company-profile record and scoped, current feature
  evidence. Loads recheck these references. Position references resolve a current
  executor-attested holding, actual cost basis, active stop/deadline and current
  exit charges/FX; stale, scoped or exiting positions are rejected.
  Holding advice acquisition admits the owner's policy target domain independently
  of the original entry confidence. Targets must be positive-return, tick-aligned
  prices no higher than `target_price` for the nominal policy ceiling; unavoidable
  upward tick rounding cannot itself invalidate that ceiling. Final activation
  separately checks the explicit nominal fraction against fresh research confidence.
  Assessments preserve persisted context and reviews require an existing
  assessment. Metadata consistency does not authenticate source truth or approve
  external financial-validation reports; formula replay belongs to the application.

## Side Effects

Financial collection/snapshot calls write migration 9's research-only tables;
reads can persist manifest references and purge expired/revoked payloads.
`[financial.rs:PgFinancialRepository::sync_registry]` disables omitted sources
and accounts. These operations perform no broker request or model promotion.

`[research_jobs.rs:PgResearchJobs]` writes migration 7's information queues, scope policies, immutable
packets/assessments, dependency invalidations, gateway receipts and outbox. It
performs no HTTP or broker operation. The owner-control result trigger writes
notifications in the same transaction as status changes. Source dependency and
invalidation locks participate in an executor's atomic sentiment-reference check.
Scheduling can persist partially because enqueue and schedule updates are separate
operations (`[research_jobs.rs:schedule]`). Holding watches distinguish unknown or
stale observations (`None`) from an explicitly empty current holding set
(`Some(empty)`); obsolete watchdog jobs are cancelled by holding revision
(`[research_jobs.rs:holding_watches]`, `[research_jobs.rs:cancel_closed_watchdogs]`).

`[weekly.rs:PgWeeklyStore]` and `[weekly.rs:PgReleaseStore]` call transactional security-definer functions in schemas
`weekly`/`release`. These write leases, campaign/trial/holdout ledgers, immutable
forecast/evidence records and release audit events. They never access a broker.

`[x_monitoring.rs:PgXCostRepository]` opens analyst connections and writes
`research.x_budget_cycles`, `research.x_cost_reservations` and
`research.x_cost_reconciliations`. `[research.rs:commit_page]` records source coverage;
the migration's `research.touch_source_ingestion` trigger clears availability on
content changes, and `commit_page` subsequently records a time after content commit.
`export_x_evidence` applies the existing content purge and reads local PostgreSQL.

Opens connections and reads/writes PostgreSQL. Migration changes database objects
and grants transactionally. Database errors expose SQLSTATE and the server message;
transport errors use a generic message without DSNs (`[lib.rs:db]`). Session SQL
errors use the generic `session database operation failed`
(`[sessions.rs:session_db]`); financial and research-job operations use their own
generic database errors (`[financial.rs:db]`, `[research_jobs.rs:db]`).

`[research.rs:state]` can insert default source state. `[research.rs:commit_page]`
transactionally writes `research.source_states`, `research.source_items`,
`research.source_item_revisions`, `research.source_invalidations` and
`research.collection_events`. Stable item IDs deduplicate replay. Changed hashes
archive the previous item and record `CONTENT_CHANGED`; status changes also
invalidate unchanged hashes. Social edit history can redact superseded content
and record `SUPERSEDED`. `mark_x_unavailable` records `UNAVAILABLE` for known
posts and dependent observations, removes their unavailable quotations from current
and historical content, and never fabricates a missing post. Existing first-seen time and earlier expiry are preserved, and
previously superseded items stay superseded. Purging uses
`research.redact_item` for current and revision content.

`[sessions.rs:PgSessionRepository]` reads/writes `research.model_sessions`, storing
the complete `Session` JSON, updating revisions and lease timestamps. Its Rust
implementation performs no separate financial, evidence, budget or audit-table
writes. Session history/audit metadata remains part of the document.

`[autonomous.rs:PgTradingJournal]` invokes executor SQL functions to acquire/renew/
release leases and load/save `execution.autonomous_accounts`. Fenced state changes
also write audit and notification-outbox records through migration 4 functions;
the runtime role has no direct financial-table mutation grant.

`[trading_research.rs:PgTradingResearch]` writes `research.trading_records`.
Dossier/individual-evidence writes are separate statements and can persist
partially. Screening retention follows universe fetch time plus source retention,
or one hour from attempt time without a universe. Assessments inherit the stored
context's retention deadline; reviews inherit the stored assessment's deadline.
The runtime bounds context retention by packet and universe-source rights. `purge_expired`
deletes scoped expired trading records and queued-update payload copies using
database time. `connect_retention` uses a separate connection with a 10-second
statement timeout and 5-second lock timeout. Hash-only
`research.holding_update_identities` prevent reusing deleted update identities.
Queueing locks its assessment, review and dossier rows while deriving the earliest
retention; source retention reductions propagate to queued copies, and source
deletion removes them transactionally.
Publication creates source MVCC versions without extending retention, so an older
repeatable-read writer must serialize/retry before shortening or deleting a source.
`[autonomous.rs:PgTradingJournal::synchronize_owner_controls]` publishes reconciled
context, applies confirmed owner controls and retains pause/close latches across
restarts. Deployment pauses override resume. The SQL close-completion predicate
checks a null context instrument, a JSON-null stored plan and terminal outstanding
orders; it does not directly inspect snapshot positions
(`[../../../migrations/0008_verified_holdings.sql:execution.synchronize_owner_controls]`).
`[holding_research.rs:queue_holding_update]` writes immutable
`research.holding_updates`; activation remains an executor decision.

## Verification

`cargo test -p trady-storage --test holding_economics --locked` covers policy-domain
advice acquisition, the EUR 205 / quantity 2 rounded 1% boundary, overshoot,
changed quantity/cost/fees, stale observations and profitable/current FX stops.
These TEST ONLY fixtures require no PostgreSQL server or financial model approval.

The guarded `tests/research_jobs.rs` target covers durable queue replay, budget,
leases, namespaces, actual FinBERT client wire correlation against a local fixture,
rights invalidation, confirmations and control-result notifications. Run it through
`python3 scripts/test-postgres.py`; the fixture starts no real model and sends no
Telegram messages.

`tests/weekly_durable.rs` runs only through the disposable PostgreSQL harness.
It covers restart/concurrency, dataset budget reuse, monotonic checkpoints,
interrupted holdouts, role separation and evidence tampering.

Run `cargo test -p trady-storage --locked`; use `python3 scripts/test-postgres.py`
for real isolated PostgreSQL checks, including privilege and authentication boundaries.
Run `cargo test --workspace --locked` for workspace verification. All commands run
from the repository root.

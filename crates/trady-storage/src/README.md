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

- `[intelligence.rs:PgIntelligenceRepository]` implements `IntelligenceRepository`
  with server-assigned ingestion times, immutable/idempotent observations,
  atomic corrections/erasure, licensed rejection retention and conditional
  derived assessment storage. `[lib.rs:OPTIONAL_INTELLIGENCE_SQL]` compiles
  [migration 10](../../../migrations/0010_optional_intelligence.sql), and
  `[lib.rs:FAIL_CLOSED_STORAGE_SQL]` compiles
  [migration 11](../../../migrations/0011_fail_closed_storage.sql); only
  `trady_analyst` receives access to these research tables.

- `[financial.rs:PgFinancialRepository]` implements `FinancialRepository` with
  synchronized source/account records, append-only observation revisions,
  manifests, bounded collection leases and shared credential budget windows.
  Migration 9 is compiled as `[lib.rs:FINANCIAL_DATA_SQL]`.
  Public methods include `connect`, `sync_registry`, `health` and `purge`.
  `[financial.rs:reserve_credits]` is crate-visible and requires its caller to
  provide the transaction; it does not open one itself.

- [research_jobs.rs](./research_jobs.rs): `PgResearchJobs::{connect_gateway,
  connect_analyst, connect_executor}`, durable `ResearchJobRepository` and
  `ResearchInformation`, `publish_verified_packet`, immutable sentiment/readiness
  reads, schedules, `accept_gateway` and outbox delivery. Executor integration uses
  `publish_control_context`, `pending_owner_controls`, `record_control_result` and
  the transaction-local SQL predicate `research.sentiment_reference_current`.
  `publish_report` accepts attributed bounded owner/account reports for gateway
  display. See the [research automation contract](../../trady-adapters/src/research_jobs/README.md).

  `[research_jobs.rs:reference_current]` is a standalone query; an atomic plan
  acceptance check must invoke the SQL predicate inside the accepting transaction.
  `latest_packets` selects/decodes scoped rows without the expiry/dependency
  validation performed by `packet`. `sentiment.read` does not itself call
  `reference_current`.

- `[research_jobs.rs:PgResearchJobs]` also exposes `install_policy`, `get_job`,
  `recent_jobs`, `budget`, `checked_readiness_reference`, `selected_session`,
  `select_session`, `bind_job_session`, `pending_deliveries` and
  `acknowledge_delivery`. `[research_jobs.rs:GatewayDelivery]` carries delivery
  identity, bot/user/chat, scope and text.
- `[intelligence.rs:PgIntelligenceRepository]` implements `register`, `insert`,
  `find`, `candidates`, `has_erased`, `invalidations`, `invalidate`, `reject`,
  `save_assessment` and `purge` for the application repository port.

`[weekly.rs:PgWeeklyStore]` implements fenced `WeeklyStore` persistence;
`PgHoldoutLedger` shares its database session/fence. `PgForecastLedger` records
immutable prospective forecasts, label documents and independently reconciled
exports. `PgReleaseStore` resolves content-addressed candidate/evidence documents
and advances deployment-owned release states. `PgActiveReleaseReader` uses only
the executor DSN to resolve current ACTIVE authority through a read-only function.
Weekly worker TTLs are 1–3,600 seconds and canonical documents are bounded to
4 MiB. Checkpoint persistence and lease renewal are separate calls
(`[weekly.rs:PgWeeklyStore::connect]`, `[weekly.rs:canonical]`,
`[weekly.rs:PgWeeklyStore::checkpoint]`). Checkpoint persistence can succeed
before the subsequent renewal fails. `[weekly.rs:ForecastLabel]`,
`ReconciledExport`, `ReleaseCandidate` and `evidence_envelope` supply the local
label/export/candidate/evidence document interfaces. Their structural and hash
checks do not establish independent provenance; `evidence_envelope` does not
call `ReleaseCandidate::validate`.

`[weekly.rs:PgForecastLedger::label]` locally checks hashes, label version and a
nonfuture observation timestamp; that Rust method does not establish forecast
horizon maturity or validate its arbitrary JSON outcome. Likewise,
`[weekly.rs:ReleaseCandidate::validate]` does not inspect the formula envelope,
and `[weekly.rs:PgReleaseStore::advance]` forwards the requested next state and
evidence keys without a Rust transition whitelist. Additional durable checks
belong to the invoked SQL functions.

- `[x_monitoring.rs:PgXCostRepository]`: `connect`, `status`, `reconcile` and
  `XCostRepository::{reserve,settle}` persist shared resource accounting;
  `[x_monitoring.rs:XReconciliation]` carries a provider total/time/evidence reference.
- `[lib.rs:X_API_MONITORING_SQL]` adds migration 5. `[research.rs:export_x_evidence]`
  exports permitted current X evidence using durable ingestion cutoffs and stored coverage.

- `[lib.rs:ROLES_SQL]`, `FOUNDATION_SQL`, `SESSIONS_SQL`, `RESEARCH_SOURCES_SQL`,
  `AUTONOMOUS_SQL`, `X_API_MONITORING_SQL`, `WEEKLY_LAB_SQL`, `RESEARCH_JOBS_SQL`,
  `VERIFIED_HOLDINGS_SQL`, `FINANCIAL_DATA_SQL`, `OPTIONAL_INTELLIGENCE_SQL`,
  `FAIL_CLOSED_STORAGE_SQL`: embedded SQL.
  `[lib.rs:migrate]` applies versions 1–11 transactionally,
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
  holdings and compare the native assessment before calling the SQL update
  queue, whose guards resolve the review and research dependencies.

## Invariants

- `[intelligence.rs:insert]` rejects Tax payloads, assigns ingestion time from
  PostgreSQL, rejects applicable stored invalidations and requires immutable
  replay. Corrections bind an existing same-feature/entity predecessor.
  `[intelligence.rs:find]` and `candidates` exclude erased/expired documents but
  do not filter invalidations; callers load those separately. Nonerasing
  corrections can leave predecessor documents readable. Erased IDs cannot be
  reused, and erased lineage cannot be imported or marked under a new ID.
  `[intelligence.rs:save_assessment]` rejects Tax and requires
  `entry_authorized=false`, `RESEARCH_ONLY_UNCALIBRATED` and an unerased, unexpired
  observation. Stored retention is clamped to the earlier of the requested and
  observation deadlines. Matching erasing invalidations always reject storage;
  non-erasing invalidations reject only when their time is at or before the
  assessment's `decision_at`.
  `[intelligence.rs:register]` atomically activates the reviewed scope registry,
  retires the previous hash and reconciles rights/retention. A retired hash cannot
  reactivate; an intentional new review must have a fresh revision/hash.
  All content operations refresh database-time expiry/current rights before
  proceeding. Migration 11 additionally enforces the scope, feature, input fields,
  source/lineage projections, payload/evidence references and typed serialization
  hashes at the SQL boundary. SQL owns predecessor validation and the atomic
  correction marker. The `canonical` text and observation JSON are erased
  together; direct SQL erasure/invalidation also removes dependent content.
  `[../../../migrations/0011_fail_closed_storage.sql:research.require_read_committed]`
  requires `READ COMMITTED` at direct SQL registry/observation/assessment/rejection
  insertion, observation erasure/retention update, invalidation, activation and
  refresh boundaries. Other isolation levels raise SQLSTATE `0A000` /
  `STORAGE_REQUIRES_READ_COMMITTED`, including when a refresh finds no scope.
  Normal repository calls use `READ COMMITTED` transactions/autocommit. Roll back
  unsupported transactions and retry at that isolation. Existing-row/advisory
  locks alone do not cover new tombstones, invalidations or children absent from
  an older snapshot; fixed-snapshot writes/cleanup are explicitly unsupported.
  Row policies hide expired/unlicensed content while awaiting physical cleanup.

- `[financial.rs:reserve_credits]` serializes weighted reservations by provider
  credential key across minute/day/month windows, retaining failed/crashed usage.
  Database checks prevent spending above a window ceiling; a trigger rejects
  window rebinding, refunds and ceiling increases. Reducing a configured limit
  below prior charges denies new reservations without refunding usage or failing
  the SQL constraint: the stored ceiling floors at spent usage, while admission
  also checks the caller's lower limit.
  `[financial.rs:check_lease]` rechecks source, account, window and approval.
  `[financial.rs:read_snapshot]` checks current consumer rights, receipt/public/
  database recording cutoffs and coverage. `[financial.rs:finish]` verifies
  observation hashes and appends correction invalidations without rewriting inputs.
  It marks availability after the payload transaction commits; an interrupted
  collection without that marker remains excluded from snapshots
  (`[financial.rs:finish]`). `[financial.rs:health]` reports local stored source
  health with `entry_authorized=false`.
  `[financial.rs:reserve]` uses 90-second collection leases and suppresses work
  under current leases, retry/interval windows or same-registry denial.
  `[financial.rs:read_snapshot_inner]` applies its cap before consumer/coverage
  filtering, so fewer observations can remain and a truncated selection cannot
  establish complete coverage.
  Append-only payload revision and availability-after-commit behavior above
  describe `[financial.rs:finish]`. The grants/guard on
  `[../../../migrations/0009_financial_data.sql:research.financial_observations]`
  permit analyst deletion/reinsertion and independent `recorded_at` updates;
  they do not make the table an append-only database audit log.

- Migration 7 keeps gateway acceptance separate from analyst sessions and
  financial mutation. The SQL enqueue function enforces source/instrument scope,
  coalescing, idempotent payloads and durable per-owner queue/day caps. Lease
  generation and server time fence worker completion/failure. Claim-time failures
  and watchdog cancellations use their own scope/state guards
  (`[research_jobs.rs:check_lease]`, `[research_jobs.rs:claim]`,
  `[research_jobs.rs:cancel_closed_watchdogs]`). Packet and assessment
  rows are insert-only; current dependency/readiness checks retain ambiguity.
  Claims use TTLs 1–300 seconds and select attempts below three. Completion
  rechecks the enabled policy; failure deliberately checks the current lease
  without rejecting policy revocation (`[research_jobs.rs:fail]`).
  `invalidate_dependencies` examines at most ten recent packets and invalidates
  on loading errors. `queue_pending_sentiment` considers at most ten eligible
  packets and returns successful enqueues while suppressing individual failures.
- Gateway owner controls bind authenticated bot/user/chat/account/mode/policy.
  Resume/close additionally bind exact plan/holding/quantity and require hashed single-use expiring
  confirmations; pause may request reduced activity immediately. The executor
  independently rechecks binding and records its durable result. Status changes
  enqueue result notifications atomically; acknowledgements do not prove broker
  submission or a fill.
  Matching `[research_jobs.rs:accept_gateway]` receipt replay returns its cached
  response before policy revalidation. Pause construction clears plan/holding/
  quantity fields; resume/close confirmations retain those exact bindings.

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
  A denied reservation can still commit a new billing cycle and policy fingerprint.
  `settle` derives the persisted outcome from actual resource bounds. `status`
  reports policy mismatch through `policy_matches`, identifies local estimates
  as distinct from a final invoice and sets `entry_authorized=false`.
- `[research.rs:export_x_evidence]` excludes records ingested or fetched after
  the cutoff and excludes denied/expired/unavailable/superseded evidence. It sets
  training permission and `issuer_mapping_verified` false and does not fabricate
  verification/feature-ready times.
  `[research.rs:commit_page]` records availability only after the content transaction
  commits, guarded by its page revision. Missing markers stay unavailable to export.
  The SQL row limit is applied before permission filtering, so export can return
  fewer eligible records than requested (`[research.rs:export_x_evidence]`).

- `[transport.rs:uses_tls]` permits plaintext TCP only for exact `127.0.0.1`
  or `::1` hosts; explicit Unix socket hosts also qualify. Plaintext eligibility
  requires every `hostaddr` override to be loopback. Remote connections require
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
  evidence. Context loads recheck these references. Position references resolve a current
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

`[weekly.rs:PgWeeklyStore::release]` clears its generation but retains its bound
run ID; a store instance cannot switch runs after its first successful binding.
`[trading_research.rs:load_assessment]` performs a scoped, unexpired typed lookup;
it does not repeat context-reference validation. `[trading_research.rs:save_review]`
requires an unexpired assessment but does not locally compare the supplied
scenario or validate `checked_at`.

`[research_jobs.rs:select_session]` checks the analyst role, ID syntax and policy;
it does not query session existence or ownership. `[research_jobs.rs:publish_report]`
requires a nonfuture observation and expiry after that observation, permitting an
already-stale report. `[research_jobs.rs:pending_deliveries]` returns at most 50
undelivered rows without claiming a lease, while `acknowledge_delivery` does not
require an affected row. `[research_jobs.rs:claim]` returns without committing its
retry-exhaustion cleanup when no candidate is selected.

`[research_jobs.rs:enqueue]` validates work before invoking
`control.enqueue_information_work`. The SQL function compares the supplied work
hash without recomputing it; its kind comparison does not explicitly reject a
missing `kind`. Migration 7 grants direct analyst job-state UPDATEs and gateway
owner-control status UPDATEs; repository lease/confirmation checks describe the
Rust operation path, rather than guards on every possible SQL write
(`[../../../migrations/0007_research_jobs.sql:control.enqueue_information_work]`).
Retained trading-record payloads are immutable; a purged row has no general
identity tombstone. Permanent hash identities apply specifically to holding
updates (`[../../../migrations/0008_verified_holdings.sql:research.holding_update_identities]`).

`[../../../migrations/0011_fail_closed_storage.sql:research.guard_intelligence_observation]`
checks input keys and canonical/evidence/dependency bindings, rather than
deserializing every nested Rust type, recalculating feature economics or proving
source truth. Registry insertion does not compare `registry_hash` with a document
digest. Intelligence SELECT policies check retention/current rights using the
row's owner/account; callers remain responsible for authenticating end-user scope.

## Side Effects

`[intelligence.rs:register]`, `insert`, `invalidate`, `reject` and `save_assessment`
write `research.intelligence_registries`, `research.intelligence_observations`,
`research.intelligence_invalidations`, `research.intelligence_rejections` and
`research.intelligence_assessments`. Erasure removes matching derived/rejection
rows and nulls observation documents and canonical text while retaining tombstone metadata.
At the supported `READ COMMITTED` isolation, deletion includes dependent rows
committed after an earlier statement in the eraser's transaction. Fixed-snapshot
erasure, invalidation and refresh fail before performing the cleanup.
Registry activation writes `research.intelligence_scopes` and permanent
`research.intelligence_registry_retirements`; rejected stale activation rolls back.
`[intelligence.rs:purge]` operates across scopes, deleting expired assessments/
rejections and nulling expired/unlicensed observation documents using database time.
Historical caller times cannot retain currently expired content. Even `find`,
`candidates`, `invalidations` and `has_erased` can commit cleanup before their
subsequent read. These cleanup side effects survive a later operation's error.
Its database errors use a generic intelligence-specific message.

Financial collection/snapshot calls write migration 9's research-only tables;
reads can persist manifest references and purge expired/revoked payloads.
`[financial.rs:PgFinancialRepository::sync_registry]` disables omitted sources
and accounts. These operations perform no broker request or model promotion.
Registry changes commit before the subsequent purge. `[financial.rs:read]`, the
`FinancialRepository` trait read, purges before opening its snapshot/manifest
transaction; a later error does not undo that earlier work. The crate-visible
`[financial.rs:read_snapshot]` helper opens the snapshot transaction and can
persist a manifest, but does not itself purge
(`[financial.rs:PgFinancialRepository::sync_registry]`).

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
`[research_jobs.rs:schedule]` cancels by active holding ID;
`cancel_closed_watchdogs` separately compares the holding ID and revision.

`[weekly.rs:PgWeeklyStore]` and `[weekly.rs:PgReleaseStore]` call transactional security-definer functions in schemas
`weekly`/`release`. These write leases, campaign/trial/holdout ledgers, immutable
forecast/evidence records and release audit events. They never access a broker.
`[weekly.rs:PgReleaseStore::active_formula]` also invokes `release.current_active`,
so exported formula authority must pass the same current owner/readiness checks
as the executor reader. The newest independent readiness record controls runtime
availability; a newer not-ready record prevents fallback to an older pass.
`release.advance` and the runtime lookup require `READ COMMITTED`; other isolation
levels raise SQLSTATE `0A000` / `STORAGE_REQUIRES_READ_COMMITTED`. Each lookup
checks committed state during the call. Historical `release.resolve` output cannot
substitute for that authority check, and later commits require another lookup.

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
archive the previous item; their invalidation reason is `UNAVAILABLE` or
`SUPERSEDED` for those resulting statuses and `CONTENT_CHANGED` otherwise.
Unavailable/superseded status changes also invalidate unchanged hashes.
Social edit history can redact superseded content
and record `SUPERSEDED`. `mark_x_unavailable` records `UNAVAILABLE` for known
posts and dependent observations, removes their unavailable quotations from current
and historical content, and never fabricates a missing post. `[research.rs:commit_page]`
preserves existing first-seen time, earlier expiry and previously superseded status.
`[research.rs:mark_x_unavailable]` instead assigns `Unavailable` to a matching
post even when it was previously superseded. Purging uses
`research.redact_item` for current and revision content.

`[research.rs:commit_page]` commits content/cursor/events before its separate
availability update, which can fail after that commit. `[research.rs:purge_content]`
also commits before item/export SELECTs; later read or decoding errors retain the
cleanup. `[sessions.rs:acquire]` updates the lease before decoding the returned
document, so decoding can fail after ownership/generation changed.

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
interrupted holdouts, role separation, SQL null/missing-field attacks, runtime
readiness expiry/renewal, stale negative readiness/pointer replacement, isolation
guards and evidence tampering. `tests/financial.rs` covers SQL
credit/status bounds and lowering limits below already charged usage.
`tests/intelligence.rs` covers all nine stored feature envelopes, direct SQL
content/dependency validation, erasure and registry withdrawal/retirement. It
also establishes fixed snapshots before observation/tombstone creation and before
dependent publication, rejects stale writers/erasers, and checks physical deletion
through the disposable administrator after a `READ COMMITTED` retry.

Run `cargo test -p trady-storage --locked`; use `python3 scripts/test-postgres.py`
for real isolated PostgreSQL checks, including privilege and authentication boundaries.
Run `cargo test --workspace --locked` for workspace verification. All commands run
from the repository root.

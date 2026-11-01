# PostgreSQL foundation contract

## Overview

Administrator role provisioning and eleven transactionally applied Trady migrations.
Use the [disposable test runner](../scripts/test-postgres.py) for local verification.

Subsequent migrations add model-session state, research-source persistence and
a separate autonomous execution/research ledger.

## Interface

- [0011_fail_closed_storage.sql](./0011_fail_closed_storage.sql): null-rejecting
  weekly/release entry points, renewable runtime readiness checks, bounded and
  monotonic provider-credit windows, and intelligence validation/erasure triggers.
  `research.intelligence_scopes` selects the current reviewed registry;
  `research.intelligence_registry_retirements` prevents reverting to retired hashes.
  Analyst-only `research.activate_intelligence_registry(text)` activates a fresh
  revision; `research.refresh_intelligence(text DEFAULT NULL,text DEFAULT NULL)`
  reconciles expiry/rights globally or for the supplied owner/account.
  `research.require_read_committed()` rejects unsupported transaction isolation
  with SQLSTATE `0A000` / `STORAGE_REQUIRES_READ_COMMITTED`.
  Observation `canonical` text preserves typed serialization for database content
  and payload hash checks and is erased with `document`.
  Analyst-callable SELECT-policy predicates are
  `research.intelligence_content_visible(text,text,text,timestamptz,boolean)`,
  `research.intelligence_observation_visible(text,text,text,jsonb,timestamptz)` and
  `research.intelligence_marker_visible(text,text,text,jsonb,timestamptz)`, each
  returning boolean (`[0011_fail_closed_storage.sql:GRANT]`). They check current
  registry/retention state and acquire shared locks. Observation tombstones with
  `document IS NULL` remain visible under `intelligence_observations_read`.
  These predicates contain no direct transaction-isolation precondition.

- [0010_optional_intelligence.sql](./0010_optional_intelligence.sql): analyst-only
  `research.intelligence_registries`, `research.intelligence_observations`,
  `research.intelligence_invalidations`, `research.intelligence_assessments` and
  `research.intelligence_rejections`. Observation content is immutable except
  erasure through `research.guard_intelligence_erasure`; marker retention and
  source/lineage dependencies support purge and erasure. Migration 11 adds the
  durable content, dependency, rights and expiry guards described below.
  No executor/gateway/lab/shadow access is granted.

- [0009_financial_data.sql](./0009_financial_data.sql): research-only provider
  accounts, source/registry revisions, shared credit windows/retry deadlines,
  financial collection leases, observation revisions, invalidations and manifests.
  Direct payload access is granted only to `trady_analyst`.
  Tables are `research.financial_accounts`, `research.financial_sources`,
  `research.provider_budget_windows`, `research.financial_registry_revisions`,
  `research.provider_retry_deadlines`, `research.financial_collections`,
  `research.financial_observations`, `research.financial_invalidations` and
  `research.financial_manifests` (`[0009_financial_data.sql:research.financial_accounts]`).

- [0000_lab_roles.sql](./0000_lab_roles.sql): separate lab, verifier, shadow,
  release-owner and releaser roles, provisioned by the administrator.
- [0006_weekly_lab.sql](./0006_weekly_lab.sql): fenced weekly freezes, campaigns,
  trial/holdout budgets, forecast/label/reconciliation ledgers and immutable
  release evidence; `release.current_active` supplies read-only runtime authority.
  Tables are `weekly.budgets`, `weekly.campaigns`, `weekly.runs`, `weekly.trials`,
  `weekly.holdouts`, `weekly.forecasts`, `weekly.labels`, `weekly.reconciliations`,
  `release.candidates`, `release.evidence`, `release.events` and `release.active`
  (`[0006_weekly_lab.sql:weekly.budgets]`).
  Workflow callables include `[0006_weekly_lab.sql:weekly.freeze]`,
  `weekly.load_run(text,bigint) -> jsonb`,
  `weekly.renew(text,bigint,integer) -> void`,
  `weekly.release_run(text,bigint) -> void`,
  `weekly.reserve_trial`, `weekly.checkpoint`, `weekly.claim_holdout`,
  `weekly.finish_holdout`, `weekly.record_ledger`, `weekly.export_ledger`,
  `[0006_weekly_lab.sql:release.candidate]`, `release.record_evidence`,
  `release.resolve`, `release.advance` and `release.current_active`.
  Load, renewal and release check the backend/generation fence; release expires
  the lease. Migration 11's renewal wrapper rejects null TTLs and retains the
  1–3600-second bound (`[0011_fail_closed_storage.sql:weekly.renew]`).
- [0007_research_jobs.sql](./0007_research_jobs.sql): bounded research work,
  verified packets, sentiment references, gateway receipts/outbox, sessions,
  reconciled control contexts and owner requests with kind-specific confirmation rules.
  Tables are `research.job_policies`, `control.research_work`,
  `research.verified_packets`, `research.sentiment_assessments`,
  `research.dependency_invalidations`, `control.gateway_receipts`,
  `control.gateway_outbox`, `control.gateway_sessions`, `research.gateway_reports`,
  `control.owner_control_context` and `control.owner_controls`; the health view is
  `control.research_source_health` (`[0007_research_jobs.sql:control.research_work]`,
  `[0007_research_jobs.sql:control.research_source_health]`).
  `[0007_research_jobs.sql:control.enqueue_information_work]` exposes
  `control.enqueue_information_work(text,text,integer,text,jsonb,text,text,text) -> text`
  to the gateway and analyst, serializing scoped enqueue operations and enforcing
  idempotency and configured work limits.
  `[0007_research_jobs.sql:research.sentiment_reference_current]` exposes
  `research.sentiment_reference_current(jsonb) -> boolean` to the executor and
  analyst, checking persisted scope, expiry, source rights and invalidations
  while taking shared locks on checked evidence rows.
- [0008_verified_holdings.sql](./0008_verified_holdings.sql):
  `execution.holding_observations`, `execution.control_latches`,
  `research.holding_updates`, hash-only `research.holding_update_identities`; `research.verified_holding`,
  `research.queue_holding_update`, `execution.record_holding_observation`,
  `execution.pending_holding_update`, `execution.synchronize_owner_controls`.
  It replaces `research.purge_trading_records` to delete expired queued research
  copies and wraps `execution.save_trader` with transactional sentiment checks.
  Queue validation locks its source rows; a trigger propagates later retention
  reductions and removes queued copies when their source records are deleted.
  Queue publication versions those rows without renewing retention, forcing
  stale repeatable-read writers to retry before they could miss a queued copy.
  The analyst interfaces are `research.verified_holding(text,text,text) -> jsonb`
  and `research.queue_holding_update(text,text,jsonb) -> void`. Executor interfaces
  are `execution.record_holding_observation(text,text,bigint,jsonb) -> void`,
  `execution.pending_holding_update(text,text,bigint) -> jsonb`,
  `execution.synchronize_owner_controls(text,text,bigint,jsonb,text,jsonb) -> jsonb`
  and `execution.save_trader(text,text,bigint,bigint,jsonb) -> void`
  (`[0008_verified_holdings.sql:GRANT]`).
- [0005_x_api_monitoring.sql](./0005_x_api_monitoring.sql):
  `research.x_budget_cycles`, `research.x_cost_reservations` and
  `research.x_cost_reconciliations`; `source_items` gains `ingested_at`,
  `ingestion_revision`, `registry_hash` and `coverage`.
  `research.touch_source_ingestion` clears availability when content changes.

- [0000_roles.sql](./0000_roles.sql): server-wide `trady_owner`, `trady_migrator`,
  `trady_analyst`, `trady_executor` and `trady_gateway` roles for a fresh installation.
- [0004_autonomous_execution.sql](./0004_autonomous_execution.sql):
  `execution.autonomous_accounts`, `research.trading_records`;
  `execution.acquire_trader`, `assert_trader`, `renew_trader`, `load_trader`,
  `save_trader`, `release_trader`; `research.put_trading_record` and
  `purge_trading_records`. This ledger is separate from paper fixture tables.
- [0001_foundation.sql](./0001_foundation.sql): schemas `policy`, `research`,
  `execution`, `control`, `audit`; tables `policy.versions`, `research.runs`,
  `execution.accounts`, `execution.account_leases`, `execution.order_intents`,
  `execution.account_snapshots`, `control.research_jobs`, `control.owner_commands`,
  `control.notification_outbox`, `audit.events`; view `execution.account_summary`.
- [0002_model_sessions.sql](./0002_model_sessions.sql): `research.model_sessions`
  and its `model_sessions_owner_account` index (`[0002_model_sessions.sql:model_sessions_owner_account]`).
- [0003_research_sources.sql](./0003_research_sources.sql): `research.source_states`,
  `research.source_items`, `research.source_item_revisions`,
  `research.collection_events` and `research.source_invalidations`; indexes
  `source_items_recent`, `source_items_lineage` and `source_items_external`
  (`[0003_research_sources.sql:source_items_recent]`,
  `[0003_research_sources.sql:source_items_lineage]`,
  `[0003_research_sources.sql:source_items_external]`).
  `[0003_research_sources.sql:research.redact_item]` is an immutable SQL
  `research.redact_item(jsonb) -> jsonb` value transform with `search_path = pg_catalog`.
- SQL execution functions: `execution.register_paper_account`, `execution.acquire_lease`,
  `execution.assert_fence`, `execution.renew_lease`, `execution.save_reconciliation`,
  `execution.reserve_paper_intent`.
- Gateway functions: `control.claim_notification`, `control.ack_notification`.
- The [migration runner](../crates/trady-storage/src/lib.rs) maintains
  `public.trady_migrations` as `trady_owner` and compares applied source exactly.

The function call shapes below include
`[0001_foundation.sql:execution.register_paper_account]` and
`[0001_foundation.sql:control.ack_notification]`:

- `execution.register_paper_account(text, text) -> void`: account and currency.
- `execution.acquire_lease(text, text, integer) -> bigint`: account, worker and TTL;
  returns the new generation.
- `execution.assert_fence(text, text, bigint) -> void`: account, worker and generation;
  internal fence check with no runtime-role execution grant (`[0001_foundation.sql:GRANT]`).
- `execution.renew_lease(text, text, bigint, integer) -> void`: account, worker,
  generation and TTL.
- `execution.save_reconciliation(text, text, bigint, jsonb) -> void`: account,
  worker, generation and reconciliation document.
- `execution.reserve_paper_intent(text, text, bigint, jsonb) -> boolean`: account,
  worker, generation and intent; true for insertion, false for a matching retry.
- `control.claim_notification(text, integer) -> TABLE(event_key text, payload jsonb,
  claim_generation bigint)`: worker and TTL; returns at most one claimed event.
- `control.ack_notification(text, text, bigint) -> boolean`: worker, event key and
  generation; reports whether an eligible event was acknowledged.

## Invariants

- `[0006_weekly_lab.sql:GRANT]` grants the listed weekly workflow functions,
  ledger export, `release.candidate` and `release.resolve` to `trady_lab`.
  `weekly.record_ledger` is granted to `trady_verifier`, `trady_shadow` and
  `trady_analyst`; `weekly.export_ledger` is also granted to `trady_verifier`.
  `release.record_evidence` is granted to `trady_verifier`, `trady_shadow` and
  `trady_release_owner`; `release.resolve` is also granted to those three roles
  and `trady_releaser`. `release.advance` is granted only to `trady_releaser`,
  and `release.current_active` to `trady_executor`. Migration 11 also grants that
  read-only function to the existing release-reader roles so `active_formula`
  uses the same authority checks. There are no direct weekly/release table grants.
  `[0011_fail_closed_storage.sql:weekly.assert_fence]` rejects null generations;
  the guarded wrappers reject null control arguments and incomplete compared JSON.
  `weekly.finish_holdout` binds required report/run/dataset fields to consumption;
  `weekly.record_ledger` requires scope and chronology fields before insertion.
  Renamed `_v6` implementations are private to the owner; runtime roles cannot
  bypass the wrappers. Fixed definer paths put `pg_temp` last.
- `[0011_fail_closed_storage.sql:provider_credits_within_limit]` enforces
  `0 <= used_credits <= credit_limit`. `research.guard_provider_credit_window`
  prevents rebinding, refunds and ceiling increases within a window.
  Legacy usage above a reduced ceiling is retained with the ceiling set to usage,
  leaving the window exhausted. Collection status is limited to
  `OK`, `PARTIAL`, `STALE`, `ERROR`, `DENIED` and `UNAVAILABLE`.
- `[0011_fail_closed_storage.sql:research.guard_intelligence_observation]`
  binds schema, scope, feature, entity, compact canonical content/payload hashes,
  evidence chronology and exact source/lineage projections. Input field sets
  follow the nine non-tax domain inputs; nested `TAX` feature tags and unknown
  envelope/input fields are rejected. Payload source/reference links must resolve
  the retained evidence. Current reviewed source rights bound retention and reject
  invalidated reimports. Typed application policy calculations remain required.
  Canonical observation text is bounded to 4 MiB; evidence has 1–64 entries,
  each with at most 256 lineage IDs. Registry sources are bounded to 128 and
  `retention_hours` to 1–87840
  (`[0011_fail_closed_storage.sql:research.guard_intelligence_observation]`,
  `[0011_fail_closed_storage.sql:research.guard_intelligence_registry]`,
  `[0011_fail_closed_storage.sql:research.intelligence_retention]`).
- `[0011_fail_closed_storage.sql:research.guard_intelligence_assessment]`
  binds the actual observation, feature/entity/evidence/dependencies, current
  registry, decision/computation chronology and research-only calibration with
  JSON boolean `entry_authorized=false`. Rejection markers require matching scoped
  metadata, source revisions, lineage and current audit-marker rights.
- `[0011_fail_closed_storage.sql:research.activate_intelligence_registry]`
  serializes scope transitions and permanently retires replaced hashes. Repeated
  registration of the current hash is idempotent; a retired hash raises an error.
  Returning to earlier settings requires a fresh reviewed revision/hash.
  Observation erasure IDs cannot be reused even under a new registry. Erased
  lineage also rejects new observation IDs and rejection markers; a reviewed
  replacement may admit newly sourced lineage without reviving erased content.
- `[0011_fail_closed_storage.sql:research.require_read_committed]` requires
  `READ COMMITTED` for intelligence registry/observation/assessment/rejection
  inserts, observation erasure/retention updates, invalidation inserts, registry
  activation and refresh (including an empty sweep). Other isolation levels fail
  before the guarded operation can publish or erase content. Normal Rust
  repository transactions/autocommit use `READ COMMITTED`. Roll back an unsupported
  transaction and retry at that isolation; retrying with another fixed snapshot
  remains unsupported. Advisory locks and locks on existing rows cannot make
  absent invalidations, new tombstones or newly committed children visible in
  `REPEATABLE READ`/`SERIALIZABLE`.
- `[0011_fail_closed_storage.sql:research.apply_intelligence_invalidation]`
  applies observation/source/lineage invalidations in the insertion transaction.
  Deletion and rights revocation require erasure; direct document erasure also
  removes derived/rejection copies. Corrections retain historical input and
  invalidate assessments from their effective time; direct correction inserts
  record that invalidation too. Registry withdrawal, capability expiry and
  shortened retention are reconciled by `research.refresh_intelligence`.
  At the supported isolation, erasure removes committed dependent content in
  the same transaction. Row policies hide expired/unlicensed content before
  physical cleanup and lock checked registry/input rows.
  Tombstone metadata and invalidations remain. Physical cleanup requires a
  repository operation or an explicit sweep; elapsed wall time does not run SQL.

- `[0005_x_api_monitoring.sql:GRANT]` grants cost-table operations and sequences
  only to the analyst, with no broker/execution authority. Monetary counters have
  nonnegative checks; `resource_bound` and `actual_resources` JSON do not have
  equivalent resource-shape/value checks in the migration. Per-scope
  serialization/policy enforcement belongs to
  `[../crates/trady-storage/src/x_monitoring.rs:PgXCostRepository]`.

- `[0008_verified_holdings.sql:execution.record_holding_observation]` checks the
  current executor fence and snapshot, account/mode/currency, an 8 MiB document
  bound and observation age strictly below 60 seconds. Queued updates must fit
  within five minutes and all underlying retention/validity deadlines
  (`[0008_verified_holdings.sql:research.queue_holding_update]`). The
  `position_state NOT IN` comparison in `research.verified_holding` and the
  assessment-validity comparison in `research.queue_holding_update` do not
  independently reject SQL NULL; complete upstream document validation remains
  required. Migration 11 does not replace those two functions.

- Foundation accounts are paper-only. `[0001_foundation.sql:execution.order_intents]` has a
  unique account/session slot; reservations accept prepared buy fixtures only.
- `[0001_foundation.sql:execution.assert_fence]` locks the lease and uses database
  time and null-safe ownership/generation checks. Takeover requires reconciliation.
- `[0001_foundation.sql:execution.save_reconciliation]` enforces account/currency,
  freshness, monotonic observation time and immutable snapshot IDs; required outbox
  and audit writes share its transaction.
- `[0001_foundation.sql:execution.reserve_paper_intent]` requires the exact intent
  field set, positive decimal strings with explicit syntax/value/scale limits,
  identity/state checks, a null broker order ID and a parseable UTC-Z creation time.
  New reservations require fresh, recovered reconciliation and a free session slot.
  Matching existing intents return false before the new-reservation checks; their
  comparison ignores state and broker order ID. IDs and session values use text
  extraction without an explicit JSON-string check; creation time is not age-checked.
- On a fresh installation, runtime roles receive exact grants, no direct execution
  table mutation/deletion, no DDL,
  and no owner membership. Shared command/job producers derive from `session_user`.
  Security-definer functions use fixed paths and default public execution is revoked.
  These files do not audit or remove pre-existing explicit grants or memberships
  (`[0000_roles.sql:CREATE ROLE]`, `[0001_foundation.sql:ALTER DEFAULT PRIVILEGES]`).
  Financial research storage is distinct: `[0009_financial_data.sql:GRANT]`
  permits analyst mutations, including observation/manifest deletion and updates
  to observation `recorded_at`. Observation payload-update restrictions do not
  make complete rows immutable. Expiry cleanup requires an explicit purge unless
  a source deletion removes dependent queued copies
  (`[0008_verified_holdings.sql:research.purge_trading_records]`).
- Notification claims use `SKIP LOCKED`, expiry and increasing generations;
  acknowledgement requires the current unexpired worker/generation.
- Cross-database authentication requires a separate host/proxy rule. The disposable
  fixture demonstrates this; production/Paperless deployment remains unverified.
- `[0001_foundation.sql:execution.acquire_lease]` and
  `[0001_foundation.sql:execution.renew_lease]` require TTLs of 1–300 seconds;
  acquisition requires a valid worker ID and only replaces an expired lease,
  incrementing generation and forcing reconciliation. Notification claim TTLs and
  worker IDs have the same bounds (`[0001_foundation.sql:control.claim_notification]`).
- `[0001_foundation.sql:execution.save_reconciliation]` accepts observations no more
  than 60 seconds old, rejects future/regressing times and allows equal timestamps.
  It does not enforce a complete JSON schema, validate cash amounts or position
  contents, or prove state/position consistency. A missing positions array can pass
  the later nonempty-position check (`[0001_foundation.sql:execution.reserve_paper_intent]`).
  Reservation does not check available cash, policy approval or session authority.
- `[0000_roles.sql:CREATE ROLE]` assumes administrator provisioning on a fresh server;
  it contains no repeat-creation guards. `[0001_foundation.sql:REVOKE]` targets
  database `trady`. Neither SQL file creates that database or wraps itself in a
  transaction. `[../crates/trady-storage/src/lib.rs:migrate]` supplies the transaction,
  owner role and advisory lock; it applies/compares compiled migrations 1 through 11
  in order. It does not apply the role-provisioning file or inspect catalog drift.
- `[0002_model_sessions.sql:research.model_sessions]` constrains session/owner/account
  ID syntax, positive policy version, nonnegative revision/lease generation, a JSON
  object and required identity/scope/revision keys with comparison checks. It does
  not supply lease-management functions, end-user authorization or a complete
  transcript schema. Only `trady_analyst` receives direct SELECT/INSERT/UPDATE grants.
  JSON null values can make comparison checks evaluate to SQL NULL, and scope's
  JSON type is unchecked. There are no scope foreign keys, row-level authorization
  or lease-transition checks; `updated_at` has a default without an update trigger.
  `[0007_research_jobs.sql:model_sessions_account_id_check]` later expands valid
  session account namespaces to paper, shadow and live.
- `[0003_research_sources.sql:research.source_states]` requires a nonnegative
  revision. Source items have a unique `(source_id, external_id)` and a source-state
  foreign key; revisions reference items with `(item_id, content_hash)` identity.
  Collection events have unique `(source_id, revision)` and bounded status values;
  invalidations reference items with a bounded reason and composite identity.
  Research documents have no JSON shape or rights/expiry checks in this migration.
  `[0003_research_sources.sql:GRANT]` gives the analyst SELECT/INSERT/UPDATE on states,
  items and revisions, SELECT/INSERT on events and invalidations, event-sequence
  usage and redaction execution; it grants no execution authority over financial state.

## Side Effects

`[0006_weekly_lab.sql:weekly.reserve_trial]` reserves cumulative budgets.
`[0006_weekly_lab.sql:weekly.claim_holdout]` records transactional consumption;
a persisted unfinished consumption prevents replay. Helper invocation order is
owned by the caller. Release evidence is content-addressed, role-separated and
scope-bound. `[0006_weekly_lab.sql:release.advance]` requires independent evidence, twenty
prospective real calendar sessions, scoped owner approval and fresh readiness.
`[0011_fail_closed_storage.sql:release.advance]` and `release.current_active`
require `READ COMMITTED`; other transaction isolation levels raise SQLSTATE
`0A000` / `STORAGE_REQUIRES_READ_COMMITTED`. The runtime lookup rechecks the
ACTIVE pointer, candidate validity, owner approval and fresh scoped readiness.
The newest verifier readiness record wins, including a not-ready record; expired
or missing fields reject authority. New independent readiness attestations can
renew runtime availability without replacing the immutable activation audit.
The returned evidence projection names that latest readiness record.
The check observes committed state during the lookup, with no promise about
changes committed after its checks. Historical `release.resolve` projections do
not provide present-time runtime authority.
`[0008_verified_holdings.sql:execution.synchronize_owner_controls]` requires the current executor fence and
fresh reconciled snapshot, persists latches and reports results through the
gateway outbox. Sensitive requests must still match their confirmed context.
`PAUSE_ENTRIES` has no confirmation, expiry or current-context-equality requirement;
`RESUME_ENTRIES` and `CLOSE_POSITION` require those checks. Every kind retains
the shared owner/account/policy/request-context checks
(`[0008_verified_holdings.sql:execution.synchronize_owner_controls]`).
Its close-completion predicate checks a null context instrument, a JSON-null
stored plan and terminal outstanding orders; it does not directly inspect the
snapshot position array. Queued holding-update payloads expire with their earliest underlying
record retention or update validity; only immutable identity hashes remain.
The `execution.save_trader` wrapper checks persisted sentiment dependencies for
new plans, target revisions and new entry attempts in the acceptance transaction.
Protective management remains available when those dependencies expire.
`research.verified_holding` grants no analyst write access to execution economics.

`[0005_x_api_monitoring.sql:research.touch_source_ingestion]` clears availability
when any other current-item field changes. Existing rows have unknown availability until refreshed; the collector
records a revision-guarded timestamp after the content transaction commits.
The migration adds cost tables, an index, sequences and analyst grants.

`[0004_autonomous_execution.sql:execution.save_trader]` performs fenced revision
transitions and atomically appends audit/outbox records. Existing order requests,
attempted flags and consumed/loss-paused session sets cannot regress; the same
proposal's exit latch cannot change. New entry reservation independently checks
the daily session slot. Shadow attempts are rejected. Only the executor receives
the narrow execution function grants; no runtime role can directly mutate the table.
`[0004_autonomous_execution.sql:research.put_trading_record]` permits immutable
owner/account/kind/ID-scoped records and retention shortening on identical retries.
The analyst may call research write/purge functions and read those records; it
receives no autonomous financial-write grant. Owner/account scope within this
shared analyst role is enforced by the application adapter, without row-level SQL policies.

Role provisioning changes server-wide roles. Migration creates database objects
and grants. Runtime functions mutate account, queue, audit and notification records.
These files alone perform no operation; role provisioning requires administrator review.

During upgrade, `[0011_fail_closed_storage.sql:provider_credits_within_limit]`
raises any legacy overdrawn ceiling to its charged usage before adding the bound.
The migration initializes `research.intelligence_scopes` from each scope's newest
`recorded_at` registry, breaking ties by descending registry hash, retires the
other hashes and invokes `research.refresh_intelligence`. It adds nullable
observation `canonical` text without backfilling existing content/hashes; new
insertion validation is not retroactive
(`[0011_fail_closed_storage.sql:research.intelligence_observations]`,
`[0011_fail_closed_storage.sql:research.intelligence_registry_retirements]`).

`[0011_fail_closed_storage.sql:research.refresh_intelligence]` erases observation
JSON and its canonical text, deletes dependent assessments/rejections, shortens
retention and records durable invalidations using database time. Registry
activation performs that reconciliation in the same transaction. Row policies
can acquire shared locks or raise serialization errors on stale snapshots;
they do not grant cross-owner application authorization or executor access.

`[0001_foundation.sql:execution.assert_fence]` takes a lease row lock held for the
transaction. Reconciliation and reservation also lock the account and write audit
and outbox rows in the caller's transaction. Reconciliation deduplicates snapshot
and outbox identities but can append another audit event on a retry.
`[0001_foundation.sql:control.claim_notification]` and
`[0001_foundation.sql:control.ack_notification]` update outbox claim/delivery state;
they perform no notification transport.

`[0002_model_sessions.sql:research.model_sessions]` and
`[0003_research_sources.sql:research.source_states]`
add research tables, indexes, an identity sequence and grants.
`[0003_research_sources.sql:research.redact_item]` removes top-level `feed_title`,
`title`, `summary` and `text` keys and expanded social context text from the
returned JSON, preserving superseded or
unavailable status and otherwise selecting metadata-only status. It does not update
stored rows or schedule collection/retention work by itself.
It removes only the named content keys; other fields remain. A present non-array
social context can raise an error (`[0003_research_sources.sql:research.redact_item]`).
Non-object social data becomes JSON null; missing or empty social context becomes
an empty array in an object social value.

## Verification

Run `cargo test --workspace --locked` from the repository root for Rust verification.
The PostgreSQL integration scenario is ignored by that command and needs the
disposable runner below (`[../scripts/test-postgres.py:main]`).

Run `python3 scripts/test-postgres.py` from the repository root. It creates a fresh
development server and a separate application fixture; it never connects to Paperless.
The storage `weekly_durable`, `financial` and `intelligence` suites cover raw SQL
null/missing fields, expiring/renewed runtime readiness, credit constraints,
direct erasure/invalidation, current-registry withdrawals and stale reactivation.
Fixed-snapshot regressions begin before any observation/tombstone or dependent
publication exists, exercise stale erasure/invalidation/refresh, inspect physical
child deletion through the disposable administrator, and reject stale authority
after negative readiness or active-pointer replacement.
Adding this migration or changing storage source requires regenerating the proof
source inventory before producing a new independently verified release.

# PostgreSQL foundation contract

## Overview

Administrator role provisioning and nine transactionally applied Trady migrations.
Use the [disposable test runner](../scripts/test-postgres.py) for local verification.

Subsequent migrations add model-session state, research-source persistence and
a separate autonomous execution/research ledger.

## Interface

- [0009_financial_data.sql](./0009_financial_data.sql): research-only provider
  accounts, source/registry revisions, shared credit windows/retry deadlines,
  financial collection leases, observation revisions, invalidations and manifests.
  Direct payload access is granted only to `trady_analyst`.
  Tables are `research.financial_accounts`, `research.financial_sources`,
  `research.provider_budget_windows`, `research.financial_registry_revisions`,
  `research.provider_retry_deadlines`, `research.financial_collections`,
  `research.financial_observations`, `research.financial_invalidations` and
  `research.financial_manifests` (`[0009_financial_data.sql:CREATE TABLE]`).

- [0000_lab_roles.sql](./0000_lab_roles.sql): separate lab, verifier, shadow,
  release-owner and releaser roles, provisioned by the administrator.
- [0006_weekly_lab.sql](./0006_weekly_lab.sql): fenced weekly freezes, campaigns,
  trial/holdout budgets, forecast/label/reconciliation ledgers and immutable
  release evidence; `release.current_active` is executor-read-only.
  Tables are `weekly.budgets`, `weekly.campaigns`, `weekly.runs`, `weekly.trials`,
  `weekly.holdouts`, `weekly.forecasts`, `weekly.labels`, `weekly.reconciliations`,
  `release.candidates`, `release.evidence`, `release.events` and `release.active`
  (`[0006_weekly_lab.sql:CREATE TABLE]`).
- [0007_research_jobs.sql](./0007_research_jobs.sql): bounded research work,
  verified packets, sentiment references, gateway receipts/outbox, sessions,
  reconciled control contexts and confirmed owner requests.
  Tables are `research.job_policies`, `control.research_work`,
  `research.verified_packets`, `research.sentiment_assessments`,
  `research.dependency_invalidations`, `control.gateway_receipts`,
  `control.gateway_outbox`, `control.gateway_sessions`, `research.gateway_reports`,
  `control.owner_control_context` and `control.owner_controls`; the health view is
  `control.research_source_health` (`[0007_research_jobs.sql:CREATE TABLE]`,
  `[0007_research_jobs.sql:CREATE VIEW]`).
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
  and its `model_sessions_owner_account` index (`[0002_model_sessions.sql:CREATE INDEX]`).
- [0003_research_sources.sql](./0003_research_sources.sql): `research.source_states`,
  `research.source_items`, `research.source_item_revisions`,
  `research.collection_events` and `research.source_invalidations`; indexes
  `source_items_recent`, `source_items_lineage` and `source_items_external`
  (`[0003_research_sources.sql:CREATE INDEX]`).
  `[0003_research_sources.sql:research.redact_item]` is an immutable SQL
  `research.redact_item(jsonb) -> jsonb` value transform with `search_path = pg_catalog`.
- SQL execution functions: `execution.register_paper_account`, `execution.acquire_lease`,
  `execution.assert_fence`, `execution.renew_lease`, `execution.save_reconciliation`,
  `execution.reserve_paper_intent`.
- Gateway functions: `control.claim_notification`, `control.ack_notification`.
- The [migration runner](../crates/trady-storage/src/lib.rs) maintains
  `public.trady_migrations` as `trady_owner` and compares applied source exactly.

The function call shapes below are declared by `[0001_foundation.sql:CREATE FUNCTION]`:

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

- `[0005_x_api_monitoring.sql:GRANT]` grants cost-table operations and sequences
  only to the analyst, with no broker/execution authority. Monetary counters have
  nonnegative checks; `resource_bound` and `actual_resources` JSON do not have
  equivalent resource-shape/value checks in the migration. Per-scope
  serialization/policy enforcement belongs to
  `[../crates/trady-storage/src/x_monitoring.rs:PgXCostRepository]`.

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
  owner role and advisory lock; it applies/compares compiled migrations 1 through 9
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
`[0008_verified_holdings.sql:execution.synchronize_owner_controls]` requires the current executor fence and
fresh reconciled snapshot, persists latches and reports results through the
gateway outbox. Sensitive requests must still match their confirmed context.
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

`[0001_foundation.sql:execution.assert_fence]` takes a lease row lock held for the
transaction. Reconciliation and reservation also lock the account and write audit
and outbox rows in the caller's transaction. Reconciliation deduplicates snapshot
and outbox identities but can append another audit event on a retry.
`[0001_foundation.sql:control.claim_notification]` and
`[0001_foundation.sql:control.ack_notification]` update outbox claim/delivery state;
they perform no notification transport.

`[0002_model_sessions.sql:CREATE TABLE]` and `[0003_research_sources.sql:CREATE TABLE]`
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

Run `cargo test --workspace` from the repository root for Rust verification.
The PostgreSQL integration scenario is ignored by that command and needs the
disposable runner below (`[../scripts/test-postgres.py:main]`).

Run `python3 scripts/test-postgres.py` from the repository root. It creates a fresh
development server and a separate application fixture; it never connects to Paperless.

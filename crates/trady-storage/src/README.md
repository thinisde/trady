# PostgreSQL repository

## Overview

[lib.rs](./lib.rs) implements the execution repository and migration runner with
the synchronous postgres client. [research.rs](./research.rs) persists research
collection state and content; [sessions.rs](./sessions.rs) persists scoped model
sessions. The bounded foundation uses local development transport.

## Interface

- `[lib.rs:ROLES_SQL]`, `FOUNDATION_SQL`, `SESSIONS_SQL`, `RESEARCH_SOURCES_SQL`:
  embedded SQL. `[lib.rs:migrate]` applies migration versions 1–3 transactionally,
  protected by advisory lock `861204733` and exact source comparison;
  `ROLES_SQL` is exposed separately and is not executed by `migrate`.
  Migration 3 covers source state, observations, revisions, collection events,
  invalidations and content redaction.
- `[lib.rs:check_service_database]`: verifies an expected runtime role on `trady`.
- `[lib.rs:PgExecutionRepository]`: `connect`, `register_paper_account` and repository
  methods for leases, intents, snapshots and reconciliation.
- `[research.rs:PgResearchRepository]`: `connect`, `purge_content`, `read_item`,
  `mark_x_unavailable` and
  `ResearchRepository::{state, commit_page, read_items}` for durable source cursors,
  normalized items, content revisions and collection status.
- `[sessions.rs:PgSessionRepository]`: `connect` and
  `SessionRepository::{create, load, list, acquire, save, release}` for owner,
  account and policy-scoped model-session documents. Leases carry the session,
  worker, generation and `ttl_seconds`.

## Invariants

- `[lib.rs:connect_local]` requires explicit loopback/Unix hosts and loopback
  `hostaddr` overrides before using `NoTls`. Remote TLS transport remains future work.
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

## Side Effects

Opens connections and reads/writes PostgreSQL. Migration changes database objects
and grants transactionally. Database errors expose SQLSTATE and the server message;
transport errors use a generic message without DSNs (`[lib.rs:db]`). Session SQL
errors use the generic `session database operation failed`
(`[sessions.rs:session_db]`).

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

## Verification

Run `cargo test -p trady-storage --locked`; use `python3 scripts/test-postgres.py`
for real isolated PostgreSQL checks, including privilege and authentication boundaries.
Run `cargo test --workspace --locked` for workspace verification. All commands run
from the repository root.

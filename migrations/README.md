# PostgreSQL foundation contract

## Overview

Administrator role provisioning and the first transactionally applied Trady migration.
Use the [disposable test runner](../scripts/test-postgres.py) for local verification.

Subsequent migrations add model-session state and research-source persistence.

## Interface

- [0000_roles.sql](./0000_roles.sql): server-wide `trady_owner`, `trady_migrator`,
  `trady_analyst`, `trady_executor` and `trady_gateway` roles for a fresh installation.
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

- Accounts are paper-only. `[0001_foundation.sql:execution.order_intents]` has a
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
- On a fresh installation, runtime roles receive exact grants, no direct financial
  mutation/deletion, no DDL,
  and no owner membership. Shared command/job producers derive from `session_user`.
  Security-definer functions use fixed paths and default public execution is revoked.
  These files do not audit or remove pre-existing explicit grants or memberships
  (`[0000_roles.sql:CREATE ROLE]`, `[0001_foundation.sql:ALTER DEFAULT PRIVILEGES]`).
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
  owner role and advisory lock; it applies/compares compiled migrations 1, 2 and 3
  in order. It does not apply the role-provisioning file or inspect catalog drift.
- `[0002_model_sessions.sql:research.model_sessions]` constrains session/owner/account
  ID syntax, positive policy version, nonnegative revision/lease generation, a JSON
  object and required identity/scope/revision keys with comparison checks. It does
  not supply lease-management functions, end-user authorization or a complete
  transcript schema. Only `trady_analyst` receives direct SELECT/INSERT/UPDATE grants.
  JSON null values can make comparison checks evaluate to SQL NULL, and scope's
  JSON type is unchecked. There are no scope foreign keys, row-level authorization
  or lease-transition checks; `updated_at` has a default without an update trigger.
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

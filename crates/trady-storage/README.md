# Trady PostgreSQL storage

## Overview

Synchronous local-development PostgreSQL persistence behind the application
execution, research and model-session repository ports.

## Subdirectories & Boundaries

- [Source contract](./src/README.md): migration runner, runtime role checks,
  `PgExecutionRepository`, `PgResearchRepository` and `PgSessionRepository`.

## Interface

- `[src/research.rs:PgResearchRepository]`: analyst-only source item/cursor storage,
  atomic revisions/events, retention purge and rights-aware reading.
- [Migration contract](../../migrations/README.md): exact tables, functions and role grants.
- [Cargo target](./Cargo.toml): `trady_storage`. Its database scenario is in `tests/postgres.rs`.

## Invariants

`[src/lib.rs:PgExecutionRepository::connect]` requires the restricted executor role.
Runtime financial mutations use narrow SQL functions with database-time leases and
fencing checks; the executor receives no direct financial-table mutation grants.

`[src/research.rs:PgResearchRepository::connect]` and
`[src/sessions.rs:PgSessionRepository::connect]` require `trady_analyst`.
Research cursors, content, collection events and model-session JSON are analyst
metadata; writing them does not confer executor authority. Research page commits
check durable cursor revisions. Model-session saves check revision, worker,
generation and database-time expiry, then renew the lease for its stored TTL.

`[src/lib.rs:migrate]` transactionally applies versions 1–3 and rejects changes to
previously applied migration text. Connections require explicit loopback or Unix
hosts; remote TLS transport remains unimplemented.

## Side Effects

Database connections and transactional schema/grant changes. Execution SQL
functions persist account/intent/snapshot state and associated audit/outbox records.
Research persistence stores cursors, items, revisions, invalidations and collection
events, including retention/rights redaction. Session persistence stores complete
session documents and renews generation-fenced leases
(`[src/lib.rs:migrate]`, `[src/lib.rs:PgExecutionRepository]`,
`[src/research.rs:PgResearchRepository]`, `[src/sessions.rs:PgSessionRepository]`).

## Verification

Run `cargo test -p trady-storage --locked` for local unit checks. Run
`cargo test --workspace --locked` for the workspace and
`python3 scripts/test-postgres.py` for explicitly ignored database scenarios and
CLI checks against a disposable PostgreSQL container. Run commands from the
repository root.

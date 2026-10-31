# Offline foundation implementation

The later [autonomous implementation](./autonomous.md) supplies separate prediction,
screening and durable execution commands. This file records the historical foundation;
its paper fixtures and bounded service probes remain separate from the new runtime.

The later [prediction-fusion](./v1/prediction-fusion.md) and
[weekly optimization](./v1/weekly-optimization.md) contracts define the next hybrid
prediction/mathematical slice. They supersede LLM-selected final numerical limits
in the target architecture. The [implementation record](./v1/prediction-implementation.md)
reports the tested pure Rust subset and remaining integration work; this historical foundation evidence
does not certify trained forecasts, LEAN backtests, Lean 4 proofs or deployment.

The later [X and news reading slice](./news.md) adds bounded source collection,
PostgreSQL observations/cursors and read-only CLI commands. The evidence in this
file describes the original foundation, rather than full research readiness.

The later [Trading 212 adapter slice](./trading212.md) adds real HTTP information
reads and low-level executor order calls. The evidence below describes the offline
foundation; the separate autonomous runtime owns execution/protection and requires
deployment-owned live release evidence.

This is the implementation record for the first milestone in [index.md](../index.md).
The v1 specifications remain the requirements. This milestone does not enable trading.

The later [OpenCode Go implementation](./opencode/README.md) adds real model
transport, persistent owner dialogue, approved skill loading, context manifests,
compaction and bounded native/JSON tool loops. Its implementation/test evidence
supplements the original offline milestone below.

## Scope and sequence

1. Establish the six Rust package boundaries, versioned paper configuration, checked
   decimal policy, explicit capabilities, fake clock/calendar/broker/quotes, and exact
   machine-readable copies of the normative JSON schemas and worked fixtures.
2. Add PostgreSQL migrations, separate runtime grants, account leases with database
   time and fencing generations, durable paper fixture intents, immutable broker
   snapshots, and a transactional notification outbox.
3. Exercise duplicate reservations, concurrent workers, lost responses, reconnection,
   takeover, stale workers, role isolation, and cross-database authentication in an
   isolated disposable database. Update package contracts from the actual code.

The initial service entry points are bounded `--once` startup probes. They validate
configuration, schemas, and optional role-specific database connections. Full
research readiness, thirteen-skill routing, Perpetual and automated execution/protection
remain future work. OpenCode model transport, approved skill loading and LLM sessions
are now implemented. All entry capabilities remain blocked. Shadow and live
foundation configuration are rejected.

## Verification evidence

Validated with Rust/Cargo 1.93.1 and the disposable `postgres:18.3` fixture. The
workspace pins direct dependency versions and includes `Cargo.lock`. The synchronous
database client supports bounded probes, demonstrations and OpenCode conversations.
Long-running scheduling remains separate from the bounded conversation runtime.

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/verify-assets.py
python3 scripts/test-postgres.py
```

The original foundation suite passed 27 tests. Database scenarios are intentionally ignored by
the default suite and passes separately through the disposable runner. That runner
also validates all three service roles, two migration applications, and the
PostgreSQL CLI demonstration. Asset checks compare both canonical schemas and nine
worked fixtures, validate all 52 skill metadata records and the 11/25 taxonomy,
and read/check the Markdown files and local links.

Prerequisites are Rust 1.93+ with rustfmt/Clippy, Python 3 with the pinned
[PyYAML dependency](../scripts/requirements-dev.txt), and Docker for the database check.
The database runner creates a fresh container, ephemeral data directory, fixture
roles/databases and an isolated second-application database, then removes them.
It never connects to `paperless-db-1` or a broker. Its trust-authentication fixture
is development-only and publishes its port on loopback.

## Working commands

```sh
cargo run --locked --bin trady -- demo
cargo run --locked --bin trady -- analyst --once
cargo run --locked --bin trady -- executor --once
cargo run --locked --bin trady -- gateway --once
cargo run --locked --bin trady -- validate-json model schemas/fixtures/llm-contracts-2.json
```

The demo reserves one paper fixture intent, confirms replay creates no duplicate,
records a missing response as `RECOVERY_REQUIRED`, then reconciles a simulated fill
to `OPEN_UNPROTECTED`. It reports €1 cash remaining from the €50/€49 fixture.
The fixture is fixed test data, with no external order or protection operation.

`trady migrate` reads `TRADY_MIGRATOR_DSN`. `trady demo-postgres` reads
`TRADY_EXECUTOR_DSN` and demonstrates persistence across a connection restart.
Service probes optionally verify their matching `TRADY_ANALYST_DSN`,
`TRADY_EXECUTOR_DSN` or `TRADY_GATEWAY_DSN`; otherwise they report `not_configured`.
Supply secrets through the service environment. The migrator identity must own no
runtime service; runtime roles cannot migrate or assume the owner role.
This foundation accepts loopback/Unix PostgreSQL connections only; remote TLS is
not implemented. Use the disposable runner for automatic local setup rather than
applying administrator provisioning to an existing server.

## Acceptance coverage

Case IDs refer to the [complete behavioral plan](./v1/implementation.md).
The evidence below covers the stated foundation behavior, with the remaining
end-to-end requirements still open.

| Cases | Implemented evidence | Remaining scope |
|---|---|---|
| CAP-01, CAP-02, CAP-04, CAP-05 | Checked linear-cost sizing, €49/€147 budgets, quantity flooring, minimum size, directed FX and stale quote/FX rejection | Real spendable-cash normalization, nonlinear charges and submission-time provider refresh |
| CAP-03 | A holding blocks paper intent reservation; no add-on operation exists | Deposit/cash-event ingestion and live account reconciliation |
| POL-01 through POL-04 | Deterministic confidence, target bands, hard policy limits and reward/risk rejection | Complete evidence readiness, exceptional-catalyst verification and stop/noise/protection checks |
| POL-06 | In-memory deposit-adjusted loss pause stays latched after deposit/rebound | Durable session guard and service enforcement |
| DAT-01 | Ten-second quotes, 60-second account/FX validity and future-data rejection | Licensed feed/venue coverage and real ingestion |
| ORD-01, ORD-03, ORD-04, ORD-08 | One racing paper intent wins; lost response stays unknown; connection restart and expired-worker takeover retain intent; stale/null fences fail | Broker submission, ambiguous external matching and predecessor-request recovery |
| ORD-05, ORD-07, ORD-09 | Partial fill observations, snapshot/outbox deduplication, immutable snapshots and inconsistent/manual holding recovery | Actual fill ledger, protective orders, cancellations, exits and corporate actions |
| LLM-01, LLM-02 | Strict schemas, duplicate keys, application envelopes; OpenCode transport, persistent follow-ups, scoped tool schemas/correlation, context manifests, compaction and idempotent recovery | Full evidence/readiness resolution, complete research tool routing and paid-provider acceptance scenarios |
| DB-01, DB-02 | Restricted grants, database-bound queue producers, no runtime DDL or direct financial-table mutation, cross-database rejection and separate application fixture access | Reviewed production authentication/network boundary and actual Paperless coexistence |
| DB-03 | Durable state reconnect, lease takeover, atomic intent/outbox writes and leased notification replay | Backup/restore drill, database outage behavior with real native protection and complete owned job workers |

## Contracts and known gaps

Model-facing JSON validation returns an opaque `ValidatedDocument`, never an
executable order. Full reference resolution, the eight readiness checks, approved
thirteen-skill routing and Perpetual matching are not implemented. Startup always
reports those entry blockers. SQL's `reserve_paper_intent` is an explicit fixture
operation constrained to paper accounts; it is not the future validated entry use case.

The canonical research object uses schema version 2; protocol and proposal-body
versions remain 1. The drift review found and corrected a stale version instruction
in the [conversation skill](../skills/conversation-and-tools/SKILL.md), advancing it
to 1.0.1 and the catalog bundle to 2.0.1. These source revisions still require a
deployment approval manifest before runtime loading. The validator rejects research
version 1. The previously referenced scoped drift-report artifact is not present
in this checkout; its results cannot be verified from this implementation record.

The `policy.versions`, research/job/command tables establish storage boundaries.
Approved policy activation, authenticated owner ingress, complete job claiming,
notifications, backup recovery and service loops remain open. Model session
persistence and bounded conversation recovery are implemented separately.
The next implementation slice should finish these stage-1/2 semantic and worker
contracts before extending the research/provider stages. All later trading and
release acceptance requirements remain applicable.

## Dependency documentation checked

Current documentation was resolved with `npx ctx7@latest library` before fetching:
[Serde attributes](https://serde.rs/container-attrs.html),
[Rust PostgreSQL configuration](https://docs.rs/postgres/0.19.14/postgres/config/struct.Config.html),
[JSON Schema validation](https://docs.rs/jsonschema/0.60.2/jsonschema/),
[checked decimal operations](https://docs.rs/rust_decimal/1.43.0/rust_decimal/struct.Decimal.html),
[Chrono date parsing](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html), and
[PostgreSQL 18 function security](https://www.postgresql.org/docs/18/sql-createfunction.html).

# Trady command line

## Overview

The `trady` binary wires offline demonstrations, configuration/database probes,
research/model/monitoring commands and a gated autonomous runtime. The typed
Trading 212 snapshot command reads demo/live accounts; the separate autonomous
command can submit/cancel against its explicitly configured demo/live account.

## Subdirectories & Boundaries

- [CLI source contract](./src/README.md): command dispatch, provider wiring and
  runtime entry points under `src/`.
- [FinBERT CLI wiring](./src/finbert/README.md): local sentiment status and
  scoring command boundary.
- [OpenCode conversation CLI](./src/opencode/README.md): model-session command
  wiring and reviewed skill interactions.
- [Telegram command wiring](./src/telegram/README.md): owner monitoring and
  durable command handling.

## Interface

- `[src/main.rs:HELP]` and `[src/main.rs:run]`: bounded analyst/executor/gateway
  probes, demos, `migrate`, `t212 snapshot` and `validate-json` dispatch.
- `[src/entry_demo.rs:run]`: `demo-entry --directory ABSOLUTE_DIRECTORY`
  calculates a synthetic entry and records it through the submission-blocking
  shadow broker; see the [simulation guide](../../docs/autonomous.md#simulated-entry).
- `[src/account.rs:run]`: `account cold-start --config FILE` discovers a demo
  account and writes immutable account, market-data, universe, capability,
  model and formula artifacts. `cold-start capability-evidence --config FILE
  --file FILE` imports an independently reviewed, account-bound capability and
  crosswalk record without promoting support. `cold-start first-order` records
  a single manually gated intent and fails closed until fresh quote, cost,
  protection, cancellation, fill, fee and FX evidence is available; see the
  [cold-start guide](../../docs/cold-start.md). OpenCode review uses bounded
  hash-preserving sections and a finalization turn, so large universe exports
  do not become one oversized provider request.
- `[src/api_health.rs:run]`: credential metadata by default and explicitly enabled,
  bounded provider GET diagnostics; see [health checks](../../docs/api-health.md).
- `[src/intelligence.rs:run]`: offline assessment/replay and analyst-backed
  import/read/invalidate/purge. Tax remains owner-only and ephemeral; see the
  [operating guide](../../docs/v1/features/implementation-report.md).
- `[src/stock_report.rs:run]`: bounded local `research-report FILE [--as-of RFC3339]`
  without provider/database access; see [stock reports](../../docs/stock-reports.md).
- `[src/financial.rs:run]`: `financial status`, `collect --once`, `snapshot`,
  `signals`, `features`, `import`, `health` and `purge` expose the shared
  [financial-data layer](../../docs/financial-data.md). Status is offline;
  database commands use the analyst role and synchronize the complete registry.

- `credentials init|status`: initialize the private central store or inspect
  names/availability; [setup](../../docs/credentials.md).

- [OpenCode commands](./src/opencode/README.md): chat, session management and skill
  manifest candidates; [setup](../../docs/opencode/README.md).
- `[src/news.rs:run]`: reviewed-source collection, news reading, X lookup/identity
  discovery and offline parsing, plus `counts-x`, `budget-x`, `reconcile-x` and
  `export-x`; see the [guide](../../docs/news.md).
- [Telegram commands](./src/telegram/README.md): owner monitoring, durable
  research/dialogue jobs and confirmed executor control requests.
- [Sentiment commands](./src/finbert/README.md): bounded local status checks and
  explicit enabled-provider scoring; preview output grants no entry authority.
- `[src/autonomous.rs:run]`: `autonomous check --config FILE`,
  `autonomous run --config FILE [--once]`, and `autonomous recover-order --config
  FILE --intent ID --broker-order ID`, plus `analyst-service`, `holding` and
  `review-holding`; detailed configuration and effects are in
  the [source contract](./src/README.md).
- `[src/inputs_service.rs:run]`, `[src/model_lab.rs:run]`: reviewed data import,
  loopback HTTP serving, offline model fit/evaluation and approved bundle export.
  `model-lab offline-infer` is diagnostic and reports `deployment_eligible:false`.
- `[src/weekly.rs:run]`, `[src/release.rs:run]`: durable weekly experiments,
  prospective forecast recording and independently gated release transitions.

- [Source contract](./src/README.md): supported commands, environment and effects.
- [Cargo target](./Cargo.toml): executable `trady` at `src/main.rs`.

## Invariants

`[src/main.rs:service]` requires bounded `--once` probes. Service, validation and
demonstration results explicitly report `entry_authorized=false`.
Autonomous execution is a separate deployment path. `[src/autonomous.rs:run]`
requires explicit enabled configuration, migrations 1–11 for database-backed
paths, restricted executor/analyst PostgreSQL access for broker execution,
broker credentials, approved models/formula/skill hashes and configured input/
universe providers. Live requires the separate analyst socket and current
persisted ACTIVE release authority; the shipped
[deployment example](../../config/autonomous.example.json) is disabled and contains
no approved providers or financial artifacts. Check reports
configuration blockers without credential/network/database verification.
`[src/autonomous_shadow.rs:run]` instead uses a read-only broker reader and local
files, with no executor DSN, execution journal or executor lease. Its child
compatibility path requires the analyst DSN; shadow may use a distinct trading
account (`[src/autonomous.rs:RuntimeConfig::validate]`).

## Side Effects

Explicit chat commands perform model HTTP calls. Ordinary chat persists
analyst-owned sessions; enabled intelligence tools require a fresh in-memory
session (`[src/opencode/mod.rs:chat]`).
Session inspection/closure retains transcript history and audit records.
Telegram discover/run resolve credentials and call Telegram; run additionally
locks/writes its local inbox/outbox state and sends authorized messages, with
optional broker information reads. Telegram check only validates local files.

Reads command arguments, environment, configuration/JSON files and time; writes
stdout/stderr. Explicit database commands invoke local storage operations.
`[src/main.rs:trading212_snapshot]` resolves a private key pair through the central loader and
performs broker information GETs.
`[src/account.rs:cold_start]` performs the same bounded read-only broker calls,
imports the configured independent historical dataset and writes private
immutable evidence. `[src/account.rs:first_order]` never retries or infers an
unknown mutation; it records the intent and reports `FIRST_ORDER_BLOCKED` when
the supervised gates are incomplete.
The autonomous runtime separately submits/cancels real demo/live orders, including
in `--once` mode, and persists execution/audit/outbox and analyst research records.
Uncertain attempts require exact-ID observation/recovery rather than automatic
submission replay. Broker execution shutdown attempts lease release; its reported retained
protection flag does not establish actual protection of a newly submitted entry.
Shadow writes immutable local cycle/reservation files and records proposals;
it performs no broker mutation (`[src/autonomous_shadow.rs:run]`).
`demo-entry` writes private fixture observations and a proposal under
`DIRECTORY/simulation-only`. It constructs no credentials or network clients,
creates no fills and reports `SIMULATION_ONLY` with `entry_authorized=false`.

## Verification

Run `cargo test -p trady-cli --locked` and
`cargo run --locked --bin trady -- demo` from the repository root.
Run `cargo test --workspace --locked` for workspace verification.

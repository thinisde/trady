# Trady Rust workspace Blueprint

## System Overview

Six packages implement domain/policy calculations, research, model inference,
automatic screening and durable execution. Provider-neutral ports connect fixtures
or explicit deployed providers to application decisions and PostgreSQL. The CLI
wires information commands and the gated [autonomous runtime](../docs/autonomous.md).

## Interface

`[trady-application/src/harness.rs:AgentHarness]` owns bounded model/tool runs and
context manifests. OpenCode-specific transport/tools and CLI wiring live in their
packages' `src/opencode/` folders; analyst-only session persistence stays in storage.
`[trady-application/src/harness.rs:ModelRequest::session_id]` carries the persisted
conversation ID; `[trady-adapters/src/opencode/llm.rs:LlmProvider::complete]` sends
it in Go's `x-opencode-session` header for each tool round and resumed run.

The [X/news slice](../docs/news.md) adds domain source records, application collection
ports, analyst-only PostgreSQL persistence and source-specific read-only adapters.

The optional [intelligence records](./trady-domain/src/intelligence/README.md)
and [calculators](./trady-policy/src/intelligence/README.md) compose with scoped
[application research](./trady-application/src/README.md), read-only
[feature adapters](./trady-adapters/src/intelligence/README.md) and
[analyst persistence](./trady-storage/src/README.md). The
[CLI](./trady-cli/src/README.md) exposes ten research features: nine can use
research storage, while tax comparisons remain ephemeral and owner-only
(`[trady-application/src/intelligence.rs:assess_ephemeral]`).

The workspace members are declared in [Cargo.toml](../Cargo.toml). Five library
crates export the Rust APIs described below. The CLI is a binary whose private
handlers expose commands, rather than a library API. The separate
[native training example](./trady-adapters/examples/README.md) fits and tunes
research classifiers; it does not register or approve a runtime model.

## Subdirectories & System Boundaries

- [trady-domain](./trady-domain/README.md): validated identifiers and execution,
  financial, research, prediction, screening, sentiment and advisory records.
- [trady-policy](./trady-policy/README.md): checked sizing and fill/exit economics,
  session accounting, holding transitions, formula limits and prediction fusion.
- [trady-application](./trady-application/README.md): provider-neutral ports,
  reconciliation, strict JSON, research/data jobs, synchronous model sessions,
  four-outcome model tooling and fenced autonomous execution.
- [trady-storage](./trady-storage/README.md): synchronous PostgreSQL execution,
  research, financial, session, weekly and release persistence behind those ports.
- [trady-adapters](./trady-adapters/README.md): fixture/memory ports, credential
  loading, bounded provider clients, reviewed input serving, native binary advice,
  separate research training and pinned offline helper execution.
- [trady-cli](./trady-cli/README.md): startup probes, validation, demonstrations
  and the configured autonomous executor with bounded analyst IPC, using a
  separate analyst service for live deployments.

## Global Invariants

Workspace Rust edition is 2024, minimum Rust is 1.93, and unsafe code is forbidden
(`[../Cargo.toml:workspace.package]`, `[../Cargo.toml:workspace.lints]`). Domain and policy
have no database or network dependencies. Foundation configuration accepts paper
accounts only (`[trady-application/src/lib.rs:FoundationConfig::validate]`);
`[trady-application/src/autonomous/mod.rs:TradingConfig]` separately binds demo/live
accounts and deployment evidence.

The binary native advisor and four-outcome prediction model have distinct
feature, label and approval contracts
([native advisor](./trady-adapters/src/perpetual/README.md),
[model lab](./trady-application/src/model_lab/README.md)). A fitted research
candidate supplies neither runtime approval nor broker-entry authority. The
CLI configuration check reports missing model, input, universe, capability and
formula evidence; a successful check still requires runtime verification
(`[trady-cli/src/autonomous.rs:configuration_check]`).

## Global Side Effects

Explicit OpenCode commands perform model HTTP calls and persist scoped sessions.

Application model-lab journal operations persist local artifacts
(`[trady-application/src/model_lab/journal.rs:EvaluationJournal]`). Explicit
intelligence database commands can mutate analyst research rows; the owner tax
comparator does not persist raw tax context.
The separate `[trady-adapters/examples/train_candidate.rs:run]` example performs
CPU fitting/tuning and exclusively writes local claims, models and reports;
interrupted runs can leave partial artifacts.

Explicit news source clients make HTTPS requests; the analyst repository persists
items, cursors and collection events, including content purges during reads.

Policy mutates caller-owned loss guards and returns persistent holding snapshots.
Adapters mutate fixture/memory
state. Explicit storage and CLI operations can access the local database and stdout.
Explicit Trading 212 clients perform broker HTTP operations; mutation belongs only
to the separately constructed executor client and explicitly enabled autonomous CLI.

## System Verification

Run `cargo test --workspace --locked` from the repository root. Database scenarios
are separate: `python3 scripts/test-postgres.py`.
Verify the native trainer separately with
`cargo test --locked -p trady-adapters --example train_candidate`.

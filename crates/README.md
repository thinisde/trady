# Trady Rust workspace Blueprint

## System Overview

Six packages implement the offline foundation. Provider-neutral observations flow
from fake adapters through application reconciliation into memory or PostgreSQL.
The CLI wires these boundaries. A separate Trading 212 executor transport supplies
low-level order calls; the CLI exposes only its information reads.

## Interface

`[trady-application/src/harness.rs:AgentHarness]` owns bounded model/tool runs and
context manifests. OpenCode-specific transport/tools and CLI wiring live in their
packages' `src/opencode/` folders; analyst-only session persistence stays in storage.

The [X/news slice](../docs/news.md) adds domain source records, application collection
ports, analyst-only PostgreSQL persistence and source-specific read-only adapters.

The workspace members are declared in [Cargo.toml](../Cargo.toml). Each package
exports the symbols described in its source contract below.

## Subdirectories & System Boundaries

- [trady-domain](./trady-domain/README.md): identifiers, money and execution observations.
- [trady-policy](./trady-policy/README.md): checked financial calculations and validation.
- [trady-application](./trady-application/README.md): ports, reconciliation and JSON boundaries.
- [trady-storage](./trady-storage/README.md): PostgreSQL migrations and fenced repositories.
- [trady-adapters](./trady-adapters/README.md): fake providers, memory storage and Trading 212 transports.
- [trady-cli](./trady-cli/README.md): startup probes, validation and offline demonstrations.

## Global Invariants

Workspace Rust edition is 2024, minimum Rust is 1.93, and unsafe code is forbidden
(`[Cargo.toml:workspace.package]`, `[Cargo.toml:workspace.lints]`). Domain and policy
have no database or network dependencies. Configuration validation accepts paper
accounts only (`[trady-application/src/lib.rs:FoundationConfig::validate]`).

## Global Side Effects

Explicit OpenCode commands perform model HTTP calls and persist scoped sessions.

Explicit news source clients make HTTPS requests; the analyst repository persists
items, cursors and collection events, including content purges during reads.

Policy has only caller-owned loss-guard mutation. Adapters mutate fixture/memory
state. Explicit storage and CLI operations can access the local database and stdout.
Explicit Trading 212 clients perform broker HTTP operations; mutation belongs only
to the separately constructed executor client.

## Verification

Run `cargo test --workspace --locked` from the repository root. Database scenarios
are separate: `python3 scripts/test-postgres.py`.

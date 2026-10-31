# Trady adapters

## Overview

Fixture providers and shared in-memory execution state for offline checks, plus
explicitly configured provider transports.

## Interface

- `[src/credentials.rs:CredentialStore]`: private named secrets and environment
  overrides shared by the CLI and provider adapters; [setup](../../docs/credentials.md).

- `[src/finbert/mod.rs:FinBertClient]` and `FinBertConfig`: explicitly enabled,
  bounded self-hosted sentiment HTTP adapter; see the [v1 contract](../../docs/v1/finbert/README.md).

- [OpenCode Go contract](./src/opencode/README.md): authenticated model calls and
  approved skill/memory tools, grouped in their own folder.
- `[src/sessions.rs:MemorySessionRepository]`: scoped session fixtures with
  generation/revision fencing.
- [X and news contract](./src/news/README.md): reviewed read-only sources, bounded
  HTTP, X polling/lookup, RSS/Atom and optional Alpha Vantage.

- [Source contract](./src/README.md): clocks, calendar, broker, quotes and memory repository.
- [Cargo target](./Cargo.toml): `trady_adapters`.
- [Trading 212 contract](./src/trading212/README.md): separate information and executor HTTP clients.

## Invariants

`[src/lib.rs:FakeBroker::new]` requires paper snapshots. Memory account registration
requires a `paper_` identifier. `[src/trading212/mod.rs:Trading212Config::validate]`
binds Trading 212 access to an explicit demo/live environment, account and currency.
The low-level order transport grants no policy or deployment authority.

## Side Effects

`[src/opencode/llm.rs:OpenCodeGoProvider]` performs model HTTP calls; runtime tools
read approved skill bundles and attributed in-session memory.

Explicit news collection performs read-only HTTPS requests and consumes source
allowance; credentials are resolved by name through the central loader.

Fixture/shared-memory mutation and wall-clock reads through `SystemClock`.
`Trading212Reader` performs authenticated HTTP reads; the separate
`Trading212Executor` can submit/cancel real orders when called with live credentials.

## Verification

Run `cargo test -p trady-adapters --locked` from the repository root.

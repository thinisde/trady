# Trady command line

## Overview

The `trady` binary wires the offline application, storage and fake adapters.
It also exposes a typed read-only Trading 212 snapshot command for demo/live accounts.

## Interface

- `credentials init|status`: initialize the private central store or inspect
  names/availability; [setup](../../docs/credentials.md).

- [OpenCode commands](./src/opencode/README.md): chat, session management and skill
  manifest candidates; [setup](../../docs/opencode/README.md).
- `[src/news.rs:run]`: reviewed-source collection, news reading, X lookup/identity
  discovery and offline parsing; see the [guide](../../docs/news.md).

- [Source contract](./src/README.md): supported commands, environment and effects.
- [Cargo target](./Cargo.toml): executable `trady` at `src/main.rs`.

## Invariants

`[src/main.rs:service]` requires bounded `--once` probes. Service, validation and
demonstration results explicitly report `entry_authorized=false`.

## Side Effects

Explicit chat commands perform model HTTP calls and persist analyst-owned sessions.
Session inspection/closure retains transcript history and audit records.

Reads command arguments, environment, configuration/JSON files and time; writes
stdout/stderr. Explicit database commands invoke local storage operations.
`[src/main.rs:trading212_snapshot]` resolves a private key pair through the central loader and
performs broker information GETs; no CLI command exposes order mutation.

## Verification

Run `cargo test -p trady-cli --locked` and
`cargo run --locked --bin trady -- demo` from the repository root.

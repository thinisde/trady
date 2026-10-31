# Telegram command wiring

## Overview

This module composes owner authentication, broker observations, Telegram transport
and a durable local reply outbox. See the [operator guide](../../../../docs/telegram.md)
for phone setup and the implemented command subset.

## Interface

- `[mod.rs:HELP]` and `[mod.rs:run]` expose `telegram discover`,
  `telegram check --config FILE` and `telegram run --config FILE [--once]`.
- `[mod.rs:Config]` loads versioned, non-secret access/path/interval/notification
  settings from the [example](../../../../config/telegram.example.json).
- `[mod.rs:discover]` reports pending private sender IDs and timestamps without
  granting access, returning message text or acknowledging updates.
- `[mod.rs:load]` validates the foundation and optional Trading 212 configuration;
  paths resolve relative to the selected Telegram configuration file.
- `[mod.rs:monitor]` composes the [application monitor](../../../trady-application/src/telegram/mod.rs),
  `TelegramClient`, `StateStore` and optional `Trading212Reader`.

## Invariants

- `[mod.rs:Config]` rejects unknown fields during deserialization.
  `[mod.rs:Config::validate]` rejects unsupported versions, empty allowlists/paths,
  invalid numeric private IDs and out-of-range intervals.
- `[mod.rs:read_json]` bounds each configuration input to 65536 bytes.
- `[mod.rs:token]` resolves `TRADY_TELEGRAM_BOT_TOKEN` through the central loader.
  `[mod.rs:monitor]` selects the read-only Trading 212 demo/live credential pair
  from the broker configuration's `environment` and constructs only a reader.
- `[mod.rs:drain]` checks current owner/chat allowlists again before sending a
  queued delivery; revoked recipients are discarded durably.
- `[mod.rs:monitor]` records each handled update before a later poll acknowledges
  it. Delivery retries retain the same durable response and event marker.
- `[mod.rs:monitor]` retries transient loop errors with 1–60 second exponential
  backoff, extended by longer provider retry delays. Startup identity failures,
  permanent API errors and local state failures terminate. `--once` does not retry.
- `[mod.rs:refresh]` obtains optional account observations independently of model
  availability. Commands use the cache; no execution, mode mutation, research job
  or service-supervision port is provided.

## Side Effects

`[mod.rs:run]` reads arguments and configuration and returns JSON for the main CLI
to print. Discovery/run resolve central credentials and call the Telegram HTTPS
API. Run optionally performs broker information GETs, creates/locks a private
local state directory, persists inbox/offset/outbox records, sends authorized
messages and writes sanitized retry errors to stderr. Continuous mode installs a
Ctrl+C flag handler. Check performs no network operation or state write.

`[mod.rs:monitor]` optionally sends startup and material broker-change/failure/
recovery alerts. A permanent send failure retains the FIFO head and stops the
monitor; the operator must resolve that recipient/configuration before restarting.
This local queue is separate from PostgreSQL command and execution notifications.

## Verification

From the repository root, run `cargo test -p trady-cli --locked telegram`.
The [CLI integration tests](../../tests/telegram_cli.rs) exercise relative-path
validation, help coexistence and missing-credential behavior without external calls.

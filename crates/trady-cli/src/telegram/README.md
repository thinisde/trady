# Telegram command wiring

## Overview

This module composes owner authentication, broker observations, Telegram transport
and a durable local reply outbox. See the [operator guide](../../../../docs/telegram.md)
for phone setup and the implemented command subset.

## Interface

- [gateway.rs](./gateway.rs) adds opt-in `expanded_gateway` configuration and
  `telegram worker --config FILE [--once]`. The
  [expanded example](./gateway.example.json) deliberately contains invalid numeric
  placeholders; verify actual owner/chat/bot IDs before deployment. Paths resolve
  relative to that file, so update paths when copying it elsewhere.
- `telegram run` in expanded mode requires only the bot token and
  `TRADY_GATEWAY_DSN`. It does not load broker credentials or require analyst or
  executor credentials, even if a legacy `trading212_config` path is present.
- `telegram worker` separately requires `TRADY_ANALYST_DSN`; it requires source
  credentials only for enabled collectors and model credentials only for actual
  dialogue work. It never requires the bot token, gateway DSN or executor DSN.
- `/ask` and ordinary text queue persistent owner dialogue through `AgentHarness`.
  `/research`, registered X URLs, `/jobs`, `/sources`, `/budget`, `/watchlist`,
  `/policy`, `/sessions` and `/session new|show|use|close` are implemented. Company,
  relations, sector, plan, performance, influencer, watchdog and Perpetual commands
  display attributed published reports where available and explicit unavailable
  results otherwise. Memory administration and mutable policy commands return
  helpful unsupported results; they grant no authority.
- `/pause_entries` (`/pause`) immediately records an entry-pause request.
  `/resume_entries` (`/resume`) and `/close_position` (`/close`) preview the exact
  executor-published context and require `/confirm TOKEN` within two minutes.
  `/cancel-confirm TOKEN` invalidates a preview. Accepted, submitted, partial,
  completed and rejected results are separate durable executor notifications.

- `[mod.rs:HELP]` and `[mod.rs:run]` expose `telegram discover`,
  `telegram check --config FILE` and `telegram run --config FILE [--once]`.
- `[mod.rs:Config]` loads versioned, non-secret access/path/interval/notification
  settings from the [example](../../../../config/telegram.example.json).
- `[mod.rs:discover]` reports pending private sender IDs and timestamps without
  granting access, returning message text or acknowledging updates.
- `[mod.rs:load]` validates the foundation and optional Trading 212 configuration;
  paths resolve relative to the selected Telegram configuration file. It resolves
  the optional `TRADY_TELEGRAM_WHITELIST_USER_ID` through the central loader before
  validating access.
- `[mod.rs:monitor]` composes the [application monitor](../../../trady-application/src/telegram/mod.rs),
  `TelegramClient`, `StateStore` and optional `Trading212Reader`.

## Invariants

- `[gateway.rs:GatewayConfig::validate]` requires a positive bot ID, 1–32 unique
  owner mappings matching both private allowlists, at most 1,000 instruments,
  daily limits 1–1,000, pending limits 1–100, a UTC hour below 24 and watchdog
  intervals 60–3,600 seconds.
- `[mod.rs:load]` preserves the read-only monitoring path below when
  `expanded_gateway` is disabled. Expanded mode validates information
  metadata in separate `paper_`, `shadow_` and `live_` namespaces; mode-changing
  Telegram arguments remain unsupported.
- `[gateway.rs:worker]` provisions reviewed owner/account/source/model scopes.
  Before accepting updates, provision these
  by running the separately credentialed worker. Worker provisioning touches only
  research metadata. Daily research is opt-in by UTC hour; holding watchdogs use
  fresh executor-published holding IDs/revisions and cancel obsolete child work.
  Missing or stale holding observations do not imply a flat account.
- `[gateway.rs:dialogue]` requires a reviewed `skill_approval` and pinned skill directory for new
  dialogue sessions. Model provider errors fail the job; missing skills or model
  credentials never silently choose an unapproved model. Session follow-ups are
  serialized and bind a recovery cursor before model I/O. Recovery requires an
  unexpired `OwnerResearch` session in `Active` or `WaitingOwner` state. An
  unavailable explicit/recovered selection fails; an unavailable implicit
  selection may create a new session. A terminal run after the saved baseline
  reuses its final reply (`[gateway.rs:eligible]`, `[gateway.rs:dialogue]`).
  `[gateway.rs:llm_config]` uses `OpenCodeGoConfig::default()` when configuration
  is omitted; its current model is `deepseek-v4.1-flash`
  (`[../../../trady-adapters/src/opencode/llm.rs:OpenCodeGoConfig::default]`).
- `[../../../trady-storage/src/research_jobs.rs:accept_gateway]` commits gateway
  update receipts and queued jobs/controls together before
  advancing the Telegram offset. Confirmation authentication stores hashes, and
  replayed preview receipts do not reveal the token. The owner-only delivery spool
  temporarily contains the original confirmation message until acknowledgement.
- `[gateway.rs:Gateway::drain]` uses a database outbox and rechecks current private
  allowlists, bot, owner/account/policy scope and enabled research policy before
  delivery. A send followed by a crash before acknowledgement may be delivered
  again; it cannot duplicate the underlying job or owner command.

- `[mod.rs:Config]` rejects unknown fields during deserialization.
  `[mod.rs:Config::validate]` rejects unsupported versions, empty allowlists/paths,
  invalid numeric private IDs and out-of-range intervals.
- `[mod.rs:Config::apply_whitelist]` replaces both access lists with the single
  central whitelist ID when configured. Invalid values fail without echoing
  their contents; missing values retain the configuration lists. Expanded gateway
  owner bindings still must match the effective allowlists.
- `[mod.rs:read_json]` bounds each configuration input to 65536 bytes.
- `[mod.rs:token]` resolves `TRADY_TELEGRAM_BOT_TOKEN` through the central loader.
  `[mod.rs:monitor]` selects the Trading 212 demo/live API key and secret
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
- `[mod.rs:monitor]` refreshes once during startup and subsequently at elapsed-time
  checkpoints in its polling loop. It has no separate refresh worker. The first
  refresh's returned change alert is discarded; notifications can enqueue a
  separate startup status and later observation alerts.

## Side Effects

`[gateway.rs:worker]` connects as the analyst, provisions research metadata and
policy, schedules/claims jobs and dispatches metered collectors, FinBERT or
dialogue. `[gateway.rs:dialogue]` persists model sessions and can invoke the
configured model provider. `[gateway.rs:Gateway::drain]` separately sends
eligible PostgreSQL outbox replies through the gateway identity.

`[mod.rs:run]` reads arguments and configuration and returns JSON for the main CLI
to print. Discovery/run resolve central credentials and call the Telegram HTTPS
API. Run optionally performs broker information GETs, creates/locks a private
local state directory, persists inbox/offset/outbox records, sends authorized
messages and writes sanitized retry errors to stderr. Continuous mode installs a
Ctrl+C flag handler. Check reads the central whitelist setting and performs no
network operation or state write.

`[mod.rs:monitor]` optionally sends startup and material broker-change/failure/
recovery alerts. A permanent send failure retains the FIFO head and stops the
monitor; the operator must resolve that recipient/configuration before restarting.
This local queue is separate from PostgreSQL command and execution notifications.

## Verification

Focused regressions are `cargo test -p trady-cli --locked telegram`,
`cargo test -p trady-application --test research_jobs --locked`, and
`cargo test -p trady-adapters --test research_jobs --locked`. The guarded
`telegram_jobs_cli` integration target uses disposable PostgreSQL and a two-request
loopback model fixture to verify persistent queued follow-ups without gateway,
executor or Telegram credentials. No real model or external send is involved.

From the repository root, run `cargo test -p trady-cli --locked telegram`.
The [CLI integration tests](../../tests/telegram_cli.rs) exercise relative-path
validation, help coexistence and missing-credential behavior without external calls.
Run `cargo test --workspace --locked` for workspace verification.

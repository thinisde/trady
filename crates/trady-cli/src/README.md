# CLI commands and wiring

## Overview

[main.rs](./main.rs) contains private command handlers composing public package boundaries.

## Interface

`[finbert/mod.rs:HELP]` and `[finbert/mod.rs:run]` add `sentiment status --config FILE`
and `sentiment score --config FILE --input FILE`; see the
[component contract](./finbert/README.md). The
[FinBERT guide](../../../docs/v1/finbert/README.md) defines source-item input, optional
confidence features, dormant deployment and the distinction between preview and
future automated plan integration.

`[opencode/mod.rs:run]` adds `chat`, `sessions list/show/close` and `skill-manifest`.
The [OpenCode guide](../../../docs/opencode/README.md) describes the approved bundle,
model configuration, environment credentials, follow-ups, recovery and context files.

`[news.rs:HELP]` and `[news.rs:run]` add `news status`, bounded `news collect --once`,
`news read`, `news x`, `news resolve-x` and offline `news parse-feed`. The [news guide](../../../docs/news.md)
documents flags, registry review, environment references and source coverage limits.

`[main.rs:HELP]` and `[main.rs:run]` define:

```text
trady analyst|executor|gateway --once [--config FILE]
trady demo
trady demo-postgres
trady migrate
trady credentials init|status
trady t212 snapshot --config FILE
trady validate-json model|results|envelope|artifact|artifact-result FILE
```

`--help`, `help` and `-h` display help. `[main.rs:service]` uses
`TRADY_ANALYST_DSN`, `TRADY_EXECUTOR_DSN` or `TRADY_GATEWAY_DSN` when present.
Migration requires `TRADY_MIGRATOR_DSN`; the database demo requires the executor DSN.
All credential names resolve through the [central store](../../../docs/credentials.md)
with environment overrides. `credentials init` creates a blank private file;
`credentials status` prints names and availability without values or network calls.
`[main.rs:trading212_snapshot]` selects `TRADY_T212_DEMO_READ_API_KEY` /
`TRADY_T212_DEMO_READ_API_SECRET` or `TRADY_T212_LIVE_READ_API_KEY` /
`TRADY_T212_LIVE_READ_API_SECRET` from the config's explicit `environment`.
The file contains only a non-secret `Trading212Config`.
See the [Trading 212 guide](../../../docs/trading212.md).

## Invariants

- `[finbert/mod.rs:run]` bounds file reads, requires explicit enabled configuration
  for scoring and always reports `entry_authorized=false`. `status` makes no
  network request. Optional thresholds use supplied features and default policy;
  these are unverified previews, not completed readiness or an active plan update.

- `[opencode/mod.rs:binding]` requires an explicit local owner and validated paper
  account/policy. `[opencode/mod.rs:chat]` loads a reviewed skill manifest, binds
  the configured provider and permits one question/file/resume action.
- `[news.rs:run]` requires explicit source selection for lookup/resolve/offline parsing;
  validates registry/flags; reserves source allowance before provider requests; and
  keeps disabled candidates inactive. Collection/read results grant no entry authority.
- `news read --for-model` applies storage/model-processing rights and retention.
  Unavailable sources return explicit statuses instead of fabricated post/article text.
  `news x` returns persisted, redacted records. Explicit post-unavailability failures
  invalidate known copies and remove stored expanded quotations.

- `[main.rs:service]` validates paper configuration and schemas. Supplied DSNs must
  use the matching restricted role; absent DSNs yield `not_configured`.
- `[main.rs:begin_demo]` uses prepared paper buy fixtures and checks replay behavior.
  `[main.rs:demo_postgres]` closes/reopens its connection and recovers one durable intent.
- `[main.rs:finish_demo]` reports unresolved broker responses and unprotected filled
  positions without submission/protection claims. All entry authority remains blocked.
- `[main.rs:main]` returns a failure exit code on invalid input or operational failure.
- `[main.rs:trading212_snapshot]` permits information reads from an explicitly bound
  demo/live account and reports `entry_authorized=false`. It exposes no order command.

## Side Effects

Explicit `sentiment score` reads files/time and posts permitted normalized text to
the configured FinBERT service. It prints JSON and makes no database or broker writes.

OpenCode chat performs model HTTP calls and analyst-role session writes. Sessions
can be listed/read/closed without model credentials; closure retains audit history.

News commands can perform read-only provider HTTPS calls and analyst-role PostgreSQL
writes/reads/content purges. `status` and offline `parse-feed` need neither credentials
nor database access; input file and response sizes are bounded.

Reads files/environment/time/process ID and prints JSON or errors. The memory demo
mutates memory. The database demo creates a timestamp-named paper account and durable
fixture records. Migration applies database DDL through the dedicated role.
The Trading 212 snapshot command performs authenticated HTTPS GETs without database writes.

## Verification

Run `cargo test -p trady-cli --locked`; the disposable database script also runs all
three role-specific startup probes and the PostgreSQL demonstration.

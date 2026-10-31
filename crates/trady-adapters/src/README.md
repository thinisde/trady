# Adapter boundaries

## Overview

[lib.rs](./lib.rs) implements application ports over explicit fixture data.

## Interface

- `[credentials.rs:CredentialStore]`: bounded strict private-file parsing,
  environment overrides, redacted secret values and metadata-only status.

- `[finbert/mod.rs:FinBertConfig]`, `FinBertClient`: `SentimentProvider` implementation
  for pinned self-hosted CPU classification; [contract](./finbert/README.md).

- [OpenCode Go](./opencode/README.md): provider transport and approved runtime tools;
  [setup](../../../docs/opencode/README.md).
- `[sessions.rs:MemorySessionRepository]`: owner/account/policy-scoped session
  fixtures implementing `SessionRepository`, with generation and revision checks.
- [News source contract](./news/README.md): `SourceClient`, safe HTTPS transport,
  RSS/Atom/news API normalization and X identity/timeline/lookup adapters.

- `[lib.rs:FakeClock]`: shared time, checked `advance`; `[lib.rs:SystemClock]`: UTC wall time.
- `[lib.rs:FakeCalendar]`: lookup from explicitly supplied sessions; absent sessions remain absent.
- `[lib.rs:FakeBroker]`: validated paper snapshot reads/replacement and client-intent lookup.
- `[lib.rs:FakeQuotes]`: fixture quote lookup by instrument.
- `[trading212/mod.rs:Trading212Reader]` and `Trading212Executor`: typed broker
  information and executor-only HTTP operations; see the [provider contract](./trading212/README.md).
- `[lib.rs:MemoryExecutionRepository]`: account registration, leases, reconciliation,
  paper reservations and outbox-count inspection. Clones share process memory.

## Invariants

- `[finbert/mod.rs:FinBertConfig::validate]` requires config v1, a 40-character model
  revision, a 1–60 second timeout, and root HTTPS or loopback/`finbert` HTTP without
  credentials/query/fragment. `[finbert/mod.rs:FinBertClient::new]` requires explicit
  enablement and disables redirects/proxies. `predict` validates bounded requests,
  HTTP 200, a 64 KiB response, exact model/item/hash identity and valid probabilities;
  application callers separately reject truncation and enforce source permissions.

- `[opencode/llm.rs:OpenCodeGoConfig::validate]` requires HTTPS or loopback HTTP;
  credentials stay outside sessions and errors. Native tools are dispatched only
  after complete provider output and local schema validation.
- `[opencode/runtime_tools.rs:SkillBundle::load]` requires exact deployment hashes,
  valid catalog/frontmatter/taxonomy and safe local paths.
- `[lib.rs:FakeBroker::set_snapshot]` preserves account identity and paper mode;
  ambiguous order lookup fails and capabilities remain unknown.
- `[lib.rs:ttl]` bounds leases to 1–300 seconds. Acquisition increments the fencing
  generation, rejects time overflow and requires reconciliation after takeover.
  `[lib.rs:check]` rejects stale/expired fences for writes.
- `[lib.rs:MemoryExecutionRepository::save_reconciliation]` rejects stale/future or
  regressing snapshots, currency changes, reused snapshot IDs with different content,
  and unknown resolution IDs before mutating intents. Outbox keys deduplicate snapshots.
- `[lib.rs:MemoryExecutionRepository::reserve_paper_intent]` supports prepared buy
  fixtures only, requires fresh flat reconciliation in the current generation, and
  blocks a consumed session or unresolved intent. Replays with the same immutable
  intent fields return false; changed fields fail.
- Memory persistence ends when all repository handles are dropped. It is not durable storage.
- `[trading212/http.rs:Http::build]` fixes authenticated production requests to
  HTTPS origins with redirects, proxies and automatic retries disabled.
  `[trading212/mod.rs:Trading212Executor::submit_order]` verifies account/currency
  binding and stock metadata before sending; ambiguous outcomes require recovery.

## Side Effects

Explicit `[finbert/mod.rs:FinBertClient::predict]` posts supplied text to the configured
model service and reads a bounded response; it performs no database or broker writes.

Explicit OpenCode calls perform authenticated model HTTP requests; approved skill
and memory tools read local instruction files or scoped session records.

Clocks, fixture snapshots, account leases, intent state, reconciliation and outbox
entries can change in shared process memory. `SystemClock` reads real UTC time.
The explicitly constructed Trading 212 clients make broker network calls; only
the executor client exposes order mutation.

## Verification

Run `cargo test -p trady-adapters --locked` from the repository root.

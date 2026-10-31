# OpenCode Go conversations

## Overview

Trady's OpenCode Go transport and persistent model/tool harness are implemented.
Provider code and approved tools live under
[adapters/src/opencode](../../crates/trady-adapters/src/opencode/mod.rs), CLI wiring
under [cli/src/opencode](../../crates/trady-cli/src/opencode/mod.rs), and configuration
under [config/opencode](../../config/opencode/README.md). The provider-neutral
[application harness](../../crates/trady-application/src/harness.rs) owns orchestration;
the [session repository](../../crates/trady-storage/src/sessions.rs) owns PostgreSQL persistence.

The current [official OpenCode Go documentation](https://github.com/anomalyco/opencode/blob/dev/services/www/src/docs/content/console/go.mdx)
lists OpenAI-compatible Chat Completions for the open models. The default base URL
is `https://opencode.ai/inference/go/openai/v1`, with model `glm-5.2`. Both are
configurable. Select a Chat Completions model and verify its context capacity when
deploying; Responses-, Anthropic- and Google-native endpoints need separate adapters.

## Interface

Supply `OPENCODE_API_KEY` and the local `TRADY_ANALYST_DSN` through the service
environment or [central credentials file](../credentials.md). The dedicated migrator uses `TRADY_MIGRATOR_DSN`; the existing
[database setup](../foundation.md) and [operations](../v1/operations.md) explain the
restricted roles. The database transport permits loopback/Unix connections.

From the repository root:

```sh
cargo run --locked --bin trady -- migrate
cargo run --locked --bin trady -- skill-manifest skills > /tmp/trady-skills.candidate.json
```

Review the catalog, instruction files and hashes, then deploy the approved manifest
in a controlled location. Generating a candidate does not approve or install it.
This example uses `/tmp/trady-skills.approved.json` for the reviewed copy:

```sh
cargo run --locked --bin trady -- chat \
  --owner local_owner --approval /tmp/trady-skills.approved.json \
  --llm-config config/opencode/llm.json \
  --message "What information do you need to research an issuer?"
```

The result contains `session_id`, `run_id`, `run_status`, canonical `final`, charged
tokens and `entry_authorized: false`. The session ID is printed to stderr before
generation so it survives interrupted commands. `--stream` sends content deltas
to stderr; stdout retains one final JSON result. Provider reasoning continuation
is preserved for replay and is not streamed as the answer.

Use the returned ID for follow-ups:

```sh
cargo run --locked --bin trady -- chat \
  --owner local_owner --session SESSION_ID \
  --approval /tmp/trady-skills.approved.json \
  --llm-config config/opencode/llm.json --message "The issuer is Apple."
cargo run --locked --bin trady -- sessions list --owner local_owner
cargo run --locked --bin trady -- sessions show SESSION_ID --owner local_owner
cargo run --locked --bin trady -- sessions close SESSION_ID --owner local_owner
```

Use `--resume` instead of `--message` for an unfinished run. A crashed worker's
45-second lease must expire before takeover; each successful save renews it.
Unexpired runs continue recorded work. Expired runs record an incomplete outcome
and correlated `CANCELLED` results for pending calls. A later owner message starts
a new run in that session.

`--message-file FILE` accepts a bounded UTF-8 question. New sessions accept one
attributed context object through `--context FILE`: `id`, `source`, `content`,
`as_of` and nullable `expires_at`. Times are RFC3339. This is untrusted memory,
not policy or current financial state. `--config FILE` selects the paper account
and policy. The CLI is a trusted local owner interface; network authentication
belongs to a future gateway integration.

| Enabled tool | Behavior |
| --- | --- |
| `skills.list` | Bounded pages of approved skill metadata |
| `skills.read` | Load an approved named skill into pinned instruction context |
| `memory.search` | Search this session's attributed memory and past final replies |
| `memory.read` | Read one scoped memory/message reference |
| `sessions.summary` | Read the current summary and token accounting |

`ToolExecutor` accepts additional reviewed information adapters with strict
argument/result schemas. Source collectors and broker adapters are not automatically
model tools. Built-in local tools run serially; the normative research/market tool
catalog remains a broader implementation requirement.

## Invariants

- `[AgentHarness::respond]` binds runs to owner, paper account, policy,
  provider/model/endpoint and tool/skill revision. Changed bindings require a new
  reviewed session. Owner research sessions expire after seven inactive days;
  closed/expired sessions stay readable but cannot generate.
- `[OpenCodeGoProvider::complete]` makes authenticated HTTPS requests with redirects
  disabled. Literal loopback HTTP is supported for tests. Credentials are
  resolved through the central loader and never enter session documents or provider error bodies.
- `[OpenCodeGoProvider]` supports native functions and canonical JSON envelopes,
  SSE streaming and full JSON responses. Refusals, truncated output and incomplete
  streams dispatch no tools. Native call IDs and reasoning continuation are retained.
- `[AgentHarness]` validates strict `ModelTurn`/`ToolResults` schemas and duplicate
  keys. Models cannot create envelope IDs, impersonate results, add tools or
  authorize orders. Reused call IDs require identical tool/version/argument hashes.
- `[RunLimits]` defaults to six model attempts, four tool rounds, twelve calls,
  four calls per batch and 120 seconds. Provider I/O is capped at 30 seconds,
  tools at 15 seconds and remaining allowance. One repair/transient retry consumes
  the same budgets; finalization reserves a turn.
- `[Attempt]` reserves estimated input plus maximum output before HTTP. Reported
  actual usage replaces that reservation; uncertain/missing charges stay reserved.
  Per-run/session token ceilings apply. Monetary pricing and global daily quotas
  are not implemented.
- `[ContextManifest]` retains exact request messages, prompt/schema versions and
  hashes, bindings, estimates, summary revisions
  and exclusions. Conservative byte estimates plus 10% margin enforce the smaller
  of the output-reserved context window and a 60% input cap. Governing rules,
  pinned skills, initial attributed context and the current run cannot be trimmed.
- `[SessionSummary]` uses attributed historical reply excerpts. Complete historical
  runs are excluded together; original messages, hashes, tool pairs and attempts
  remain stored. Summaries never replace financial facts.
- `[SkillBundle::load]` checks all 52 files against deployment hashes, frontmatter,
  versions, 11/25 taxonomy mappings, duplicate entries and traversal/symlinks.
  Conversation, uncertainty and memory skills load initially; others load by name.
- `[PgSessionRepository]` compares revision, worker, fencing generation and
  server-time lease expiry on every save. Recovery reuses recorded tool results.

## Side Effects

Explicit chat reads approved files, calls the configured model endpoint and persists
`research.model_sessions`: transcripts, tool records, attempts, usage/reservations,
run transitions, summaries and context manifests. Only the analyst role has session
table grants; executor/gateway cannot read transcripts or change sessions. Closure
retains history. Ctrl-C is cooperative; an in-flight read may take up to its timeout.

Failed/cancelled/limited runs return typed incomplete results without proposals.
LLM commands expose no executor order client, shell/filesystem/SQL tools or Telegram
posting. Full readiness, automatic fresh account/market context, scheduled session
lifecycles and the Telegram gateway remain future work.

## Verification

```sh
cargo test -p trady-adapters --test harness --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/verify-assets.py
python3 scripts/test-postgres.py
```

Tests cover real local HTTP, split SSE calls, native/JSON continuations, malformed
output, retries, limits, cancellation, idempotent recovery, finalization and
compaction. The disposable database runner verifies reconnection, racing workers,
revision/generation fencing, roles and CLI dialogue across separate processes.
No paid OpenCode Go request or external account was used for these checks.

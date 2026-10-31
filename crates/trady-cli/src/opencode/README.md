# OpenCode conversation CLI

## Overview

[mod.rs:HELP and run](./mod.rs#L19) expose the trusted local owner interface for
durable research conversations: skill-manifest generation, chat, and session
management. The module binds owner/account/policy scope and connects the
[application harness](../../../trady-application/src/harness.rs),
[OpenCode adapters](../../../trady-adapters/src/opencode/README.md), and
`PgSessionRepository`.

## Interface

[mod.rs:run](./mod.rs#L95) accepts `command: &str` and `args: &[String]`, returning
`Result<Value>`. Its three commands are:

```text
trady skill-manifest SKILL_ROOT
trady chat --owner OWNER --approval MANIFEST [--session ID] --message TEXT
trady chat --owner OWNER --approval MANIFEST --session ID --resume
trady sessions list --owner OWNER
trady sessions show ID --owner OWNER
trady sessions close ID --owner OWNER
```

- [mod.rs:run, skill-manifest branch](./mod.rs#L97) accepts exactly one root
  path and returns `SkillBundle::candidate` as JSON review data. The caller
  reviews and saves that candidate as the manifest passed to `--approval`.
- [mod.rs:chat](./mod.rs#L165) accepts `--owner`, `--approval`, `--session`,
  `--message`, `--message-file`, `--resume`, `--llm-config`, `--config`,
  `--skills-root`, `--context`, and `--stream`. `--resume` and `--stream` are
  switches; the other options take a value.
- [mod.rs:binding](./mod.rs#L75) uses `--config` or the compile-time embedded
  [config/paper.json](../../../../config/paper.json), validates `FoundationConfig`, and
  forms `Scope` from `--owner`, the configuration's account ID, and policy
  version. [chat](./mod.rs#L184) uses `--llm-config` or
  `OpenCodeGoConfig::default()` and defaults `--skills-root` to `skills`,
  relative to the process working directory.
- [mod.rs:repository](./mod.rs#L69) connects through `TRADY_ANALYST_DSN`.
  [chat](./mod.rs#L198) loads provider credentials with
  `OpenCodeGoProvider::from_env`; the default credential variable is
  `OPENCODE_API_KEY`. The adapter's default endpoint is
  `https://opencode.ai/inference/go/openai/v1/chat/completions`
  ([OpenCodeGoConfig](../../../trady-adapters/src/opencode/llm.rs#L18)).
- [mod.rs:sessions](./mod.rs#L107) accepts `list`, `show ID`, or `close ID`,
  with only `--owner` and `--config` options. `list` returns session metadata;
  `show` serializes the scoped repository load; `close` saves a closed session.
- [mod.rs:chat](./mod.rs#L267) returns `session_id`, last-run `run_id` and
  `run_status`, `final`, `charged_tokens`, and `entry_authorized: false`.
  This module returns structured values and writes progress to stderr.

## Invariants

- [mod.rs:HELP and binding](./mod.rs#L19) define a trusted local owner interface.
  Owner identity comes from the local `--owner` argument, then scope validation;
  model text does not invoke these CLI commands. Repository and harness calls
  receive that bound scope.
- [mod.rs:options](./mod.rs#L21) rejects unknown options, duplicate flags, and
  missing option values. [binding](./mod.rs#L84) requires `--owner`.
- [mod.rs:chat](./mod.rs#L168) requires exactly one of `--message`,
  `--message-file`, or `--resume`. Resume requires `--session`; `--context`
  cannot accompany an existing session ID.
- [mod.rs:chat](./mod.rs#L190) requires `--approval` for every invocation,
  including resume. `SkillBundle::load` checks the reviewed manifest against
  current catalog and listed-file hashes before constructing runtime tools.
  The command performs no interactive approval step
  ([SkillBundle::load](../../../trady-adapters/src/opencode/runtime_tools.rs#L97)).
- [mod.rs:chat](./mod.rs#L203) creates new sessions with provider identity,
  configured run limits, serialized governing configuration, tool revision,
  and the bundle's initial skills. Existing-session continuation passes the
  supplied ID to `AgentHarness::respond`.
- [mod.rs:read](./mod.rs#L59) checks input-file metadata against 262,144 bytes
  before reading text. This reader handles configuration, approval, context,
  and message files. [chat](./mod.rs#L213) decodes initial context as
  `MemoryItem`, requiring a valid ID, nonempty source/content, source at most
  256 bytes, content at most 16,000 bytes, a nonfuture `as_of`, and a future
  expiry when present.
- [mod.rs:chat](./mod.rs#L246) uses `--stream` only to display callback text on
  stderr. HTTP SSE versus full JSON is selected by LLM configuration `stream`;
  native tools versus JSON model-turn envelopes is selected by the pinned
  session's `limits.native_tools`. These are separate settings in the
  [provider and harness contract](../../../trady-adapters/src/opencode/README.md).
- [mod.rs:chat and sessions](./mod.rs#L269) return
  `entry_authorized: false` for chat and successful close responses.

## Side Effects

- [mod.rs:read, run, and chat](./mod.rs#L59) read user-selected input files and
  inspect the local skill root through `SkillBundle`. `skill-manifest` returns
  the candidate value for the caller to save.
- [mod.rs:repository and chat](./mod.rs#L69) read environment configuration,
  connect to PostgreSQL, and pass repository/provider/tools into
  `AgentHarness::respond`. Chat can create and persist sessions and runs and
  cause OpenCode HTTP requests through those dependencies. New-session creation
  occurs before message-file reading and cancellation-handler registration.
- [mod.rs:sessions, close branch](./mod.rs#L139) acquires a repository lease,
  marks the last run cancelled if nonterminal, appends its cancellation event,
  marks the session closed, and saves. Lease release is attempted even when
  saving fails.
- [mod.rs:chat](./mod.rs#L241) installs a Ctrl-C handler that cancels the token
  passed to the harness. It prints `Session: ID` to stderr; with `--stream`, it
  also prints and flushes callback fragments, then adds a newline on success.

## Verification

Run from the repository root:

```sh
cargo test -p trady-cli --locked
```

Conversation CLI integration coverage is in
[tests/opencode.rs](../../tests/opencode.rs).

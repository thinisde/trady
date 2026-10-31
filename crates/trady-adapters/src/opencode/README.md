# OpenCode Go adapters

## Overview

[mod.rs:llm and runtime_tools](./mod.rs#L3) group the OpenCode Go Chat
Completions transport, approval of local skill bundles, and session retrieval
tools. The transport implements `LlmProvider`; the tool collection implements
`ToolExecutor`. Those traits and the conversation lifecycle belong to the
[application harness](../../../trady-application/src/harness.rs).

## Interface

### Chat Completions transport

- [llm.rs:OPENCODE_GO_BASE_URL](./llm.rs#L18) defaults to
  `https://opencode.ai/inference/go/openai/v1`.
  [OpenCodeGoConfig::endpoint](./llm.rs#L76) appends `/chat/completions`, giving
  `https://opencode.ai/inference/go/openai/v1/chat/completions` by default.
- [llm.rs:OpenCodeGoConfig](./llm.rs#L23) exposes `base_url`, `model`,
  `api_key_env`, `stream`, and `limits`, with `validate()` and `endpoint()`.
  Deserialization supplies defaults and rejects unknown fields. Defaults are
  model `glm-5.2`, credential environment variable `OPENCODE_API_KEY`,
  `stream: true`, and `RunLimits::default()`.
- [llm.rs:OpenCodeGoProvider::from_env and new](./llm.rs#L86) construct the
  provider from the central credential loader or an explicit key.
  [LlmProvider::identity](./llm.rs#L325) returns provider `opencode-go`, model,
  and endpoint; [LlmProvider::complete](./llm.rs#L333) sends messages,
  `max_tokens`, and the configured streaming setting.
- [llm.rs:read_json](./llm.rs#L164) reads `choices[0].message` from a full JSON
  response. [read_stream](./llm.rs#L199) assembles SSE `data:` events, assistant
  text, reasoning, indexed native tool-call fragments, and token usage; a
  blank-line-dispatched `[DONE]` event terminates the stream.

HTTP streaming and the model tool protocol are independent.
[LlmProvider::complete](./llm.rs#L346) adds native function definitions only
when `request.native_tools` is true and tools are present; `force_final` sets
`tool_choice` to `none`. Native names use
[ToolSpec::alias](../../../trady-application/src/harness.rs), for example
`skills_list_v1` for `skills.list`. The harness takes `request.native_tools`
from the session's `limits.native_tools`; when false, requests omit those
function definitions and use the harness's JSON envelope protocol.
[AgentHarness::normalize](../../../trady-application/src/harness.rs)
converts native calls into a validated `TOOL_REQUEST`, or validates assistant
content as a model-turn envelope such as `TOOL_REQUEST` or `FINAL`. Envelope
validation belongs to the harness in both full JSON and streaming modes.

### Approved skill bundles and runtime tools

- [runtime_tools.rs:SkillApproval](./runtime_tools.rs#L16) contains
  `schema_version`, `bundle_version`, `catalog_hash`, and a path-to-hash `files`
  map; unknown fields are rejected.
- [runtime_tools.rs:SkillBundle](./runtime_tools.rs#L59) exposes `approval`.
  `candidate(root)` returns review data, `load(root, approval)` verifies a
  supplied approval, and `read(name)` clones a cached `PinnedSkill`.
  [initial_skills](./runtime_tools.rs#L249) loads `conversation-and-tools`,
  `uncertainty-and-incidents`, and `memory-and-review` in that order.
- [runtime_tools.rs:RuntimeTools](./runtime_tools.rs#L261) exposes `bundle` and
  implements `ToolExecutor::{specs, revision, execute}`.
  [revision](./runtime_tools.rs#L335) hashes `runtime-tools-v1`, the approved
  catalog hash, the serialized approved file map, and the serialized tool
  specifications.

[ToolExecutor::specs and execute](./runtime_tools.rs#L270) provide exactly five
tools, each version `1.0.0`:

- `skills.list`: schemas require `offset` from 0 to 52 and `limit` from 1 to 4.
  Returns metadata in skill-name order, descriptions up to 120 characters,
  and nullable `next_offset`.
- `skills.read`: accepts an approved skill `name`; returns name, version,
  hash, and `loaded: true`, and attaches the full pinned skill through
  `ToolOutput::trusted_skill` for the harness to consume.
- `memory.search`: accepts `query` and `limit` from 1 to 4. Performs
  case-insensitive substring matching over unexpired memory and canonical
  `FINAL` replies in the supplied session; returns attributed excerpts up to
  160 characters, with IDs such as `message_<sequence>` for historical replies.
- `memory.read`: accepts an `id` for unexpired session memory or a canonical
  `FINAL` message reference. Returns attributed content up to 800 characters;
  it does not require a preceding search.
- `sessions.summary`: accepts an empty object and returns the session's latest
  summary, or null, plus charged tokens.

## Invariants

- [llm.rs:OpenCodeGoConfig::validate](./llm.rs#L42) validates run limits and
  requires HTTPS, allowing HTTP only for literal loopback addresses. URLs
  cannot contain credentials, queries, or fragments. Model IDs are 1–128
  ASCII letters/digits or `-_.`; environment-variable names are 1–96 uppercase
  ASCII letters/digits or `_`.
- [llm.rs:OpenCodeGoProvider::new](./llm.rs#L97) rejects blank or CR/LF-bearing
  keys, disables redirects, and sets connection/request timeouts of 10/30
  seconds. [complete](./llm.rs#L350) further caps each request at the lesser of
  `request.timeout` and 30 seconds. Credentials remain in private provider
  state; provider failures carry symbolic codes and retryability flags.
- [llm.rs:checked_response](./llm.rs#L132) accepts only `stop` or `tool_calls`
  finish reasons, limits content/reasoning and each call's arguments to
  262,144 bytes, and allows 1–4 native calls with nonempty IDs and names.
  Responses without native calls require nonempty content. Both readers
  reject non-assistant roles and refusals; bodies/streams are limited to
  1,048,576 bytes. Streaming also limits individual lines to 262,144 bytes,
  requires `text/event-stream`, and fails on EOF before `[DONE]`.
- [llm.rs:complete and transport](./llm.rs#L360) mark authentication failures
  nonretryable, rate limits and selected transient HTTP failures retryable,
  and send timeouts/transport errors retryable. These are error classifications;
  the provider delegates retry decisions to its caller.
- [runtime_tools.rs:SkillBundle::load](./runtime_tools.rs#L97) requires approval
  schema version 1 and exact bundle-version, catalog-hash, and listed-file-hash
  equality after inspecting the bundle. `candidate()` returns a review
  candidate; deployment approval is supplied separately.
- [runtime_tools.rs:read_inside and SkillBundle::inspect](./runtime_tools.rs#L64)
  check the root and each catalog-relative path component for symlinks, require
  normal relative path components, and check file metadata against a 64,000-byte
  size limit before reading UTF-8. Approval covers `catalog.yaml` and its listed
  skill files. Inspection requires catalog version 1, 52 unique skill entries,
  GICS `industry_group` taxonomy with 11 sectors and 25 subsectors, valid parent
  mappings, matching frontmatter names/versions, and nonblank descriptions.
- [runtime_tools.rs:ToolExecutor::specs and execute](./runtime_tools.rs#L270)
  declare strict object schemas with required fields and no extra properties;
  `execute` itself does not validate those schemas or clamp pagination limits.
  It checks cancellation/deadline at entry and reads the supplied session.
  Unknown tools return `DENIED`; missing memory returns `UNAVAILABLE`.

## Side Effects

- [llm.rs:OpenCodeGoProvider::from_env](./llm.rs#L87) reads the configured
  credential name through the central loader. [complete](./llm.rs#L350) performs a blocking bearer-
  authenticated HTTP POST containing the model request.
- [llm.rs:read_json and read_stream](./llm.rs#L164) invoke `on_text` for
  assistant content. Full JSON invokes it after validation; streaming invokes
  it immediately for text fragments, which can precede a later failure.
  Reasoning is accumulated without that callback. Cancellation is checked
  before sending and before each streaming line read.
- [runtime_tools.rs:SkillBundle::candidate and load](./runtime_tools.rs#L92)
  read catalog/skill metadata and file contents. Runtime tool execution reads
  cached skills and borrowed session data and returns `ToolOutput`; the harness
  consumes any attached trusted skill.

## Verification

Run from the repository root:

```sh
cargo test -p trady-adapters --test harness --locked
```

The integration target is [tests/harness.rs](../../tests/harness.rs).

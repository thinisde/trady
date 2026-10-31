# OpenCode configuration

## Overview

Non-secret configuration for the OpenCode Go conversation runtime.

## Interface

- [llm.json](./llm.json): `OpenCodeGoConfig` base URL, model, credential environment
  variable name, streaming switch and `RunLimits`.
- [Usage and setup](../../docs/opencode/README.md): migrations, reviewed skill
  manifests, chat, follow-ups and session inspection.

## Invariants

`[../../crates/trady-adapters/src/opencode/llm.rs:OpenCodeGoConfig::validate]`
requires HTTPS or literal loopback HTTP, a model ID without provider prefix,
bounded capacities/timeouts and a credential environment variable name.
The file contains no credential value. Verify the model's context window;
`native_tools: false` selects canonical JSON envelopes.

## Side Effects

None from the file alone. Explicit `trady chat --llm-config` calls read it and
use credentials supplied through the environment or
[central credentials store](../../docs/credentials.md).

## Verification

Run `cargo test -p trady-adapters --test harness --locked` and
`python3 scripts/test-postgres.py` from the repository root.

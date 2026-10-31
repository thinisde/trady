# OpenCode Go credentials

## OPENCODE_API_KEY

This is the provider-issued key used by Trady's OpenCode Go model transport.

1. Sign in to the [OpenCode Console](https://opencode.ai/console).
2. Follow the [official Go setup](https://opencode.ai/v2/docs/console/go/):
   select the appropriate workspace, activate a Go plan and complete its billing
   setup. Check current plans and usage limits in the console.
3. Copy the API key supplied by the console.
4. Put the raw value in `OPENCODE_API_KEY` in
   `config/credentials.local.json`, following the [store setup](../README.md).

Trady reads the name selected by `api_key_env` in
[the model configuration](../../opencode/llm.json); its default is
`OPENCODE_API_KEY`. Select a model supported by the configured endpoint and your
Go access. Trady loads the key directly from the central store; OpenCode's TUI
connection setup is not required for this Rust client.

The key enables provider authentication. Durable conversations also require
[`TRADY_ANALYST_DSN`](../postgresql/README.md). See the
[conversation guide](../../../docs/opencode/README.md) for chat and session setup.

For replacement, issue a new key through the console, update the stored value and
restart clients using it.

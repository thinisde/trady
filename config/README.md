# Deployment and fixture configuration

## Overview

Explicit paper defaults, non-secret broker examples and disposable database authentication fixtures.

Secret values use the [central credentials store](../docs/credentials.md), separate
from provider configuration. `credentials init` creates the ignored private
`credentials.local.json`; environment variables override its entries.

## Interface

- [credentials.example.json](./credentials.example.json): blank named entries for
  API keys, separate T212 demo/live read pairs, shared secrets, hashing/signing keys
  and service database credentials.
  Keep the committed example blank; edit only the private initialized copy.

- [FinBERT](./finbert/README.md): disabled `FinBertConfig` example,
  localhost endpoint, pinned model revision and bounded timeout. The
  [FinBERT guide](../docs/v1/finbert/README.md) documents explicit later activation.

- [Telegram monitoring](./telegram.example.json): non-secret polling, private
  owner/chat allowlists, relative paths and local outbox configuration. The empty
  allowlists require explicit owner setup; see the [guide](../docs/telegram.md).
- [OpenCode Go](./opencode/README.md): grouped LLM configuration and runtime limits.
- [research.json](./research.json): version 1 reviewed-source registry consumed by
  `[../crates/trady-application/src/research.rs:ResearchConfig]`. All sources ship
  disabled, without guessed X IDs or content/model-processing permission. See the
  [news guide](../docs/news.md) for activation, credentials and request budgets.

- [paper.json](./paper.json): version 1 `FoundationConfig`, policy version 1 and
  unknown provider capabilities.
- [dev-pg_hba.conf](./dev-pg_hba.conf): authentication fixture mounted only by the
  [disposable database runner](../scripts/test-postgres.py).
- [Trading 212 demo](./trading212.demo.example.json) and
  [live](./trading212.live.example.json): non-secret adapter examples. Replace the
  example broker account number/currency before use; see the [guide](../docs/trading212.md).

## Invariants

`[finbert/config.example.json:enabled]` is false.
`[../crates/trady-adapters/src/finbert/mod.rs:FinBertConfig::validate]` rejects unsafe
endpoint shapes and invalid revisions/timeouts; `FinBertClient::new` rejects a
disabled provider before any inference request.

`[../crates/trady-cli/src/telegram/mod.rs:Config::validate]` requires explicit
private owner/chat IDs and bounded polling/refresh/freshness intervals. The bot
token is resolved as `TRADY_TELEGRAM_BOT_TOKEN` through the central credential
loader, outside Telegram configuration.

`[../crates/trady-application/src/lib.rs:FoundationConfig::validate]` rejects live,
shadow, unsupported versions and unsafe policy. `[paper.json:capabilities]` contains
no assumed support. `[dev-pg_hba.conf]` rejects Trady identities connecting to other
databases while allowing the separate application fixture's own access.
`[../crates/trady-adapters/src/trading212/mod.rs:Trading212Config::validate]` requires
explicit environment/account bindings and a bounded timeout; arbitrary URLs and
credentials are not configuration fields.

## Side Effects

None from these files alone. The CLI reads paper configuration; the test runner
mounts the authentication fixture into a new disposable server.
The explicit `t212 snapshot` CLI command can read the configured demo/live account.
`[../crates/trady-cli/src/telegram/mod.rs:monitor]` can poll Telegram, send private
monitoring replies and persist its local inbox/offset/outbox when explicitly run.

## Verification

Run `cargo test -p trady-cli --locked` and `python3 scripts/test-postgres.py` from
the repository root.

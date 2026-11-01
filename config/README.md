# Deployment and fixture configuration

## Overview

Explicit paper defaults, non-secret broker examples and disposable database authentication fixtures.

Secret values use the [central credentials store](../docs/credentials.md), separate
from provider configuration. `credentials init` creates the ignored private
`credentials.local.json`; environment variables override its entries.

## Interface

- [No-submit demo](./autonomous.demo-no-submit.json): record-only shadow runtime
  with an owner-delegated five-stock allowlist. The
  [deployment selection](./deployment.demo-selection.json) records the chosen
  instruments, provider access checks and remaining evidence; it is not loaded
  by the runtime and grants no release authority. The
  [skill candidate](./skills.demo.candidate.json) and
  [approved manifest](./skills.demo.approved.json) pin the reviewed 52-file
  bundle 2.1.1. Its hashes are installed only in this shadow configuration;
  [the review scope](../docs/demo-skill-review-2026-11-01.md) grants no model,
  formula, source or live-deployment authority. See the
  [training and setup report](../docs/demo-training-setup-2026-11-01.md).

- [Pending shadow deployment](./autonomous.pending.json): disabled native runtime
  configuration with a separate shadow namespace and paused entries. The
  [approval handoff](./deployment.pending.json) lists required artifacts and
  evidence; it is an operator manifest and is not loaded by the runtime.
  See the [remediation record](../docs/workflow-remediation-2026-10-31.md).

- [Cold-start demo setup](./cold-start.demo.json): provider-owned account
  discovery, independent OHLCV integrity checks, dynamic broker-intersected
  universe selection, shadow-only seed model/formula/cost artifacts and the
  supervised first-order intent gate. It does not contain a fabricated broker
  account number and it never grants entry authority by itself. See the
  [cold-start guide](../docs/cold-start.md).

- [intelligence.example.json](./intelligence.example.json): version-1
  `[../crates/trady-application/src/intelligence.rs:IntelligenceConfig]` with
  all ten research features enabled and no source registrations. Results require
  explicit reviewed rights, capabilities, entity mappings, audit-marker permission
  and retention. Omitting `enabled` selects all ten; an explicit list selects only
  those features, and `enabled: []` disables every feature. [Operating guide](../docs/v1/features/implementation-report.md).

- [financial-data.json](./financial-data.json): version-1 `FinancialConfig`,
  disabled provider accounts, typed endpoint requests, per-consumer rights and
  local credit limits. `[../crates/trady-application/src/financial.rs:FinancialConfig::validate]`
  validates it. See the [financial guide](../docs/financial-data.md).

- [x-pilot.example.json](./x-pilot.example.json): separate disabled five-author
  recent-search pilot and $10 cycle ceiling ($8 ordinary/$2 support), consumed by
  `[../crates/trady-application/src/research.rs:ResearchConfig]`. Rule approvals,
  stable identities, rights and provider-limit reviews require operator input;
  the [operator guide](../docs/news.md#x-pilot-and-resource-budget) describes the gates.

- [Autonomous runtime](./autonomous.example.json): disabled deployment template;
  `trady autonomous check --config config/autonomous.example.json` inspects it
  without credentials or network. [Operator contract](../docs/autonomous.md)
  describes required approvals, providers, artifacts and execution/recovery commands.

- [Credential setup READMEs](./credentials/README.md): how to obtain every API
  key/token, database connection string and locally generated secret.

- [credentials.example.json](./credentials.example.json): blank named entries for
  API keys, separate T212 demo/live API key/secret pairs, shared secrets, hashing/signing keys
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
- [dev-pg_hba.conf](./dev-pg_hba.conf): SCRAM authentication fixture for isolated
  diagnostic/test servers. The [disposable database runner](../scripts/test-postgres.py)
  generates role passwords and supplies the corresponding test DSNs.
- [Trading 212 demo](./trading212.demo.example.json) and
  [live](./trading212.live.example.json): non-secret adapter examples. Replace the
  example broker account number/currency before use; see the [guide](../docs/trading212.md).

## Invariants

`[autonomous.pending.json:enabled]` is false, its trading mode is `shadow`, and
`[autonomous.pending.json:controls.pause_entries]` is true. Its empty registries,
unknown capabilities and reserved example hosts intentionally prevent readiness.
`[../crates/trady-cli/src/autonomous.rs:run]` reports
`approved_input_service_unconfigured` for the reserved placeholder input endpoint.

`[x-pilot.example.json:x_budget.enabled]` and every pilot source are false.
The example supplies no identity IDs, ownership/coverage approvals or content/model
rights. Its billing dates and rates require deployment review before enablement.

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

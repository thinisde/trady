# Trady Blueprint

## System Overview

Trady is a planned Rust trading bot that narrows S&P 500 stocks by sector and subsector, researches companies and public statements, and stores profiles and relationships in PostgreSQL. It combines X, market-news APIs, Google News RSS and approved collectors. The LLM uses JSON tools, [52 runtime skills](./skills/README.md) and [Perpetual's advisory predictions](./docs/v1/perpetual.md) to propose entry, stop and target limits. Rust validates plans and controls orders; Telegram provides conversations, controls and holding-watchdog updates.

**Current repository status:** the six-crate Rust workspace includes strict JSON contracts, checked policy calculations, fake providers, paper demonstrations, PostgreSQL recovery, and an [OpenCode Go conversation harness](./docs/opencode/README.md) with durable sessions, approved skill loading and bounded tool execution. The three service commands remain bounded startup probes. Complete research readiness, Perpetual, Telegram and validated order/protection orchestration remain incomplete. Automated paper, shadow and live execution are not enabled.

Start with the [Trady v1 specification](./docs/v1/README.md), [daily selection workflow](./docs/v1/sector-workflow.md), [company intelligence](./docs/v1/company-intelligence.md) and [owner-selected X accounts/watchdogs](./docs/v1/watchdogs-and-x.md). The bundle includes separate guides for all 11 sectors and 25 industry-group subsectors.

## Interface

The [central credentials guide](./docs/credentials.md) covers API keys, shared
secrets, hashing/signing keys and database credentials. Run
`cargo run --locked --bin trady -- credentials init`, then edit the ignored
`config/credentials.local.json`. Existing environment variables override file
entries; `credentials status` reports availability without exposing values.
T212 configuration selects its demo/live endpoint and matching credential pair.

The [FinBERT sentiment contract](./docs/v1/finbert/README.md) adds an optional
self-hosted Docker classifier, rights-aware Rust adapter and conservative
confidence/target overlay. [Compose](./services/finbert/compose.yaml), its
[offline override](./services/finbert/compose.offline.yaml) and the
[disabled example configuration](./config/finbert/config.example.json) are prepared;
the image/model service has not been built or launched. Inspect configuration
without network calls using
`cargo run --locked --bin trady -- sentiment status --config config/finbert/config.example.json`.

The [Telegram monitoring guide](./docs/telegram.md) documents owner-authorized
phone commands, optional demo/live broker observations, change alerts and durable
local replies. Start with `telegram discover`, configure the numeric private IDs,
then run `telegram check` and `telegram run --config FILE`.

The [OpenCode guide](./docs/opencode/README.md) documents model credentials,
reviewed skill manifests, persistent owner dialogue and session commands. Provider
code, CLI wiring and configuration are grouped in `opencode/` subfolders.

The [X and news reading guide](./docs/news.md) documents the implemented read-only
X polling/lookup, Google/publisher RSS, optional news API and PostgreSQL item/cursor
path. Start with `cargo run --locked --bin trady -- news status`; sources require
operator review and remain disabled in the shipped registry.

Start with the [implementation record](./docs/foundation.md) for working commands,
validation evidence and remaining requirements. The [v1 specification](./docs/v1/README.md)
remains authoritative for the complete product.

The [Trading 212 implementation](./docs/trading212.md) adds demo/live information
reads and an executor-only order transport. It does not enable automated live trading;
the durable execution/protection workflow and release gates remain required.

Run the offline demonstration from the repository root:

```sh
cargo run --locked --bin trady -- demo
```

It shows a duplicate intent being rejected, a lost response requiring recovery,
and a reconciled fake fill recorded as unprotected. Inspect a bounded service probe
with `cargo run --locked --bin trady -- analyst --once` (also `executor` or `gateway`).

## Subdirectories & System Boundaries

- [Inference services](./services/README.md): optional model-serving components;
  [FinBERT](./services/finbert/README.md) exposes bounded sentiment HTTP inference.

- [docs](./docs/README.md): documentation index and boundaries.
- [docs/v1](./docs/v1/README.md): product requirements, planned component boundaries, trading policy, data contracts, operating procedures, and implementation acceptance criteria.
- [skills](./skills/README.md): versioned Trady runtime instructions for research, comparison, entry readiness, holding management and conversations.
- [Rust workspace](./crates/README.md): domain, policy, application, storage, adapters and CLI contracts.
- [Migrations](./migrations/README.md): local PostgreSQL tables, leases, grants and outbox functions.
- [JSON schemas](./schemas/README.md): canonical schemas and worked fixtures.
- [Configuration](./config/README.md): paper defaults and disposable database authentication.
- [Verification scripts](./scripts/README.md): asset and isolated database checks.
- [Original v1.1 design](./docs/v1/v1.1.md): historical input. The v1 index records which original choices have been superseded.

## Global Invariants

- Planned behavior is explicitly distinguished from implemented behavior.
- [The v1 specification](./docs/v1/README.md) is the current design entry point; the original v1.1 text is historical.
- `[crates/trady-application/src/lib.rs:FoundationConfig::validate]` accepts paper configuration only; entry authority remains blocked.
- `[crates/trady-application/src/contracts.rs:Contracts::validate]` checks JSON syntax/shape; validated model output cannot authorize orders.
- `[migrations/0001_foundation.sql:execution.assert_fence]` guards financial writes with an active database-time lease and generation.

The planned runtime invariants are defined in the [architecture](./docs/v1/architecture.md) and [trading policy](./docs/v1/strategy.md).

## Global Side Effects

Explicit Telegram monitoring polls the fixed Telegram HTTPS API, reads optional
broker information and persists a private local inbox/offset/outbox. It sends
messages only to configured private owner chats and exposes no execution control.

Explicit OpenCode chat commands call the configured model endpoint and persist
scoped transcripts, tool results, usage and context manifests with the analyst role.

The explicit `t212 snapshot` command makes authenticated broker information GETs.
Separately constructed `Trading212Executor` clients can submit/cancel real orders
with live credentials. They do not provide the live execution/protection workflow.

The memory demo mutates process memory and writes JSON to stdout. Explicit database
commands apply migrations or persist paper fixture state in a local Trady database.
The isolated integration script creates and removes its own development container.
Explicit news source clients can perform read-only HTTPS requests and persist research
observations. Provider ownership remains in [architecture](./docs/v1/architecture.md).

## System Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/verify-assets.py
python3 scripts/test-postgres.py
```

The asset script requires the pinned [Python development dependency](./scripts/requirements-dev.txt).
The PostgreSQL script requires Docker and runs a separate ignored integration test
against a disposable server. See the [implementation record](./docs/foundation.md)
for results and limits; the full [acceptance plan](./docs/v1/implementation.md) remains open.

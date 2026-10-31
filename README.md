# Trady Blueprint

## System Overview

Trady is a Rust trading bot that narrows S&P 500 stocks by sector and
subsector and researches companies and public statements. Its
[hybrid prediction design](./docs/v1/prediction-fusion.md) combines LLM research,
FinBERT, calibrated Perpetual forecasts and quantitative signals. Approved
mathematics determines buy/stop/target limits, quantity and horizon; Rust
independently enforces risk and controls orders. The [52 runtime skills](./skills/README.md)
guide research; Telegram provides conversations and monitoring. Missing predictive
support can produce `NO_TRADE`; no model invents an outcome probability or return.

**Current repository status:** the six-crate workspace includes the [autonomous runtime](./docs/autonomous.md), [reviewed input service](./services/inputs/README.md), offline model fitting/evaluation, durable research jobs and Telegram owner controls. The [weekly lab](./lab/README.md) supplies bounded experiments, pinned proof/backtest helpers and a persisted release lifecycle. Separate OS identities, verified holding reviews, TLS database connections and read-only shadow recording are implemented. Shipped configurations remain disabled; real data rights, fitted and approved financial artifacts, prospective shadow sessions and owner release approval are deployment inputs. The three service probes and original paper demonstrations remain separate.

The [ten proposed market intelligence extensions](./docs/v1/features/README.md) and [twelve proposed LLM research features](./docs/v1/llm-features/README.md) are optional, separately documented designs; they do not enable trading. The [OpenCodeGo-only model policy](./docs/v1/llm-features/model-routing-and-governance.md) specifies `deepseek-v4.1-flash` as the only active generative model and reserves `deepseek-v4.1-pro` for verified, owner-approved complex cases. The latter is not documented as currently available at OpenCodeGo; enforcement of the new allowlist is future implementation work.

Start with the [Trady v1 specification](./docs/v1/README.md), [daily selection workflow](./docs/v1/sector-workflow.md), [company intelligence](./docs/v1/company-intelligence.md) and [owner-selected X accounts/watchdogs](./docs/v1/watchdogs-and-x.md). The bundle includes separate guides for all 11 sectors and 25 industry-group subsectors.

## Interface

The [financial-data guide](./docs/financial-data.md) documents SEC/FRED facts,
historical prices, catalog enrichments, shared research signals, bounded LLM tools
and the central credential path. Sources ship disabled; runtime capabilities and
consumer rights require deployment review. Historical bars grant no quote or
order authority.

The [weekly optimization specification](./docs/v1/weekly-optimization.md) and
[local Codex skill](./.agents/skills/trady-weekly-optimization/SKILL.md) define
cumulative forecast calibration, offline QuantConnect LEAN experiments, Lean 4
proof obligations and independent promotion. Executable helpers and an opt-in
systemd timer are supplied in the [lab](./lab/README.md); installation, financial
model approval and live release require deployment evidence.
The [prediction implementation record](./docs/v1/prediction-implementation.md)
identifies tested Rust calculations and the independent financial validation requirements.

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
The separate [X pilot](./docs/news.md#x-pilot-and-resource-budget) adds versioned
recent search, preflight counts, a shared resource budget and local evidence export.
Its [five-account example](./config/x-pilot.example.json) also ships disabled.

Start with the [implementation record](./docs/foundation.md) for working commands,
validation evidence and remaining requirements. The [v1 specification](./docs/v1/README.md)
remains authoritative for the complete product.

The [Trading 212 implementation](./docs/trading212.md) adds demo/live information
reads and an executor-only order transport. The [autonomous operator guide](./docs/autonomous.md)
documents execution, exact-ID recovery and deployment prerequisites. Inspect the
disabled configuration with `cargo run --locked --bin trady -- autonomous check --config config/autonomous.example.json`.

Run the offline demonstration from the repository root:

```sh
cargo run --locked --bin trady -- demo
```

It shows a duplicate intent being rejected, a lost response requiring recovery,
and a reconciled fake fill recorded as unprotected. Inspect a bounded service probe
with `cargo run --locked --bin trady -- analyst --once` (also `executor` or `gateway`).

## Subdirectories & System Boundaries

- [Offline lab](./lab/README.md): dataset, helper, evidence and release workflows.
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
- `[crates/trady-application/src/lib.rs:FoundationConfig::validate]` accepts paper configuration only. Separate `[crates/trady-application/src/autonomous/mod.rs:AutonomousTrader]` validates deployment authority and persists fenced attempts before broker mutations.
- `[crates/trady-application/src/contracts.rs:Contracts::validate]` checks JSON syntax/shape; validated model output cannot authorize orders.
- `[migrations/0001_foundation.sql:execution.assert_fence]` guards financial writes with an active database-time lease and generation.

The planned runtime invariants are defined in the [architecture](./docs/v1/architecture.md) and [trading policy](./docs/v1/strategy.md).

## Global Side Effects

Explicit financial collection reserves shared provider credits, performs bounded
HTTPS requests and persists observations. Financial reads can purge expired data
and store snapshot manifests; registry synchronization disables omitted sources
and accounts. See the [financial-data guide](./docs/financial-data.md).

Explicit Telegram monitoring polls the fixed Telegram HTTPS API and persists a
private local inbox/offset/outbox. The optional expanded gateway queues scoped
research/dialogue and durable owner pause/resume/close requests. Sensitive
requests need a current, single-use confirmation and executor revalidation;
Telegram never receives broker mutation credentials.

Explicit OpenCode chat commands call the configured model endpoint and persist
scoped transcripts, tool results, usage and context manifests with the analyst role.

The explicit `t212 snapshot` command makes authenticated broker information GETs.
The explicit `autonomous run` command submits/cancels real demo/live orders under
reviewed configuration and persists execution, audit, outbox and research state.
Its approved input service receives frozen account/scenario data for prediction.
Low-level `Trading212Executor` clients expose broker mutations only to executor code.
Shadow mode constructs a read-only broker reader and writes immutable observations
and proposals. Explicit input imports, model fitting, weekly jobs and release
commands write their scoped files or database records. Helpers execute pinned
offline commands; the deployed lab identity enforces network isolation. Helpers
cannot activate a release themselves.

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

The [2026-10-31 demo verification](./docs/demo-verification-2026-10-31.md) records
537 passing automated tests, real demo-account GET responses, production formula
examples and actual OpenCode Go replies. The `deepseek-v4.1-flash` pin is stored
in the adapter and configuration and passed real model requests, including a
streamed two-turn conversation. The report also records a separate `glm-5.2`
diagnostic request.

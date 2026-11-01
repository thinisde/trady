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

### Cold-start account setup

Treat a new account as having no execution history. Do not manufacture orders,
fills, fees, FX, execution costs or performance claims. The pending handoff in
[`config/deployment.pending.json`](./config/deployment.pending.json) currently
tracks five independent deployment inputs: the reviewed input service, a
runtime-compatible native model, formula authority, an approved universe and
runtime/broker capabilities. Missing history is evidence that the prospective
sample has not started; it is not a substitute for any of those approvals.

The initializer contract is ordered so each artifact can be independently
reviewed and hash-pinned before it can authorize a later stage:

1. Discover the demo account, base currency, balance, positions, pending orders,
   terminal orders and fills, then persist one immutable account snapshot. The
   existing [`t212 snapshot`](./docs/trading212.md) read and
   `ExecutionBroker::observe` boundary provide the account-observation inputs.
2. Import licensed historical OHLCV data with timestamp, duplicate and missing-
   candle checks, provenance and rights; historical bars remain research data
   and do not establish quote or order authority. The existing
   [`financial import`](./crates/trady-cli/src/financial.rs) path is the import
   boundary.
3. Import and review a permitted universe, resolve broker instrument IDs and
   register the owner allowlist through the
   [`inputs-service`](./services/inputs/README.md) import/review/export contract.
4. Register a runtime-compatible market-prediction seed model with its feature
   schema, label definition, training-data hash and evaluation-report hash;
   keep it shadow-only until an independent reviewer approves the exact bundle.
   `model-lab fit`, `evaluate` and `bundle` are separate stages and fitting does
   not create approval.
5. Register the deterministic baseline formula, run mathematical and Rust/Lean
   parity checks, emit a reproducible report and obtain independent approval of
   the exact formula hash. Keep the market-prediction model, execution-cost
   model and trading-decision policy as separate reviewed artifacts even though
   the current `FormulaAuthority` binds their hashes for one calculation.
6. Verify broker capabilities individually: cash-account trading, instrument and
   quote availability, market and limit orders, stop-loss and take-profit
   protection, cancellation, partial-fill reporting, and fee/FX reporting. A
   successful account response never proves all of these capabilities; the
   runtime checks the per-field evidence through
   `validate_account_entry`.

After those inputs are approved, the first-order procedure refreshes the account,
quote and instrument metadata; calculates execution costs and policy size;
validates protection; writes an immutable order intent and audit record; obtains
explicit human approval; submits exactly one protected demo order; and reconciles
the broker response, fills, fees, FX and costs. The requested command shape is:

```sh
trady account cold-start first-order \
  --config config/cold-start.demo.json \
  --require-manual-approval
```

The command is implemented as a separate, read-only bootstrap and supervised
first-order path in `[crates/trady-cli/src/account.rs:run]`. It records an
immutable intent and returns `FIRST_ORDER_BLOCKED` until the fresh input and
protection gates are satisfied; it never bypasses the existing autonomous
preflight. See the [cold-start guide](./docs/cold-start.md) and the
[cold-start run record](./docs/cold-start-run-2026-11-01.md).

That first reconciled order creates the account's first genuine execution record.
Only then may the deployment collect its policy-required prospective sample,
obtain an independent review (including a separately identified OpenCode Go
review where selected), compare expected and actual execution costs, recalibrate
the execution-cost and prediction artifacts, and promote autonomous entries.

The [1 November training and readiness report](./docs/demo-training-and-readiness-2026-11-01.md)
records actual walk-forward tuning, fitted native artifacts, current demo-account
GETs and the outstanding deployment inputs. The research model did not beat its
average validation baseline. The selected demo configuration remains a shadow
recorder with five explicit entry blockers; no actual-order readiness or model
approval is claimed.

The [ten optional market intelligence services](./docs/v1/features/README.md) have deterministic research implementations, replay, approved normalized imports and optional analyst storage. All are enabled by default for research, uncalibrated and outside live trading authority; see the [implementation report](./docs/v1/features/implementation-report.md).

The [twelve proposed LLM research features](./docs/v1/llm-features/README.md) are optional, separately documented designs; they do not enable trading. The [OpenCodeGo-only model policy](./docs/v1/llm-features/model-routing-and-governance.md) specifies `deepseek-v4.1-flash` as the only active generative model and reserves `deepseek-v4.1-pro` for verified, owner-approved complex cases. The latter is not documented as currently available at OpenCodeGo; enforcement of the new allowlist is future implementation work.

Start with the [Trady v1 specification](./docs/v1/README.md), [daily selection workflow](./docs/v1/sector-workflow.md), [company intelligence](./docs/v1/company-intelligence.md) and [owner-selected X accounts/watchdogs](./docs/v1/watchdogs-and-x.md). The bundle includes separate guides for all 11 sectors and 25 industry-group subsectors.

## Interface

The [full repository audit](./docs/audit-2026-10-31/report.md) records 698 passing
automated tests, directory-contract corrections, requests for all ten intelligence
features, disposable PostgreSQL queries and real service checks. Trading212 API
calls were excluded. GDELT's empty response and the static deployment/validation
boundaries remain visible in that report.
The [audit remediation](./docs/audit-2026-10-31/remediation.md) records subsequent
implementation fixes, regression checks and the remaining upstream GDELT limit.

The [workflow remediation record](./docs/workflow-remediation-2026-10-31.md)
documents the X/RSS/GDELT repairs and scoped database credential rotation.
The owner-selected [pending shadow configuration](./config/autonomous.pending.json)
remains disabled; its [approval handoff](./config/deployment.pending.json) lists
the evidence still required before a full trading decision workflow can pass.

The [API health checks](./docs/api-health.md) add `trady api-health` and explicit
`--online` diagnostics. [Stock research reports](./docs/stock-reports.md) add
`trady research-report FILE` with dated sources, financial calculations and
visible evidence gaps. Both commands produce research-only output.

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
the [live diagnostic](./docs/live-demo-run-2026-10-31.md) separately records a built,
running local service and actual inference. Current licensed source evidence is
still required for production sentiment readiness. Inspect configuration without
network calls using
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

- [Offline lab](./lab/README.md): reconciliation, sealed acceptance evaluation,
  pinned calculator replay and Lean proof/audit helpers, plus separate historical
  collection and purged research-label preparation. Training and helper success
  supply no release authority.
- [Services](./services/README.md): optional
  [FinBERT](./services/finbert/README.md) HTTP inference and
  [runtime deployment templates](./services/runtime/README.md) for service
  identities, private IPC, egress controls, timers and disabled-config preparation.

- [docs](./docs/README.md): documentation index and boundaries.
- [docs/v1](./docs/v1/README.md): product requirements, planned component boundaries, trading policy, data contracts, operating procedures, and implementation acceptance criteria.
- [skills](./skills/README.md): versioned Trady runtime instructions for research, comparison, entry readiness, holding management and conversations.
- [Rust workspace](./crates/README.md): domain, policy, application, storage, adapters and CLI contracts.
- [Migrations](./migrations/README.md): administrator role provisioning and eleven
  ordered application migrations for PostgreSQL tables, leases, scoped grants,
  release authority and outbox functions.
- [JSON schemas](./schemas/README.md): canonical schemas and worked fixtures.
- [Configuration](./config/README.md): disabled runtime examples, deployment input
  templates and disposable database authentication.
- [Verification and credential scripts](./scripts/README.md): asset checks,
  isolated database scenarios and scoped local-container password rotation.
- [Original v1.1 design](./docs/v1/v1.1.md): historical input. The v1 index records which original choices have been superseded.

## Global Invariants

- Planned behavior is explicitly distinguished from implemented behavior.
- [The v1 specification](./docs/v1/README.md) is the current design entry point; the original v1.1 text is historical.
- `[crates/trady-application/src/lib.rs:FoundationConfig::validate]` accepts paper configuration only. Separate `[crates/trady-application/src/autonomous/mod.rs:AutonomousTrader]` validates deployment authority and persists fenced attempts before broker mutations.
- `[crates/trady-application/src/contracts.rs:Contracts::validate]` checks JSON syntax/shape; validated model output cannot authorize orders.
- `[migrations/0001_foundation.sql:execution.assert_fence]` guards financial writes with an active database-time lease and generation.
- The [binary native advisor](./crates/trady-adapters/src/perpetual/README.md) and
  [four-outcome model](./crates/trady-application/src/model_lab/README.md) have
  separate feature/label and approval contracts. The
  [daily-bar trainer](./crates/trady-adapters/examples/README.md) writes research
  artifacts with `deployment_eligible=false`; tuning does not grant model,
  formula, source or account approval.

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
The explicit credential-rotation helper changes the nine Trady login passwords,
atomically replaces the private store, checks authentication and ends existing
Trady-role sessions in the selected local container
(`[scripts/rotate-postgres-credentials.py:rotate]`).
Explicit news source clients can perform read-only HTTPS requests and persist research
observations. Provider ownership remains in [architecture](./docs/v1/architecture.md).

## System Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/verify-assets.py
python3 scripts/test-postgres.py
python3 -m unittest discover -s lab/tests -p 'test_*.py'
python3 -B -m unittest discover -s lab/training -v
cargo test --locked -p trady-adapters --example train_candidate
python3 -m unittest discover -s scripts/tests -p 'test_*.py'
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

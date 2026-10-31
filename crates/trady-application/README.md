# Trady application

## Overview

Application ports, paper reconciliation and strict versioned JSON boundaries.
The crate also coordinates bounded model sessions through provider, reviewed-tool
and persistence ports, plus read-only owner monitoring.

## Subdirectories & Boundaries

- [Source contract](./src/README.md): configuration, provider/repository ports,
  broker transport, research collection, JSON contracts and model-session use cases.
  Its [Telegram child contract](./src/telegram/README.md) documents authorization,
  observation caching and unsent command replies.

## Interface

- [Optimization contract](./src/optimization/README.md): approved formula/policy/
  prediction bindings, deterministic replay and bounded weekly research decisions.
  `WeeklyStore` and `WeeklyLab` are ports; concrete durable storage, proof/backtest
  helpers and release authorization remain separate responsibilities.

- `[src/sentiment/mod.rs:SentimentProvider]`, `assess_sentiment`: rights-aware source
  projection and exact prediction correlation; [design](../../docs/v1/finbert/README.md).

- `[src/research.rs:NewsSource]`, `ResearchRepository`, `ResearchConfig`,
  `collect_source`: bounded registry-driven research collection and durable progress.
- `[src/harness.rs:AgentHarness]`, `RunInput`, `LlmProvider`, `ToolExecutor`,
  `SessionRepository`: synchronous generation/tool orchestration and durable sessions.
- [Cargo target](./Cargo.toml): `trady_application`.

## Invariants

`[src/lib.rs:FoundationConfig::validate]` accepts version 1 paper configuration only.
`[src/lib.rs:BrokerInformationPort]` and `BrokerPort` expose no order submission or
cancellation. Concrete storage/provider implementations remain outside this crate.

The separate `[src/broker.rs:BrokerExecutionPort]` exposes typed submission and
cancellation for an executor. Requests confer no authority: durable intent/fencing,
policy/readiness and protection coordination are caller responsibilities. Transport
acknowledgments do not establish fills or protected positions.

`[src/harness.rs:RunInput]` binds scope, session ID, optional input and cancellation.
`[src/harness.rs:AgentHarness::respond]` acquires a 45-second session lease;
`[src/harness.rs:SessionLease]` carries its TTL. Provider timeouts are bounded to
30 seconds, and session writes require atomic revision/worker/generation/expiry
checks from the repository. Stored session history, tool results and run audit
metadata do not authorize trading. Unfinished tools may execute again on resume.

## Side Effects

`[src/lib.rs:reconcile_account]` invokes supplied broker, repository and clock ports.
`[src/harness.rs:AgentHarness]` invokes supplied provider, tool and session-repository
ports, streams generated text and persists context/run/tool state. PostgreSQL
connections and source-provider HTTP calls belong to concrete adapters.

## Verification

Run `cargo test -p trady-application --locked` from the repository root.
Run `cargo test --workspace --locked` for workspace verification and
`python3 scripts/test-postgres.py` for isolated PostgreSQL checks.

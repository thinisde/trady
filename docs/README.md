# Trady documentation

## Overview

This directory contains the Trady system requirements and the implementation record
for the initial offline Rust foundation.

## Subdirectories & Boundaries

- [Reviewed input service and model lab](../services/inputs/README.md): actual
  data imports/reviews, bounded HTTP and offline four-outcome model workflows.
- [Offline weekly lab](../lab/README.md): durable experiments, pinned helpers and
  independent release evidence.
- [Isolated runtime deployment](../services/runtime/README.md): separate service
  identities, private analyst IPC and opt-in scheduler/network policy.
- [Autonomous runtime](./autonomous.md): automatic universe screening, mathematical
  limits, calibrated predictions, native advice, durable execution/protection,
  exact-ID recovery and deployment prerequisites.
- [Verification reports](./reports/README.md): dated README drift findings and
  verification evidence.

- [Central credentials](./credentials.md): private storage, environment overrides,
  API/shared/hashing/signing keys, database roles and setup/status commands.

- [FinBERT sentiment](./v1/finbert/README.md): self-hosted CPU Docker service, Rust
  adapters, source rights/freshness, threshold calculation and later activation.

- [OpenCode Go conversations](./opencode/README.md): provider calls, durable sessions,
  context manifests, approved skills and bounded tool execution.
- [v1 specification](./v1/README.md): current requirements, Rust architecture, research sources, trading policy, Telegram, memory, multi-round tools, runtime skills, operations and acceptance criteria.
- [Ten proposed market intelligence features](./v1/features/README.md): separate optional designs for options, coordination, event surprise, short interest, insiders, cross-asset state, alternative attention, accounting anomalies, order books and tax scenarios.
- [Original v1.1 input](./v1/v1.1.md): preserved historical design, superseded by the current v1 decisions where requirements differ.
- [Offline foundation](./foundation.md): working commands, evidence, acceptance coverage and remaining work.
- [X and news reading](./news.md): bounded source adapters, registry, durable observations, commands and remaining coverage gaps.
- [Financial data](./financial-data.md): reviewed provider collection, shared
  budgets, normalized observations and rights-aware historical snapshots.
- [Trading 212 adapter](./trading212.md): demo/live information reads, executor order transport and verification limits.

## Interface

The [prediction-fusion design](./v1/prediction-fusion.md) owns model semantics,
calibrated baseline, scenario utility and mathematical limit authority. The
[weekly workflow](./v1/weekly-optimization.md) owns cumulative evaluation, offline
tools and release gates; its developer skill is separate from runtime skills.

The [v1 reading map](./v1/README.md#reading-map) is the requirements entry point.
The [foundation record](./foundation.md) distinguishes implemented symbols from future contracts.

## Invariants

Current requirements, historical input and implementation evidence are labeled
separately. Internal links reference existing files. No live deployment is claimed.

## Side Effects

None at runtime. These Markdown documents do not create databases, run research, send messages or trade.

## Verification

Run `python3 scripts/verify-assets.py` from the repository root. Behavioral
requirements remain in [implementation and acceptance](./v1/implementation.md).

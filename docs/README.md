# Trady documentation

## Overview

This directory contains Trady's requirements, operator guides and dated
implementation, training and verification evidence. Its production-code child is
the archived read-only audit SQL; the other documentation records requirements
or observed behavior without activating a deployment.

## Subdirectories & Boundaries

- [31 October audit](./audit-2026-10-31/README.md): archived read-only intelligence
  inspection queries, their exact returned fields and the accompanying audit and
  remediation records.
- [API health checks](./api-health.md): offline credential metadata and explicit
  bounded GET diagnostics for configured integrations.
- [Stock research reports](./stock-reports.md): dated US/EU evidence, growth and
  cash margins, missing-data flags and optional trailing valuation calculations.
- [31 October API/research verification](./api-research-verification-2026-10-31.md):
  actual diagnostics, feature checks and remaining integration limits.
- [31 October stock research](./market-research-2026-10-31.md): primary-source
  US/EU candidates for moderate-risk valuation review over 6–12 months.

- [Reviewed input service and model lab](../services/inputs/README.md): actual
  data imports/reviews, bounded HTTP and offline four-outcome model workflows.
- [Offline weekly lab](../lab/README.md): durable experiments, pinned helpers and
  independent release evidence.
- [Isolated runtime deployment](../services/runtime/README.md): separate service
  identities, private analyst IPC and opt-in scheduler/network policy.
- [Autonomous runtime](./autonomous.md): automatic universe screening, mathematical
  limits, calibrated predictions, native advice, durable execution/protection,
  exact-ID recovery and deployment prerequisites.
- [Cold-start account workflow](./cold-start.md): runtime account discovery,
  shadow-only seed artifacts, independent OpenCode Go review and the supervised
  first-order gate.
- [Provider API research](./provider-api-research-2026-11-01.md): current free-tier
  limits, Trading 212 capabilities/fees, the selected Invest profile and the
  read-only entitlement probes.
- [Alpaca volatility selection](./alpaca-volatility-selection-2026-11-01.md):
  the bounded five-symbol ranking and the selected XOM demo candidate, with
  stale/out-of-session quote limitations.
- [Pre-open checklist](./pre-open-checklist-2026-11-02.md): work that can be
  completed before the next NYSE session and gates that require a fresh quote or
  genuine fill.
- [Simulated entry and deployment-input search](./simulated-entry-2026-11-01.md):
  recorded synthetic BUY with broker writes blocked, verification and local
  search evidence for the missing reviewed deployment bundle.
- [1 November training and demo readiness](./demo-training-and-readiness-2026-11-01.md):
  bounded walk-forward tuning, fitted native artifacts, fresh demo-account/history
  reads, tested repairs and exact missing deployment evidence.
- [Verification reports](./reports/README.md): dated README drift findings and
  verification evidence.

- [Central credentials](./credentials.md): private storage, environment overrides,
  API/shared/hashing/signing keys, database roles and setup/status commands.

- [FinBERT sentiment](./v1/finbert/README.md): self-hosted CPU Docker service, Rust
  adapters, source rights/freshness, threshold calculation and later activation.

- [OpenCode Go conversations](./opencode/README.md): provider calls, durable sessions,
  context manifests, approved skills and bounded tool execution.
- [v1 specification](./v1/README.md): current requirements, Rust architecture, research sources, trading policy, Telegram, memory, multi-round tools, runtime skills, operations and acceptance criteria.
- [Ten optional market intelligence features](./v1/features/README.md): implemented deterministic research for options, coordination, event surprise, short interest, insiders, cross-asset state, alternative attention, accounting anomalies, order books and tax scenarios; enabled by default for research. [Implementation and verification](./v1/features/implementation-report.md) describe normalized ingestion, replay, storage and remaining data/calibration gates.
- [Twelve proposed OpenCodeGo LLM features](./v1/llm-features/README.md): adversarial committee, falsification, historical analogies, earnings-call analysis, causal chains, research budgets, counterfactuals, narrative drift, multi-horizon reviews, incident analysis, credibility and stress cases. Includes [strict Flash/Pro model governance](./v1/llm-features/model-routing-and-governance.md).
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
The [audit query contract](./audit-2026-10-31/README.md) describes the five SELECT
statements in `[audit-2026-10-31/sample-queries.sql:BEGIN READ ONLY]`. They display
counts and privileges; their comments do not assert a passing result.

## Invariants

Current requirements, historical input and implementation evidence are labeled
separately. Internal links reference existing files. No live deployment is claimed.

## Side Effects

The Markdown documents have no runtime effects. Explicitly executing the archived
`[audit-2026-10-31/sample-queries.sql:SELECT]` statements reads the selected
database and changes transaction state through `BEGIN READ ONLY` and `ROLLBACK`.
The file contains no database mutations or notification transport.

## Verification

Run `python3 scripts/verify-assets.py` from the repository root. Behavioral
requirements remain in [implementation and acceptance](./v1/implementation.md).

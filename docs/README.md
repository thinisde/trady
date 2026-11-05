# Trady documentation

## Overview

This directory contains Trady's operator guides and the EUR day-trading target
authority. Shared research, storage, and legacy IEX30/v1 modules remain
available for offline replay; the target runtime uses a reviewed EUR universe,
IBKR SMART Live bid/ask, YFinance/Alpha Vantage research, and Trading 212
demo execution. The [owner's data-source policy](./eur-data-sources.md)
defines their roles and the absence of quote fallback.

The [5 November real-session follow-up](./eur-blocker-resolution-2026-11-05.md)
records actual DEMO fills, preserved failures and the remaining release gates.
The [6 November–6 December DEMO month preparation](./eur-demo-month-2026-11.md)
describes the finite operation, fresh-input research cadence, native ledger
audit and outstanding activation evidence.

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
- [EUR migration plan](./trady-bot-implementation-plan.md): target contracts,
  phased migration, and acceptance tests.
- [IEX30 code audit](./iex30-audit.md): implemented, wired, configured and
  still-blocked features for the focused runtime.
- [Cold-start account workflow](./cold-start.md): runtime account discovery,
  shadow-only seed artifacts, independent OpenCode Go review and the supervised
  first-order gate.

- [Central credentials](./credentials.md): private storage, environment overrides,
  API/shared/hashing/signing keys, database roles and setup/status commands.

- [FinBERT sentiment](./v1/finbert/README.md): self-hosted CPU Docker service, Rust
  adapters, source rights/freshness, threshold calculation and later activation.

- [OpenCode Go conversations](./opencode/README.md): provider calls, durable sessions,
  context manifests, approved skills and bounded tool execution.
- [Offline foundation](./foundation.md): shared commands and remaining work.
- [X and news reading](./news.md): the bounded focused registry and remaining
  source-approval gaps.
- [Financial data](./financial-data.md): shared historical/offline adapters.
- [Trading 212 adapter](./trading212.md): demo/live information reads, executor order transport and verification limits.

## Interface

The [EUR demo profile](../config/eur-demo.example.json) is the target operator
entry point. The IEX30 guide and audit are retained as migration references.

## Invariants

Current requirements, historical input and implementation evidence are labeled
separately. Internal links reference existing files. No live deployment is claimed.

## Verification

Run the workspace checks documented in the root README. No document claims a
live deployment or authorizes broker mutations by itself.

## Requested EUR day-trading migration

- [Complete codebase audit](./trady-codebase-audit.md)
- [EUR day-trading implementation plan](./trady-bot-implementation-plan.md)
- [Per-file decision matrix](./trady-codebase-file-matrix.md)

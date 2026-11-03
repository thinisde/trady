# Trady documentation

## Overview

This directory contains Trady's operator guides and focused IEX30 deployment
evidence. Shared research, storage and legacy v1 modules remain available for
offline tests, but the runtime path is the IEX30 universe, Alpaca IEX data and
Trading 212 demo execution.

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
- [IEX30 deployment](./iex30.md): the fixed 30-symbol universe, Alpaca IEX
  market-data path, Trading 212 demo execution and watchdog schedule.
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

The [IEX30 deployment guide](./iex30.md) is the focused operator entry point.
The [code audit](./iex30-audit.md) distinguishes implemented, wired, configured
and blocked behavior. Shared foundation, research, storage and broker documents
remain reference material for offline and supervised workflows.

## Invariants

Current requirements, historical input and implementation evidence are labeled
separately. Internal links reference existing files. No live deployment is claimed.

## Verification

Run the workspace checks documented in the root README. No document claims a
live deployment or authorizes broker mutations by itself.

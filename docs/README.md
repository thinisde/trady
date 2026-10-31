# Trady documentation

## Overview

This directory contains the Trady system requirements and the implementation record
for the initial offline Rust foundation.

## Subdirectories & Boundaries

- [Central credentials](./credentials.md): private storage, environment overrides,
  API/shared/hashing/signing keys, database roles and setup/status commands.

- [FinBERT sentiment](./v1/finbert/README.md): self-hosted CPU Docker service, Rust
  adapters, source rights/freshness, threshold calculation and later activation.

- [OpenCode Go conversations](./opencode/README.md): provider calls, durable sessions,
  context manifests, approved skills and bounded tool execution.
- [v1 specification](./v1/README.md): current requirements, Rust architecture, research sources, trading policy, Telegram, memory, multi-round tools, runtime skills, operations and acceptance criteria.
- [Original v1.1 input](./v1/v1.1.md): preserved historical design, superseded by the current v1 decisions where requirements differ.
- [Offline foundation](./foundation.md): working commands, evidence, acceptance coverage and remaining work.
- [X and news reading](./news.md): bounded source adapters, registry, durable observations, commands and remaining coverage gaps.
- [Trading 212 adapter](./trading212.md): demo/live information reads, executor order transport and verification limits.

## Interface

The [prediction-fusion design](./v1/prediction-fusion.md) owns model semantics,
calibrated baseline, scenario utility and mathematical limit authority. The
[weekly workflow](./v1/weekly-optimization.md) owns cumulative evaluation, offline
tools and release gates. The [implementation record](./v1/prediction-implementation.md)
identifies the tested Rust subset; the local developer skill is separate from
runtime skills.

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

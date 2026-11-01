# Optional intelligence adapters

## Overview

Bounded normalized imports, SEC ownership XML parsing, venue-specific displayed books, deterministic memory storage and optional research tools. The [application service](../../../trady-application/src/intelligence.rs) owns rights, scope, evidence and retention gates.

## Interface

- `[mod.rs:ReadOnlyFeatureSource]` and `JsonFeatureSource::observation` read a bounded regular UTF-8 file into a strict `FeatureObservation`. `[mod.rs:MemoryIntelligenceRepository]` implements the research repository for offline replay and tests.
- `[ownership.rs:OwnershipFilingParser::parse]` takes XML and `[ownership.rs:OwnershipParseContext]`, returning an `OwnershipFiling`. The context supplies trusted accession, issuer, form, filing/publication times, currency and amendment targets.
- `[book.rs:BookFeedReader]`, `BookUpdate`, `BookChange`, `BookSide`, `FeedMode`, `BookLevel` and `BookSnapshot` describe the provider-neutral feed contract. `SequenceBookBuilder::{new,apply,snapshot,history_len,research_book}` maintain bounded per-instrument/venue books and project policy inputs.
- `[tools.rs:tool_specs]` and `[tools.rs:IntelligenceTools<B,R>]` wrap an existing `ToolExecutor` with optional read-only tools for enabled non-tax feature kinds. Tax stays in the local owner CLI. With no configuration, `specs` and `revision` delegate to the base executor; recognized intelligence calls remain intercepted as unavailable, while Tax calls are denied.

## Invariants

- `[mod.rs:JsonFeatureSource::observation]` requires a caller byte limit of
  1,024–4,194,304 bytes, a regular file within that limit, UTF-8 and a strict
  `FeatureObservation` document.
- `[ownership.rs:parse_tree]` limits XML to 2 MiB, 50,000 elements and depth 32.
  `[ownership.rs:OwnershipFilingParser::parse]` also bounds reporting owners to
  256 and transaction/holding rows to 10,000, with the form and normalized issuer
  CIK matching the supplied trusted context.
- `[ownership.rs:OwnershipFilingParser::parse]` handles structured Forms 3/4/5 and amendments, owner roles, holdings, transactions, derivative terms and footnotes. It bounds input size/depth/nodes/rows and rejects DTDs, processing instructions, unknown entities, malformed XML and overflow. Parsed transaction dates do not establish publication time. Parsed rows still require explicit review before counting reviewed cash transactions.
- `[book.rs:SequenceBookBuilder::apply]` requires monotonic sequences and clocks, positive tick-aligned prices, positive retained sizes and an uncrossed book. Zero-size deltas delete levels; sizes have no grid-alignment check. A gap, invalid delta or backing-depth overflow requires a valid newer snapshot. Capacity overflow does not silently discard levels. Recovery changes the projected feed epoch and clears incompatible history; recovered snapshots still describe partial depth.
- `[book.rs:SequenceBookBuilder::new]` bounds instrument/venue pairs to 128, levels to 100 per side and snapshot history to 256. A depth limit is a hard backing-state capacity; feeds needing more levels must refresh snapshots or use a suitably reviewed capacity. Snapshots remain partial, venue-specific displayed liquidity.
- `[tools.rs:IntelligenceTools::requires_ephemeral_session]` requires in-process conversation storage when intelligence tools are enabled. `validate_context` rechecks expiry, input existence, invalidations, registry revision and external-LLM rights before each model request; the harness rejects durable conversation storage before acquiring a lease.
- `[mod.rs:MemoryIntelligenceRepository]` preserves immutable/idempotent observations and correction vintages. Erasure removes dependent payloads, assessments and rejection markers; purging removes expired content. Erased identity metadata prevents ID reuse.
- Tools enforce owner/account scope, strict arguments, byte limits, current source rights and `entry_authorized=false`. This package provides no subscription transport, broker method or execution port.

## Side Effects

`[mod.rs:JsonFeatureSource::observation]` reads one local file. `[book.rs:SequenceBookBuilder]` mutates bounded tracks and history. `[mod.rs:MemoryIntelligenceRepository]` mutates in-process collections without a configured capacity; explicit `purge` removes expired content while registries, invalidations and erased-identity metadata remain. Tool execution can query/purge the supplied repository and save permitted derived assessments; ephemeral conversation outputs can be sent to the configured model only under external-LLM permission. XML parsing performs no network access. No adapter opens a market-data subscription or places orders.

## Verification

Run `cargo test --workspace --locked` from the repository root.

Run `cargo test -p trady-adapters --locked`, `cargo test -p trady-policy --lib intelligence:: --locked` and `python3 scripts/test-postgres.py` from the repository root.

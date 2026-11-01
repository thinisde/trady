# EGM Drift Report — 2026-10-31

Scope: the 48 README contracts modified by the full-tree EGM bootstrap. Each contract was checked against the local source and verification targets.

Result: **11 discrepancies across 9 contracts; 39 contracts with no detected drift.** Ten discrepancies concern concurrent `intelligence` additions and migration 10. The remaining discrepancy is an incorrect claim that the FinBERT integration-test target does not exist.

Findings marked `[Concurrent change]` describe source added during this session. The earlier 537-test verification predates those additions and does not establish their runtime behavior. This report records the working-tree differences for triage.

## README.md

- [Concurrent change] The overview describes all ten market-intelligence extensions as unimplemented. The working tree now exports `intelligence` from the domain, policy, application, adapters and storage crates and contains ingestion/assessment code plus migration 10. The categorical implementation status is stale; activation is not established by these source additions. Evidence: [application intelligence](../crates/trady-application/src/intelligence.rs), [domain entry point](../crates/trady-domain/src/lib.rs), [policy entry point](../crates/trady-policy/src/lib.rs), [adapter entry point](../crates/trady-adapters/src/lib.rs) and [storage migration registration](../crates/trady-storage/src/lib.rs).

## crates/README.md

No drift detected.

## crates/trady-adapters/README.md

No drift detected.

## crates/trady-adapters/src/README.md

- [Concurrent change] The interface/module index omits the newly exported `intelligence` module. [lib.rs](../crates/trady-adapters/src/lib.rs) now declares it, with a new [component entry point](../crates/trady-adapters/src/intelligence/mod.rs); its research interfaces and effects have no coverage in this source contract.

## crates/trady-adapters/src/autonomous/README.md

No drift detected.

## crates/trady-adapters/src/financial/README.md

No drift detected.

## crates/trady-adapters/src/finbert/README.md

- The Verification section states that the package has no `finbert` integration-test target. Cargo metadata identifies the existing target at [tests/finbert/main.rs](../crates/trady-adapters/tests/finbert/main.rs). The documented absence is incorrect; `cargo test -p trady-adapters --test finbert --locked` selects that target. The [service README](../services/finbert/README.md) already uses this valid command.

## crates/trady-adapters/src/inputs_service/README.md

No drift detected.

## crates/trady-adapters/src/news/README.md

No drift detected.

## crates/trady-adapters/src/opencode/README.md

No drift detected.

## crates/trady-adapters/src/perpetual/README.md

No drift detected.

## crates/trady-adapters/src/research_jobs/README.md

No drift detected.

## crates/trady-adapters/src/screening/README.md

No drift detected.

## crates/trady-adapters/src/telegram/README.md

No drift detected.

## crates/trady-adapters/src/trading212/README.md

No drift detected.

## crates/trady-adapters/src/weekly_lab/README.md

No drift detected.

## crates/trady-application/README.md

No drift detected.

## crates/trady-application/src/README.md

- [Concurrent change] The interface and effects sections omit [intelligence.rs](../crates/trady-application/src/intelligence.rs), now exported by [lib.rs](../crates/trady-application/src/lib.rs). Its `IntelligenceConfig`, `IntelligenceRepository`, `ingest`, `assess_ephemeral` and `read` add source-rights gates, immutable observations, invalidation, rejection and assessment-persistence ports that the source contract does not cover.

## crates/trady-application/src/autonomous/README.md

No drift detected.

## crates/trady-application/src/model_lab/README.md

No drift detected.

## crates/trady-application/src/optimization/README.md

No drift detected.

## crates/trady-application/src/sentiment/README.md

No drift detected.

## crates/trady-application/src/telegram/README.md

No drift detected.

## crates/trady-cli/README.md

No drift detected.

## crates/trady-cli/src/README.md

No drift detected.

## crates/trady-cli/src/finbert/README.md

No drift detected.

## crates/trady-cli/src/opencode/README.md

No drift detected.

## crates/trady-cli/src/telegram/README.md

No drift detected.

## crates/trady-domain/README.md

No drift detected.

## crates/trady-domain/src/README.md

- [Concurrent change] The exported-module/type inventory omits [intelligence/mod.rs](../crates/trady-domain/src/intelligence/mod.rs), now exported by [lib.rs](../crates/trady-domain/src/lib.rs). `FeatureKind`, `FeatureInput`, `FeatureAssessment`, `IntelligenceScope`, `SourceEvidence`, `FeatureObservation` and `AssessmentRecord`, their validation rules and the feature submodules are absent from the contract.

## crates/trady-domain/src/sentiment/README.md

No drift detected.

## crates/trady-policy/README.md

No drift detected.

## crates/trady-policy/src/README.md

- [Concurrent change] The calculation/module inventory omits [intelligence/mod.rs](../crates/trady-policy/src/intelligence/mod.rs), now exported by [lib.rs](../crates/trady-policy/src/lib.rs). Its `assess` dispatcher and feature-specific deterministic research calculations are not described.

## crates/trady-policy/src/limits/README.md

No drift detected.

## crates/trady-policy/src/prediction/README.md

No drift detected.

## crates/trady-policy/src/sentiment/README.md

No drift detected.

## crates/trady-storage/README.md

- [Concurrent change] The Invariants section says `migrate` applies versions 1–9. [lib.rs](../crates/trady-storage/src/lib.rs) now embeds `OPTIONAL_INTELLIGENCE_SQL` and registers `(10, OPTIONAL_INTELLIGENCE_SQL)` in the same transaction, so the documented schema-change scope omits migration 10.
- [Concurrent change] The repository/interface and side-effect inventories omit the new [intelligence repository](../crates/trady-storage/src/intelligence.rs) exported by [lib.rs](../crates/trady-storage/src/lib.rs), including its optional-research storage boundary.

## crates/trady-storage/src/README.md

- [Concurrent change] The Interface section says `migrate` applies versions 1–9 and lists SQL constants only through `FINANCIAL_DATA_SQL`. [lib.rs](../crates/trady-storage/src/lib.rs) now exports `OPTIONAL_INTELLIGENCE_SQL` and applies version 10, outside the documented constant inventory and migration range.
- [Concurrent change] The interface, invariants and side-effect sections omit [intelligence.rs](../crates/trady-storage/src/intelligence.rs), which is now exported by [lib.rs](../crates/trady-storage/src/lib.rs). Its research persistence behavior is outside the documented repository inventory.

## lab/README.md

No drift detected.

## lab/evaluator/README.md

No drift detected.

## lab/lean/README.md

No drift detected.

## lab/proofs/README.md

No drift detected.

## lab/proofs/TradyProof/README.md

No drift detected.

## migrations/README.md

- [Concurrent change] The overview and runner description still specify nine migrations/versions 1 through 9, and the schema inventory omits [0010_optional_intelligence.sql](../migrations/0010_optional_intelligence.sql). The [runner](../crates/trady-storage/src/lib.rs) now applies version 10. The new migration adds `research.intelligence_registries`, `research.intelligence_observations`, `research.intelligence_invalidations`, `research.intelligence_assessments` and `research.intelligence_rejections`, with associated constraints and grants absent from the contract.

## scripts/README.md

No drift detected.

## services/README.md

No drift detected.

## services/finbert/README.md

No drift detected.

## services/runtime/README.md

No drift detected.


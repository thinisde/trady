# Intelligence stability review and default enablement — 2026-10-31

The ten deterministic research features were reviewed before changing their default feature flags. Review covered their current typed contracts, calculation and missingness paths, checked arithmetic, source provenance, correction/replay behavior, storage, model-tool integration and tax isolation. This is a review of the implemented research software; historical predictive calibration and provider-specific data collection remain separate work.

## Reviewed feature scope

| Feature | Reviewed behavior and remaining limits |
|---|---|
| Options | European IV bid/mid/ask inversion, quote freshness/alignment, carry assumptions, skew/term structure and licensed explicit event baselines. American exercise remains unsupported. |
| Coordination | Publication windows, source coverage/baselines, origin deduplication/deletion and suppression-only tiers. Price/volume-deviation inputs are validated but do not contribute a returned quantitative feature. |
| Event surprise | Frozen pre-release estimates, comparable actuals, revision history, reaction windows and confounders. Expectation/actual/reaction components are persisted together in an observation. |
| Short interest | Reported-position vintages, float/volume adjustments, delayed publication, borrow context and crowding flags. Daily short-sale volume cannot replace reported positions. |
| Insider filings | Bounded Forms 3/4/5 XML parsing, cash-versus-award classification, amendments, distinct-owner clusters and checked notionals. Owner-history normalization and fundamentals/price-response joins are future analytics. |
| Cross-asset | Sparse point-in-time joins, compatible calendars/units, primary flows, trailing statistics, stress and historical breadth. Missing required series abstain. |
| Attention | Audited entity/query mappings, compatible scale/bucket histories, bot/coverage exclusions, median/MAD scores and verified event timing. No calibrated confidence or directional forecast is inferred. |
| Accounting | Comparable periods/facts, taxonomy/unit/basis checks, checked ratios and empirical ranks. Fact revisions are descriptive; callers supply observation corrections or invalidations for stored dependents. |
| Microstructure | Sequenced venue books, capacity/gap recovery, freshness, depth metrics, compatible dynamics and elapsed markouts. Displayed depth provides no fill-probability model. |
| Tax | Reviewed dated linear rules, confirmed lots and FX, partial sales/splits, withholding reconciliation and preserved safety constraints. Owner-only ephemeral comparisons do not model tax-deferral timing value or reconcile raw broker statements automatically. |

The [earlier drift report](./2026-10-31_DRIFT_REPORT.md) also identifies conceptual examples and proposed names that differ from the shipped interfaces. The [domain](../../../crates/trady-domain/src/intelligence/README.md), [policy](../../../crates/trady-policy/src/intelligence/README.md), [adapter](../../../crates/trady-adapters/src/intelligence/README.md) and [synthetic payload](../../../schemas/fixtures/intelligence/README.md) contracts identify the implemented boundaries. This review does not claim that every proposed analytics extension or vendor transport is implemented.

## Stability fix

[Application ingestion](../../../crates/trady-application/src/intelligence.rs) previously recorded only selected rejection paths. Immutable observation-ID collisions and missing correction predecessors returned errors without an eligible licensed rejection marker. A shared ingestion failure path now applies the existing audited-source, bounded-retention and erasure checks to every failed non-tax ingestion attempt. It records a stable `ingestion_rejected` code and no raw error text. Missing audit permission suppresses the marker. Tax returns before repository access.

Regression coverage verifies that collision/correction failures leave the original observation intact, cannot create a correction invalidation, retain only permitted markers, and cannot recreate erased markers.

## Default behavior

- [The shipped configuration](../../../config/intelligence.example.json), `IntelligenceConfig::research_defaults(scope)` and deserialization with an omitted `enabled` field select all ten features.
- An explicit `enabled` list selects exactly those features; `enabled: []` and `IntelligenceConfig::disabled(scope)` remain opt-outs.
- Default source registrations remain empty. Enabling calculations grants no source entitlement. Every feature returns unavailable for unregistered/unreviewed input, and live-mode assessments remain denied.
- Nine non-tax model tools are available when an intelligence configuration is attached to chat. Those chats require ephemeral sessions and recheck licensed dependencies before each model request. Ordinary chat attachment remains explicit through `--intelligence-config`.
- Tax stays in the ephemeral owner comparator. All results remain `RESEARCH_ONLY_UNCALIBRATED` with `entry_authorized=false`.

## Verification

The pre-change workspace baseline passed 516 tests with 21 database-dependent tests skipped. Post-change verification completed as follows:

| Check | Result |
|---|---|
| `cargo test --workspace --locked` | Passed: 519 tests; the 21 database-dependent tests were run separately below. |
| `cargo test --locked -p trady-adapters -p trady-cli --test intelligence` | Passed: 18 adapter and 3 CLI tests, including all ten defaults, explicit opt-out/subset selection, missing-source denial, rights/erasure/privacy regressions and the rejection audit fix. |
| `python3 scripts/test-postgres.py` | Passed: all 21 database-dependent tests and CLI smoke checks against a disposable PostgreSQL instance. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed. |
| `cargo fmt --all -- --check` and `git diff --check` | Passed. |
| `python3 scripts/verify-assets.py` | Passed: canonical assets, skill records and Markdown/local links. |
| `target/debug/trady intelligence status` | Confirmed all ten flags enabled, no source registrations and `entry_authorized=false`. |

The first database run exposed an old test assumption that only the malformed-input rejection would exist. The updated regression checks collision and missing-predecessor markers as well, selects the malformed marker by its null validated identifier, and verifies that denied audit rights add no row and erasure removes all three markers. The complete disposable-database rerun passed.

No source subscription, collection schedule or trading authority is created by this default change.

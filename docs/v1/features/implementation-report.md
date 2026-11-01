# Optional intelligence implementation and verification

Implemented 2026-10-31 in the Rust workspace. All ten features have strict input/output types, deterministic calculations, synthetic fixtures and rights-aware service integration. After the [stability review](./stability-review-2026-10-31.md), all ten are enabled by default for research, labelled `RESEARCH_ONLY_UNCALIBRATED`, and emit `entry_authorized=false`.

## Implemented scope

| Feature | Research behavior | Model tool |
|---|---|---|
| Options | European IV intervals, move scales, skew, term structure, explicit-baseline event variance | `options.expectations` |
| Coordination | Coverage-aware bursts, origin deduplication, suppression-only attention risk | `market.coordination_check` |
| Event surprise | Pre-release frozen expectations, compatible actuals, reaction and confounder handling | `event.surprise_check` |
| Short interest | Publication vintages, float ratios, days-to-cover, optional borrow context | `short.crowding` |
| Insider filings | Bounded Forms 3/4/5 XML parser, classification, amendments, distinct-owner clusters | `issuer.insider_activity` |
| Cross-asset | Sparse aligned context, primary flows, beta/correlation, stress and breadth | `macro.cross_asset_context` |
| Attention | Audited query scales, robust median/MAD surprise and event timing | `attention.snapshot` |
| Accounting | Comparable facts/periods, checked anomaly metrics, restatements and cohort ranks | `issuer.earnings_quality` |
| Microstructure | Bounded sequence books, spread/depth metrics, dynamics and markouts | `market.microstructure` |
| Tax | Owner-reviewed dated rules, lots/FX/fees, withholding-once scenario illustrations | Owner CLI only |

The [domain](../../../crates/trady-domain/src/intelligence/README.md), [policy](../../../crates/trady-policy/src/intelligence/README.md) and [adapter](../../../crates/trady-adapters/src/intelligence/README.md) contracts describe interfaces, invariants and side effects. [Application service](../../../crates/trady-application/src/intelligence.rs), [PostgreSQL repository](../../../crates/trady-storage/src/intelligence.rs) and [migration 10](../../../migrations/0010_optional_intelligence.sql) add provenance, immutable vintages, corrections, retention and erasure.

## Operation

Inspect the default research configuration without a database or network request:

```sh
cargo run --locked --bin trady -- intelligence status --config config/intelligence.example.json
```

For real research, copy the configuration, optionally narrow its `enabled` list, and explicitly register reviewed source revisions/capabilities, allowed consumers, normalized/derived storage rights, audit-marker permission, retention and entity IDs. `entity_aliases` maps stable subjects to reviewed external aliases. Only cross-asset inputs can use `cross_asset_context` for explicitly reviewed related entities. No permission is inferred from publicly visible data.

Import input is a strict `[../../../crates/trady-domain/src/intelligence/mod.rs:FeatureObservation]`, containing `schema_version`, `observation_id`, `scope`, `entity_id`, `payload_hash`, `evidence`, nullable `supersedes`, and `payload: {"feature":"ACCOUNTING","input":{...}}`. `payload_hash` uses the application's canonical serialized-payload SHA-256; each evidence item records source revision, subject, units/basis, chronology, source reference/hash, lineage and coverage. The [synthetic payload fixtures](../../../schemas/fixtures/intelligence/README.md) and [test envelope builder](../../../crates/trady-adapters/tests/intelligence_support/mod.rs) show the actual contract.

With reviewed files prepared, these commands use the real service:

```sh
trady intelligence assess --config REVIEWED_CONFIG --input OBSERVATION --as-of RFC3339 --mode PAPER
trady intelligence replay --config REVIEWED_CONFIG --input REPLAY
trady intelligence tax-scenarios --config REVIEWED_CONFIG --input OWNER_TAX_OBSERVATION
```

Replay accepts `{"schema_version":1,"events":[{"at":"RFC3339","observation":{...}}],"queries":[{"at":"RFC3339","feature":"ACCOUNTING","entity_id":"ID"}]}`. Events and queries must each be chronological and historical; tax is excluded. Every event is counted, including events after the final query. `rejected_events` contains stable failure codes and event indexes even when an audit hash is not licensed. Replay uses process memory; current rights and erasure still constrain historical reads.

`import`, `read`, `invalidate` and `purge` require the existing analyst database credentials and migrations through [11](../../../migrations/0011_fail_closed_storage.sql). PostgreSQL assigns ingestion time and rejects erased-ID reuse. Non-tax ingest/read attempts activate the validated reviewed registry before input checks or unavailable-mode returns, allowing the database to reconcile revocation and expiry even when no insertion succeeds. A replaced registry hash is retired permanently; an intentional regrant requires a fresh reviewed snapshot/revision, and stale cached callers fail closed. Configurations are complete snapshots for their owner/account scope. Corrections retain earlier decision-time vintages; erasure removes linked payloads, assessments and rejection markers and also blocks earlier replay. No migration or import was applied to a live database during this implementation.

`trady chat --intelligence-config REVIEWED_CONFIG` adds enabled non-tax tools to the existing chat interface and still requires the usual owner/skill/provider arguments. These sessions are ephemeral and reject `--session`/`--resume`. Tool results, model requests and model replies stay out of durable conversation storage. Before each model request, source rights, expiry, input existence, registry revision and invalidations are rechecked. External-LLM permission is required independently of display and derived-storage permission.

Tax calculations require the owner audience and explicitly approved context/rules. Tax ingestion returns before all repository operations, including rejection logging. Raw tax payloads never enter research storage, replay or general model tools. Safety exits and constraints remain unchanged by the calculation.

## Verification

| Check | Result |
|---|---|
| `cargo test --workspace --locked` | Passed: 519 tests, 0 failures; 21 environment-dependent tests skipped by the default run and passed separately below |
| `cargo test -p trady-adapters --test intelligence --locked` | Passed: 18 integration tests across all ten services, enabled defaults, source gates, licensed rejection failures and shared boundaries |
| `cargo test -p trady-cli --test intelligence --locked` | Passed: enabled default status, explicit opt-out, missing-source denial, typed assessment, complete replay accounting and argument checks |
| `python3 scripts/test-postgres.py` | Passed: all 21 environment-dependent tests against disposable PostgreSQL, including intelligence persistence/erasure/role tests and existing database/CLI suites |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed after integration and documentation changes |
| `cargo fmt --all -- --check` | Passed after formatting the new migration include |
| `python3 scripts/verify-assets.py` | Passed: canonical JSON assets, skill records and repository Markdown/local links |

Two independent Goldfish review rounds identified and verified fixes for subject and source/reference binding, licensed rejection retention, tax logging isolation, durable chat copies, book depth loss, correction retries and replay failures. The follow-up also found an omitted options-baseline citation and a memory erasure-upgrade deduplication gap; both now have passing regressions. Broader reference-field matching covers baseline and audit references instead of a fixed source-field whitelist.

All updated package contracts were read and checked for required sections, source symbols, Markdown fences and resolvable local links. The [EGM drift report](./2026-10-31_DRIFT_REPORT.md) records the earlier documentation comparison. The [subsequent stability review](./stability-review-2026-10-31.md) covers default enablement and the licensed rejection audit fix. The default example is exercised through the actual CLI with all ten research flags enabled and no source registrations; missing source permissions still produce unavailable results.

## Remaining promotion gates

The implementation consumes approved normalized inputs and supplied feed messages. It provisions no paid vendor subscription, exchange connection, consensus service or scheduled collection. Direct source transports beyond the existing project adapters remain provider-specific work. European pricing deliberately withholds unsupported American-option IV; displayed books require snapshot recovery when their bounded backing state fills.

Synthetic correctness tests establish no alpha, forecast calibration, causal effect or empirical tax/legal correctness. Historical datasets, cost/ablation studies, prospective paper/shadow evaluation, jurisdiction-rule maintenance and independent release approval remain necessary before any production dependency or promotion into trading decisions. No order, deployment, live activation, commit or staging was performed for this work.

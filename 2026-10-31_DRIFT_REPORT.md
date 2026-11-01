# Financial-data contract drift report

Scope: the financial-data additions in the 17 documents below, checked against
the current working tree on 2026-10-31. Differences from the broader v1 design
are included even where that design already labels them as future work. This is
not an audit of unrelated, pre-existing subsystems described by these READMEs.

## README.md

No drift detected.

## config/README.md

No drift detected.

## config/credentials/README.md

No drift detected.

## config/credentials/financial-data/README.md

No drift detected.

## crates/trady-domain/src/README.md

No drift detected.

## crates/trady-application/src/README.md

No drift detected.

## crates/trady-adapters/src/README.md

No drift detected.

## crates/trady-adapters/src/financial/README.md

No drift detected.

## crates/trady-adapters/src/news/README.md

No drift detected.

## crates/trady-storage/src/README.md

No drift detected.

## crates/trady-cli/README.md

No drift detected.

## migrations/README.md

No drift detected.

## docs/financial-data.md

No drift detected.

## docs/credentials.md

No drift detected.

## docs/news.md

No drift detected.

## docs/v1/free-financial-data.md

- **Collection and scheduling scope:** the proposed separate historical,
  fundamental and macro ports are represented by one typed `FinancialProvider`
  port. `FinancialPage` has no continuation or request-usage field. The financial
  CLI implements explicit single-pass collection, with per-source cadence and
  leases; it has no durable backfill queue, cursor resumption, recursive saturated
  window splitting, priority scheduler or 80% optional-work deferral. See
  [the application ports](crates/trady-application/src/financial.rs) and
  [CLI collection](crates/trady-cli/src/financial.rs).
- **Capability and identity models:** `Capability` records review evidence,
  revision, expiry and credit weight, while `FinancialSource` contains the
  configured endpoint and identity. The design's plan, venue/field entitlement,
  delay, reset-policy and effective-dated symbol mappings are not separately
  machine-readable records. Extend these types and mapping validation before
  relying on automatic historical identity or entitlement reconstruction. See
  [domain contracts](crates/trady-domain/src/financial.rs) and
  [FinancialSource](crates/trady-application/src/financial.rs).
- **Observation metadata:** the implemented envelope does not represent every
  proposed field, including timestamp precision/timezone, provider-account ID,
  normalization time, effective mapping interval and frequency. SEC concept
  metrics retain period bounds and accession, but do not expose fiscal year,
  filing form or all XBRL contexts as distinct payload fields. See
  [FinancialObservation and FinancialPayload](crates/trady-domain/src/financial.rs)
  and [SEC normalization](crates/trady-adapters/src/financial/parse.rs).
- **Historical reconstruction:** `read_snapshot_inner` uses only the current
  source registry hash. Archived registry documents are not consulted to
  reconstruct older mappings or capabilities. There is no separate reconstructed
  history mode or authenticated vintage-delivery manifest. See
  [snapshot selection](crates/trady-storage/src/financial.rs).
- **Quota dimensions:** `BudgetLimits` and `reserve_credits` enforce UTC calendar
  minute/day/month credits. The design's independent endpoint ceilings, aggregate
  IP limits, sub-minute request spacing, hourly limits, byte allowances and
  unique-symbol counters are absent. Credentials are grouped by their fixed
  credential names, with one account per provider, rather than configurable
  account namespaces. Account changes can lower an existing window's ceiling;
  raising it waits for reset. See [budget contracts](crates/trady-application/src/financial.rs),
  [reservation storage](crates/trady-storage/src/financial.rs) and
  [shared news reservations](crates/trady-storage/src/research.rs).
- **Retry and caching policy:** rate-limit deadlines are durable, but the
  financial client has no jittered retry loop, bounded transient-failure retry
  worker or conditional-response cache. Source leases coalesce active work for a
  source, rather than maintaining the proposed parameter/window/vintage cache.
  See [FinancialClient::fetch](crates/trady-adapters/src/financial/mod.rs) and
  [PgFinancialRepository::reserve](crates/trady-storage/src/financial.rs).
- **Invalidation propagation:** corrections append
  `research.financial_invalidations`, but no consumer of that table propagates
  changes to dependent Perpetual contexts, forecasts, exports or unexecuted
  readiness dossiers. The proposed dependency graph, invalidation outbox and
  recomputation worker remain necessary for that behavior. See
  [correction writes](crates/trady-storage/src/financial.rs) and
  [migration 9](migrations/0009_financial_data.sql).
- **Consumer tool breadth:** implemented financial tools use approved `source_id`
  plus a 1–3 result limit, rather than the broader instrument/metric/form/date and
  stored-snapshot arguments proposed in the table. `filings.read`,
  `events.upcoming`, `features.snapshot` and `data.quality` are absent. See
  [registered schemas](crates/trady-adapters/src/research_jobs/mod.rs) and
  [tool dispatch](crates/trady-storage/src/research_jobs.rs).
- **Feature and calendar semantics:** `signals` provides last close, arithmetic
  window return and mean close. It does not implement the proposed log momentum,
  volatility, ATR, relative volume, accounting ratios, macro surprises or
  event/news/breadth features. Provider-response completeness and increasing bar
  dates do not establish expected-session continuity, corporate-action coverage or
  indicator warmup. Add explicit calendar and feature contracts before using
  those stronger meanings. See [signals](crates/trady-application/src/financial.rs)
  and [price parsers](crates/trady-adapters/src/financial/parse.rs).
- **Model and laboratory composition:** `model_features` exports a compatible
  Perpetual record, but no financial-specific automatic model-training,
  watchdog/readiness adapter, weekly dataset exporter, registered ablation or
  QuantConnect LEAN data bridge is wired to it. The proposed immutable laboratory
  manifest requires additional lineage, calendar, rights and evaluation fields.
  See [model_features](crates/trady-application/src/financial.rs) and
  [the existing lab boundary](lab/README.md).
- **Revocation beyond local storage:** purge removes local observations and
  dependent local manifests. It has no traversal of external exports, embeddings
  or trained-model influence, and it retains registry, collection and invalidation
  metadata. The design's broader deletion and artifact-review propagation needs
  explicit downstream ownership. See [purge](crates/trady-storage/src/financial.rs).
- **Operational diagnostics:** `health` reports source outcomes, budget usage,
  reset times and retry deadlines. The proposed latency, quarantine, successful
  watermark, invalidation-backlog and data-caused-entry-block diagnostics are not
  separately implemented. See [health](crates/trady-storage/src/financial.rs).

## docs/v1/free-financial-api-catalog.md

- **Additional candidate routes:** the catalog also describes SEC company-facts,
  older submission files and filing sections; FRED series/release metadata;
  Twelve Data quote/price; Finnhub general news, profile, earnings calendar and
  quote; and Alpha Vantage overview/daily history. These are not dispatched by
  the implemented financial/news request enums. See
  [financial requests](crates/trady-application/src/financial.rs),
  [fixed routes](crates/trady-adapters/src/financial/mod.rs),
  [Finnhub/GDELT dispatch](crates/trady-adapters/src/news/financial_news.rs) and
  [existing Alpha Vantage](crates/trady-adapters/src/news/alpha.rs).
- **GDELT discovery timing:** the collector validates `seendate` against the
  request and preserves unknown publication, but does not retain that vendor
  timestamp in a distinct stored field. It marks saturation partial without
  implementing recursive window splitting. Add a distinct discovery timestamp
  before consumers need the catalog's original discovery-time lineage. See
  [GDELT normalization](crates/trady-adapters/src/news/financial_news.rs) and
  [SourceItem](crates/trady-domain/src/research.rs).
- **CoinGecko route and batching:** the implementation uses one coin's
  `/coins/markets` response rather than the catalog's batched `/simple/price`
  example. The normalized payload lacks the example's 24-hour change. See
  [CoinGecko request construction](crates/trady-adapters/src/financial/mod.rs)
  and [MarketContext](crates/trady-domain/src/financial.rs).
- **Tiingo corporate actions:** the implementation reads raw daily OHLCV. It does
  not normalize the catalog's dividend/split fields, adjusted series or symbol
  metadata route. See [daily normalization](crates/trady-adapters/src/financial/parse.rs).
- **Provider resource accounting:** the catalog's FMP bandwidth, Tiingo
  hourly/unique-symbol/bandwidth and GDELT minimum-spacing requirements have no
  independent counters in the shared ledger. Conservative call ceilings and the
  response-size cap cannot represent all of those account policies. See
  [BudgetLimits](crates/trady-application/src/financial.rs) and
  [reserve_credits](crates/trady-storage/src/financial.rs).
- **Offline artifact provenance:** yfinance import accepts canonical daily-bar
  JSON and records import-time availability. There is no pinned Python importer
  or artifact envelope containing its revision, original download identity and
  adjustment/session configuration as proposed by the catalog. See
  [offline_page](crates/trady-adapters/src/financial/parse.rs).

# EGM contract bootstrap drift addendum — 2026-10-31

Scope: the 48 README contracts modified by this bootstrap/remediation, reviewed
sequentially against current working-tree source. Earlier financial-data findings
above are preserved. The findings below concern documentation coverage; the
listed interfaces already exist in code. Live workflow readiness is recorded in
the [remediation report](docs/workflow-remediation-2026-10-31.md).

## README.md

No drift detected.

## config/README.md

No drift detected.

## config/credentials/postgresql/README.md

No drift detected.

## crates/README.md

No drift detected.

## crates/trady-adapters/README.md

No drift detected.

## crates/trady-adapters/src/README.md

No drift detected.

## crates/trady-adapters/src/autonomous/README.md

No drift detected.

## crates/trady-adapters/src/financial/README.md

No drift detected.

## crates/trady-adapters/src/finbert/README.md

No drift detected.

## crates/trady-adapters/src/inputs_service/README.md

- **Public limits are missing from the contract.** [mod.rs](crates/trady-adapters/src/inputs_service/mod.rs) exports `MAX_IMPORTS = 20_000` and `MAX_IMPORT_BYTES = 8 * 1024 * 1024` (lines 29–30). [store.rs](crates/trady-adapters/src/inputs_service/store.rs) uses the first for active store capacity and half the second for raw-document bytes; configuration validation also uses the second as the response-size ceiling. Add the two constants and distinguish these scopes in Interface/Invariants.
- **Payload helper contracts are missing.** `ImportPayload::kind` and `ImportPayload::key` in [mod.rs](crates/trady-adapters/src/inputs_service/mod.rs) (lines 238–264) are public but absent from the interface inventory. `kind` maps payload variants to import kinds; `key` chooses a kind-specific identity, including `account:instrument:market_snapshot` for forecasts. Document those helpers and the compound-key rule.

## crates/trady-adapters/src/intelligence/README.md

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

## crates/trady-adapters/src/weekly_lab/README.md

No drift detected.

## crates/trady-application/README.md

No drift detected.

## crates/trady-application/src/README.md

- **Source fingerprint helper is omitted.** `FinancialSource::registry_hash` in [financial.rs](crates/trady-application/src/financial.rs) (lines 179–181) hashes the serialized source configuration. The contract lists `FinancialSource` and registry synchronization but omits this public helper and its fingerprint scope.
- **Invalidation matching contracts are omitted.** `Invalidation::matches` and `RejectedInput::matches` in [intelligence.rs](crates/trady-application/src/intelligence.rs) (lines 185–198 and 236–247) require equal scopes and select observation, source or lineage identity according to the invalidation target. The README inventories the records but does not expose these public matching helpers or their target-specific rules.

## crates/trady-application/src/model_lab/README.md

No drift detected.

## crates/trady-application/src/sentiment/README.md

No drift detected.

## crates/trady-cli/README.md

No drift detected.

## crates/trady-cli/src/README.md

No drift detected.

## crates/trady-cli/src/opencode/README.md

No drift detected.

## crates/trady-domain/README.md

No drift detected.

## crates/trady-domain/src/README.md

No drift detected.

## crates/trady-domain/src/intelligence/README.md

- **The public type inventory is incomplete.** The Interface table omits 102 public record/enum definitions across the ten feature modules. Add the following types to the contract, or provide linked subordinate contracts that inventory them:
- [accounting.rs](crates/trady-domain/src/intelligence/accounting.rs): `FinancialConcept`, `FinancialPeriodKind`, `FinancialPeriod`, `FinancialUnit`, `ShareCountAdjustment`, `ComparableFinancialFact`, `AccountingMetric`, `AccountingCohort`, `AccountingPeerObservation`, `AnomalySignal`.
- [attention.rs](crates/trady-domain/src/intelligence/attention.rs): `AttentionSemantics`, `AttentionEntityKind`, `AttentionAgentCoverage`, `VerifiedAttentionEvent`, `AttentionThresholds`, `AttentionStatus`, `AttentionPattern`, `AttentionEventTiming`, `AttentionEventComparison`, `AttentionQueryAssessment`, `AttentionConcordance`.
- [coordination.rs](crates/trady-domain/src/intelligence/coordination.rs): `PromotionKind`, `CoordinationCoverage`, `CoordinationThresholds`, `CoordinationMarketDeviation`, `SourceBurst`.
- [cross_asset.rs](crates/trady-domain/src/intelligence/cross_asset.rs): `CrossAssetKind`, `CrossAssetUnit`, `CrossAssetObservation`, `CrossAssetSeries`, `CrossAssetPair`, `CrossAssetStressMethod`, `CrossAssetStressDirection`, `CrossAssetStress`, `CrossAssetBreadthMember`, `CrossAssetBreadth`, `CrossAssetStatus`, `CrossAssetAlignedValue`, `CrossAssetInterval`, `CrossAssetFlowAssessment`, `CrossAssetStressAssessment`, `CrossAssetBreadthAssessment`.
- [event.rs](crates/trady-domain/src/intelligence/event.rs): `EstimateDispersionDefinition`, `EventUnit`, `ActualObservation`, `EventPrice`, `EventPricingProxy`, `EventConfounder`, `SurpriseNormalization`, `EventOutcome`.
- [insider.rs](crates/trady-domain/src/intelligence/insider.rs): `OwnershipTable`, `FilingCoverage`, `FilingFootnote`, `ReportingOwner`, `InsiderLineRef`, `InsiderSplitAdjustment`, `InsiderHolding`, `InsiderTransactionKind`, `InsiderRowAssessment`.
- [microstructure.rs](crates/trady-domain/src/intelligence/microstructure.rs): `MicrostructureBookState`, `MicrostructureFeedKind`, `MicrostructureLevel`, `MicrostructureRouting`, `MicrostructureAuction`, `MicrostructureMarkoutSide`, `MicrostructureMarkout`, `MicrostructureStatus`.
- [options.rs](crates/trady-domain/src/intelligence/options.rs): `OptionsRight`, `OptionsExercise`, `OptionsUnderlying`, `OptionsPricingAssumptions`, `OptionsVolatilityObservation`, `OptionsEvent`, `OptionsStatus`, `OptionsIssue`, `OptionsIvInterval`, `OptionsSkew`.
- [short_interest.rs](crates/trady-domain/src/intelligence/short_interest.rs): `ShortPositionReport`, `ShortFloatObservation`, `ShortVolumeScope`, `ShortAverageVolume`, `ShortBorrowObservation`, `ShortMarketContext`, `ShortCrowdingThresholds`, `ShortCrowdingFlag`, `ShortInterestIssue`.
- [tax.rs](crates/trady-domain/src/intelligence/tax.rs): `TaxLotMethod`, `TaxLossTreatment`, `TaxAllowanceAllocation`, `TaxRateRange`, `TaxFxRate`, `TaxSplit`, `TaxWithholding`, `AnnualTaxState`, `TaxCashObservation`, `TaxDisposalScenario`, `TaxSafetySnapshot`, `TaxAssessmentStatus`, `TaxScenarioKind`, `TaxAmountRange`, `TaxLotAllocation`, `TaxScenarioEstimate`.

## crates/trady-domain/src/sentiment/README.md

No drift detected.

## crates/trady-policy/README.md

No drift detected.

## crates/trady-policy/src/README.md

No drift detected.

## crates/trady-policy/src/intelligence/README.md

No drift detected.

## crates/trady-policy/src/limits/README.md

No drift detected.

## crates/trady-policy/src/prediction/README.md

No drift detected.

## crates/trady-policy/src/sentiment/README.md

No drift detected.

## crates/trady-storage/README.md

No drift detected.

## crates/trady-storage/src/README.md

- **Gateway delivery record is omitted.** [research_jobs.rs](crates/trady-storage/src/research_jobs.rs) defines public `GatewayDelivery` (lines 847–854), returned by `PgResearchJobs::pending_deliveries`, with `id`, `bot_id`, `owner_user_id`, `chat_id`, `scope` and `text`. The README describes outbox delivery but omits this public response contract.
- **Research/gateway callable inventory is incomplete.** The public `PgResearchJobs` methods `install_policy`, `get_job`, `recent_jobs`, `checked_readiness_reference`, `pending_deliveries`, `acknowledge_delivery`, `selected_session`, `select_session` and `bind_job_session` are absent from the named interface in the README. Their implementations are in [research_jobs.rs](crates/trady-storage/src/research_jobs.rs). Add the callable names and map the existing broad descriptions to their read/write, analyst/gateway-role and lease requirements.
- **Release callable inventory is incomplete.** [weekly.rs](crates/trady-storage/src/weekly.rs) exposes `ReleaseCandidate::subject_hash`, `PgActiveReleaseReader::resolve_active`, and `PgReleaseStore::{record_evidence,advance,active_formula}`. The README describes release behavior broadly but omits these individual public entry points. Name them and bind the existing read-only authority, evidence-write and transition descriptions to the appropriate methods.

## docs/v1/features/README.md

No drift detected.

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

No drift detected.

## scripts/README.md

No drift detected.

## services/README.md

No drift detected.

## services/finbert/README.md

No drift detected.

## services/runtime/README.md

No drift detected.

# Full-tree bootstrap audit follow-up — 2026-10-31

This follow-up covers all 51 source-directory contracts. It preserves the prior
report. The entries record corrected differences; remaining caller/deployment
boundaries are documented in the contracts and consolidated audit report.

## README.md

- Corrected: Added the current audit entry point and the discovered Python lab verification command.

## crates/README.md

- Corrected: Rolled up the optional intelligence boundaries and application-local model-lab persistence.

## crates/trady-adapters/README.md

No drift detected.

## crates/trady-adapters/src/README.md

No drift detected.

## crates/trady-adapters/src/autonomous/README.md

No drift detected.

## crates/trady-adapters/src/financial/README.md

No drift detected.

## crates/trady-adapters/src/finbert/README.md

No drift detected.

## crates/trady-adapters/src/inputs_service/README.md

No drift detected.

## crates/trady-adapters/src/intelligence/README.md

No drift detected.

## crates/trady-adapters/src/news/README.md

- Corrected: Added provider-enforced publication/discovery clocks, continuation and raw-row saturation rules, and provenance-only publisher URL behavior.

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

- Corrected: Distinguished the supplied input binding from the recorded request-byte hash and documented required helper component-map entries.

## crates/trady-application/README.md

- Corrected: Qualified the external-storage boundary to acknowledge local model-lab journal and holdout artifacts.

## crates/trady-application/src/README.md

- Corrected: Qualified local model-lab persistence and separated force/cadence bypass from the narrow legacy parser retry exception.

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

- Corrected: Corrected the child reply timer boundary: process launch, I/O setup and enqueue precede the monotonic wait timer.

## crates/trady-cli/src/finbert/README.md

No drift detected.

## crates/trady-cli/src/opencode/README.md

No drift detected.

## crates/trady-cli/src/telegram/README.md

No drift detected.

## crates/trady-domain/README.md

No drift detected.

## crates/trady-domain/src/README.md

No drift detected.

## crates/trady-domain/src/intelligence/README.md

No drift detected.

## crates/trady-domain/src/sentiment/README.md

No drift detected.

## crates/trady-policy/README.md

No drift detected.

## crates/trady-policy/src/README.md

No drift detected.

## crates/trady-policy/src/intelligence/README.md

No drift detected.

## crates/trady-policy/src/limits/README.md

No drift detected.

## crates/trady-policy/src/prediction/README.md

No drift detected.

## crates/trady-policy/src/sentiment/README.md

No drift detected.

## crates/trady-storage/README.md

- Corrected: Corrected assessment invalidation timing and retention clamping to the observation deadline.

## crates/trady-storage/src/README.md

- Corrected: Corrected changed-content invalidation codes, assessment invalidation timing and retention clamping.

## lab/README.md

No drift detected.

## lab/evaluator/README.md

- Corrected: Corrected missing-payoff behavior and qualified the owner/mode checks on existing diagnostic directories.

## lab/lean/README.md

- Corrected: Corrected executable hashing to construction time rather than each request.

## lab/proofs/README.md

- Corrected: Qualified Lean account/reference validation, private dispatch and caller-supplied revision bindings.

## lab/proofs/TradyProof/README.md

- Corrected: Qualified account-reference validation: account identity, mode, side and the complete status allowlist are outside these checks.

## migrations/README.md

- Corrected: Corrected role/function grants, nullable validation boundaries, financial constraints, confirmation/readiness checks and explicit intelligence erasure behavior.

## scripts/README.md

- Corrected: Added the credential-file default and documented container-loopback authentication without saved DSN endpoint correspondence.

## services/README.md

No drift detected.

## services/finbert/README.md

- Corrected: Corrected Content-Length rejection behavior: missing, zero, negative and oversized lengths return HTTP 413.

## services/runtime/README.md

- Corrected: Qualified evaluator isolation: permitting AF_UNIX alone does not deny PostgreSQL Unix sockets.

## Implementation remediation, 2026-11-01

The entries above preserve the original documentation audit. Subsequent
implementation fixes and final execution evidence are recorded in the
[remediation report](./docs/audit-2026-10-31/remediation.md) and
[verification record](./docs/audit-2026-10-31/remediation-verification.json).

- Evaluator contracts now describe mandatory Linux seccomp/TSYNC isolation,
  exact diagnostic-directory ownership and modes, and execution from verified,
  sealed executable snapshots.
- Storage contracts now describe append-only migration 11, null-rejecting
  weekly/release gates, current readiness, durable credit ceilings, intelligence
  content/rights validation, permanent registry retirements and erased-lineage
  tombstones. Protected publication, cleanup and runtime authority boundaries
  require `READ COMMITTED`; unsupported transaction isolation fails explicitly.
- Application contracts now describe valid non-tax registry registration before
  input/query validation and disabled-feature returns, including rejected input
  and unavailable reads.
- Credential contracts now describe saved-endpoint SCRAM identity validation,
  conservative HBA checks, narrow DSN parsing, private stdin transport and file
  plus server serialization. The server lock and protected commands share one
  administrative connection, including rollback.
- Weekly helper and proof contracts now describe complete manifest validation,
  checked component lookups, the 26-theorem inventory, original numeric lexemes,
  checked Unicode escapes and distinct input-rejection evidence. Finite parity
  retains its explicitly bounded scope.
- News contracts now distinguish malformed GDELT responses from valid empty
  article arrays and document the 60-second default backoff. The sampled upstream
  HTTP 429 remains an external availability limitation.

Historical audit results and prior migrations were retained. Trading212 API
requests remain excluded from verification.

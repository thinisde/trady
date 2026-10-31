# Application boundaries

## Overview

[lib.rs](./lib.rs) owns configuration, ports and reconciliation.
[contracts.rs](./contracts.rs) owns syntax validation, with no research authority.
[harness.rs](./harness.rs) owns durable model-session orchestration through
provider, tool and repository ports. Concrete storage/provider implementations
remain outside this crate.
[research.rs](./research.rs) coordinates registry-driven collection; the
[Telegram source contract](./telegram/README.md) covers owner authorization and
read-only monitoring. [perpetual.rs](./perpetual.rs) binds advisory predictions to
persisted scenarios; [screening.rs](./screening.rs) collects and stores research
shortlists. [Autonomous execution](./autonomous/README.md) consumes validated
readiness/formula evidence; [optimization](./optimization/README.md) owns approved
formula bindings and bounded weekly experiment ports.

## Interface

- `[financial.rs:FinancialRepository]` / `FinancialProvider`: shared bounded
  collection and snapshot ports. `FinancialConfig::validate`, `collect`, `snapshot`,
  `signals` and `model_features` enforce approved-source, consumer and temporal
  boundaries; the [operator guide](../../../docs/financial-data.md) covers use.
  `[financial.rs:BudgetLimits]`, `ProviderAccount`, `DataRequest`, `FinancialSource`,
  `DataWindow`, `CollectionLease`, `FinancialPage`, `FinancialFailure` and
  `FinancialCollectionReport` expose bounded configuration, requests and outcomes.
  `[financial.rs:FinancialConfig::account]` resolves provider accounts;
  `[financial.rs:FinancialRepository]` declares `sync_registry`, `reserve`,
  `finish`, `fail` and `read`. Callers synchronize the registry; `collect` does not.

- [research_jobs.rs](./research_jobs.rs): `JobPolicy`, typed `Work`, durable
  `ResearchJobRepository`, `ResearchInformation`, verifier packets, source/claim
  dependencies, syndication clustering, `score_sentiment_job` and immutable
  `SentimentReadinessRef`. The executable composition and trusted-verifier handoff
  are documented in [research automation](../../trady-adapters/src/research_jobs/README.md).
  `[research_jobs.rs:ResearchJob]`, `JobStatus`, `JobLease`, `Verification`,
  `SourceDependency`, `ClaimReview` and `VerifiedResearchPacket` describe queue and
  verifier records. `JobPolicy::validate`, `Work::validate` and
  `VerifiedResearchPacket::validate` expose their explicit checks.
  `[research_jobs.rs:digest]`, `hash_id`, `syndication_clusters`,
  `check_dependencies`, `readiness_reference` and `valid_tool_input` provide
  identity/dependency/readiness/schema helpers. `[research_jobs.rs:HoldingWatch]`,
  `daily_key` and `watchdog_key` supply scheduling records/keys; cursor persistence,
  cancellation and redelivery coalescing belong to callers/storage.

- [Point-in-time model lab](./model_lab/README.md): supplied chronological
  observations, four-outcome fitting, separate calibration/validation, durable
  sealed-holdout evaluation and exact-calculation inference. Fitting produces no
  deployment approval; TEST ONLY artifacts remain production-ineligible.

- [x_monitoring.rs](./x_monitoring.rs): `[x_monitoring.rs:XRule]`, `XBudgetPolicy`,
  `XResources`, `XCostPlan`, `XCostScope`, `XCostRepository`, `XCountsSource`,
  `XCountsReport`, `fingerprint`, `planned_post_cost` and `measure_x_counts`
  define versioned author queries, integer resource costs and resumable preflight.
  `[x_monitoring.rs:XRule::hash]`, `XBudgetPolicy::estimate` and `XCostPlan::costs`
  expose rule identity and checked estimates. `[x_monitoring.rs:XCostRepository]`
  declares `reserve`/`settle`; implementations serialize billing reservations.

- `[sentiment/mod.rs:SentimentProvider]`, `assess_sentiment`, `text_sha256`,
  `validate_probabilities`: provider port, rights-aware projection and correlated
  sentiment observations; [contract](./sentiment/README.md).

- `[research.rs:ResearchConfig]`, `SourceConfig`, `XAccount`: bounded registry and
  weekly-reviewed account configuration; `validate`, `activation_gap`, `valid_key`, `valid_x_id`.
- `[research.rs:NewsSource]`, `ResearchRepository`, `CollectionRequest`, `SourcePage`,
  `SourceFailure`, `CollectionReport`: source-neutral collection/repository ports and outcomes.
- `[research.rs:prepare_state]`, `reserve_source_requests`, `collect_source`: registry
  fingerprint handling, durable daily request reservations and bounded collection.
  `[research.rs:SourceFailure::unavailable]` creates an unavailable source result
  without a retry time.
- `[perpetual.rs:PerpetualRepository]`: mutable `load_context`, `save_assessment`,
  `load_assessment`, `save_review` ports. `[perpetual.rs:PerpetualAdvisor::assess]`
  returns `AdvisoryOutput { assessment, model }`. Public use cases are `assess`,
  `validate_record`, `validate_review_for_scenario`, `record_review`.
- `[screening.rs:UniverseSource::fetch_universe]`,
  `ScreeningDataPort::observations`, `ScreeningRepository::store_screening_snapshot`:
  source/observation/persistence ports. `ScreeningRequest`, `ScreeningSnapshot`,
  `ScreeningFailure`, `MAX_SNAPSHOT_BYTES`, `hex_digest` and `run_screening`
  describe scoped research runs, including failed attempts.
- `[autonomous/mod.rs:AutonomousTrader]`, `TradingConfig`, `LiveRelease`,
  `ExecutionBroker`, `TradingInputs`, `ExecutionJournal`, `TradingState`,
  `recover_order` and readiness records: see the [component contract](./autonomous/README.md).
- `[optimization/mod.rs:FormulaAuthority]`, `ApprovedFormula`, `BoundCalculation`,
  `FormulaProposal`, `calculate_authoritative_limits` and the re-exported weekly
  state/store/lab/decision APIs: see the [component contract](./optimization/README.md).

- [broker.rs](./broker.rs): `BrokerExecutionPort`, `BrokerOrderRequest`, `OrderKind`,
  `TimeInForce`, sanitized `BrokerFailureKind`/`BrokerFailure`/`BrokerResult` and `CancellationReceipt`.
  `[broker.rs:BrokerExecutionPort]` declares `submit_order` and `cancel_order`;
  callers establish durable reservation, current fencing, readiness, protection
  and available sell quantity before invoking these transport operations.
- `[broker.rs:TimeInForce]` has `Day` and `GoodTillCancel`; `[broker.rs:OrderKind]`
  has `Market { extended_hours }`, `Limit { price, time_in_force }`,
  `Stop { price, time_in_force }` and `StopLimit { stop, limit, time_in_force }`.
  Requests carry local intent/account IDs, instrument/currency, side and positive quantity.
- `[broker.rs:BrokerFailure]` exposes `kind`, optional `http_status`,
  `retry_after_seconds`, `broker_order_id` and `outcome_unknown`. `new(kind)` initializes
  optional fields to `None` and ambiguity to false. `[broker.rs:BrokerFailureKind]`
  distinguishes invalid request/response, account mismatch, unsupported operation,
  transport, HTTP, rejection and rate limiting. `CancellationReceipt` contains the broker ID.
  Sanitized diagnostics are a producer obligation; the DTO does not scrub supplied
  strings. `[broker.rs:BrokerFailure::fmt]` displays structured diagnostic fields.

- `[lib.rs:FoundationConfig]`: `validate` and `entry_blockers`.
- `[lib.rs:Clock]`, `ExchangeCalendar`, `ExchangeSession`, `QuotePort`:
  reproducible time/session/quote interfaces.
- `[lib.rs:BrokerInformationPort]`: account snapshot reads; `[lib.rs:BrokerPort]`
  additionally exposes capabilities and intent lookup.
- `[lib.rs:ExecutionRepository]`: account leases, unresolved intents, reconciliation
  storage and paper fixture reservations.
- `[lib.rs:reconcile]`, `reconcile_account`: observation/repository reconciliation.
- `[contracts.rs:Contract]`, `Contracts`, `ValidatedDocument`, `LLM_SCHEMA`,
  `RESEARCH_SCHEMA`: nine document selectors, validators and immutable validated JSON.
  The selectors are `ModelTurn`, `ToolResults`, `ProposalEnvelope`,
  `StoreArtifactArguments`, `StoreArtifactResult`, `PriorityArguments`,
  `PriorityResult`, `RotationArguments`, `RotationResult`; `parse_json` exposes
  the strict JSON parser. `parse_json_bounded(input, maximum_bytes)` adds an
  explicit limit up to 8 MiB. Proposal validation selects `ApplicationProposalEnvelope`.
  `[contracts.rs:Contracts::new]` constructs the validators;
  `[contracts.rs:ValidatedDocument]` exposes `contract` and borrowed `value` accessors.
- `[telegram/mod.rs:Monitor]`, `OwnerAccess`, `IncomingMessage`, `IncomingUpdate`,
  `MonitorCommand`, `CommandReply`, `BrokerScope`: owner monitoring through an
  independent optional broker scope; details are in the child contract.
  `[telegram/gateway.rs:authenticate]`, `GatewayCommand`, `GatewayRequest` and
  `OwnerControlContext` add typed authenticated routing and control-preview bindings;
  durable queue/control handling belongs to supplied workflows.
- `[harness.rs:AgentHarness]`: `new` and
  `respond(repo, provider, tools, request, on_text)`. `[harness.rs:RunInput]` wraps
  `scope`, `session_id`, optional new `input` and a `CancellationToken`.
- `[harness.rs:Scope]`, `ProviderIdentity`, `RunLimits`, `SessionKind`,
  `SessionStatus`, `RunStatus`, `Session`, `SessionLease`, `SessionRepository`:
  scoped session/run state, limits and fenced persistence ports.
- `[harness.rs:LlmProvider]`, `ModelRequest`, `Completion`, `ProviderError`,
  `Usage`, `ChatMessage`, `NativeCall`, `NativeFunction`, `CancellationToken`:
  synchronous generation, streaming callback and cooperative cancellation.
  `[harness.rs:ModelRequest]` carries the stable stored `session_id` on every
  generation call, including resumed runs.
- `[harness.rs:ToolExecutor]`, `ToolSpec`, `ToolOutput`: versioned registered
  information tools, argument/result schemas and typed results. Native aliases
  replace dots with underscores and append `_v1` (`[harness.rs:ToolSpec::alias]`).
- `[harness.rs:PinnedSkill]`, `MemoryItem`, `HistoryMessage`, `SessionSummary`,
  `ContextManifest`, `Attempt`, `RunEvent`, `ToolRecord`, `RunRecord`,
  `PROMPT_VERSION`, `content_hash`, `new_id`, `valid_id`: persisted context, tool
  and run records plus prompt/hash/identifier helpers. `RunLimits::default`,
  `validate`, `RunStatus::terminal`, `Usage::total`, `ChatMessage::text`,
  `Session::{new, charged_tokens, active_run}`, `CancellationToken::{cancel,
  is_cancelled}` and `ToolOutput::{ok, error}` expose their record helpers.

## Invariants

- `[financial.rs:collect]` requires a prefetch reservation and validates each
  observation's source, entity, receipt and requested-window semantics.
  `[financial.rs:snapshot]` rechecks current rights and excludes future or expired
  inputs; unavailable/partial/stale data do not produce price features.
  `[financial.rs:model_features]` requires a numerical-consumer snapshot and bounded
  lineage, recomputes derived values and uses the existing Perpetual feature type.
  Callers obtain these trusted records from the rights-aware snapshot/repository
  boundary; a deserialized consumer label establishes no permission.
  `[financial.rs:FinancialConfig::validate]` requires version 1, research-only
  operation, disabled automatic paid fallback, at most 11 unique provider accounts
  and 64 unique sources. `[financial.rs:BudgetLimits::validate]` requires
  `0 < minute <= day <= month <= 1000000`. `[financial.rs:DataWindow::validate]`
  bounds windows to 366 days ending before today. Model consumers must separately
  approve the schema produced by `[financial.rs:model_features]`.

- `[research_jobs.rs:Work::validate]` scopes jobs by owner, explicit paper/shadow/live
  account namespace and policy version; job policy mode must match that namespace.
  `[research_jobs.rs:VerifiedResearchPacket::validate]` and `check_dependencies`
  check supplied policy registry, content/rights hashes, issuer mapping and model
  revision before scoring/readiness. Source-registry hashes receive format checks;
  current source rights and trusted fresh materialization require repository/caller
  resolution. `[research_jobs.rs:readiness_reference]` counts concrete syndication chains once; ambiguous
  origins and unresolved contradictions cannot establish entry readiness.

- `[x_monitoring.rs:XRule::query]` restricts search to one author, literal OR terms
  and an explicit repost choice. `[research.rs:prepare_state]` requires a higher
  version for changed approved rules and clears counts/cursors on registry changes.
- `[x_monitoring.rs:XBudgetPolicy::validate]` checks cycle/rates/allocations and
  review references; `active` requires an enabled policy inside its approved cycle.
  `[research.rs:collect_source]` requires a reviewed active X budget and fresh,
  complete, affordable per-rule counts for recent search before provider calls;
  X timeline/lookup do not use the search-count preflight.
- `[x_monitoring.rs:measure_x_counts]` commits pending buckets/tokens independently
  of observation watermarks. Missing/gapped/repeated pages remain unknown; pending
  pagination can bypass cadence, preserving provider backoff and daily request caps.

- `[sentiment/mod.rs:assess_sentiment]` clones/redacts source items for model use,
  requires known current English text and distinct lineages, rejects present
  social metadata reporting missing context, and accepts absent social metadata.
  It fails the batch for missing/mismatched/invalid/truncated predictions.
  It preserves source times/expiry and hashes exactly the selected text. Callers
  resolve instrument relevance and recheck current time through threshold policy.

- `[research.rs:collect_source]` admits configured sources only, preserves the original
  pending interval, commits before cursor progression and never advances the successful
  watermark on a source error or page cap. `--force` cannot bypass retry/debit limits.
- `[research.rs:reserve_source_requests]` commits conservative request debits before I/O;
  `[research.rs:prepare_state]` resets changed source cursors without refunding allowance.
- `[research.rs:SourceConfig::activation_gap]` blocks disabled or unverified/
  stale-review timeline/search X accounts. X lookup has no `x_account` identity
  metadata. Collection does not verify claims or authorize policy/order changes.
- `[research.rs:ResearchConfig::validate]` bounds pages to 1–10, page size to
  5–100, lookback to 60–604800 seconds and overlap to 1–3600 seconds below lookback.
  Sources have unique bounded IDs, explicit rights/retention, allowed hosts and
  restricted credential-environment names. X timeline/recent-search activation requires configured
  ownership/user-ID metadata with nonfuture verification at most seven days old;
  it performs no external ownership verification.
- `[research.rs:collect_source]` requires the exact registered source configuration
  and valid collection interval. Durable per-page request debits are conservative:
  X timelines/search/counts reserve 2, Google/publisher RSS 4 and other sources 1,
  without refunds. Separate per-resource monetary accounting is a transport contract.
  Completion alone advances the watermark; a completed page may still report gaps.
- `[research.rs:ResearchConfig::validate]` permits 1–32 sources; source cadence is
  60–86400 seconds, daily allowance 1–10000 and retention 1–8760 hours. Credential
  configuration validates restricted environment-variable names, not availability.
- `[research.rs:ResearchRepository::commit_page]` requires atomic persistence
  from implementations. Shared-provider allowances require
  `[research.rs:ResearchRepository::reserve_shared_requests]` enforcement; its
  default returns `true` without enforcing a shared budget.
- `[perpetual.rs:PerpetualRepository]` implementations must join actual referenced
  records and atomically store immutable assessment/input/provenance bindings.
  `[perpetual.rs:assess]` validates scenario/context, advisor identity and current
  model provenance before saving. OK advice requires an approved model, matching
  feature snapshot, in-range features and bounded expiry; regression/interval/
  contribution output is unsupported by this application boundary.
- `[perpetual.rs:validate_review_for_scenario]` requires a current OK assessment,
  exact final scenario, valid current model/features and bounded quote/FX drift.
  `record_review` can retain non-OK/expired advice for audit; activation needs the
  separate scenario validation. Reviews grant no order authority.
- `[screening.rs:run_screening]` validates owner/run/policy and keeps an empty
  owner allowlist restrictive. Missing observations remain missing. Successful
  and sanitized failed runs are stored; snapshots over 8 MiB drop source/result
  payloads and record `InvalidInputs`. Repository implementations must atomically
  bind owner/run/content, deduplicate identical retries and reject changed retries.
- `[autonomous/mod.rs:AutonomousTrader]` is a separate trading workflow from the
  paper-only foundation reconciliation. Its journal must fence atomic revisions;
  readiness requires current dossier, skill, screening, advice and formula bindings.
  Shadow blocks entry; live requires configured release gates. See its child
  contract for actual submission/protection/recovery behavior and prerequisites.

- `[broker.rs:BrokerOrderRequest::validate]` requires positive quantity and supplied
  prices. This transport request is not execution authority. `[broker.rs:BrokerFailure]`
  carries an explicit unknown-outcome flag; `[broker.rs:CancellationReceipt]` confirms
  request acceptance, never terminal cancellation.
  The intent ID is correlation data rather than an idempotency guarantee.
  `[broker.rs:BrokerFailure::outcome_unknown]` requires caller reconciliation
  before further mutation when true.

- Configuration requires version 1, valid policy, paper mode and a `paper_` account
  (`[lib.rs:FoundationConfig::validate]`). Entry blockers always include missing
  readiness verification, approved skills, Perpetual and the validated execution/protection workflow.
  `[lib.rs:FoundationConfig::entry_blockers]` adds a capability blocker unless
  cash-account, instrument, protective-order and quote capabilities are usable.
- `[lib.rs:ExchangeSession::permits_entry]` uses explicit opening/closing timestamps
  and blocks entries with 30 minutes or less remaining.
- `[lib.rs:reconcile_observed]` checks snapshot/intent validity, freshness and account
  identity. Unmatched/ambiguous local intent associations, inconsistent orders,
  untracked nonterminal orders and unexplained holdings require recovery.
  Observed holdings are never declared protected.
- `[lib.rs:reconcile_account]` checks paper/account identity and uses prior stored
  observations outside `RecoveryRequired` when checking holding changes, then
  saves through a fenced repository.
  Its caller acquires/renews the fence; reconciliation does not submit/cancel orders
  or invoke broker intent lookup.
- `[contracts.rs:Contracts::validate]` limits input to 262144 bytes; rejects duplicate
  object keys, trailing documents, unsupported schema fields/versions, invalid UTC
  calendar timestamps, unsupported decimal precision, invalid price ranges and targets
  over 10%. Envelopes require positive lifetime at most exactly five minutes; entry
  envelopes require account and dossier bindings. These are syntax/shape checks.
  Validation does not resolve evidence or check an envelope against the current
  clock. `[contracts.rs:semantic_shape]` also requires unique `call_id`, `claim_id`
  and `skill_name` values within their respective arrays.
- Schema-valid research, model-declared readiness and tool names do not authorize
  entry. Harness context instructions do not confer executor authority
  (`[harness.rs:AgentHarness::context]`).
- `[harness.rs:Scope::validate]` requires bounded owner/account identifiers, a
  `paper_`, `shadow_` or `live_` account namespace and positive policy version.
  These are information scopes, not execution grants. `[harness.rs:Session::new]`
  validates limits, skill-body hashes, a nonempty model and governing context of
  at most 32000 bytes. New sessions are active `OwnerResearch` sessions with
  schema version 1, revision 0 and seven-day expiry.
- `[harness.rs:RunLimits::validate]` bounds model turns to 2–32, tool calls to
  1–128 and tool rounds below model turns. Deadlines are 2–240 seconds, provider
  timeouts 1–30 seconds and tool timeouts 1–15 seconds; context/output/run/session
  token budgets and tool-result byte limits are checked separately. Defaults are
  6 model turns, 4 tool rounds, 12 calls and a 120-second deadline.
- `[harness.rs:AgentHarness::locked]` requires an unexpired, nonclosed/nonfailed
  session, compatible stored schema/prompt/hash, matching provider identity/tool
  revision and collision-free tool names/aliases. New nonblank input
  is bounded to 16000 bytes and requires no active last run; `input = None`
  resumes that active run with its original deadline. One retry allowance is
  shared by retryable provider failures and invalid structured output. Budget
  reservations charge reported usage, or reserved tokens when usage is absent.
- `[harness.rs:AgentHarness::normalize]` validates native or structured model
  turns. Native batches contain 1–4 registered function aliases. Reusing a
  recorded call ID requires the same tool, arguments hash and version;
  `[harness.rs:AgentHarness::dispatch]` checks argument and successful-result
  schemas and per-result/cumulative retained data byte limits. Any output carrying
  data receives result-schema checks, including a non-OK output.
  `max_tool_input_bytes` caps cumulative retained result-data JSON.
- `[harness.rs:AgentHarness::context]` rechecks skill hashes, treats memory and
  generated history summaries as untrusted data, and keeps history in whole run
  groups. The byte-based token estimate must fit the smaller of 60% of the context
  window and the window minus output allowance and 1024 tokens. Original history
  remains stored; context manifests record selected message/skill hashes,
  exclusions, revisions, scope, provider and estimates.
- `[harness.rs:SessionRepository::save]` requires atomic revision, worker,
  fencing-generation and server-time-expiry checks from its implementation.
  Provider timeout is passed to the adapter; the harness checks cancellation and
  the overall deadline after generation without independently timing that request.
  Tool deadlines are cooperative, and adapters must enforce their timeout contracts
  (`[harness.rs:LlmProvider]`, `[harness.rs:ToolExecutor]`).
  Registered tools must expose reviewed information operations, and provider
  failures must redact credentials; these are adapter/caller obligations.
- `[harness.rs:AgentHarness::dispatch]` can add `ToolOutput::trusted_skill` from
  an OK output after checking its body hash. An existing skill name requires
  identical hash/version; integrity failures propagate.
- `[harness.rs:AgentHarness::dispatch]` reuses persisted tool results, but a
  started tool without a saved result can execute again on resume. Terminal
  finalization supplies `CANCELLED` results for missing pending calls and appends
  correlated native tool messages or a `TOOL_RESULTS` envelope before the final
  response. Propagated errors can leave an unfinished run for recovery
  (`[harness.rs:AgentHarness::finish]`, `[harness.rs:AgentHarness::append_results]`).

## Side Effects

`[financial.rs:collect]` calls supplied reservation, provider, commit and failure
ports. Pure snapshot/signal/feature construction performs no network or database
operation and grants no execution authority.

`[research_jobs.rs:score_sentiment_job]` calls the supplied sentiment provider and
repository ports; numerical outputs come exclusively from the provider. Pure
dependency/readiness helpers perform no I/O and authorize no plan or order.

`[x_monitoring.rs:measure_x_counts]` reads state, reserves requests, invokes counts
and commits pagination/status/backoff without advancing observation watermarks.
Concrete network/storage and serialized billing effects belong to implementations.

`[sentiment/mod.rs:assess_sentiment]` calls the supplied sentiment provider after local
eligibility checks; it does not persist assessments or mutate financial plans.

`[research.rs:collect_source]` and `reserve_source_requests` invoke supplied source and
repository ports. Concrete network/database effects belong to their adapters.

`[perpetual.rs:assess]` loads referenced context, generates an assessment UUID,
invokes the advisor and persists the result before returning. Review use cases
load assessments and save accepted review records. `[screening.rs:run_screening]`
calls source/observation ports and persists success or sanitized failure;
provider failure may return `Ok` with `snapshot.failure` populated.

`[autonomous/mod.rs:AutonomousTrader]` renews execution leases, persists plans/order
attempts and can submit/cancel through broker ports. `[optimization/weekly.rs:run_weekly]`
freezes/checkpoints state and invokes supplied trusted-lab ports; generated weekly
decisions authorize neither entry nor live activation.

`[lib.rs:reconcile_account]` reads through ports and persists reconciliation.
Contract validation performs no provider or database calls; schemas are embedded.

`[harness.rs:AgentHarness::respond]` acquires a 45-second session lease for a new
worker and releases it after the run operation. The harness saves input/run state,
provider attempts, tool starts/results, context manifests and terminal state
through the repository. The PostgreSQL implementation renews lease expiry on
each successful save (`[../../trady-storage/src/sessions.rs:save]`). Provider calls
stream through `on_text`; tools execute
sequentially. Cancellation or elapsed tool deadlines replace returned outputs with
`CANCELLED` (`[harness.rs:AgentHarness::dispatch]`).

`[harness.rs:AgentHarness::finish]` sets run/session `WaitingOwner` for
`NEEDS_INPUT`; other finished runs leave the session active, including failed or
cancelled runs. Every owner-research finish, including failure/cancellation,
refreshes the seven-day session expiry.
History and run-event records are session metadata, separate from financial
execution state.

`[telegram/mod.rs:Monitor]` changes its in-memory observation cache and returns
unsent replies/alerts. It performs no Telegram transport or broker-execution calls.

## Verification

Run `cargo test -p trady-application --test research_jobs --locked` for job scope,
provenance/rights/model invalidation, sentiment-correlation and owner-control gates.

Run `cargo test -p trady-application --locked` from the repository root.
Run `cargo test --workspace --locked` for workspace verification and
`python3 scripts/test-postgres.py` for isolated PostgreSQL integration checks.

# Application boundaries

## Overview

[lib.rs](./lib.rs) owns configuration, ports and reconciliation.
[contracts.rs](./contracts.rs) owns syntax validation, with no research authority.
[harness.rs](./harness.rs) owns durable model-session orchestration through
provider, tool and repository ports. Concrete storage/provider implementations
remain outside this crate.
[research.rs](./research.rs) coordinates registry-driven collection; the
[Telegram source contract](./telegram/README.md) covers owner authorization and
read-only monitoring.

## Interface

- `[sentiment/mod.rs:SentimentProvider]`, `assess_sentiment`, `text_sha256`,
  `validate_probabilities`: provider port, rights-aware projection and correlated
  sentiment observations; [contract](./sentiment/README.md).

- `[research.rs:ResearchConfig]`, `SourceConfig`, `XAccount`: bounded registry and
  weekly-reviewed account configuration; `validate`, `activation_gap`, `valid_key`, `valid_x_id`.
- `[research.rs:NewsSource]`, `ResearchRepository`, `CollectionRequest`, `SourcePage`,
  `SourceFailure`, `CollectionReport`: source-neutral collection/repository ports and outcomes.
- `[research.rs:prepare_state]`, `reserve_source_requests`, `collect_source`: registry
  fingerprint handling, durable daily request reservations and bounded collection.

- [broker.rs](./broker.rs): `BrokerExecutionPort`, `BrokerOrderRequest`, `OrderKind`,
  `TimeInForce`, sanitized `BrokerFailureKind`/`BrokerFailure`/`BrokerResult` and `CancellationReceipt`.
- `[broker.rs:TimeInForce]` has `Day` and `GoodTillCancel`; `[broker.rs:OrderKind]`
  has `Market { extended_hours }`, `Limit { price, time_in_force }`,
  `Stop { price, time_in_force }` and `StopLimit { stop, limit, time_in_force }`.
  Requests carry local intent/account IDs, instrument/currency, side and positive quantity.
- `[broker.rs:BrokerFailure]` exposes `kind`, optional `http_status`,
  `retry_after_seconds`, `broker_order_id` and `outcome_unknown`. `new(kind)` initializes
  optional fields to `None` and ambiguity to false. `[broker.rs:BrokerFailureKind]`
  distinguishes invalid request/response, account mismatch, unsupported operation,
  transport, HTTP, rejection and rate limiting. `CancellationReceipt` contains the broker ID.

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
  the strict JSON parser. Proposal validation selects `ApplicationProposalEnvelope`.
- `[telegram/mod.rs:Monitor]`, `OwnerAccess`, `IncomingMessage`, `IncomingUpdate`,
  `MonitorCommand`, `CommandReply`, `BrokerScope`: owner monitoring through an
  independent optional broker scope; details are in the child contract.
- `[harness.rs:AgentHarness]`: `new` and
  `respond(repo, provider, tools, request, on_text)`. `[harness.rs:RunInput]` wraps
  `scope`, `session_id`, optional new `input` and a `CancellationToken`.
- `[harness.rs:Scope]`, `ProviderIdentity`, `RunLimits`, `SessionKind`,
  `SessionStatus`, `RunStatus`, `Session`, `SessionLease`, `SessionRepository`:
  scoped session/run state, limits and fenced persistence ports.
- `[harness.rs:LlmProvider]`, `ModelRequest`, `Completion`, `ProviderError`,
  `Usage`, `ChatMessage`, `NativeCall`, `NativeFunction`, `CancellationToken`:
  synchronous generation, streaming callback and cooperative cancellation.
- `[harness.rs:ToolExecutor]`, `ToolSpec`, `ToolOutput`: versioned registered
  information tools, argument/result schemas and typed results. Native aliases
  replace dots with underscores and append `_v1` (`[harness.rs:ToolSpec::alias]`).
- `[harness.rs:PinnedSkill]`, `MemoryItem`, `HistoryMessage`, `SessionSummary`,
  `ContextManifest`, `Attempt`, `RunEvent`, `ToolRecord`, `RunRecord`,
  `content_hash`, `new_id`, `valid_id`: persisted context, tool and run records
  plus hash/identifier helpers.

## Invariants

- `[sentiment/mod.rs:assess_sentiment]` clones/redacts source items for model use,
  requires known current English text with complete social context and distinct
  lineages, and fails the batch for missing/mismatched/invalid/truncated predictions.
  It preserves source times/expiry and hashes exactly the selected text. Callers
  resolve instrument relevance and recheck current time through threshold policy.

- `[research.rs:collect_source]` admits configured sources only, preserves the original
  pending interval, commits before cursor progression and never advances the successful
  watermark on a source error or page cap. `--force` cannot bypass retry/debit limits.
- `[research.rs:reserve_source_requests]` commits conservative request debits before I/O;
  `[research.rs:prepare_state]` resets changed source cursors without refunding allowance.
- `[research.rs:SourceConfig::activation_gap]` blocks disabled/unverified/stale-review X
  accounts. Collection does not verify claims or authorize policy/order changes.
- `[research.rs:ResearchConfig::validate]` bounds pages to 1–10, page size to
  5–100, lookback to 60–604800 seconds and overlap to 1–3600 seconds below lookback.
  Sources have unique bounded IDs, explicit rights/retention, allowed hosts and
  restricted credential-environment names. X activation requires configured
  ownership/user-ID metadata with nonfuture verification at most seven days old;
  it performs no external ownership verification.
- `[research.rs:collect_source]` requires the exact registered source configuration
  and valid collection interval. Durable per-page request debits are conservative:
  X timelines reserve 2, Google/publisher RSS 4 and other sources 1, without refunds.
  Completion alone advances the watermark; a completed page may still report gaps.

- `[broker.rs:BrokerOrderRequest::validate]` requires positive quantity and supplied
  prices. This transport request is not execution authority. `[broker.rs:BrokerFailure]`
  carries an explicit unknown-outcome flag; `[broker.rs:CancellationReceipt]` confirms
  request acceptance, never terminal cancellation.
  The intent ID is correlation data rather than an idempotency guarantee.

- Configuration requires version 1, valid policy, paper mode and a `paper_` account
  (`[lib.rs:FoundationConfig::validate]`). Entry blockers always include missing
  readiness verification, approved skills, Perpetual and the validated execution/protection workflow.
- `[lib.rs:ExchangeSession::permits_entry]` uses explicit opening/closing timestamps
  and blocks entries with 30 minutes or less remaining.
- `[lib.rs:reconcile_observed]` checks snapshot/intent validity, freshness and account
  identity. Unmatched, ambiguous or inconsistent orders and unexplained holdings
  require recovery. Observed holdings are never declared protected.
- `[lib.rs:reconcile_account]` checks paper/account identity and uses prior stored
  observations when checking holding changes, then saves through a fenced repository.
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
  `paper_` account and positive policy version. `[harness.rs:Session::new]`
  validates limits, skill-body hashes, a nonempty model and governing context of
  at most 32000 bytes. New sessions are active `OwnerResearch` sessions with
  schema version 1, revision 0 and seven-day expiry.
- `[harness.rs:RunLimits::validate]` bounds model turns to 2–32, tool calls to
  1–128 and tool rounds below model turns. Deadlines are 2–240 seconds, provider
  timeouts 1–30 seconds and tool timeouts 1–15 seconds; context/output/run/session
  token budgets and tool-result byte limits are checked separately. Defaults are
  6 model turns, 4 tool rounds, 12 calls and a 120-second deadline.
- `[harness.rs:AgentHarness::locked]` requires an unexpired, nonclosed/nonfailed
  session and matching provider identity and tool revision. New nonblank input
  is bounded to 16000 bytes and requires no active last run; `input = None`
  resumes that active run with its original deadline. One retry allowance is
  shared by retryable provider failures and invalid structured output. Budget
  reservations charge reported usage, or reserved tokens when usage is absent.
- `[harness.rs:AgentHarness::normalize]` validates native or structured model
  turns. Native batches contain 1–4 registered function aliases. Reusing a
  recorded call ID requires the same tool, arguments hash and version;
  `[harness.rs:AgentHarness::dispatch]` checks argument and successful-result
  schemas and per-result/cumulative retained data byte limits.
- `[harness.rs:AgentHarness::context]` rechecks skill hashes, treats memory and
  generated history summaries as untrusted data, and keeps history in whole run
  groups. The byte-based token estimate must fit the smaller of 60% of the context
  window and the window minus output allowance and 1024 tokens. Original history
  remains stored; context manifests record selected message/skill hashes,
  exclusions, revisions, scope, provider and estimates.
- `[harness.rs:SessionRepository::save]` requires atomic revision, worker,
  fencing-generation and server-time-expiry checks from its implementation.
  Provider timeouts and tool deadlines are cooperative: synchronous calls are
  checked around their return, and adapters must enforce their timeout contracts
  (`[harness.rs:LlmProvider]`, `[harness.rs:ToolExecutor]`).
- `[harness.rs:AgentHarness::dispatch]` reuses persisted tool results, but a
  started tool without a saved result can execute again on resume. Terminal
  finalization supplies `CANCELLED` results for missing pending calls and appends
  correlated native tool messages or a `TOOL_RESULTS` envelope before the final
  response. Propagated errors can leave an unfinished run for recovery
  (`[harness.rs:AgentHarness::finish]`, `[harness.rs:AgentHarness::append_results]`).

## Side Effects

`[sentiment/mod.rs:assess_sentiment]` calls the supplied sentiment provider after local
eligibility checks; it does not persist assessments or mutate financial plans.

`[research.rs:collect_source]` and `reserve_source_requests` invoke supplied source and
repository ports. Concrete network/database effects belong to their adapters.

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
cancelled runs. Owner-research completion refreshes the seven-day session expiry.
History and run-event records are session metadata, separate from financial
execution state.

`[telegram/mod.rs:Monitor]` changes its in-memory observation cache and returns
unsent replies/alerts. It performs no Telegram transport or broker-execution calls.

## Verification

Run `cargo test -p trady-application --locked` from the repository root.
Run `cargo test --workspace --locked` for workspace verification and
`python3 scripts/test-postgres.py` for isolated PostgreSQL integration checks.

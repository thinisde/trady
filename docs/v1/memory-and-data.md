# Trady v1 sessions, memory and PostgreSQL data

Status: target data and LLM contracts; implemented subsets are recorded in the
[foundation](../foundation.md), [news](../news.md) and [OpenCode](../opencode/README.md)
guides. FinBERT assessment persistence below is planned, not existing DDL.

## Authority and storage

Use a dedicated `trady` database in the existing PostgreSQL server when its version, capacity and access arrangement are verified. PostgreSQL is authoritative for policy, research history, model sessions, position plans, broker reconciliation and queues. Local Markdown/JSON memory files are readable projections with database revision metadata; they are not a second source of financial truth.

| Information | Authoritative writer | Use in a model context |
|---|---|---|
| Owner policy and watchlist | Executor applies authenticated owner requests | Read-only governing rules; model cannot rewrite them |
| Instrument mapping and account/position state | Executor through validated broker synchronization | Timestamped snapshot, never an inferred cash balance |
| News items and evidence | Registered collectors and verification workflow | Untrusted cited data with rights/freshness metadata |
| Research proposals and explanations | Analyst | Hypotheses awaiting deterministic checks |
| Active plans, orders, fills and ledger | Executor | Read-only facts; not editable model memory |
| Session summaries and strategy observations | Analyst | Derived context with evidence and expiry |
| Manual knowledge notes | Authenticated owner request, stored by analyst | Attributed context; never policy merely because the owner wrote a note |

Do not reuse the Paperless application's schema, tables, credentials or document store. Database provisioning and host-level access restrictions are described in [operations](./operations.md).

## Session control

A session groups related model turns; a research run is one bounded job that may contain several model/tool rounds using that session. Owner follow-ups create later runs in the same dialogue. Session types are `DAILY_MARKET`, `POSITION`, `OWNER_RESEARCH` and `WEEKLY_REVIEW`. A held position has a dedicated ongoing session so its thesis, changed thresholds and failed hypotheses remain connected across days and restarts.

Each session records its ID and type, subject/position ID, creator, status, model/provider identity, prompt/schema versions, owner-policy version, creation/last-use/expiry times, token/cost totals and current summary revision. Use `ACTIVE`, `WAITING_OWNER`, `COMPACTING`, `CLOSED` and `FAILED` lifecycle states. A failed run does not delete its messages or necessarily close its session.

Default daily-market sessions close after end-of-day reconciliation; position sessions close after confirmed liquidation and outcome capture; owner research sessions expire after seven inactive days; weekly-review sessions close when the report is stored. Expired sessions may be read through retrieval but cannot silently continue with an outdated policy snapshot.

Append messages with a per-session sequence number, role, timestamps, content hash, relevant evidence IDs and usage. Serialize generation within a session using a leased job. Coalesce overlapping holding updates; a second event does not append a competing model answer to the same generation. Discard a result for execution if its policy/expected-plan version changed or refreshed account facts invalidate it, while retaining it for audit. A permissible quote/account refresh records new validation snapshots rather than silently rewriting the original model context.

Persist model-turn IDs, native/canonical call IDs, argument hashes, tool/skill versions, typed results/errors, continuation metadata and tool-cost reservations. Retain assistant tool requests and matching results in order; do not flatten them into a chat summary before the next round. Model output is the JSON `TOOL_REQUEST` or `FINAL` envelope from [LLM conversations and tools](./llm-and-tools.md), and tool results are JSON too. Structured research artifacts remain available when a dialogue ends with no action or a clarification.

The owner can list sessions, inspect the current one, start a fresh owner-research session and close an owner-research session through [Telegram](./telegram.md). A reset changes conversational context; it does not clear durable memory, policy, orders or position protection. An owner cannot close the financial state machine by closing a conversation.

## Context assembly and limits

Preserve the historical design's configurable OpenAI-compatible HTTP endpoint as an initial adapter option, with base URL, secret reference and model selected by deployment. Keep its wire format inside `LlmProvider`; provider-native protocols can use separate adapters. Compatibility is a deployment claim to test, not proof that session state, structured output, usage or tool behavior matches another provider. Local sessions, validation and budget accounting remain authoritative for every adapter.

Build a new explicit context for every model call rather than relying on opaque provider-side conversation state. The local session remains authoritative if the provider offers conversation IDs; store those IDs only as adapter metadata.

Assemble context in this order:

1. Versioned system rules, permitted output schema and the separation between proposals and orders.
2. Current owner policy, approved instruments and required risk/target ceilings.
3. Approved pinned runtime skill bodies and the allowed versioned tool catalog.
4. Fresh synchronized account, current holding, active plan and pending-order summary.
5. Current research question, verified evidence, source gaps, quotes and contradictions.
6. Relevant retrieved knowledge, historical outcomes and a compact session summary.
7. Recent dialogue and complete tool-request/result pairs needed for the next turn.

Tag each retrieved fragment with its ID, author/source, time, version, rights, confidence/verification status and trust class. Keep all source material and memory separate from system instructions. Never retrieve broker or Telegram secrets into a prompt.

Record a context manifest containing the exact snapshot/plan/policy versions, evidence and chunk revisions, skill bundle/version/hashes and completion references, tool schemas/call/result IDs, content hashes, summary revision, model, token estimate and exclusions. This makes a proposal explainable without treating a later edited note as the original evidence. Retention rules still govern raw source text; a manifest does not authorize storing it forever.

Read context/output capacity from verified provider configuration. Reserve the configured maximum output plus system/tool overhead and a 10% estimation margin; also cap assembled input at 60% of the declared context window by default. Use the smaller allowed input budget. Local estimates are conservative when the exact tokenizer is unavailable. If governing rules, the current position and minimum evidence do not fit, return a context-budget error and no proposal rather than trimming policy.

Drop irrelevant old turns and low-ranked historical chunks first. Compact before the input limit is reached: create a new summary revision with the thesis, unresolved questions, source references, accepted/rejected plan changes and invalidation conditions. Validate its references and preserve the original allowed records; never summarize an order status into a replacement ledger fact. A failed compaction leaves the prior summary and current plan in place.

Initial run limits: one active generation workflow per session, up to six model turns, four tool rounds and twelve calls within a 120-second run deadline. A provider call has at most 30 seconds, and one transient/repair retry consumes the same turn/deadline/cost allowance. Reserve finalization capacity and enforce the additional limits in [LLM conversations and tools](./llm-and-tools.md#application-loop-and-limits). An uncertain provider charge is reserved against the cost budget. Retry output is a new recorded attempt and cannot duplicate a proposal or an order.

## Proposal contract

Use the [exact versioned JSON contracts](./llm-contracts.md) inside a canonical `FINAL` message. Supported proposal kinds are `ENTRY_PROPOSAL`, `PLAN_UPDATE`, `HOLD` and `NO_ACTION`; ordinary dialogue may instead have a null proposal and a structured reply/research artifact. Reject unknown fields and enum values, malformed decimals, invalid IDs, non-finite values, unsupported currencies and missing required references. Schema-constrained provider output is useful when available but never replaces local validation.

Model-owned body fields are separate from the application envelope:

| Model body field | Contract |
|---|---|
| `schema_version` | Exactly 1 |
| `kind` | One of the four research kinds |
| `instrument_id` | Approved synchronized instrument for an actionable plan; nullable for market-wide `NO_ACTION` |
| `context_refs` | Account/market snapshot and confidence/economics calculation IDs actually supplied to the run; nullable members only for nonactionable outcomes |
| `evidence_ids` | Existing permitted evidence supplied to this run |
| `reason`, `thesis`, `uncertainties`, `invalidation_conditions` | Bounded research text/lists; thesis may be null; text is not an executable condition |

| Application envelope field | Contract |
|---|---|
| `envelope_version` | Exactly 1 |
| `proposal_id`, `run_id`, `session_id` | Application-generated IDs bound to the request |
| `policy_version`, `context_manifest_id`, `account_snapshot_id` | Trusted policy/context binding; not model-selected account authority |
| `research_dossier_id` | Application-bound finalized artifacts/skill evidence; non-null for entry |
| `created_at`, `valid_until` | Trusted UTC timestamps; proposal lifetime at most five minutes |
| `body` | Schema-validated model proposal body |

`ENTRY_PROPOSAL` adds `entry_range`, `currency`, `stop_price`, `target_net_fraction`, nullable `target_price_hint`, complete `trailing`, `holding_deadline_session` and nullable `exceptional_catalyst`. The exact nested keys/types are normative in the linked schema. Quantities and authoritative prices are calculated by the executor, not accepted from the model.

`PLAN_UPDATE` requires `position_id`, `expected_plan_version` and every replacement threshold field, with no new `entry_range`. Omitted fields cannot implicitly remove a stop or reset a peak. `HOLD` requires position/version references but has no threshold fields and cannot extend expiry. `NO_ACTION` permits only the common body fields and cannot reserve an entry slot. Model-supplied application-envelope keys are rejected as extras.

A natural-language invalidation condition remains research context. Only typed price/time conditions supported by policy can become deterministic exit conditions. Unknown conditions trigger a clarification/research failure, never arbitrary code execution. Refusal, truncation, timeout, invalid output or missing evidence leaves the current position plan active and produces no new entry.

## Persistent memory files

Proposed analyst-owned projection layout, outside the source checkout:

```text
/var/lib/trady/memory/
  index.json
  owner/preferences.md
  instruments/<instrument-id>.md
  positions/<position-id>/thesis.md
  positions/<position-id>/timeline.md
  sessions/<session-id>/summary.md
  daily/<exchange-session-id>.md
  lessons/<lesson-id>.md
```

Every file includes its record ID, database revision, content checksum, trust class, source references, generated timestamp and expiry where relevant. Use application-generated path-safe IDs; a model cannot choose a filesystem path. Store credentials in a separate secret facility, never in memory files.

Commit the canonical memory revision and a projection job in one transaction. Write a temporary sibling file, flush as required and atomically rename; then mark the projection revision complete. If the file write fails, retry the projection job without repeating model generation. A startup reconciliation detects missing or mismatched files and rebuilds them from the database.

Manual edits are not imported automatically. `/memory add` creates an attributed note; a future explicit import workflow would validate file path, rights and provenance. `/memory forget` removes eligible research/owner-memory content and its derived projections after confirmation, but cannot erase orders, fills, ledger events or active protection. The result names retained audit records and tombstones affected references.

## Knowledge base and retrieval

Start with PostgreSQL full-text search, tags, instrument/event/session filters and recency ranking. No vector extension is required for v1 or assumed present in `paperless-db-1`. A later embedding adapter may add semantic retrieval after extension availability, retention and cost review.

Maintain separate policy, evidence, instrument-reference, session-summary and outcome/lesson collections. Return up to eight permitted chunks within the context token budget, including relevant counterexamples or losing/no-trade outcomes when available. Rank by instrument/event relevance, verification, recency and source independence; a heavily repeated rumor cannot outrank a primary correction merely by frequency.

Expired, deleted, unlicensed and contradicted chunks are excluded from actionable evidence. Historical reads may show tombstones and explanatory metadata. Summaries or embeddings derived from deleted source text inherit applicable deletion restrictions. A weekly lesson is a hypothesis with supporting sample counts and counterexamples; adopting it as policy requires a separate authenticated policy version.

## Planned relational model

Names below are proposed tables, not existing DDL. Use foreign keys, explicit currency fields, timezone-aware timestamps and checked `NUMERIC` representations sized for supported broker precision. Scope all money, plans and uniqueness constraints to account/mode. Broker IDs remain external identifiers alongside internal IDs.

| Schema and tables | Purpose and important constraints |
|---|---|
| `policy.versions`, `policy.instruments` | Append-only approved configuration and instrument mapping; one active policy per account/mode |
| `research.sources`, `research.cursors`, `research.collection_runs` | Source registry, progress and completeness; unique source/scope cursor |
| `research.items`, `research.evidence`, `research.evidence_links` | Unique source/external-ID items, deduplicated claims, support/contradiction and provenance |
| `research.sessions`, `research.messages`, `research.context_manifests` | Session lifecycle and exact allowed context; unique session/sequence |
| `research.runs`, `research.proposals`, `research.reports` | Job attempts, schema-checked immutable model bodies and published research results |
| `research.tool_calls`, `research.tool_results` | Unique run/call ID, tool/version/argument hash, bounded result/error, status, usage and request/result correlation |
| `research.skill_revisions`, `research.skill_runs` | Approved skill bundle versions/hashes, loaded/completed/blocked state and checked artifact references |
| `research.artifacts`, `research.artifact_dependencies`, `research.readiness_dossiers` | Exact stage outputs, source/skill/model dependencies and immutable entry dossiers |
| `research.universe_snapshots`, `research.index_memberships`, `research.classifications` | Effective-dated S&P 500 members/share classes, GICS/provider editions and permitted instrument intersection |
| `research.entities`, `research.entity_aliases`, `research.company_profiles`, `research.profile_facts`, `research.profile_reviews` | Stable identities, revisioned profiles, period/unit-aware facts and validated finalist reviews |
| `research.person_roles`, `research.social_accounts`, `research.relationships`, `research.mention_events` | Effective roles/handles, ownership evidence, directed economic links and attributed praise/criticism/quote/repost events |
| `research.social_subscriptions`, `research.social_events`, `research.coverage_intervals` | Approved listener bindings, provider/post/version dedupe and explicit successful/partial/unrecoverable intervals |
| `research.watchdog_templates`, `research.watchdog_bindings`, `research.watchdog_events` | Validated selectors, real position/plan binding, lifecycle, coalescing and health alerts |
| `research.ml_models`, `research.ml_datasets`, `research.feature_snapshots`, `research.ml_validation_reports`, `research.ml_assessments`, `research.ml_outcomes` | Approved manifests, point-in-time lineage/features, calibrated advice, LLM use/disagreement and delayed outcomes |
| `research.knowledge_documents`, `research.knowledge_chunks` | Versioned attributed knowledge and full-text retrieval |
| `research.memory_revisions`, `research.memory_projections` | Canonical memory and reconstructable file projection state |
| `execution.accounts`, `execution.account_snapshots`, `execution.market_snapshots` | Reconciled cash/quotes/FX and freshness/coverage |
| `execution.positions`, `execution.plans`, `execution.risk_checks` | Quantity, active-plan pointer, immutable plan revisions and validation results |
| `execution.entry_readiness` | Executor-owned acceptance/rejection of finalized dossiers, score at least 90 and current execution-time checks |
| `execution.entry_slots`, `execution.rotation_authorizations` | Default one account/session entry slot; optional rotation uses unique typed ordinal slots and one serialized acceptance; no two ordinary attempts |
| `execution.order_intents`, `execution.orders`, `execution.fills` | Intent before effect, unique client intent and broker order/fill identifiers |
| `execution.cash_events`, `execution.ledger_events`, `execution.portfolio_snapshots` | Append-only external flows, fees/dividends/corrections and valuations |
| `execution.account_leases` | One current executor generation per account, database-time expiry |
| `control.commands`, `control.confirmations`, `control.notification_outbox` | Authorized ingress, single-use confirmation payloads and durable message delivery |
| `control.information_requests`, `control.information_results` | Typed account-scoped broker-information refresh requests and sanitized executor-owned responses; no trading payloads |
| `control.jobs`, `control.job_attempts` | Typed leased work with stable deduplication keys and bounded retries |
| `audit.events`, `audit.operating_costs`, `audit.performance_reports` | Process-attributed immutable events, external expenses and computed reports |

Store quote currency and FX direction explicitly. Avoid floating-point money and mutable balance fields as an alternative to a ledger. Financial corrections append reversing/correcting events. Licensed text can be removed without cascading into financial records: financial rows retain permitted opaque evidence IDs/hash metadata, not compulsory copies of the article.

Profiles, graph ownership, refresh/deletion and projections are detailed in [company intelligence](./company-intelligence.md); subscription/watchdog lifecycles in [listeners/watchdogs](./watchdogs-and-x.md); typed writes in [research contracts](./research-contracts.md); model lineage in [Perpetual](./perpetual.md). The model has no arbitrary SQL, direct fact-verification authority or model-promotion permissions.

Store all permitted collected outcomes, including exclusions and inaccessible sources. Rights/deletion propagate through claims, profiles, graph edges, social features, summaries and projections. Model artifacts may need quarantine/retirement under dataset rights; a financial audit reference cannot require retaining deleted article content.

## Sentiment assessment lineage

The [FinBERT adapter](./finbert/README.md) currently returns typed assessments/CLI JSON
without database writes. Plan `research.sentiment_assessments` and
`research.sentiment_evidence` as analyst-owned immutable records: model ID/revision,
wire/calculation versions, inference time, issuer/instrument scope, source/item and
lineage IDs, selected-text hashes, original source times/expiry, three probabilities,
base/adjusted confidence, resulting ceiling and explicit unavailable reasons.
No table/migration for these records is included in this change.

Bind each future readiness dossier or proposed holding update to these exact
records and the current approved model/policy version. The executor rechecks age,
expiry, invalidation and threshold constraints, then records its own acceptance.
An optimistic model response cannot overwrite an old assessment or active plan.
Source corrections/deletion, permission changes, instrument mapping changes and
model/policy changes invalidate dependent results. Apply source retention/deletion
requirements to derived probabilities and quoted text as well as raw items; do not
retain source content in logs or assume a content hash grants perpetual retention.

## Transactions, queues and permissions

Runtime identities are `trady_analyst`, `trady_executor` and `trady_gateway`. A `trady_owner` role owns objects; a separate migrator may assume it only during deployment. Runtime roles cannot own schemas, create roles/databases, change grants or run migrations.

Analyst writes research records and permitted research jobs, reads approved policy and sanitized account/plan views, and cannot write execution state. Executor writes execution state, approved policy versions and execution audit events, reads proposals and command requests, and cannot rewrite source evidence to justify an order. Gateway writes authenticated commands/confirmations, reads owner-facing report views and handles notification delivery; it cannot write plans, fills or policy versions directly.

The source registry is an exception to general analyst-owned research data: analysts may read `research.sources` and update authorized collection progress, but cannot change approved domains, credentials or rights. Registry changes use the reviewed deployment/migration path. Owner-confirmed spending caps are policy changes applied by the executor, not collector edits.

The analyst may enqueue only schema-validated read-only broker information requests through narrow functions. The executor validates their account/mode/operation and writes sanitized results; gateway reads owner-facing views. Tool results cannot impersonate owner commands or create trading intents. The approved skill bundle is deployment-owned and read-only to runtime identities; recording a skill revision does not grant permission to alter its body.

Expose typed enqueue/view functions or narrowly granted tables for shared jobs, notification requests and audit writes. Prevent the analyst from inserting an owner-command job or consuming executor work simply because both use `control.jobs`. Bind each event's producer identity in the database wrapper or separate writable queue table; a caller-supplied `producer=owner` field is insufficient. Grant runtime access to exact objects/functions, revoke unintended `PUBLIC` grants, set safe search paths, and keep security-definer functions narrowly scoped with fixed object resolution.

Use short transactions and explicit row locks or compare-and-swap versions for active plans, slot reservation and command application. Durable queues are at least once. Claim jobs with a lock/lease, commit before remote work and deduplicate effects by stable keys. Notifications may be delivered twice after a send/ack crash; order submission may not be retried merely because a job lease expired.

The executor renews its account lease against database time and carries a fencing generation on writes. Loss of the lease or database connection stops new broker submissions. Takeover reconciles broker state, outstanding/unknown intents and possible predecessor requests before acting; an external broker that ignores fencing requires this recovery step.

Put policy activation, command completion and audit event in one transaction. Put an order intent, entry-slot reservation or exit latch, and required outbox events in one transaction. Broker calls occur after commit; acknowledged broker IDs/fills are then reconciled. This gives auditable recovery, not an impossible atomic database-and-broker transaction.

## Retention and recovery defaults

Proposed defaults, capped by actual provider terms: allowed raw news content at most seven days; permitted evidence summaries 90 days; raw model message bodies 30 days; owner/session summaries 180 days; permitted financial and execution metadata for the owner's configured recordkeeping period. Purge derived source text when its source rights require it. Daily backups and restore checks cover canonical records; projections can be regenerated.

Record retention policy versions and purge results. Restoring an old backup must reapply deletion tombstones before research retrieval resumes. An unavailable memory filesystem does not stop price monitoring. An unavailable database stops new execution work and leaves broker-native protection in force. See [operations](./operations.md) for startup reconciliation and incident handling.

## Verification

The required checks include session serialization, bounded context construction, stale plan rejection, memory-file reconstruction, privilege denial, queue deduplication, retention propagation and restart after an ambiguous broker call. [Implementation](./implementation.md) defines concrete acceptance cases. No database or files are provisioned by this documentation task.

# Trady v1 architecture

Status: target Rust architecture with implemented subsets identified in the
[foundation](../foundation.md), [news](../news.md), [OpenCode](../opencode/README.md)
and [FinBERT](./finbert/README.md) records. Full services/readiness/execution remain incomplete.

## Process boundaries

Use one Cargo workspace and one deployable Rust application with three subcommands. Run them as separate services and operating-system identities. Process separation prevents a model-facing component from acquiring trading authority through an accidental tool call.

| Planned process | Responsibilities | Credentials and network access |
|---|---|---|
| `trady analyst` | Sources/listeners, sector/group screening, profiles/relationships, watchdog research scheduling, Perpetual inference, JSON sessions/tools/skills, reports/proposals and memory | News/LLM credentials, approved local ML artifacts, permitted domains and restricted research DB role; no broker trading credential |
| `trady executor` | Broker reconciliation, read-only information-request handling, quotes, readiness/risk validation, plan activation, entry sizing, exit monitoring, order state machines | Broker trading credential, market-data credential, restricted executor database role; no model tools or arbitrary web browsing |
| `trady gateway` | Telegram ingress, owner authentication, command records, notifications and acknowledgements | Telegram bot credential and restricted command/outbox database role; no broker or LLM credential |
| `finbert` inference sidecar | CPU classification of permitted English X/news/related texts; pinned tokenizer/model revision; no execution authority | Localhost/private HTTP and model cache; no Trady database, broker, Telegram or LLM credentials |

PostgreSQL is the durable handoff between processes. An outbox/job table provides work queues; notifications may wake workers but are never the only durable record. A dedicated queue service is unnecessary for v1. File memory belongs to the analyst and cannot act as an execution queue.

The executor loads owner-approved policy from a versioned database record. A Telegram policy command creates an authenticated request; the executor validates and versions the supported change. The analyst cannot impersonate that command or edit active policy.

## Planned Rust workspace

| Proposed package | Owns | Must not depend on |
|---|---|---|
| `trady-domain` | Typed IDs, decimal money, instruments, proposals, plan versions, order states, confidence inputs | HTTP clients, database drivers, provider DTOs |
| `trady-policy` | Pure sizing, confidence, target, stop, eligibility and transition validation | Network, filesystem, LLM responses as instructions |
| `trady-application` | Use cases, state machines, port definitions, scheduling orchestration | Concrete broker/news implementations |
| `trady-storage` | PostgreSQL repositories, transactions, leases, outbox and memory projection metadata | Trading decisions from natural-language text |
| `trady-adapters` | Broker, quotes, source-specific news/RSS/scrapers, Telegram, LLM HTTP adapters | Authority to bypass application policy |
| `trady-cli` | Configuration, process wiring, shutdown and service entry points | Strategy logic duplicated outside the policy package |

All Trady application services use Rust. The separately hosted FinBERT container
uses Python/PyTorch/Transformers behind a provider-neutral Rust port. Its Compose
files and adapter exist but the container has not been started. Preserve
provider-neutral interfaces and fake adapters so library choices do not change
policy behavior; the workspace manifests record selected Rust dependencies.

Use decimal types for prices, money, fees and quantities; checked arithmetic; explicit currency conversion; UTC timestamps; exchange-local session identifiers; and closed enums for lifecycle states. Validate deserialized provider responses before converting them to domain values. Failures return typed errors rather than panic in an order workflow.

## Planned ports

These rows describe target contracts; existing traits may implement only a subset.
The FinBERT row is implemented as described in its linked contract.

| Port | Required operations or data |
|---|---|
| `BrokerPort` | Account, positions, open orders, individual status, fills, cash events, instruments; submit validated entry/exit; cancel; discover verified order capabilities |
| `QuotePort` | Bid/ask, observed and received timestamps, venue/feed coverage, currency, tradability, halts, FX observations, historical bars |
| `NewsSource` | Bounded cursor collection, individual-item lookup, provenance, rights, completeness and failure metadata |
| `SentimentProvider` | Implemented `predict` and `model_revision`; `assess_sentiment` binds permitted distinct-lineage source text to exact FinBERT predictions; see [contract](./finbert/README.md) |
| `LlmProvider` | Bounded structured generation with model identity, usage, context capacity, timeout and cancellation |
| `ToolDispatcher` | Versioned JSON schemas, scope/budget checks, correlated requests/results and approved information-tool execution |
| `SkillRepository` | Approved skill metadata/bodies, pinned versions/hashes and progressive loading without arbitrary paths |
| `BrokerInformationPort` | Sanitized account/position/order/history reads and typed refresh requests; no order mutation |
| `ResearchRepository` | Evidence, sessions, retrieval snapshots, proposals, memory revisions and run records |
| `ExecutionRepository` | Account snapshots, entry-slot reservation, position/plan compare-and-swap, order intent, fills and ledger |
| `NotificationPort` | Transactional outbox events with stable delivery keys |
| `Clock` / `ExchangeCalendar` | Reproducible time, sessions, holidays, early closes and trading cutoffs |

Do not expose `place_order` as an LLM tool, include broker secrets in prompts, or let a model choose a destination URL outside the source registry. Domain objects cross application boundaries; provider field names stay inside adapters.

## Research-to-entry flow

After collection/relevance verification, the analyst requests a required FinBERT
assessment for selected English texts and calculates the [sentiment overlay](./finbert/README.md#threshold-calculation-version-1).
Bind its model revision, item hashes/expiry and adjusted points/ceiling into the
future readiness dossier. Required inference failure blocks entry/raised targets;
it does not stop current price protection. The pure calculation and CLI preview
exist; this durable dossier/executor binding remains planned.

1. The scheduler or an authenticated owner request creates an idempotent research job.
2. Collectors ingest permitted material; verifiers map instruments, collapse repeated stories, and retain contradictions.
3. The analyst creates a session context from approved policy, synchronized account state, relevant memory, evidence and quotes.
4. The analyst loads the required scenario skills and runs bounded JSON tool-request/result rounds until a validated final answer, clarification or incomplete result. A final proposal may be `ENTRY_PROPOSAL`, `PLAN_UPDATE`, `HOLD`, or `NO_ACTION`; none is an order command.
5. The analyst stores every turn/tool result, final artifacts, skill-completion/readiness dossier, validation errors, usage and context manifest. Only a finalized eligible dossier queues a proposal-ready event for new entry.
6. The executor checks complete required research/skills, independent-source comparison, resolved contradictions and at least 90 confidence, then rechecks policy, quote freshness, account state and economics. A stale, incomplete or invalid proposal produces a recorded rejection.
7. For an entry, a database transaction reserves the session's entry slot and stores the sized order intent. Commit before contacting the broker.
8. The executor submits once, reconciles the actual fill and protects the filled quantity. Telegram receives a research decision and separately an execution outcome.

No transaction is held open across an LLM request or broker request. Durable intent plus reconciliation handles the gap between a database commit and an external effect.

## Holding and exit flow

Price observation → fresh executable bid and FX validation → update favorable peak → evaluate latched exit, hard stop, trailing floor, target and time conditions → persist one exit intent → coordinate existing protective orders → submit/reconcile → report actual outcome.

In parallel, fresh evidence can produce a `PLAN_UPDATE` referencing the active position, old plan version, and owner-policy version. The executor serializes that update against price processing. It first evaluates the old plan against the newest accepted quote. If an exit condition is already met, the exit wins. Otherwise it may atomically activate a validated new plan. If the new target is already met, activation immediately latches an exit.

LLM downtime does not stop price monitoring. Telegram downtime does not stop protection. Native broker protection is required for live overnight holdings because a server process alone cannot protect a position during an outage.

## Research component ownership and event flow

| Component / owner | Input → output | Concurrency and failure |
|---|---|---|
| `UniverseRepository` / analyst read path | Effective member/classification/owner/broker records → eligible snapshot | Immutable snapshot; unavailable membership blocks entry |
| `SectorScanner` / analyst | Common market/evidence snapshot → sector/group/finalist records | Bounded multi-run jobs; explicit incomplete coverage |
| `CompanyProfiler` / analyst | Filings/management statements → profile revision/review | Compare-and-swap; provenance mandatory |
| `RelationshipResolver` / analyst | Resolved entities/mentions → directed evidence graph | Ambiguous targets remain drafts |
| `SocialListener` / analyst | Approved subscriptions/streams/polls → durable social events | At-least-once dedupe, backfill, access gaps |
| `WatchdogCompiler` / analyst | Validated template → scoped collection/review jobs | Never changes financial plans; protection is separate |
| `AttentionScheduler` / analyst | Holding/global/alternative work → priority queues | 80/10/10 research default; executor safety capacity separate |
| `PerpetualAdvisor` / analyst | Approved model + point-in-time features/scenario → advisory assessment | Bounded Rust inference; unusable results block required entries, retain holding plan |
| `SentimentProvider` / analyst | Rights-checked X/news/text → pinned FinBERT probabilities → Rust threshold overlay | Bounded HTTP; stale/invalid/truncated results fail; no model call under executor locks |
| `RotationEvaluator` / executor | Validated comparison/policy/state → rotation authorization | Serialized with exits; opt-in, old trigger wins |

```text
Sources / X events → durable items → entity/claim verification
                                      ↓
S&P 500 snapshot → sector → group → finalist → profile / relationships
                                      ↓                 ↓
                             scenario economics → Perpetual advisory tool
                                      ↓
LLM proposed limits + explanation + watchdog → readiness dossier
                                      ↓
Executor rechecks → intent / fills → price protection + active watchdog binding
```

These are proposed interfaces. Exact contracts/transitions are in [sector workflow](./sector-workflow.md), [company intelligence](./company-intelligence.md), [listeners/watchdogs](./watchdogs-and-x.md), [Perpetual](./perpetual.md) and [research schemas](./research-contracts.md). Never hold an executor serialization lock across model/tool network calls.

## Scheduling and timeliness

Initial universe: US regular sessions, calendar timezone `America/New_York`; owner reports default to `Europe/Berlin`. Store instants in UTC and derive sessions from the exchange calendar, including holidays and early closes.

| Work | Proposed initial cadence | Failure behavior |
|---|---|---|
| Pre-market research | 30 minutes before open | Missed/expired work cannot create a late catch-up order |
| Opening entry eligibility | 15 minutes after open | Generate a fresh actionable proposal from current context; fresh account and quote required |
| Midday research | 12:00 exchange-local time, if open | Additional entry only if the session entry slot is unused |
| Held-symbol source collection | Every 5 minutes during the session | Deduplicate; source-specific rate limits and health status |
| Influence/holding X events | Supported stream/webhook, otherwise permitted 60-second polling initially | Actual delay/access/gaps recorded; no subsecond promise |
| Daily sector/group funnel | Pre-market and eligibility refresh | Effective universe, at most five candidates/one deep finalist |
| Held-position LLM reassessment | Every 15 minutes or a material verified event | Coalesce jobs; budget and version checks; last valid plan remains active |
| Quote processing | Streaming preferred; otherwise poll at most every 5 seconds when provider limits permit | Quotes older than 10 seconds are unusable for new decisions; feed coverage must be suitable |
| Pending-order reconciliation | Provider-aware polling, initially up to every 5 seconds | Respect separate endpoint limits; unknown state freezes new submissions |
| Overnight research | Hourly while a position is held | Queue evidence; no extended-hours trading in initial policy |
| End-of-day reconciliation | 10 minutes after actual close | Block next entry on unresolved differences |
| Weekly review | 30 minutes after the week's last actual session closes | Numeric metrics calculated in code |

These intervals are defaults, not promises about provider delivery latency. A market-data provider that cannot support the freshness envelope cannot enable live mode. Use a configurable 60-second account/FX age limit for new orders, a 5-minute proposal lifetime, and at most one queued reassessment per held symbol. Revalidate at submission rather than treating these lifetimes as permission to use stale quotes.

Pre-market research prepares a thesis and evidence set. It does not create an entry permission that lasts until the opening check; the opening job refreshes inputs and requests a bounded new proposal. Missed jobs cannot reuse an expired proposal or execute after the configured entry cutoff.

## Proposed configuration contract

The following is an illustrative configuration shape, not an executable configuration shipped by this repository:

```yaml
name: Trady
mode: paper
account:
  base_currency: EUR
  max_distinct_stocks: 1
  entry_cash_fraction: "0.98"
  max_entry_attempts_per_session: 1
  add_to_position: false
  leverage: false
strategy:
  confidence_entry_min: 90
  normal_target_percent: "1.00"
  strong_target_percents: ["2.00", "3.00"]
  exceptional_target_max_percent: "10.00"
  exceptional_targets_enabled: false
  max_initial_loss_percent: "2.00"
  min_reward_to_risk: "1.50"
  entry_pause_daily_loss_percent: "2.00"
  max_holding_sessions: 5
research:
  enabled_source_types: [x_api, news_api, google_news_rss, publisher_rss, website]
  universe: effective_sp500_owner_broker_intersection
  shortlist_max: 5
  held_symbol_collect_seconds: 300
  held_symbol_research_seconds: 900
  attention_percent: {holding: 80, global: 10, alternatives_max: 10}
rotation:
  enabled: false
  max_rotations_per_session: 1
  min_alternative_confidence: 95
  advantage_hurdle_fraction: "0.005"
  safety_buffer_fraction: "0.0025"
sentiment:
  provider: self_hosted_finbert
  required_for_entry_and_target_increase: true
  threshold_calculation_version: 1
  adapter_config: config/finbert/config.example.json
perpetual:
  execution: native_rust
  require_usable_assessment_for_entry: true
  limits_authority: advisory_only
  approved_model_manifest: deployment_owned_reference
llm:
  normalized_output: json
  max_output_tokens: 4000
  max_model_turns_per_run: 6
  max_tool_rounds_per_run: 4
  max_tool_calls_per_run: 12
  run_timeout_seconds: 120
skills:
  root: /opt/trady/skills
  catalog: catalog.yaml
  require_approved_bundle: true
  require_entry_completion: true
memory:
  root: /var/lib/trady/memory
  authoritative_store: postgres
postgres:
  database: trady
  connection_secret: service_specific_secret_reference
telegram:
  private_owner_only: true
  authorized_user_ids: []
  authorized_chat_ids: []
```

Startup rejects missing required secret references, an empty owner allowlist in Telegram mode, unsupported live broker capabilities, invalid percentages, or any target ceiling above 10%. Paper and live configurations have different account IDs and secret references. The application must not infer live mode from a credential that happens to work.

For an entry-capable v1 deployment, also reject an entry-confidence floor below 90, a disabled required-readiness gate, an unapproved/missing required skill revision or an invalid tool-schema/alias mapping. Research-only service startup may report a blocked entry capability instead of pretending the incomplete bundle is ready to trade.

## Interface

The proposed ports and durable events above are the component contracts. Full message fields and database ownership are specified in [memory and data](./memory-and-data.md); order semantics are specified in [trading policy](./strategy.md).

[LLM conversations and tools](./llm-and-tools.md) defines repeated calls, provider adapters and read-only T212 information access. [Runtime skills](./runtime-skills.md) defines the actual instruction bundle and required artifact validation. The executor processes typed information requests at lower priority than protection/reconciliation; displaying broker information never creates an order intent or gives the analyst broker credentials.

## Invariants

- Exactly one active executor lease per account; every write carries its fencing generation.
- Leases use database time. On lost database connectivity or failed renewal, the old executor stops submissions. A new executor first reconciles external orders, including possible in-flight requests from its predecessor.
- Because a broker may not enforce fencing tokens, database fencing alone is insufficient: recovery and ambiguous-order handling remain mandatory.
- Durable queues are at least once; consumers deduplicate stable event IDs.
- LLM and source text cannot grant capabilities, mutate owner policy, or bypass enum/schema validation.
- Order triggers and plan activation share account/position serialization and an audit record.

## Side Effects

The analyst reads external sources and calls the configured model; the executor talks to quotes and the broker; the gateway sends Telegram messages. Each writes only its assigned PostgreSQL records. Memory files are reconstructable analyst output. No process needs access to the Paperless database or its documents.

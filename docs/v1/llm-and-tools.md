# Trady v1 conversations and tool calls

Status: proposed runtime protocol. Tool names below are Trady contracts to implement, not tools that exist in a running bot today. The model analyzes information; deterministic Rust retains trading authority.

## Two forms of multi-round conversation

Support both owner dialogue across messages and an iterative model/tool loop within one research run. An owner can ask a question, receive an answer, follow up, and clarify missing information in the same persistent session. Within each answer, the model may request tools, inspect their results, request further tools based on those results, and finally return structured JSON.

The application owns the loop, session routing, tool execution, budgets and persistence. A model tool request does not execute anything by itself. One run may span several model turns; each owner follow-up starts another bounded run linked to the same session history. Do not close a conversation merely because a single model request completed.

Load the relevant approved [runtime skills](./runtime-skills.md) before generation. New-entry research must finish its required skills, source comparison and [entry-readiness gate](./strategy.md#finalized-research-and-entry-readiness). Requesting tools or producing an optimistic interim answer cannot reserve an entry slot.

## JSON message contract

All accepted model turns and tool results use versioned JSON objects. Human-readable explanations are strings inside JSON, not free text surrounding it. Reject Markdown fences, multiple concatenated JSON documents, trailing prose, invalid types and unsupported fields in the canonical message. Documentation examples use fences only for readability.

The [exact JSON contracts](./llm-contracts.md) are normative for version fields, enums, every research/proposal property, nullable values, application-owned metadata and representative research-bearing finals. This document describes their flow and tool integration; adapters must not invent alternate field names.

| Message | Required fields and meaning |
|---|---|
| Model `TOOL_REQUEST` | `protocol_version`, `message_type`, and a nonempty `tool_calls` array containing `call_id`, registered `tool`, and schema-validated `arguments` |
| Application `TOOL_RESULTS` | `protocol_version`, `message_type`, and a `results` array correlated to requested call IDs, with status, typed data/error and metadata |
| Model `FINAL` | `protocol_version`, `message_type`, `reply`, `research`, and `proposal`; no outstanding tool calls |

Every final `reply` contains `status` (`ANSWER`, `NEEDS_INPUT`, or `INCOMPLETE`), bounded `text`, `evidence_ids`, `snapshot_ids`, and `questions`. Questions are empty unless input is needed. `proposal` is either null or the strict research-proposal body from [memory and data](./memory-and-data.md#proposal-contract). Application-generated run/session/owner/account IDs, timestamps, readiness attestations and proposal-envelope IDs are bound outside model output.

`research` is null for a simple informational reply or a versioned structured artifact containing `scope`, `workflow_status`, `search_log`, `claims`, `source_comparison`, `skill_findings`, nullable `entry_readiness` and nullable `review`. Claim cards retain source/event dates, materiality, supporting/contradicting item IDs and verification records. Comparison rows identify actual reporting lineages, disputed facts and resolution evidence. Readiness includes typed `PASS`/`FAIL`/`UNKNOWN` checks and references to calculated confidence/economics. Skill findings record the pinned skill/version, artifact references and blockers. `review` contains an attributed summary, observations and proposed lessons with sample counts/counterexamples. Keep each list/text bounded and reference stored tool records rather than repeating full articles.

The model's `workflow_status` and skill findings are requested assessments; the application validates the referenced artifacts and constructs authoritative completion records. A declared `FINAL` research status or claimed `COMPLETED` skill is not self-authenticating. Return structured research even when the outcome is blocked/no-action, so its missing checks can be inspected and continued later.

An explanatory answer can use `proposal: null`. An incomplete or clarification result cannot carry an entry proposal. `HOLD` and `NO_ACTION` remain valid research outcomes. A final proposal must pass local schema, source, skills, confidence, freshness and financial checks before becoming executable. A well-formed JSON response alone is insufficient.

Tool results use `OK`, `PARTIAL`, `STALE`, `ERROR`, `DENIED`, `UNAVAILABLE` or `CANCELLED`. `data` is nullable and validates against that tool's output schema. `error` is null on `OK`; otherwise it contains a stable code, bounded explanation and retry eligibility. `meta` includes `as_of`, `snapshot_id` when relevant, source IDs, freshness, truncation and an application-issued continuation cursor when applicable. Currency, decimal quantities and prices remain explicit and money uses decimal strings.

Preserve unavailable, incomplete and stale statuses when sending results to the model. An empty list after a failed fetch is not proof of no news, and a partial source comparison is not completed entry research.

## Provider adaptation

Prefer tested native function calling where available. Normalize provider-specific function-call items into the canonical JSON protocol, parse JSON arguments locally and serialize the structured result back into the provider's required tool-output format. Preserve the original call IDs and all provider continuation items required for replay; do not replace an assistant tool-call message with an ordinary user message. Provider-specific continuation metadata is adapter state, not governing policy or user-facing explanation.

The current [OpenAI function-calling guide](https://developers.openai.com/api/docs/guides/function-calling) documents application-executed tools and matching outputs in a multi-step conversation. [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs) distinguishes schema-constrained responses from tool calling. Select the final-response format and schema according to the actual adapter; do not assume every compatible provider accepts the same fields or supports both features together.

Canonical registry names such as `news.search` may be mapped to stable provider-valid aliases such as `news_search_v1`. The application owns a collision-free, versioned mapping. The model cannot add aliases or turn an unrecognized name into a valid operation.

For a provider without native tools, offer an explicitly tested JSON-envelope mode: the model returns `TOOL_REQUEST`, Rust executes approved calls, and the next request includes `TOOL_RESULTS`. Require reliable JSON generation and local validation. A provider that cannot maintain correlation, JSON correctness, context limits and finalization stays research-only or disabled. Never simulate a successful tool call by asking the model to invent its result.

Accumulate a complete streaming tool request before validation or dispatch. Refusals, unfinished streams and output truncation create no tool execution. Preserve raw permitted provider artifacts and normalized records for debugging without displaying private provider internals as the research answer.

## Application loop and limits

1. Bind a run to its authorized owner/account/mode, active session, policy and skill versions. Build the context and allowed tool catalog.
2. Call the provider with the current transcript and validate the complete normalized JSON turn.
3. On `TOOL_REQUEST`, validate every call's schema, scope, rights, deadline and budget. Persist requests before dispatch.
4. Execute independent approved information calls concurrently within limits. Calls depending on earlier output wait for a later round.
5. Persist each result, source/snapshot references, usage and errors; append matching JSON results to the same conversation.
6. Invoke the model again. Repeat until a valid `FINAL`, clarification, cancellation, error or bound is reached.
7. Store the final response/report and application-generated completion/readiness result. The executor independently decides whether any proposal is eligible.

Proposed defaults replace the earlier two-retrieval-round limit:

| Limit | Initial value |
|---|---:|
| Model turns per run, including final response/repair | 6 |
| Tool-request rounds per run | 4 |
| Total tool calls per run | 12 |
| Calls requested in one round | 4 |
| Concurrent independent tool calls | 3 |
| Entire run wall-clock deadline | 120 seconds |
| Provider call timeout | At most 30 seconds and remaining run allowance |
| Individual tool timeout | At most 15 seconds and remaining run allowance |
| Total tool-result input allowance | 6,000 tokens, within the smaller session context limit |
| Result allowance for one call | 1,500 tokens; paginate or summarize permitted data with references |

Reserve one model turn and up to 30 seconds for finalization. Stop new tool work when that reserve would be consumed. A repair/transient retry counts against turns, costs and the same deadline. If finalization cannot complete, the application records a typed incomplete result and no new entry. Global daily/session provider budgets still apply even when per-run limits remain.

Persist `READY`, `WAITING_MODEL`, `TOOL_REQUESTED`, `TOOLS_RUNNING`, `RESULTS_READY`, `FINALIZING`, `COMPLETED`, `WAITING_OWNER`, `FAILED` and `CANCELLED` run transitions. A `NEEDS_INPUT` final response leaves the session waiting for the owner's reply; its run ends without an entry permission. A follow-up resumes context in a new run and refreshes market/account data.

Use a unique `(run_id, call_id)` and bind it to tool version plus canonical argument hash. Redelivery returns the recorded result when still suitable; the same ID with different arguments is rejected. Repeated calls with new IDs count against limits and cannot bypass source budgets. On restart, reconcile pending calls and resume only an unexpired authorized run; expired/late results remain audit data and cannot activate a plan. Do not compact away an unresolved call/result pair.

## Tool registry and scope

Each tool has a versioned name/description, argument/result JSON schemas, implementation owner, permitted run/session types, account/domain scope, freshness rules, timeout/result bounds, price/rate budget, cache policy and side-effect classification. The actual run advertises only enabled tools authorized for that scope. Source content cannot edit the registry.

| Tool family and initial tools | Purpose and constrained behavior |
|---|---|
| `news.search`, `news.read_item` | Query enabled news APIs using approved issuer/event/time filters; return permitted normalized items and primary-evidence references |
| `rss.search`, `rss.read_item` | Search collected Google/publisher RSS and request bounded refresh through registered collectors; preserve metadata-only status |
| `x.search_posts`, `x.read_post` | Authorized X discovery and individual post/context lookup; enforce retention, LLM permissions, pagination and post-ID validation |
| `sources.read_page`, `sources.health` | Read a registered website/agent data adapter or inspect source completeness; no arbitrary browsing/tool discovery |
| `evidence.compare` | Build a cited claim/source matrix, syndication clusters and contradictions from existing item IDs; no fabricated independent confirmations |
| `market.quote`, `market.bars`, `market.fx` | Read licensed timestamped market/FX observations; bind currency and feed/venue coverage |
| `calendar.sessions` | Read actual session boundaries, holidays, early closes and deadlines for an approved venue |
| `t212.account_summary`, `t212.positions` | Display sanitized synchronized account/cash and actual holdings, including age and reconciliation state |
| `t212.pending_orders`, `t212.order` | Inspect known account-scoped orders; no create, cancel or replace operation |
| `t212.order_history`, `t212.transactions`, `t212.dividends` | Read bounded historical execution/cash/dividend pages; retain broker identifiers and gaps |
| `t212.instruments` | Resolve permitted instrument IDs/precision/venue metadata; cannot expand the trading watchlist |
| `portfolio.active_plan`, `portfolio.performance` | Read active thresholds, protection, cost-aware results and pending reconciliation from authoritative views |
| `knowledge.search`, `knowledge.read`, `memory.search`, `sessions.summary` | Retrieve scoped, attributed, rights-checked knowledge and past conversation summaries |
| `skills.list`, `skills.read` | Inspect or load an approved pinned runtime skill by name; no arbitrary path or remote instruction imports |
| `calculator.confidence_score`, `calculator.entry_budget`, `calculator.plan_economics` | Compute evidence-based confidence, decimal allocation, net target/stop costs and reward/risk using authoritative verification/snapshot/policy inputs |
| `universe.snapshot`, `market.sector_snapshot` | Effective membership/classification and common-interval sector/group observations; cannot expand tradability |
| `entities.resolve`, `profiles.get`, `relationships.get` | Scoped identities, profile revisions and bounded directed relationships; ambiguous matches remain unresolved |
| `x.listener_status`, `watchdog.status` | Verified listener coverage and personalized holding-watchdog state; no subscription/activation changes |
| `research.get_artifact`, `research.store_artifact` | Scoped immutable stage reads and exact typed drafts for validation/persistence; no arbitrary DB writes |
| `calculator.research_priority`, `calculator.rotation_economics` | Separate research priority and common-horizon net comparison; no extra confidence points or execution authority |
| `perpetual.assess` | Approved native Rust model and point-in-time features → advisory prediction/calibration/drift/explanation; LLM still proposes limits |

These tools obtain information and calculations. Collector tools can make permitted billable requests and persist deduplicated research items; they are not free or side-effectless merely because they cannot trade. A result from `evidence.compare` is a review artifact, not extra corroborating evidence. Calculator output does not reserve funds or authorize entry.

New tools require reviewed Rust adapters/registry versions specifying schemas, permissions, costs, freshness and failure cases. [Research contracts](./research-contracts.md) define the exact new write/calculator payloads; [Perpetual](./perpetual.md) defines inference arguments/results and the scenario loop. No tool exposes shell/code, arbitrary SQL/filesystem access, secrets, source/model-registry edits, owner impersonation, direct Telegram posting or broker trading/funding.

The new read tools take only these required argument keys: `universe.snapshot {session_id}`, `market.sector_snapshot {universe_snapshot_id, market_snapshot_id}`, `entities.resolve {query, entity_type}`, `profiles.get {entity_id}`, `relationships.get {entity_id, max_hops, max_neighbors}`, `x.listener_status {social_account_ids}`, `watchdog.status {position_id}`, and `research.get_artifact {artifact_id}`. IDs use the canonical type; query is bounded text, entity_type is COMPANY/PERSON/PRODUCT/REGULATOR, max_hops is 1–2 and max_neighbors 1–10; account arrays have at most 32 scoped IDs. Reject extras and attempts to select accounts/URLs/files. Results retain standard tool metadata and immutable references. Exact typed read-result schemas must be supplied before advertising these adapters; that is a phase-1 deliverable, not permission to trust arbitrary returned fields.

## FinBERT observations and threshold constraints

The [FinBERT path](./finbert/README.md) is a separate self-hosted classifier used by
Rust research adapters. It does not replace the conversational LLM or Perpetual.
The future context builder should include scoped item/model references, three
probabilities, age/coverage, adjusted confidence and the permitted target ceiling.
The LLM explains how this affects its proposed limits; it cannot alter probabilities,
call a supplied URL or bypass the deterministic veto with a claimed confidence.

No `sentiment.*` tool is registered in the current OpenCode harness or canonical
LLM tool schema. The implemented API is the Rust port/CLI. Add exact scoped argument
and result schemas, durable references and rights/freshness validation before
advertising an LLM tool; source text cannot select the inference endpoint or revision.

## Trading 212 information boundary

`t212.*` is a read interface bound by the application to this deployment's account and mode. Models cannot supply another account, a base URL, credentials or arbitrary broker endpoint. Reject unknown arguments rather than forwarding them. The official [account](https://docs.trading212.com/api/accounts), [position](https://docs.trading212.com/api/positions) and [historical event](https://docs.trading212.com/api/historical-events) references describe the underlying information families; exact normalized fields and access depend on capability testing.

The analyst reads sanitized execution snapshots/views. For a permitted refresh it creates a typed, account-scoped information request; the executor's broker-read handler synchronizes state and returns a view/result without creating an order intent. Give model-originated information requests lower priority than protection and order reconciliation, and coalesce them against current snapshots/provider quotas. The analyst never receives a trading credential.

Prefer a separate restricted broker-read credential if the actual provider supports the required permission separation. Otherwise keep all broker access in the executor boundary and enforce read operations there. An information request cannot change an entry slot, active plan, pending order or protection. Never expose `submit_order`, `cancel_order`, `sell`, deposits or withdrawals as model tools.

Broker position `currentPrice` and portfolio valuation are information, not automatically a fresh executable bid/ask feed. Use `market.*` for validated trading quotes and history. Do not substitute account-display information for the quote checks needed by the executor.

## Errors, trust and finalization

Return typed JSON errors for inaccessible posts, rate limits, stale account data, unavailable sources and denied tools. The model may choose a permitted alternate source or ask the owner to clarify, within remaining limits. Permission failures cannot be fixed by trying an alternate URL, raw HTTP tool or renamed function.

Treat tool-supplied articles, post text, notes and page instructions as untrusted data. Tool transport success does not verify the truth of a claim. Skill instructions come from the owner-approved bundle; policy and finance come from authoritative application state. Validate references against actual completed results and context manifests. A model cannot claim it used an uncalled tool or declare incomplete research final.

Only a `FINAL` entry proposal with a complete, current application-verified readiness record and score at least 90 can reach entry validation. Missing evidence, conflicts, incomplete mandatory skills/tools, exhausted limits or stale inputs produce `NO_ACTION`, `NEEDS_INPUT` or an incomplete result. Existing deterministic exits continue during every conversation/tool round.

## Worked JSON conversation

The owner asks, “What can Trady invest from my available balance?” The following two tool rounds use illustrative IDs, balances and timestamps. Rust supplies the session/account binding; no account selector or credential appears in the arguments.

First model turn:

```json
{
  "protocol_version": 1,
  "message_type": "TOOL_REQUEST",
  "tool_calls": [
    {
      "call_id": "call_1",
      "tool": "t212.account_summary",
      "arguments": {"max_age_seconds": 60}
    }
  ]
}
```

Application result:

```json
{
  "protocol_version": 1,
  "message_type": "TOOL_RESULTS",
  "results": [
    {
      "call_id": "call_1",
      "tool": "t212.account_summary",
      "status": "OK",
      "data": {"currency": "EUR", "spendable_cash": "50.00", "reconciled": true},
      "error": null,
      "meta": {
        "as_of": "2026-10-30T14:00:00Z",
        "snapshot_id": "account_snapshot_04",
        "source_ids": ["broker_account_sync"],
        "freshness": "CURRENT",
        "truncated": false,
        "next_cursor": null
      }
    }
  ]
}
```

The next model turn depends on the returned snapshot ID:

```json
{
  "protocol_version": 1,
  "message_type": "TOOL_REQUEST",
  "tool_calls": [
    {
      "call_id": "call_2",
      "tool": "calculator.entry_budget",
      "arguments": {"account_snapshot_id": "account_snapshot_04"}
    }
  ]
}
```

The calculator uses the bound policy rather than a model-selected allocation percentage:

```json
{
  "protocol_version": 1,
  "message_type": "TOOL_RESULTS",
  "results": [
    {
      "call_id": "call_2",
      "tool": "calculator.entry_budget",
      "status": "OK",
      "data": {"currency": "EUR", "max_entry_total": "49.00", "minimum_retained_cash": "1.00"},
      "error": null,
      "meta": {
        "as_of": "2026-10-30T14:00:01Z",
        "snapshot_id": "account_snapshot_04",
        "source_ids": ["broker_account_sync", "policy_version_07"],
        "freshness": "CURRENT",
        "truncated": false,
        "next_cursor": null
      }
    }
  ]
}
```

Final model response:

```json
{
  "protocol_version": 1,
  "message_type": "FINAL",
  "reply": {
    "status": "ANSWER",
    "text": "The synchronized spendable balance is EUR 50.00. A future eligible entry has a total-cost budget of EUR 49.00, retaining at least EUR 1.00. Research and high-confidence readiness must pass before entry.",
    "evidence_ids": [],
    "snapshot_ids": ["account_snapshot_04"],
    "questions": []
  },
  "research": null,
  "proposal": null
}
```

Telegram renders the final `reply.text`. A follow-up such as “Why is that not all €50?” reuses this session and refreshes any stale facts. A stock-selection run uses the same loop for search, post lookup, primary-source reading, comparisons and plan economics; it may finish with a validated proposal instead of a display-only answer.

## Verification

Acceptance must cover two or more tool rounds, correlated IDs, JSON-only responses, native/JSON adapters, dependent calls, restart, partial/error results, budget exhaustion, Telegram follow-ups, prohibited broker calls and final high-confidence readiness. See [implementation](./implementation.md). These protocol examples are illustrative JSON, not live requests.

# Trady v1 Telegram controls and updates

Status: a read-only monitoring subset is implemented. The [Telegram setup guide](../telegram.md)
documents `/status`, `/mode`, `/balance`, `/position`, `/orders` and their aliases,
owner authentication, optional Trading 212 observations and durable local delivery.
The commands, conversations, trading controls and full notifications below remain
the target product interface unless explicitly listed as implemented in that guide.

## Identity and transport

Use one Telegram bot in a private owner chat. Authorize the numeric sender user ID and chat ID against configured allowlists on every message and callback, and require a private-chat context. A username, forwarded-message author, display name or text claiming to be the owner is insufficient. An empty allowlist disables command handling.

Use long polling initially so a public webhook is unnecessary. Persist each accepted update and its deduplication key before advancing the polling offset. Only one gateway consumer is active. If webhooks are later selected, verify the configured secret header, persist the update before acknowledgement and keep polling disabled. Telegram's API defines the two delivery modes and update identifiers in its [official reference](https://core.telegram.org/bots/api).

Bind confirmation callbacks to the same owner and private chat. Reject unauthorized traffic without returning holdings, account balance or private reports. Keep bot credentials out of logs, URLs in alerts and model context. Split long messages within the transport limits, escape formatting and disable unwanted link previews.

## Read and research commands

| Command | Behavior |
|---|---|
| `/help` or `/start` | Explain commands and show whether this owner/chat is authorized; never change trading mode |
| `/status` | Mode, entry pause, account/quote age, holding, active target/stop, pending orders, next work and degraded sources |
| `/mode` | Display the current deployment mode; no mode-changing argument is accepted |
| `/balance` | Confirmed spendable cash, reserved/unsettled funds, account equity and the 98% next-entry budget with timestamp |
| `/position` | Actual quantity, acquisition cost, current estimated return, plan version, target, stop, trailing floor, deadline and protection state |
| `/plan` | Current thesis, confidence components, source links and accepted/rejected recent plan changes |
| `/research` | Queue a bounded watchlist/market research run and return its job ID |
| `/research <ticker-or-url>` | Research an instrument or registered source URL without placing an order |
| `/ask <message>` | Start or continue the selected owner-research conversation; use bounded JSON tool rounds as needed |
| Bare `https://x.com/.../status/...` URL | Same research-request path; report accessible content, verification and relevance |
| `/sources` | Enabled source types, last successes, coverage gaps and budget/health state |
| `/watchlist` | Show current approved instruments and policy version |
| `/policy` | Show owner policy and explain which settings are adjustable through Telegram |
| `/report today` or `/report week` | Computed account/project results, flows, costs, activity and unresolved reconciliation |
| `/budget` | Source/model cost usage, configured caps and reserved uncertain charges |
| `/t212 account`, `/t212 positions`, `/t212 orders`, `/t212 history` | Display sanitized broker information through the read-only information boundary; show age/gaps |
| `/jobs [job-id]` | Recent research/command status or a specific queued result |
| `/sectors` / `/sectors <code>` | Sector/group ranking, effective universe, unknown coverage and selection reasons |
| `/company <ticker>` | Attributed profile, baseline filings, management statements, revision and gaps |
| `/relations <ticker>` | Disclosed links and praise/criticism/quote/repost events with sources/ambiguity |
| `/influencers` | Owner's five accounts, scoped priorities, verified identities, listener mode and gaps |
| `/watchdog` | Holding selectors, critical dependencies, health, lifecycle and deferred-news counts |
| `/perpetual [assessment-id]` | Model/calibration/drift, scenario and LLM use/disagreement; no model promotion or limit mutation |

Do not fabricate a fresh account status during a broker outage. Show the last synchronized time and stale status. An unauthorized or unrecognized ticker may be researched but cannot join the trading universe through this command.

## Entry control, closing and policy

| Command | Effect and confirmation |
|---|---|
| `/pause` | Immediately request an entry pause; no confirmation needed for this reduction in activity |
| `/resume` | Resume eligible entries after reconciliation and all policy checks; fail with reasons if conditions are not met |
| `/close` | Preview a close of the current holding and request a single-use confirmation |
| `/cancel <order-id>` | Preview cancellation of an owned pending entry; confirm before applying |
| `/watchlist add <instrument-id>` / `/watchlist remove <instrument-id>` | Preview an instrument-verified policy change; confirm and version it |
| `/policy set <supported-key> <value>` | Preview an allowed setting change with old/new values; confirm and version it |
| `/confirm <token>` | Apply the exact unexpired payload bound to this owner, chat, mode, account and current versions |
| `/cancel-confirm <token>` | Invalidate a pending confirmation |

`/pause` stops new entries and asks the executor to cancel/reconcile an unfilled entry where possible; it does not instantly undo a fill. Keep native protection, price exits, reporting and research active. If a fill races with cancellation, protect the actual holding. `/resume` never resets a consumed entry slot, a latched daily-loss pause, unknown order status or unresolved protection failure.

`/close` creates an authenticated owner-close condition after confirmation. Rust coordinates protection and quantities and submits the eligible exit; no LLM call is needed. Report `REQUESTED`, `ACCEPTED`, `SUBMITTED`, `PARTIAL` and final broker-confirmed outcome separately. A close outside the supported session may wait until trading is possible; show that explicitly.

`/cancel` may not remove the only protective order from an open holding or cancel a latched risk exit to continue holding. For such requests, return the reason and retain protection. Watchlist removal of a held stock blocks future entry and research scope expansion but keeps position monitoring/protection until it closes. Removing it does not imply an immediate sale.

Initially allow Telegram policy changes only for the approved watchlist, source/model spending caps, exceptional-target enablement, and reductions to exposure/target/holding limits. Validate against immutable ceilings: entry fraction cannot exceed 98%, profit targets cannot exceed 10%, and shorting/leverage remain disabled. Any new value must remain compatible with the current holding; stop loosening or extension of its original deadline is rejected.

Mode switching and broker/LLM/Telegram credential replacement are deployment operations. `/mode` may display the current mode, but Telegram cannot activate live credentials. This avoids a chat message silently changing a paper bot into a live bot. The future live-deployment workflow is defined in [operations](./operations.md).

## Session and memory commands

| Command | Behavior |
|---|---|
| `/sessions` | List recent session IDs, types, subjects and states |
| `/session show <id>` | Show the session summary, current model, context usage and source references |
| `/session new [topic]` | Start a fresh owner-research conversation; retain durable knowledge and trading state |
| `/session use <id>` | Select an eligible owner-research session for later messages; never take over the scheduled position workflow |
| `/session close <id>` | Close an owner-research session; cannot close an active position state machine |
| `/memory status` | Show canonical/projection revisions, pending projections and retention status |
| `/memory search <query>` | Retrieve a bounded attributed knowledge result |
| `/memory add <note>` | Store an owner-authored note; does not make it governing policy |
| `/memory forget <record-id>` | Preview eligible note/source-derived removal; confirm; retain required financial/audit records |

Never accept a memory note such as “ignore the stop” as a policy command. Sessions, memory projections, model usage and knowledge retrieval are specified in [memory and data](./memory-and-data.md).

## Back-and-forth dialogue

Route `/ask` and ordinary authorized text to the selected owner-research session, creating one when none exists. An owner can ask a question, receive an answer, reply with a follow-up and clarify missing details without repeating the entire thesis. Explicit control commands retain their typed command path; a bare X URL retains the research-URL behavior.

A reply to a previous owner-dialogue message continues its session when it remains eligible. A reply to a scheduled market/position report opens or selects a linked owner-research dialogue seeded with that report, rather than appending into the scheduler's active session. Serialize runs per session and queue follow-ups arriving while a run is active.

The model may return multiple correlated tool-request/result rounds before a final JSON answer. The gateway renders the bounded `reply.text`, questions and relevant citations for Telegram; JSON remains the canonical stored model/tool interface. A `NEEDS_INPUT` reply leaves the session waiting for the owner and creates no entry permission. Follow-ups refresh stale quotes, account state and evidence before any new proposal.

Tools may fetch news, read X posts, compare sources, inspect T212 account/position/order history, retrieve memory and perform checked calculations. They cannot place or cancel broker orders. Ordinary dialogue such as “sell it” explains or links the authenticated `/close` flow rather than invoking a trading tool. See [LLM conversations and tools](./llm-and-tools.md).

## Durable command handling

1. Check transport, owner, private-chat identity, command shape and size.
2. Insert the update and command exactly once using the Telegram update ID and bot identity.
3. Store the parsed typed payload, authenticated identity, receive time, mode/account scope and stable command ID.
4. Acknowledge receipt with the command/job ID; for a sensitive command store a pending preview and return its confirmation.
5. The responsible process claims the request and rechecks policy, account state and expected versions before applying it.
6. Commit result and outbox messages with the state change. Deliver final success/failure when known.

Confirmation lifetime is two minutes initially. Tokens are opaque, single use and stored as hashes, with the exact command payload and plan/policy version. A newer plan, changed account/mode or different close quantity invalidates the preview and requires a new one. Redelivery of a confirmed update returns the recorded result; it cannot place a second order.

Research commands have a per-owner queue/rate limit and coalesce identical work. Stop accepting expensive jobs when their budget is exhausted. `/status` and `/pause` remain available independently of LLM availability. A failed command never becomes successful merely because the gateway delivered an acknowledgement.

## Notifications

| Event | Owner-facing content |
|---|---|
| Startup or recovery | Mode, account identity hint, reconciliation result and degraded services |
| Scheduled research completed | What changed, candidates/holding, sources, confidence, proposed prices and rejection/no-action reasons |
| Submitted URL researched | URL, accessible context, source verification, thesis relevance and whether a proposal was produced |
| Plan accepted or rejected | Old/new plan version, target/stop, score change, evidence and validator result |
| Entry/order lifecycle | Intended action, actual submission state, partial/full fills, fees and protected quantity |
| Exit latched or filled | Trigger, requested quantity, fill status and realized net outcome when known |
| Critical fault | Unknown order, stale feed, unprotected holding, conflicting manual activity or lost execution lease |
| Daily/weekly report | Account results, deposits/withdrawals, external expenses, completed round trips and unresolved differences |
| Cost or source limit | What stopped, resulting research coverage and whether current protection remains active |
| Sector/finalist selected | Sector → group → stock reasons, peer controls, adverse evidence and no-action alternative |
| Public-person event | Actual actor/action, issuer, novelty, verification, lasting relevance and existing reaction |
| Watchdog or ML degraded | Missing coverage/model support, affected research and independent protection state |
| Alternative compared | Keep/switch/cash evidence, costs/hurdle, entry-slot/rotation gates and executor outcome |

Send pre-market/midday summaries and material held-position changes, not every poll. Coalesce routine updates within fifteen minutes; deliver order/protection faults promptly without routine coalescing. Optionally apply owner quiet hours to routine overnight news. Critical messages bypass quiet hours and include a stable incident ID to group repeats.

Example format, using illustrative values rather than a real recommendation:

```text
Trady · PAPER · research R-42
Example issuer: new primary announcement verified; one reporting lineage.
Confidence: 82/100. Proposed net target: 2%. Plan P-7 remains active.
Proposal awaits price, cost and protection checks; no order submitted.
Sources: <permitted source links>. Coverage: one API delayed.
```

Label proposals, activated plans and executed fills distinctly. Report realized results only after broker reconciliation, and do not call a proposed target a promised gain.

## Failure behavior and verification

Telegram downtime does not disable broker protection or quote-driven exits. Keep an outbox and retry notifications with bounded exponential backoff. Delivery can duplicate after a crash between remote send and local acknowledgement; include event IDs and never translate delivery retry into execution retry. A prolonged outage raises service health for the operator to inspect locally.

If an incident predates the first available update after a long outage, reconciliation still discovers it; Telegram is a control/notification surface, not the financial ledger. Acceptance covers numeric-ID spoofing, group chats, duplicate updates, stale confirmations, delayed close requests, pause/fill races, memory-note injection and notification retry. See [implementation](./implementation.md).

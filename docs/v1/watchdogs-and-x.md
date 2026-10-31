# Trady X listeners and holding watchdogs

Status: researched design and instruction artifacts, checked 2026-10-31. The [X/news
reading slice](../news.md) implements bounded timeline polling and post lookup. No
live subscriptions, activated account identities or watchdog services were created.

## Five priority people on X

Use the following **owner-selected five-person watchlist, in the owner's qualitative order**. These are people whose own public actions/statements can matter, rather than news-reporting accounts. This is potential influence, not a measured ranking of average causal price impact. Priority is scoped triage, not a prediction weight or permission to trade outside the universe. Sector-specific people and agencies belong to the holding's watchdog without consuming these five slots.

| Rank, person / candidate handle | Priority and scope | Evidence and interpretation |
|---|---|---|
| 1. Donald Trump — `@realDonaldTrump` | P0: trade policy, tariffs, sanctions; S&P 500, currencies and bonds as context | [Official attribution](https://www.whitehouse.gov/releases/2026/10/trump-vance-administration-leads-nationwide-coalition-in-fighting-war-on-fraud/), [tariff action](https://www.whitehouse.gov/presidential-actions/2025/04/regulating-imports-with-a-reciprocal-tariff-to-rectify-trade-practices-that-contribute-to-large-and-persistent-annual-united-states-goods-trade-deficits/). [Published research](https://www.sciencedirect.com/science/article/pii/S0167923621000877) finds topic-dependent historical associations; a new post still needs content/effective-rule verification. |
| 2. Elon Musk — `@elonmusk` | P0: TSLA/company announcements, AI and technology; crypto as context | [Tesla biography](https://www.tesla.com/elon-musk), [filed handle attribution](https://ir.tesla.com/_flysystem/s3/sec/000110465924059357/tm2413800d2_defa14a-gen.pdf), [historical SEC complaint summary](https://www.sec.gov/newsroom/press-releases/2018-219). The [settlement](https://www.sec.gov/newsroom/press-releases/2018-226) was without admitting or denying allegations; historical impact does not establish a new claim's truth or direction. |
| 3. Keith Gill / Roaring Kitty — `@TheRoaringKitty` | P0 for GME/meme-stock activity; lower-priority context for unrelated members | [Congressional testimony](https://www.congress.gov/117/meeting/house/111207/witnesses/HHRG-117-BA00-Wstate-GillK-20210218.pdf) establishes the person/persona. Verify the owner-provided handle against current API identity and original cross-links before activation. Image/video memes may remain ambiguous; do not invent a company or intent from them. |
| 4. Bill Ackman — `@BillAckman` | P1: activist-investment targets and macroeconomic commentary | [Pershing Square's October 2026 announcement](https://pershingsquareholdings.com/pershing-square-holdings-ltd-to-publish-its-quarterly-portfolio-review-on-november-2-2026/) identifies his role and links the account. Distinguish personal macro opinion, disclosed position, campaign proposal and actual corporate action. |
| 5. Carl Icahn — `@Carl_C_Icahn` | P1: activist targets and corporate actions | [His official site](https://carlicahn.com/) links the account; [2013 Apple letter](https://carlicahn.com/our_letter_to_tim_cook/) describes his public position/buyback advocacy. A historical campaign or today's demand does not establish current ownership, board acceptance or causal return. |

Resolve each candidate handle to an immutable platform user ID using permitted API lookup; do not ship guessed numeric IDs. Check ownership evidence, role and recent accessible posting activity at activation and at least weekly; recheck after a handle/role change. Historical official attribution is a seed, not proof that an account remains active today. If one fails verification, report fewer active listeners rather than silently substituting an impostor. Review the five slots monthly using documented event materiality, coverage, relevance across the universe and false-alert rate; keep old selections and reasons.

Preserve the owner's five accounts until an authenticated watchlist change; monthly reviews propose changes rather than automatically substituting people. Altman, Nadella, Pichai and other executives can still be company-specific sources when relevant. For later evaluation, store original events, mapped issuers, timestamps, market/group-adjusted observations and competing announcements over a fixed window, including neutral/nonmoving events. Do not claim an empirical influence ranking before those observations exist.

Trump's Truth Social account and official White House/agency releases are parallel discovery channels. Add an authorized API/feed or a reviewed site collector if permitted; do not assume a public full-history API exists. Record missing coverage. Cross-posts, screenshots, quotes and X reposts of the same Truth Social statement form one event lineage. A post about GME, crypto, bonds or currencies remains context unless its actual instrument is an eligible approved S&P 500 stock; no meme-stock exception is created.

Extract event type, named/resolved companies, actual speaker, novelty, publication/first-seen time, materiality, scope/priority and uncertainty. Combine those versioned features with price/volume through the [Perpetual advisor](./perpetual.md). P0/P1 expresses attention priority within scope, not a probability, causal coefficient or addition to confidence points. Queue severity/holding safety can override social priority.

The strategy investigates lasting business/policy consequences, verifies how much price reaction already occurred and refuses to chase an expired move. Do not claim to compete for subsecond reactions. The daily sector funnel is supplemented by event-driven collection and holding reviews; Trady is not limited to one analysis per day. Unverified numerical return/latency examples from an owner note remain attributed leads until their primary evidence is checked.

## Listener capabilities and event direction

The official X Activity API documents per-user activity subscriptions and webhook delivery. Its `post.create` covers **the filtered person's own original posts, replies, quote posts and reposts**. In contrast, `post.reply.create`, `post.quote.create` and `post.repost.create` describe **other users reacting to that person's posts**. Registering only those reaction events would miss the user's requested outgoing behavior. [X Activity introduction](https://docs.x.com/x-api/activity/introduction), [event payloads](https://docs.x.com/x-api/activity/event-payloads)

Use a verified `user_id` filter with `post.create` plus supported delete/change coverage for each approved person. Classify payload references as `ORIGINAL`, `REPLY`, `QUOTE` or `REPOST`; preserve the actual actor, original author, parent/root IDs, quoted/reposted IDs and edit chain. Fetch bounded parent/quoted/original context when permitted. “Comments” here means public replies, not private messages. Incoming replies are optional discovery at lower priority, not the person's own speech.

Select the actual delivery adapter after checking the application's current entitlement, subscription limits, event types, authentication, billing and webhook support. Do not confuse X Activity subscriptions with Account Activity access that may require user authorization. Current documentation is not proof that this account has access. Verify webhook challenge/signatures using the provider's supported method, persist before acknowledging, deduplicate provider event UUIDs and stable post/version IDs, and reject unverified payloads. The source registry/deployment operator controls subscriptions; the model cannot register arbitrary URLs/accounts or grant itself access. [X Activity quickstart](https://docs.x.com/x-api/activity/quickstart)

Where supported, Filtered Stream rules for approved authors are an alternative delivery path. Include replies and reposts; do not use blanket exclusions that remove them. Confirm author/repost matching with fixtures against the selected endpoint, since matching semantics can differ. The stream documents edited posts with new IDs linked by `edit_history_tweet_ids`. Do not assume the same channel delivers deletions. [Filtered Stream](https://docs.x.com/x-api/posts/filtered-stream/introduction)

Maintain a capability matrix for original/reply/quote/repost/edit/delete delivery, eligible accounts, history window, measured delay and cost. If streaming/webhooks are unavailable, poll permitted user timelines/recent search with pagination and overlap, initially every 60 seconds when provider limits and cost allow. This is `POLLING`, never described as real-time. Reposting and later un-reposting can occur between polls and be missed. A search result, public webpage and user timeline do not guarantee identical coverage. [User-post timeline](https://docs.x.com/x-api/users/tweets/introduction)

Persist connection state, subscription IDs, cursors, last delivered/last durably stored times and unresolved coverage intervals. Reconnect with bounded exponential backoff; backfill only through supported history/replay features. A stream heartbeat proves connectivity, not that an account posted nothing. Exhausted pagination, access loss, edits/deletes not supported, or an unrecoverable interval remains a visible gap. Use current credit/rate budgets, deduplicate across collectors and cap reaction floods. Exact costs belong in deployment configuration, not a hardcoded claim that access is free. [X usage and billing](https://docs.x.com/x-api/fundamentals/post-cap)

## Holding watchdog

Relevant permitted English posts and related news can pass through
[FinBERT](./finbert/README.md) after actor/context attribution and lineage deduplication.
The watchdog schedules a coalesced holding review for adverse or changed sentiment;
it records the model revision, item hashes and freshness. A negative classification
does not establish a directed criticism, endorsement, real-world event or sell
instruction. At negative probability 0.60 the threshold overlay disallows a target
increase pending review; at 0.80 it also vetoes entry. On inference failure retain
the validated holding plan and price protection, report the gap and refresh required
evidence. Durable scheduling/binding is planned; the adapter/calculation are implemented.

Prepare a validated **per-stock watchdog template before entry**. On the first confirmed partial fill, bind it to position ID, actual issuer/instrument, current plan version, thesis/evidence and protected quantity. Protection runs first; profile enrichment cannot delay it. Rust compiles allowlisted selectors and schedules from the template; the model supplies structured research fields, never executable code, SQL, shell, arbitrary cron or unapproved source URLs.

| Watch category | Personalized selectors and behavior |
|---|---|
| Company and people | Official releases/filings, verified company/CEO/CFO/relevant leader accounts, exact product aliases and disambiguated issuer names |
| Thesis events | Product launch/order conversion, results/guidance, original promise deadlines, required regulatory decisions, known lawsuits, recalls and corrections |
| Related entities | Verified top customers/suppliers/competitors and influential people whose stated topic has an evidenced exposure path; bounded graph expansion |
| Sector drivers | Guide-specific commodity/rate/reimbursement/demand observations and a few peer controls; compare beneficiaries versus harmed businesses |
| Market and safety | Quotes, FX, native protection, broker/order state, trading halt, broad market shock and source health always retain priority |

Each rule stores category, registered source/account/entity IDs, plain-text topic terms, evidence-backed rationale, severity, collection cadence, verification requirement, cooldown and expiry. Resolve sources/accounts from the registry and enforce rule-count/rate/spending bounds. Topic matching creates a candidate event; a verification/comparison job determines materiality. News cannot turn into a direct sell instruction.

During a holding, reserve a proposed **80% of the research/model budget for the holding and its relationships**, 10% for global macro/five-person triage, and at most 10% for new opportunities. Safety/quote/reconciliation capacity is reserved separately and cannot be consumed by research. Unused research allowance may flow to holding work; opportunity work never borrows protection capacity. A critical holding event preempts speculative scans. Ignoring unrelated articles means deferring their full analysis, while retaining permitted metadata and the deferral reason; it does not disable market-wide shock detection.

Held sources collect every five minutes, with X delivery when available; reassess every fifteen minutes or on a new material event. Coalesce repeated reports of the same event into one job; deliver a critical coverage/protection alert promptly without waiting for the model. An initial five-minute cooldown prevents duplicate noncritical alerts; a changed material fact/severity bypasses it. These are configurable scheduling goals, not provider latency guarantees. Critical watchdog source loss blocks thesis updates requiring that source and alerts the owner; native protection and the last validated plan remain active.

Use states `PREPARED → ACTIVE → DEGRADED → ACTIVE`, with `ACTIVE/DEGRADED → CLOSING → RETIRED` on reconciled exit. `DEGRADED` means coverage failure, not loss of broker protection. Version templates by compare-and-swap; obsolete event jobs retain audit records but cannot update a newer plan without fresh review. Keep holding sources until quantity and orders reconcile to flat; then retire selectors with no remaining consumers. Shared subscriptions use reference counting so closing one holding cannot remove the global five-person watchlist.

## Exceptional alternative and switching

An unrelated story earns deeper work only if it passes primary-claim, eligible-member, plausible materiality, timing and tradability triage. Use [opportunity comparison](../../skills/opportunity-comparison.SKILL.md) and finish the alternative's entire sector/group/company/readiness workflow. Compare keeping the holding, switching and cash over the **same remaining horizon**, including both legs' costs, FX/spread/slippage, supported protection, tax effects where configured and uncertainty. Confidence is not a win probability. An attractive alternative never cancels an exit that already latched.

V1 retains **one entry attempt per exchange session by default**. Therefore a stock bought today cannot be rotated into a second stock today under that default; report `ENTRY_SLOT_USED` and keep managing it. A holding from an earlier session can be considered if today's entry slot is unused. Do not silently waive this rule to pursue a headline. The full opt-in rotation gate and optional two-attempt policy are specified in [trading policy](./strategy.md#opportunity-rotation).

The rotation calculator and executor, not a model request to sell, decide whether an approved comparison meets the configured gate. A switch exits/reconciles the old position before a fresh entry check; failed/stale second-leg checks leave cash. There is no atomic broker guarantee across the two stocks and no promise that the new stock can still be bought after selling the old one.

## Interfaces and verification

`SocialListener` produces durable social events; `WatchdogCompiler` validates templates and creates scoped jobs; `AttentionScheduler` prioritizes them. These run in the analyst without broker credentials. `RotationEvaluator` runs in the executor, checking comparison/policy/current account state and writing any authorization. Executor serialization evaluates existing exits before plan/rotation acceptance. Data and artifact fields are in [memory](./memory-and-data.md) and [research contracts](./research-contracts.md).

Verify outgoing versus incoming replies/reposts, missing parent context, deletion/edit propagation, listener outage/replay, quota exhaustion, reaction floods, irrelevant CEO arguments, holding-specific regulator alerts, stale watchdog versions, used entry slots and second-leg failure. No path can create two simultaneous holdings or let news disable price protection.

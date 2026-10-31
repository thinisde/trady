# Trady five-account X pilot and API cost controls

Status: proposed watchlist with an offline-tested recent-search implementation,
checked 2026-10-31. The [implementation record](../news.md#x-pilot-and-resource-budget)
describes the disabled example, preflight counts, resource ledger and local export.
This document records a proposed cost-conscious watchlist and collection strategy;
it does not activate accounts, subscriptions, API spending, model training or a
scheduler. The [X/news reading implementation](../news.md) implements bounded timeline
polling, post lookup and incremental recent search; streaming/webhooks remain future work.

## Purpose and decision status

Collect original company, investment and policy statements that add information
beyond Trady's existing news sources. Minimize paid resources before delivery,
collect once, and reuse permitted observations across research and evaluation.
The shortlist is a research hypothesis, not an empirical ranking of market impact
or a recommendation to copy the authors' trades.

The existing [X/watchdog specification](./watchdogs-and-x.md) and
[disabled source registry](../../config/research.json) preserve the owner's
Trump/Musk/Gill/Ackman/Icahn selection. The proposed pilot is
**Ackman/Musk/Bessent/Burry/Altman**. Documenting the pilot does not authorize
replacing that selection, changing its order, or excluding its required outgoing
replies/reposts. An authenticated owner decision must approve any replacement,
coverage reduction and versioned registry migration. Do not silently run both
five-account lists and describe their combined cost as a five-account budget.

Keep S&P 500 eligibility, long-only execution, one holding, risk/readiness checks
and holding protection unchanged. Relevant statements create research evidence;
they do not submit orders or expand the trading universe.

## Proposed five accounts

The order below is presentation order, not a causal-return ranking, source-trust
score or model weight. Handles are candidate identity seeds, not verified numeric
user IDs. Collection roles and filters are proposed Trady design choices.

| Candidate | Information to investigate | Evidence for inclusion and limitation |
|---|---|---|
| Bill Ackman — `@BillAckman` | Disclosed investments, company theses, activism and material economic commentary | [Pershing Square's investor-communications announcement][ackman] explicitly links this account. Distinguish a disclosed position from an opinion or proposed campaign; confirm current exposure and corporate actions independently. A post listener does not capture the contents of a Spaces discussion. |
| Elon Musk — `@elonmusk` | Tesla, vehicle autonomy, robotics, results and material business announcements | The [SEC's 2018 announcement][musk] documents allegations concerning market-moving, misleading tweets. This is historical evidence of relevance and verification risk, not a reliable trading edge or proof that a new statement is true. Apply a narrow business filter for the proposed pilot. |
| Scott Bessent — `@SecScottBessent` | Trade negotiations, tariffs, sanctions, Treasury financing and economic policy | The [Treasury biography][bessent] identifies the current secretary; [Treasury's homepage][treasury] attributes this X account. Use as a macro-risk input. A political statement or negotiation update is not necessarily an enacted measure; verify the formal action and effective time. Reassess the slot when the office-holder or role changes. |
| Michael Burry — `@michaeljburry` | Valuation, accounting, competition and challenges to bullish investment theses | [Scion's official site][burry] identifies Cassandra Unchained as his current analytical publication. That page does not independently verify this X handle: activation requires an original cross-link and current API identity check. Start with author-only coverage if approved and affordable; measure actual activity rather than assuming low volume. |
| Sam Altman — `@sama` | AI partnerships, computing demand, infrastructure, financing and commercial developments | [His personal site][altman] links `@sama`; [OpenAI's partnership announcement][openai] establishes business relationships that may provide a researched exposure path to listed companies. Supplier/partner relevance is an inference requiring issuer-specific evidence, not a blanket bullish AI signal. |

This selection combines investor disclosures, company announcements, macro policy
and contrary analysis, but remains concentrated in US and technology/AI themes.
It is not complete market coverage. Continue licensed news, official releases and
holding-specific sources; test whether the X observations actually add anything.

Trump's statements remain relevant. The existing design includes Truth Social
and White House/agency releases as separate channels; monitoring an X handle is
not proof of complete cross-platform coverage. This pilot does not remove that
requirement. Gill, Icahn and other executives can be proposed as replacements or
holding-specific sources only through the existing owner/source approval process.

### Identity and activation gate

For every proposed account, resolve the handle through permitted API lookup and
record the returned stable user ID, current handle, original ownership evidence,
role, accessible recent activity and UTC verification time. Never guess IDs or
approve identity from a matching display name, badge or third-party screenshot.
Unresolved identity means fewer active sources, not a substitute lookalike.

Follow the existing seven-day identity-review policy and mismatch blocking.
The current timeline adapter re-resolves identity on each page; this document
neither removes that safeguard nor claims an identity cache is implemented.
Any later caching design must retain expiry, handle-change detection and
fail-closed ownership checks, with its own reviewed tests.

## Filtering before delivery

Use one versioned rule per author so that volume, relevance and gaps are
attributable. The following are illustrative Filtered Stream/search-style query
candidates, not installed rules or accepted production configuration. Validate
operators and author/repost behavior against the selected endpoint and entitlement
before use. X documents the underlying [filter operators][operators].

| Account | Proposed query |
|---|---|
| Ackman | `from:BillAckman (stock OR shares OR investment OR portfolio OR earnings OR guidance OR acquisition OR merger OR tariff OR tariffs OR inflation OR rates)` |
| Musk | `from:elonmusk (Tesla OR TSLA OR robotaxi OR Cybercab OR FSD OR Optimus OR earnings OR guidance)` |
| Bessent | `from:SecScottBessent (trade OR tariff OR tariffs OR sanctions OR Treasury OR debt OR yields OR inflation OR economy)` |
| Burry | `from:michaeljburry` |
| Altman | `from:sama (OpenAI OR partnership OR Microsoft OR NVIDIA OR Amazon OR compute OR datacenter OR "data center" OR funding OR chips)` |

These examples do not blanket-exclude replies, quotes or reposts. Their actual
matching behavior still needs fixtures. For an explicitly approved reduced-coverage
pilot, `-is:retweet` can be tested as a separate rule version; it must not silently
replace the owner's full outgoing-action requirement. Do not add `-is:reply` by
default: a short reply can contain important information.

Keyword rules intentionally miss posts without those terms, including some
short replies, image-only messages, new company names and indirect references.
Record that selection bias. Do not label the result a complete account timeline
or infer that an unmatched account said nothing. FinBERT/LLM filtering after
receipt reduces downstream work, not charges already incurred for returned data.

Do not fetch follower lists, engagement histories, full reply trees or media
routinely. Fetch bounded parent/quoted context only for a specific information
need, with appropriate rights and budget. Preserve who actually authored each
statement; a repost is not automatically an endorsement.

## Delivery choice and implemented boundary

| Mode | Intended use | Boundary |
|---|---|---|
| Existing timeline polling | Bounded collection using the shipped X adapter | Implemented but disabled by default. Local keyword filtering does not prevent the initial resource charge. Preserve identity, pagination, rights and cursor safeguards. |
| Incremental recent search | Batch collection for a daily pilot where immediate reactions are unnecessary | Implemented and disabled in a separate example. Versioned author rules, identity checks, bounded durable pagination, explicit gaps and fresh counts gates are tested offline. It is polling, not real-time. |
| Proposed standard Filtered Stream | Event-driven collection with server-side topic filters | X documents a persistent HTTP stream and pay-per-use access. Implement reconnect, budget control and supported gap recovery before activation. [Provider reference][stream] |
| X Activity webhooks | Per-user outgoing post activity where full account coverage is required | A separate API: `post.create` is selected by user ID; keyword/Boolean filtering belongs to Filtered Stream. Local rejection does not avoid delivery charges. Verify access, payloads and signatures. [Provider reference][activity] |
| Filtered Stream Webhooks | Enterprise delivery option | The dedicated provider page currently describes Enterprise access. Do not assume the standard stream entitlement includes this webhook product. [Provider reference][stream-webhooks] |

Choose one primary collector per overlapping scope. Do not run routine timeline
polling, search and streaming over identical data and expect three independent
signals. Backfill is a bounded recovery action, not another permanent collector.
Daily batching is a pilot trade-off, not a change to holding-watchdog requirements.
No latency guarantee is inferred from choosing webhooks rather than a stream.

## Cost model and proposed budget

Published USD rate snapshot on 2026-10-31, subject to verification in the
application's Developer Console before activation. [X pricing][pricing]

| Billable unit | Published unit price |
|---|---:|
| Returned post read | $0.005 per resource |
| X Activity `post.create` webhook event | $0.005 per delivered event |
| Returned user read | $0.010 per resource |
| Recent Counts request | $0.005 per request |
| X Activity `post.delete` event | Not billed |

Read billing is resource-based; request counts alone are not a monetary budget.
Batching the same returned posts is not inherently cheaper. Extra identity or
context reads can add costs. Provider spending limits apply per billing cycle;
verify current rates, entitlement and accounting rather than relying on this
snapshot. [X pricing][pricing]

Use a proposed **$10 per billing cycle** pilot ceiling: target at most **$8** for
ordinary post collection and reserve **$2** for identity, counts, context, recovery
and accounting uncertainty. These are proposed allocations, not measured needs or
a guarantee that the five accounts fit. Keep model/hosting costs separate.

Illustrative arithmetic, excluding extra resources and infrastructure:

| Hypothetical collection | Post reads over 30 days | Post-read estimate |
|---|---:|---:|
| Five accounts, ten delivered posts each per day | 1,500 | $7.50 |
| Same hypothetical volume, upstream rules admit 40% | 600 | $3.00 |
| $8 ordinary-post allocation fully consumed | 1,600 | $8.00 |

Do not treat the 40% admission rate or ten posts/day as observed measurements.
Slower polling can reduce repeated lookup/request work but does not reduce the
price of the same set of returned posts. In particular, the shipped adapter's
per-page identity lookup means collection cadence and pagination also matter.
Provider deduplication is not a substitute for a durable local cache or a
conservative budget estimate.

### Measure before activation

Use [Recent Counts][counts] to estimate the prior seven days for each proposed
query without downloading the matching post bodies. Counts are planning estimates,
not a completeness guarantee or a forecast of next month's activity. Five
single-request counts cost an illustrative $0.025 at the snapshot rate; comparing
five author-only and five topic-filtered queries costs $0.05 if each needs one
request. These requests have not been executed for this document.

Record the query/version, requested interval, completeness, per-day counts,
measurement time and planned monthly cost, with a burst margin and allowance for
non-post resources. A missing or incomplete count is unknown, not zero. Inspect
any necessary pagination and endpoint limits. Do not activate an unfiltered
high-volume source merely because its average week looked affordable.

### Budget enforcement to implement

The bounded request implementation now supplies the gate described here; see
[the operator contract](../news.md#x-pilot-and-resource-budget). Continuous-delivery
headroom/reconnect controls still require a separate stream implementation.

The existing durable request reservations are not a dollar-denominated billing
ledger. Add a reviewed collector-owned cost gate that reserves conservative
resource bounds before bounded requests, persists consumption and reconciles
against provider usage/billing. Include identity and context resources, retries,
query changes and shared consumers; do not assume every repeat is free.

For continuous delivery, stop optional collection with headroom for in-flight
events and accounting delay. Configure the provider spending limit as the outer
control; a local estimate cannot promise an exact final invoice. Restrict any
auto-recharge policy separately. On exhaustion, record a visible gap and stop
optional X reads instead of secretly using another app or unapproved scraper.

Neither a research budget nor X availability can disable quote monitoring,
broker reconciliation, existing protective orders or latched exits. Required
missing evidence blocks dependent new decisions; it is not neutral sentiment.

## Shared collector, storage and interfaces

```text
Approved identity registry + versioned queries + budget/rights gates
                              |
                 One primary X collection path
                              |
              PostgreSQL observations and lineage
                    /          |           \
          daily research   holding review   permitted offline export
                                                |
                                     local weekend evaluation
```

Reuse the storage/rights/cursor contracts in [news reading](../news.md). The
proposed listener and budget gate belong to research collection, without broker
credentials. They do not create an alternative model-controlled execution path.

Minimum logical record requirements, not a new implemented wire schema:

| Record | Required information |
|---|---|
| Identity/rule | Stable account ID, current handle, ownership review/expiry, rule text/hash/version, approval and effective interval |
| Observation | Provider post/event IDs, actor and original author, references/edit lineage, publication and first-seen times, content/version hash, source and rights revision |
| Availability | Durable ingestion, verification and feature-ready times; transport mode, cursor, incomplete intervals and selected coverage |
| Budget | Resource category/count, reservation, conservative estimate, provider reconciliation and budget-policy version |
| Evaluation | Canonical event lineage, resolved eligible issuers, novelty, uncertainty, decision-time feature/model versions and outcome maturity |

Deduplicate exact posts and revisions in storage, then link cross-posts and news
repeats into event lineage where supported. The existing implementation handles
known IDs/shared URLs; automatic semantic claim clustering remains future work.
Several copies of one statement are not independent corroboration.

Advance cursors only after durable persistence. Preserve pending pagination across
bounded runs; errors and access failures must not advance the successful
watermark. Query changes start a recorded coverage revision, not an invented
continuous history. Stream recovery must preserve gaps outside supported history.

## Research and weekly evaluation

[Prediction fusion](./prediction-fusion.md) keeps text sentiment, a person's
opinion, an LLM scenario and a calibrated return forecast as different quantities.
Extract supported issuer links, claim type, novelty, uncertainty and timing before
permitted sentiment/model processing. A bearish Burry statement can challenge a
long thesis; it does not authorize short selling. A named partnership does not
prove a quantified supplier revenue benefit.

The [local weekly workflow](./weekly-optimization.md) uses a trusted, rights-checked
export, not independent X queries by Codex or every backtest worker. Set routine
weekend X fetches to zero. Separately approved compliance or gap-recovery work has
its own collector budget. Do not re-download expired content to recreate an
otherwise unavailable historical dataset.

Review source utility weekly, but propose changes only from adequate evidence:

| Dimension | Evaluation requirement |
|---|---|
| Cost and relevance | Returned resources and actual spend by account/rule; unique relevant event count; cost per unique relevant event |
| Novel information | Compare with existing news/filing sources using observed arrival times; a rewritten headline is not an independent event |
| Timeliness and coverage | Whether usable information preceded the decision; record outages, filtering bias, unavailable context and missed intervals |
| Incremental value | Compare a no-X baseline and account/rule ablations on the same eligible decisions and chronological held-out periods |
| Safety and uncertainty | False alerts, unsupported issuer mappings, contradiction handling, and decisions blocked because evidence was incomplete |

Use the time information became available to the decision system, not just its
publication timestamp. Later retrieval, edits, confirmations, sentiment outputs
or outcome labels cannot be backdated into an earlier decision. Keep no-action,
neutral and nonmoving events; do not evaluate only memorable successful calls.
One week or one dramatic price move is not sufficient evidence to promote a rule
or assign a causal-return weight. Report sample size and abstain when unsupported.
Account/rule replacements require owner approval; model/formula changes retain
the independent evaluation and release gates. No scheduler is installed here.

## Data rights, security and invariants

[X's developer guidelines][guidelines] restrict unauthorized AI/ML training and
require compliant data handling. API access is not a blanket license to retain,
redistribute, send to a cloud model or train on content. Review the applicable
agreement and deployment permissions before enabling each use.

Keep `store_content`, `llm_processing` and retention decisions from the source
registry; a permission for inference does not establish training permission.
Exclude X text and X-derived training inputs from FinBERT/Perpetual/fusion fitting
unless separately documented applicable rights permit that use. Derived features
are not an automatic exemption. Rights-ineligible data must be excluded from
optimization; public market data or separately licensed sources can supply the
eligible baseline instead.

- Propagate edits, deletions, unavailable/private content and retention expiry to
  affected records and dependent artifacts. Transport failure alone is not deletion.
- Send only permitted minimized context to cloud models; local Codex execution
  does not make a cloud model local. Keep provenance and policy decisions auditable.
- Treat all post text as untrusted data, never tool instructions, SQL, shell or
  authorization to fetch arbitrary URLs. Keep credentials out of docs and logs.
- No social statement, sentiment score, failed collector or exhausted budget may
  bypass owner policy, Rust validation, protected exits or deployment approval.

## Implementation milestones and verification

| Milestone | Acceptance evidence |
|---|---|
| Owner decision and preflight | Explicit watchlist/coverage approval; verified identities, rights, entitlement and measured counts; disabled defaults preserved until approval |
| Offline collector work | Implemented: query/version validation, resource-cost ledger, incremental recent search, resumable counts and local inference/evaluation export with deterministic fixtures; no production subscription required |
| Bounded shadow pilot | Owner-enabled budget and permitted data collection; measured delivery/cost/gaps; no execution authority or automatic watchlist changes |
| Review and handoff | Longer held-out comparison, documented limits and owner-approved source configuration; retain current policy when evidence is insufficient |

Verify author versus incoming-reaction attribution; keyword misses; optional
repost exclusion; handle reuse; repeated pages and failed cursor commits;
cross-source duplicates; late edits/deletes; unsupported backfill; rate/credit
exhaustion; stream in-flight costs; rights expiry; and point-in-time feature
availability. Verify that optimizer workers perform no X network calls and that
all collection failures leave existing holding protection independent.

Documentation review must check the reading-map link, relative links, source
attribution, examples and the explicit proposed/implemented boundary. Repository
checks for a later implementation include `python3 scripts/verify-assets.py` and
`cargo test --workspace --locked`, plus the news/database checks in
[the implementation record](../news.md#verification-and-remaining-scope).

## Side Effects

Explicit enabled collector commands can make paid read-only X requests after
durable resource reservations and persist observations, coverage and cost records.
The separate example ships disabled and does not modify `config/research.json`
or populate user IDs. Export reads/purges permitted local research without X calls.
No paid endpoints were accessed during implementation; no current volumes,
entitlements or empirical profitability were validated. No webhook, training,
deployment, order authority or weekend automation is installed.

## Sources and verification date

Provider capabilities/prices and primary account evidence were checked on
2026-10-31. Recheck mutable capabilities, roles, identity and terms before
activation. Primary evidence supports the stated roles/attribution; the pilot's
selection, filter terms and budget allocations remain proposed design decisions.

[ackman]: https://pershingsquareholdings.com/pershing-square-holdings-ltd-notes-additional-quarterly-communications/
[musk]: https://www.sec.gov/newsroom/press-releases/2018-219
[bessent]: https://home.treasury.gov/about/general-information/officials/scott-bessent
[treasury]: https://home.treasury.gov/
[burry]: https://www.scionasset.com/
[altman]: https://blog.samaltman.com/quora
[openai]: https://openai.com/index/scaling-ai-for-everyone/
[operators]: https://docs.x.com/x-api/posts/filtered-stream/integrate/operators
[stream]: https://docs.x.com/x-api/posts/filtered-stream/introduction
[activity]: https://docs.x.com/x-api/activity/introduction
[stream-webhooks]: https://docs.x.com/x-api/webhooks/stream/introduction
[pricing]: https://docs.x.com/x-api/getting-started/pricing
[counts]: https://docs.x.com/x-api/posts/counts/introduction
[guidelines]: https://docs.x.com/developer-guidelines

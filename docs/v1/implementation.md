# Trady v1 implementation and acceptance

Status: implementation backlog and acceptance specification. The [foundation](../foundation.md)
and [X/news reading record](../news.md) describe implemented subsets and verification;
the full acceptance cases and live release gates below remain requirements.

## Delivery sequence

| Phase | Deliverables | Exit criteria |
|---|---|---|
| 1. Contracts and capabilities | Domain types, policy/config schema, fake clock/calendar, broker/quote/source ports and provider capability records | Exact target/stop/cost meanings agreed; unknown broker capabilities do not get assumed defaults |
| 2. Rust and persistence foundation | Cargo workspace, typed errors, decimal calculations, process wiring, migrations, role grants, leased jobs, outbox and account fencing | Fake-account reconciliation and restart work; runtime privilege boundaries are demonstrated |
| 3. Research and knowledge | Effective S&P 500/GICS snapshots, sector/group funnel, company profiles/relationships, non-X/RSS/site sources, five-person X listeners and watchdogs | All 11/25 guides route correctly; profiles/events keep provenance, contradictory facts and coverage gaps |
| 3a. FinBERT sentiment | Self-hosted CPU Docker sidecar, rights-aware Rust adapter, exact model/item/hash correlation, sentiment threshold overlay and holding-review integration | Offline contracts pass; no fabricated neutral result, duplicate-lineage amplification or sentiment bypass of hard risk limits; real model startup deferred until deployment |
| 4. Sessions, tools, skills and Telegram | Multi-round JSON tool loops, T212 information boundary, approved skill loader/artifacts, context manifests, compaction, dialogue and notifications | Correlated tool turns and owner follow-ups survive restart; required skills validate; unauthorized tools/commands have no authority |
| 5. Perpetual and paper execution | Point-in-time feature/label pipeline, offline Rust training/evaluation, approved advisory model/tool; readiness, 98% sizing, entry slots, plans/protection and optional rotation | No leaked/unvalidated ML input or incomplete research entry; all quote/cost/partial-fill and advisory/limit-ownership cases pass |
| 6. Read-only and shadow operation | Real approved feeds, broker snapshots, reports, scheduled research and shadow intents | At least 20 exchange sessions with no unexplained reconciliation difference, duplicate intent or protection logic defect |
| 7. Restricted live pilot | Owner-enabled account-specific configuration, verified protection, budget caps, incident runbooks and rollback | All live gates pass and the owner explicitly authorizes the reviewed deployment |

Read-only research can be useful before execution is implemented. The 20-session shadow period is a proposed operational observation window, not evidence of statistical profitability. Extend it when outages, missing events or untested failure paths leave material uncertainty.

Paper balances begin with the owner's representative €50 example and include deposit scenarios. Do not use an inflated demo balance that hides fees, minimum order sizes, rounding or the economics of a 1% target. Paper/shadow and live use the same policy functions and state transitions; adapters differ.

## Required behavioral acceptance cases

### FinBERT implementation and acceptance

The [sentiment contract](./finbert/README.md) implements the sidecar files, domain/port,
HTTP adapter, explicit CLI and pure threshold overlay. Twelve focused Rust tests
and six offline Python tests cover permitted/denied/expired/unsupported-language
text, lineage deduplication, exact response correlation, invalid/oversized responses,
redirects/unavailability, inference failures, probability normalization, stale
assessments and exact adverse-probability boundaries. Compose and offline-overlay
configuration validate without starting a service.

Required deployment/integration cases remain: load the pinned real model only when
launch is intended; exercise readiness, cold/offline cache and resource failure;
evaluate X/news tone and truncation on representative permitted fixtures; persist
instrument-bound assessments and invalidations; block entry/target increases on
missing required inference; bind calculations to readiness and checked plan updates;
prove existing exits/protection continue through inference outages and that old exit
latches win. The current source/CLI calculation grants no automated entry authority.

### Remaining product acceptance cases

These are future tests that check externally meaningful behavior. Pure policy/state-machine cases use deterministic fixtures; persistence/concurrency cases use a disposable PostgreSQL instance; provider cases use recorded permitted fixtures or demo environments. Never run tests against `paperless-db-1`'s Paperless database or a live brokerage account.

| ID | Scenario | Required outcome |
|---|---|---|
| CAP-01 | €50 confirmed cash with known fees/FX | Total estimated entry cost is at most €49; quantity rounds down; reserve is at least €1 |
| CAP-02 | Confirmed cash increases from €50 to €150 while flat | Next eligible entry uses a total-cost budget up to €147 without changing policy |
| CAP-03 | Deposit arrives while a stock is held | Cash event recorded; no add-on purchase or forced rebalance |
| CAP-04 | Unsettled proceeds, pending deposit, broker reservation or tiny cash balance | Do not double-count spendable cash; skip an unaffordable/minimum-size order |
| CAP-05 | Fees change or FX becomes stale between proposal and submission | Revalidate/reduce/reject; never spend the original nominal budget blindly |
| POL-01 | Confidence 69/70/79/80/89/90/94/95/100 | No entry below 90; at/above 90 require readiness; holding target bands and exceptional gates still apply; model confidence has no authority |
| POL-02 | Model or owner configuration asks for 11%, leverage or 99% allocation | Reject unsupported values; owner changes cannot exceed v1 ceilings |
| POL-03 | A 1% target with a proposed 2% stop | Reject because reward/risk is below 1.5, despite the absolute 2% stop ceiling |
| POL-04 | Technical stop cannot meet risk/reward after spread and fees | Skip entry rather than fabricate a tighter stop |
| POL-05 | €49 entry realizes a 1% net position gain from a €50 account | Report approximately 0.98% account growth before outside costs, with separate denominators |
| POL-06 | Session loss guard trips then a deposit/rebound arrives | Entry pause remains latched; exits continue; percentage metric identifies its denominator |
| PLN-01 | Fast rise and a supported 87-point update before +1% | A valid 2% target can activate; stop cannot loosen and original deadline cannot extend |
| PLN-02 | Old target quote and optimistic update race | Serialize both; an old target already reached latches exit and cannot be cancelled |
| PLN-03 | New lower target is already satisfied at activation | Atomic activation immediately latches one exit |
| PLN-04 | Model, memory projection or news service is unavailable | Last valid plan and native protection remain; price-triggered exits need no generation |
| PLN-05 | Quote reaches a trailing floor then the process restarts | Durable peak/floor and exit latch survive; no reset to a looser rule |
| PLN-06 | Good research arrives after an exit has latched | Preserve exit; do not reopen the position or replace the exit with `HOLD` |
| PLN-07 | Fifth holding session or no overnight protection | Apply actual-session time exit; an unresolved close remains visible as exposure |
| ORD-01 | Morning and midday jobs race for an entry slot | One durable slot and intent; at most one entry submission attempt that session |
| ORD-02 | Buy fills and its profit target or stop is reached the same day | Exit is allowed; historical buy-or-sell shared slot does not block it |
| ORD-03 | Timeout after broker accepted an order | State becomes unknown; reconcile the actual order/fill; no blind retry |
| ORD-04 | Crash after intent commit but before request; crash after request but before saved broker ID | Recover each conservatively; a consumed entry slot stays consumed; ambiguity blocks new submission |
| ORD-05 | Entry partially fills while cancellation or protection is being placed | Actual held quantity is protected; late fills do not create unprotected/oversold shares |
| ORD-06 | Native stop and software target/owner close overlap | Verified coordination prevents selling more than held; unresolved original orders block replacements |
| ORD-07 | Duplicate fill, repeated quote, duplicate job or partial exit | Deduplicate financial events and keep one unresolved position exit; reconcile remaining quantity |
| ORD-08 | Two executors or an old process resumes after lease loss | Fenced writes fail; stale process stops requests; takeover reconciles possible predecessor effects |
| ORD-09 | Unexpected manual holding, split or broker symbol change | Enter recovery and reconcile; no improvised second holding or unknown-instrument sale |
| ORD-10 | Limit price not filled, market halted or close passes | Do not chase entry; preserve pending/overnight risk in reports until broker-confirmed resolution |
| DAT-01 | Quote older than 10 seconds, FX/account older than 60 seconds or inadequate venue coverage | Reject new applicable decisions; never substitute a headline/last price for a fresh bid/ask |
| DAT-02 | Holiday, early close or US/EU daylight-saving mismatch | Use the actual exchange session; no fixed German-time or missed-job catch-up order |
| SRC-01 | X, Google RSS and multiple publishers copy the same rumor | One reporting lineage; no artificial confidence gain or exceptional target |
| SRC-02 | Old story is reshared or an original release is corrected | Use event time and contradictions; invalidate affected actionable evidence |
| SRC-03 | Source rate limit/page cap or unresolvable Google redirect | Preserve cursor/completeness status; report partial/metadata-only evidence |
| SRC-04 | Submitted X post is deleted, private or refers to an unapproved ticker | Clear research result without fabricated content or watchlist/order changes |
| SRC-05 | Custom scraper changes DOM or returns empty text | Mark parser unhealthy; no successful empty result treated as proof of no news |
| SRC-06 | Source URL redirects to private IP, XML entities or oversized content | Fetch/parser rejects it within bounds and does not reach internal services |
| SRC-07 | Non-X primary source is unavailable or model-processing rights are missing | Do not count it as usable evidence; block entries missing mandatory checks |
| LLM-01 | Malformed schema, refusal, truncation, unknown evidence ID or timeout | No new executable plan; retain prior protection and failure audit |
| LLM-02 | Article/memory says to reveal keys, submit an order or rewrite policy | Treat as data; no access to those capabilities or writes |
| LLM-03 | New policy/plan activates while generation is running | Store the response but reject stale expected versions for execution |
| LLM-04 | Context budget exhausted or session compaction fails | Preserve required rules/current state; fail without truncating policy or altering orders |
| LLM-05 | Two tool rounds with the second depending on the first snapshot/item ID | Preserve JSON requests/results in order and produce a final structured answer using actual returned IDs |
| LLM-06 | Missing call result, duplicate ID with changed arguments or prose around JSON | Reject invalid correlation/format; no final entry from an unresolved tool turn |
| LLM-07 | Native tool adapter and JSON-envelope fallback receive equivalent results | Same normalized semantics and validation; unsupported provider behavior never fabricates successful tools |
| LLM-08 | Tool/model limits, global source budget or finalization deadline reached | Respect caps/reserves, retain partial artifacts and return incomplete/no action rather than an entry |
| LLM-09 | Model requests order submission/cancellation, another account or arbitrary URL/SQL | Registry/schema/scope denial; no broker trading credential or financial mutation |
| LLM-10 | Truncated streamed arguments or a late result after run expiry | No dispatch of partial calls; late data cannot reactivate expired research |
| LLM-11 | Research-bearing finals, all four proposal kinds and model-supplied envelope/unknown fields | Validate exact versioned schemas; reject extras, mismatched types and incomplete replacement thresholds |
| LLM-12 | Broken/empty artifact pointers, duplicate skill/check identities or claimed completion without referenced evidence | Semantic completion validation fails; schema validity alone never proves readiness |
| SKL-01 | Many sites/X/RSS items copy one issuer rumor | One origin; primary/independent comparison remains incomplete; no entry even if the model claims 99 confidence |
| SKL-02 | Apparent breakthrough is an old prototype story reshared today | Skill records original event/development stage and counterevidence; no fabricated new-catalyst readiness |
| SKL-03 | Completed good research but final-model latency ages its quote | Executor refreshes/recalculates; entry only if the new inputs still pass, otherwise reject/reassess |
| SKL-04 | Required skill missing, hash changed or version unsupported | Block entries; current holding protection remains independent |
| SKL-05 | Model declares skill/readiness complete without source/tool/calculator records | Application/executor reject the forged completion; no slot is reserved |
| SKL-06 | Required research complete, independent primary-supported evidence, score 93 and valid fresh economics/account | Final proposal may enter automatically only after executor readiness checks and durable intent/slot reservation |
| SKL-07 | Material sources disagree despite a high numerical score | Contradiction blocks entry until a cited resolution exists |
| MEM-01 | Restart during summary/file projection, or file edited manually | Rebuild from canonical revisions; do not import manual instructions/financial balances |
| MEM-02 | Source deletion/retention purge or restore of an older backup | Propagate to affected chunks/summaries/projections; reapply tombstones before retrieval |
| MEM-03 | Relevant losing and no-trade examples exist | Retrieval does not select only winners; notes remain attributed hypotheses |
| DB-01 | Analyst/gateway attempts financial writes or owner-command forgery | Database grants/typed queues reject them; no execution effect |
| DB-02 | Runtime role attempts Paperless login/object access or migration | Denied under the reviewed database/authentication boundary; Paperless still works |
| DB-03 | Database goes down, backup restored or queue lease expires | Stop new submissions; reconcile before replay; native protection is retained |
| DB-04 | Analyst requests scoped T212 information refresh while protection work is queued | Typed read operation only; correct account/mode, lower priority, bounded refresh and sanitized result |
| TG-01 | Spoofed username, forwarded owner message, group chat or duplicate update | Numeric IDs/private chat govern authorization; no duplicate command effect |
| TG-02 | Expired/reused confirmation or changed position/version | Reject stale preview; one confirmation cannot close twice |
| TG-03 | Pause races with an entry fill, then owner requests resume | Cancel/reconcile if possible, protect fills, preserve daily/unknown-state guards |
| TG-04 | Owner submits URL, requests a fresh session or adds an instructive memory note | Research/session/note behavior works without changing policy or placing an order |
| TG-05 | Telegram send succeeds before gateway crashes | Outbox replay may duplicate the identified message but cannot duplicate execution |
| TG-06 | Owner asks, follows up, answers clarification or replies to a scheduled report | Correct persistent/linked owner session, fresh facts, bounded tool rounds and no conversation-originated order command |
| REP-01 | Deposit, withdrawal, fees, dividends and outside API costs | Distinguish flows/P&L, avoid fee double counting and label any estimated return |
| REP-02 | Unsold gain, incomplete valuation or pending exit | No completed win or invented realized result |
| REP-03 | Source/model budget is reached mid-run | Account for reserved/actual costs, stop paid work and continue independent exits |
| UNI-01 | Membership/classification changes tomorrow or two share classes share an issuer | Use effective dates and exact class/venue; no future membership or mistaken instrument eligibility |
| UNI-02 | Sector gain comes from one mega-cap; another sector has missing observations | Breadth/weighting visible; missing component is unknown and cannot win priority ranking |
| UNI-03 | Energy/Materials/Utilities has one industry group | Required branch filtering still narrows exposure; group label alone is insufficient |
| UNI-04 | A selected sector/group has no owner/broker-eligible members | Preserve exclusions and choose another researched focus or no action; never expand the universe |
| UNI-05 | A multi-stage run reaches budget then resumes after a material correction | Keep progress, invalidate affected artifacts, refresh facts and routed topic skills |
| CO-01 | A CEO changes role or an undated biography conflicts with a filed appointment | Effective role history and original disclosure govern attribution; no stale-role assumption |
| CO-02 | A CEO praises a competitor or quotes someone else's criticism | Store actual directed speaker/target/quoted edges, ambiguity and materiality; no invented contract or endorsement |
| CO-03 | Supplier/customer statements conflict with the finalist's growth promise | Persist both and block thesis-dependent entry until material conflict resolves |
| CO-04 | Corrected/deleted source contributed to a profile, graph, feature and memory file | Invalidate/purge permitted derivatives and dossier dependencies; retain only allowed audit references |
| X-01 | Selected person authors a reply/repost while many others react to them | Capture their own `post.create` action separately from incoming reaction events |
| X-02 | X stream loses connection; polls hit a page cap; a repost is later removed | Preserve explicit incomplete interval/capability gap, dedupe replay and do not promise complete history |
| X-03 | TheRoaringKitty posts a meme or an ineligible instrument is mentioned | Retain scoped P0 context/uncertainty; no guessed ticker/intent or exception to member/owner eligibility |
| X-04 | Trump cross-posts an official policy story across X/Truth Social; source access missing | One lineage, verified effective action or explicit coverage blocker; no assumed public API |
| X-05 | Account ID/ownership is unverified or a handle changes | Disable pending listener; resolve stable user ID/ownership before activation, never silently substitute a person |
| WD-01 | A fill is partial while watchdog enrichment is incomplete | Protect actual quantity first; activate validated template against real position/version without delaying protection |
| WD-02 | An unrelated reaction flood competes with a holding-specific regulator alert | Holding/critical work wins, bounded alternative budget, metadata/deferral reasons retained |
| WD-03 | Position closes while a shared five-person subscription and old jobs exist | Retire position binding only after flat reconciliation; shared subscription remains; stale jobs cannot change plans |
| WD-04 | Required watchdog source fails or model/inference goes down | Critical coverage alert, no unsupported update/new entry, last plan and native protection continue |
| ML-01 | Rust classifier returns raw logit or an uncalibrated score | Never label it a probability; exact adapter/calibration path and model version required |
| ML-02 | Wrong feature order/version/hash, NaN, future release or revised membership enters inference | Reject input/artifact; point-in-time lineage, finite values and model compatibility enforced |
| ML-03 | Random time split or overlapping label horizon leaks future information | Promotion fails; chronological purge/gap and separate calibration/untouched test required |
| ML-04 | No approved model, stale assessment, out-of-range regime or unsupported scenario | Explicit abstain/unavailable and blocked required entry; no fabricated prediction/interval |
| ML-05 | LLM changes target/stop/horizon after assessment | Recalculate/reassess the exact final scenario; stale advice cannot satisfy readiness |
| ML-06 | Perpetual likes a setup but primary evidence conflicts or score is below 90 | No entry; prediction is not independent evidence or confidence points |
| ML-07 | LLM disagrees with a usable assessment and keeps supported limits | Record reason and actual assessment; LLM proposal remains possible within every independent Rust gate |
| ML-08 | Price target latches during slow inference or offline training spikes CPU | Exit/protection proceed independently; no delayed latch or inference-caused executor starvation |
| ML-09 | New extractor/model revision or delayed outcomes change monitoring | Version/replay/evaluate before promotion; label-free drift is not proof of predictive accuracy |
| ROT-01 | Holding bought today, default one-entry policy, attractive alternative | `ENTRY_SLOT_USED`; no second buy or research-originated sell |
| ROT-02 | Enabled eligible comparison races with reached old target/stop | Old exit wins; comparison cannot return to holding or duplicate sale |
| ROT-03 | Comparison clears numeric hurdle with unsupported forecasts or double-counted fees | Validation rejects; scenario truth/support and consistent net costs remain mandatory |
| ROT-04 | Old sale fills but alternative becomes stale/unaffordable or proceeds unavailable | Reconcile flat, reject/delay second leg and keep cash; no simultaneous holdings or blind retries |
| ROT-05 | Two-attempt policy receives two ordinary entries or repeated rotations | Typed slot/rotation constraints reject; at most one ordinary and one replacement attempt |

## Requirement traceability

| Requested behavior | Authoritative design | Acceptance evidence |
|---|---|---|
| Name Trady and use Rust | [Overview](./README.md), [architecture](./architecture.md) | Phases 1–2; process/workspace build identity |
| Use X plus other market-news/agent APIs | [Research](./research.md) | SRC-01, SRC-07; phase 3 adapter capability checks |
| Use Google News RSS and optional custom scrapers | [Research](./research.md) | SRC-03, SRC-05, SRC-06 |
| Telegram control, research updates and submitted X URLs | [Telegram](./telegram.md) | SRC-04, TG-01 through TG-05 |
| LLM sessions, persistent context, memory files and knowledge | [Memory and data](./memory-and-data.md) | LLM-03, LLM-04, MEM-01 through MEM-03 |
| Multi-round back-and-forth dialogue with JSON responses | [LLM conversations and tools](./llm-and-tools.md), [Telegram](./telegram.md) | LLM-05 through LLM-08, LLM-10, TG-06 |
| News/X/T212 information and other scoped model tools | [Tool catalog](./llm-and-tools.md#tool-registry-and-scope) | LLM-05, LLM-09, DB-04; source/market adapter cases |
| Scenario SKILL.md files and finalized high-confidence entry | [Runtime skills](./runtime-skills.md), [entry readiness](./strategy.md#finalized-research-and-entry-readiness) | POL-01, SKL-01 through SKL-07 |
| PostgreSQL using the existing server with a new database | [Operations](./operations.md) | DB-01 through DB-03; verified isolated provisioning/restore |
| LLM chooses stocks, entries, stop losses and sell targets | [Trading policy](./strategy.md), [proposal schema](./memory-and-data.md) | POL-01 through POL-04, LLM-01; no broker tools in analyst |
| Continue research and adjust thresholds during holding | [Architecture](./architecture.md), [trading policy](./strategy.md) | PLN-01 through PLN-06 |
| Confidence points affect targets | [Trading policy](./strategy.md) | POL-01, POL-02, SRC-01 |
| Automatically sell at an active threshold | [Trading policy](./strategy.md) | ORD-02, ORD-06, PLN-02, PLN-03 |
| Aim for 1%, sometimes 2–3%, exceptional ceiling 10% | [Trading policy](./strategy.md) | POL-01, POL-02, POL-05; report target versus achieved gain |
| Dynamic deposits and 98% eligible-entry allocation | [Trading policy](./strategy.md) | CAP-01 through CAP-05, REP-01 |
| Detailed v1 docs and retained original input | [v1 index](./README.md), [original](./v1.1.md) | Reading map, relative-link checks and superseded-rule table |
| Sector → subsector → S&P 500 stock with separate researched guides | [Funnel](./sector-workflow.md), [sectors](../../skills/sectors/README.md), [subsectors](../../skills/subsectors/README.md) | UNI-01 through UNI-05; catalog contains every 11/25 code |
| CEO/company deep dive, profiles, praise/criticism and cross-stock links | [Company intelligence](./company-intelligence.md) | CO-01 through CO-04; provenance and revision checks |
| Owner's five influential X people, their replies/reposts and scoped priority | [X listeners](./watchdogs-and-x.md) | X-01 through X-05; active account identity/capability checks |
| Personalized holding watchdog and selective alternative attention | [Watchdogs](./watchdogs-and-x.md), [rotation](./strategy.md#opportunity-rotation) | WD-01 through WD-04, ROT-01 through ROT-05 |
| Perpetual advisory tool with LLM-chosen limits | [Perpetual](./perpetual.md), [skill](../../skills/perpetual-review.SKILL.md) | ML-01 through ML-09; actual model/version/calibration evidence before promotion |

## Live release gates

Before a live release, all required acceptance cases pass, deployment capability records have evidence, and the owner receives a concrete configuration containing account identity, approved universe, percentage limits, source/model caps, data/protection assumptions and known residual risks. Obtain explicit live-deployment authorization then; writing these docs or sending `/start` is not live authorization.

Demonstrate the exact broker's fractional protection, cancellation/late-fill behavior and no-oversell coordination. If unavailable, use a compatible broker/profile or stay in paper/shadow mode. Neither positive paper P&L nor an LLM recommendation can waive those gates. Keep the reviewed previous build/config available for rollback, which still requires account reconciliation rather than reverting the broker to a backup state.

## Verification contract

The initial Cargo workspace now has formatting, lint, domain/policy/application/CLI
tests and a separate isolated PostgreSQL scenario. The [foundation record](../foundation.md)
lists actual commands, results and partial coverage. Passing that subset does not
complete the acceptance cases above or authorize a live release.

For contract updates, explicitly read every created/modified file, verify local
links and anchors, check Markdown fences/tables/headings, parse structured examples
where tooling is available, and review allocation/target arithmetic and requirements
coverage. Source/fee/broker claims must have primary references or be labeled as
deployment decisions requiring verification.

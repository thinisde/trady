# Trady v1 trading policy and execution

Status: proposed policy to implement and evaluate in paper mode. Numeric defaults are starting settings, not measured predictors of returns. The owner versions policy; the LLM proposes plans inside it.

## Objective and scope

Aim for a normal 1% net position gain, 2–3% when evidence and market conditions justify it, and at most 10% for an exceptional verified catalyst. Record daily account performance separately. A day without an eligible opportunity is a valid no-trade day; a loss does not cause a larger next trade, an extra entry attempt, or an increased target.

V1 uses one dedicated cash account, one distinct stock at a time, a small owner-approved universe, no shorting or leverage, and no additions to an open holding. Research continues throughout a holding. A fast rise may justify a higher target and a tighter trailing floor; a slower rise may retain the existing plan until its target, stop, expiry, or maximum holding time is reached.

The LLM chooses the candidate and proposes an entry range, stop, target, holding thesis and invalidation conditions. Rust evaluates those proposals and submits orders. There is no `SELL` or `BUY` model tool. A `PLAN_UPDATE` can cause a deterministic exit after validation, but it is never an order instruction.

## Capital and deposits

Let `A` be reconciled spendable cash in account currency. Derive it from the broker's documented cash fields, excluding pending deposits, unsettled proceeds that are not spendable, cash reserved for open orders and amounts owed. Do not subtract a reservation twice if the broker's available-cash field already excludes it. Exclude margin buying power.

```text
entry_cost_budget = floor_to_currency_unit(A × 0.98)
minimum_remaining_cash = A - entry_cost_budget
```

The budget includes purchase consideration, estimated entry fees, applicable transaction charges, FX costs and an uncertainty allowance. Size the quantity downward to the broker's supported increment, then recalculate costs using the final limit price. Use a bounded search if charges are nonlinear. Reject an unaffordable or below-minimum order rather than rounding quantity upward.

| Confirmed spendable cash | Maximum estimated entry cost | Cash retained before later account changes |
|---:|---:|---:|
| €50.00 | €49.00 | At least €1.00 |
| €150.00 after an owner deposit | €147.00 | At least €3.00 |
| €500.00 | €490.00 | At least €10.00 |

These are total-cost budgets, not share consideration alone. Fees, rounding, instrument minimums and a stricter risk check can reduce the amount invested. Unknown costs block the entry. The 2% reserve is a sizing constraint, not a guarantee about later cash deductions or losses.

Detect deposits and withdrawals through broker cash events and reconciliation. Pending transfers do not count. If a deposit arrives while holding, record it and continue managing the same quantity; use the increased balance at the next eligible entry. Do not rebalance daily or sell merely to reinvest 98%. An outside manual withdrawal requires account resynchronization before another order.

By default reserve one entry attempt per account/session; the [opt-in bounded rotation policy](#opportunity-rotation) is the only permitted second-attempt exception. Local failures before reservation do not consume a slot. A committed entry intent consumes its slot even if cancelled, failed or never sent; partial fills belong to it. Exits/replacement exits consume no entry slot. Selling then buying requires flat reconciliation, confirmed spendable proceeds and an unused eligible ordinary/replacement slot.

A stop or target can sell on the same day as entry; an already-used entry slot never blocks that exit.

## Finalized research and entry readiness

New entry requires a final dossier, completed mandatory skill artifacts and high computed confidence. The proposed v1 definition of high confidence is at least **90/100**. This is a checklist gate as well as a score gate; neither can compensate for failure of the other.

| Required check | Evidence needed before entry |
|---|---|
| Research finalized | Valid JSON `FINAL`, no outstanding tool requests, a complete current research artifact and no incomplete/clarification status |
| Required skills completed | All thirteen routed shared/selected-sector/group/company/relationship/watchdog/Perpetual skill artifacts validated under [runtime integration](./runtime-skills.md) |
| Focus and advisor current | Effective S&P 500/owner/broker eligibility, completed screening, current profile/relationship review, prepared watchdog and required usable Perpetual assessment matching final limits |
| Primary material claim verified | Readable original release/filing/official evidence with checked issuer and event date |
| Multiple-source comparison complete | Primary evidence plus at least one genuinely independent supporting/reporting lineage; copied/syndicated posts counted together |
| Contradictions resolved | No unresolved material conflict, missing mandatory fact or undisclosed required-source coverage gap |
| Confidence high | Current deterministic confidence calculation at least 90/100, with references to checked component inputs |
| Economics and market state valid | Fresh bid/ask/FX/account, acceptable costs/reward/risk, approved instrument, supported protection and valid entry range/session |
| Account eligible | Flat account, unused session entry slot, no entry pause, unknown order or conflicting position/order |

The analyst stores an immutable research/readiness dossier with artifact/evidence/tool IDs and policy, skill, quote/account and calculation versions. The executor writes its own acceptance/rejection checks and revalidates at submission. Only then can the entry slot and order intent be reserved. A score supplied by the model or a checklist marked complete without its supporting records is rejected.

Model completion can outlast a quote's allowed age. The executor obtains a current quote and refreshes account/FX and deterministic confidence/economics calculations before submission, binding the accepted readiness record to those new snapshots. This cannot complete an unread source or missing skill. If refreshed facts invalidate the thesis, score, entry range or other policy checks, reject the proposal and require reassessment rather than stretching the setup.

Incomplete tools, a stale required snapshot, unresolved contradiction, missing skill, exhausted run budget or confidence below 90 produces no new entry. A later run may finish the research with fresh inputs. This does not add per-trade owner approval to automated operation, and it does not stop protection or exits on an existing holding.

The detailed workflow and included files are in [runtime skills](./runtime-skills.md); the final JSON and iterative calls are in [LLM conversations and tools](./llm-and-tools.md).

## Confidence points

Calculate a reproducible evidence score from checked inputs. The model explains its interpretation but cannot assign its own authoritative score. Store each component, input observation, calculation version and freshness timestamp.

| Component | Maximum | Initial scoring rule |
|---|---:|---|
| Primary verification | 25 | 25 for a relevant, checked issuer, filing or official release; otherwise 0 |
| Catalyst relevance | 20 | 20 for a direct material event tied to the instrument; 10 for a documented indirect event; 0 for a generic narrative |
| Independent confirmation | 15 | 15 for two independent reporting lineages, 8 for one, 0 for copied posts only; do not count the primary source again |
| Price confirmation | 20 | 20 for a positive 15-minute return, price above session VWAP and relative volume at least 1.5; 10 when two conditions hold; otherwise 0 |
| Execution economics | 10 | 10 when spread is at most 0.20% and estimated round-trip costs at most 0.40%; 5 when spread is at most 0.35% and costs at most 0.60%; otherwise 0 |
| Evidence freshness | 10 | 10 when the catalyst is at most 24 hours old and monitored sources are current; 5 when at most 72 hours old with checked continuing relevance; otherwise 0 |

Relative volume compares volume so far with the median volume at the same session minute over 20 comparable previous sessions. VWAP and bars require suitable feed coverage. Missing observations give no points; unavailable venue coverage is not invented. Store semantic verifier results and their evidence so relevance judgments can be audited. Fix the scoring algorithm within a policy version and evaluate it before changing defaults.

Entry requires all readiness checks independently of points: finalized skill-backed research, primary verification and independent comparison, exact instrument mapping, adequate quotes/bars, permitted source processing, spendable cash, an unused entry slot, achievable protective orders and acceptable risk/reward. An unresolved material contradiction blocks entry even when the arithmetic score is high. A score is not a probability of winning.

| Validated points | Permitted profit target |
|---:|---|
| Below 70 | No new entry; keep managing existing protection; no target increase |
| 70–79 | No new entry; existing-holding target up to 1% |
| 80–89 | No new entry; existing-holding target up to 2% |
| 90–94 | High-confidence entry eligible only after readiness passes; target normally 1%, permitted up to 3% |
| 95–100 | High-confidence entry eligible only after readiness passes; up to 3% ordinarily, more than 3% and up to 10% only with the exceptional gate below |

Targets may remain below the band maximum. An exceptional target requires a checked primary announcement of a material invention, product breakthrough or comparable catalyst, independent confirmation, the full price-confirmation and economics scores, an explicit expiry and a reason that the already-observed price move has not exhausted the setup. Hype, an unverified post or an LLM claim of a breakthrough does not qualify. Treat more-than-3% targets as disabled in the initial live policy until the owner enables a paper-evaluated policy version.

An existing holding whose score drops below 70 receives no target increase. Retain the active plan while obtaining a reassessment; a valid update may lower the target or tighten the stop. A low score alone does not become an unconfigured market sell. Entry eligibility and holding management are separate decisions.

## Define prices, net returns and risk

Use account-currency position returns for target validation. Let `C` be actual acquisition cost, including attributable entry fees and FX. Let `V(b)` be estimated net liquidation proceeds for the filled quantity at executable bid `b`, after current FX and estimated exit charges.

```text
estimated_net_return(b) = (V(b) - C) / C
target condition: estimated_net_return(b) >= active_target_fraction
```

Round-trip trading costs belong in this estimate. Exclude unknown future personal taxes and external hosting/LLM costs from the trigger; report actual withholding and project costs separately. An account-currency return can differ from a stock's quoted price return because of FX. Persist both the instrument-currency target/stop prices and their account-currency assumptions. Never execute on an LLM's arithmetic.

For a net target `t`, solve for a sell price whose conservative proceeds satisfy `V(b) >= C × (1 + t)`, rounding the sell limit upward to the valid tick. When charges are not linear, use a checked numeric solver. At entry, validate estimated downside at the proposed stop and require target reward divided by estimated stop loss to be at least 1.5. The stop's maximum initial estimated loss is 2%, but this reward/risk rule is often stricter: a 1% target permits at most approximately 0.67% planned downside, before tick rounding and costs.

Skip an entry whose economically valid stop is inside ordinary spread/noise, violates minimum distances or cannot be protected. Do not move a technical stop arbitrarily close merely to make the ratio pass. A stop bounds the planned trigger, not the fill price: gaps, illiquidity, halts and fees can produce a larger loss.

Enter only when the fresh executable ask lies inside the proposed inclusive entry range. Round the buy limit to a valid tick without exceeding the range's maximum, size against that limit and reject if a valid tick cannot satisfy both bounds. A drop below the lower bound is a new setup requiring reassessment, not permission to buy a supposedly cheaper stock. After each actual fill, recompute acquisition cost and derived target prices for the filled quantity, recheck protection and record the calculation; price hints from the proposal are never authoritative.

The 10% ceiling is a ceiling on the configured estimated net target, not a guarantee that actual gains will be exactly capped. Favorable gaps or price improvement may produce a realized return above 10%. Exit promptly when the active rule triggers; never configure a target above 10% or deliberately continue a triggered holding to seek more.

## Adaptive holding and threshold changes

The [FinBERT sentiment overlay](./finbert/README.md#threshold-calculation-version-1)
affects these decisions after the base confidence calculation. Version 1 subtracts
10 points when any selected independent lineage has negative probability at least
0.60, recomputes the existing target band, and disallows target increases pending
review. At 0.80 it also vetoes entry regardless of the remaining score. Positive or
neutral tone adds no points. Base 95 with negative probability 0.65 becomes 85,
blocking entry and reducing the permitted target ceiling from 3% to 2%.

The pure Rust function returns threshold constraints; it does not activate them.
Source permissions, exact model/text identity and freshness must pass, and a
required unavailable result blocks new entries/target increases while existing
protection continues. A FinBERT finding alone cannot submit an exit, relax a stop
or override readiness, reward/risk, Perpetual requirements or an existing exit latch.

The active plan contains an entry range, initial and current stop, target return and derived target price, confidence components, evidence references, trailing settings, time limit and version. Thresholds remain fixed between validated updates except for deterministic trailing changes.

| Situation | Allowed response |
|---|---|
| Strong new evidence before the current target is met | Propose a higher target within the current score band; retain or tighten the stop |
| Slower rise with no material new evidence | Keep the active target and time limit; do not extend because the position has not won yet |
| Deteriorating evidence | Lower target, tighten stop or shorten expiry through a checked update |
| Score falls after an exceptional catalyst | Revalidate immediately; the lower permitted target ceiling applies to the next activated plan |
| Target or stop already met | Persist an exit latch; ignore subsequent proposals to continue holding |
| LLM or collection unavailable | Keep existing thresholds and price monitoring; block new entries when required evidence is stale |
| FinBERT unavailable/stale or adverse sentiment | Retain active protection; block sentiment-dependent entry/target increases; request fresh evidence and a checked risk-reducing plan review |

Stops never move downward for a long position. A plan cannot increase its original holding deadline, and the maximum is five exchange sessions including the entry session. Splits and similar corporate actions require a reconciled economic adjustment; an adjusted numerical stop is not a policy-approved increase in risk.

Proposed trailing default: activate after the first fresh observation of at least +0.50% estimated net return. Track the peak observed return and set the trailing floor to `max(previous_floor, peak_return - 0.50 percentage points)`. Evaluate it on fresh accepted quotes; persist the peak and floor so restart does not reset protection. The initial stop, trailing floor, hard target and time exit all remain active; the first reached condition wins.

At the normal 1% target, a holding exits unless a higher target was activated before the old target was reached. For example, an 87-point update at +0.6% may raise the target from 1% to 2%; an update received after a +1% trigger cannot undo the sale. This allows a stronger rise to run longer without perpetual target chasing.

Serialize quote evaluation and plan activation for each position. Before replacing a plan, evaluate the old version against the latest accepted quote. Atomically latch any old exit first. Otherwise compare the expected plan version, validate the new one, and activate it; if its lower target or tighter stop is already reached, latch the exit immediately. Concurrent or stale versions are rejected and must be reassessed.

## Deterministic exits and broker protection

Rust may create an exit because of a hard stop, trailing floor, profit target, plan deadline, maximum holding duration, authenticated owner close or the separately enabled deterministic opportunity-rotation gate below. News text and ML predictions are never order commands. Bad news can produce a lower validated threshold, which code evaluates like every other plan.

Use limit orders for entries and profit-taking. A proposal expires after five minutes and entry orders expire or are cancelled after the configured five-minute entry window, never later than the session entry cutoff. Do not chase an unfilled entry with a replacement submission. Protect any filled portion while cancelling and reconciling its unfilled remainder.

Native broker stop-market protection is the proposed live loss-exit default where supported; it trades fill priority for uncertain price. If only stop-limit protection is available, it can remain unfilled after a gap and requires a separately approved, tested capability profile. An ordinary profit-taking limit must not be reused as the only loss-exit mechanism. This replaces the original blanket limit-only rule for protective exits.

The live adapter must demonstrate that the actual fractional quantity can be protected and that stop, target and software exits cannot oversell it. Prefer broker-supported linked/OCO or reduce-only behavior. If independent protective orders reserve the same shares without safe coordination, live mode remains disabled until a verified alternative broker/capability profile is selected. Never cancel protection speculatively just to discover whether a target order will work.

To replace protection, confirm the old order's state, account for late fills, and establish the approved replacement sequence. A software exit cannot submit the full position while a native stop may still sell it. Native triggers and application targets are economically different when FX changes; record that distinction and reconcile actual fills. A software 10-second quote check cannot guarantee the same stop price as the broker's native trigger.

Live overnight holding requires verified native loss protection. Otherwise schedule liquidation 15 minutes before the actual exchange close and block new entries within 30 minutes of close. If liquidation is unresolved at close, alert and reconcile; do not mark the position flat or pretend the system prevented overnight exposure. Native protection does not remove gap risk.

## Opportunity rotation

Rotation is a specified opt-in capability, initially disabled. The default remains one entry attempt per exchange session. With rotation enabled, an older holding can rotate using today's unused slot. An owner-reviewed optional policy can allow at most **two total attempts**, exactly one ordinary attempt and one replacement attempt, with at most one rotation in a session. Two ordinary entries, retrying a failed entry, simultaneous holdings and repeated hopping remain forbidden. Startup rejects a two-attempt policy unless bounded rotation is enabled. A failed submitted second leg consumes its attempt.

The LLM produces comparison research, not a sell command. The executor accepts a comparison only if the old plan has not latched an exit, actual position/version is known/protected, the alternative's research is complete except for the necessarily blocked flat-account check, all evidence/ML inputs are current, and alternative computed confidence is at least 95. Final entry still requires flatness and all thirteen skills after sale. A probability or exciting target alone cannot justify turnover.

Use one common horizon no later than the original holding deadline. The calculator estimates account-currency future wealth for switching, holding and cash from fresh executable prices, share precision, spendable cash, all relevant trading/FX costs, configured taxes and conservative scenario intervals. Sunk purchase cost belongs in realized reporting, not an extra forward switching fee. Compute `advantage = (switch_wealth_low - max(hold_wealth_high, cash_wealth) - safety_buffer_amount) / current_liquidation_equity`. The proposed hurdle is `0.005` (0.5 percentage point) after a safety buffer initially `0.0025` of current liquidation equity. Never subtract a fee twice.

These comparisons are uncertain scenarios. Unsupported intervals, incomparable horizons, missing material cost/tax assumptions, unavailable protection or expiry block rotation. The opt-in policy requires paper/shadow evaluation. The model cannot set the hurdle, enable rotation, raise entry caps or certify its own scenarios. [Perpetual](./perpetual.md) advises, never proves positive advantage.

On acceptance, Rust writes an executor-owned rotation authorization and latches one exit under account/position serialization. Existing protection/order coordination applies. After broker-confirmed zero quantity, reconciled old orders and available cash, refresh the alternative and obtain/validate a fresh final entry proposal; reserve its eligible slot atomically with its intent. Materially changed price, evidence, profile, scenario or ML inputs require reassessment. Failed eligibility leaves cash; no guaranteed two-leg execution exists.

An old target/stop/deadline trigger wins any race and cannot be cancelled by comparison. Rejected rotation leaves the active plan intact. The 98% cash rule, loss guard, session cutoff and 10% target cap apply to the replacement. Outages cannot stall protection or an accepted exit. See [watchdogs](./watchdogs-and-x.md) and [comparison records](./research-contracts.md).

## State machines and idempotency

| Position state | Meaning and next allowed work |
|---|---|
| `FLAT` | No holding or unresolved order; may evaluate an entry |
| `ENTRY_PENDING` | Durable entry intent exists; reconcile until rejected, cancelled or filled |
| `OPEN_UNPROTECTED` | Some quantity filled; establish protection immediately; block new entries |
| `OPEN_PROTECTED` | Reconciled quantity and protection; monitor and accept valid plan updates |
| `EXIT_LATCHED` | An exit condition is durable; never return to holding because of new research |
| `EXIT_PENDING` | Exit submitted or coordinated with native protection; reconcile fills |
| `CLOSED` | Broker confirms zero remaining quantity and no conflicting live orders |
| `RECOVERY_REQUIRED` | State, quantity or order outcome is ambiguous; reconcile and alert |

`CLOSED` becomes `FLAT` only after cash and outstanding orders reconcile. Partial fills update quantities and remain in their pending lifecycle. Record whether an exit was software-triggered, broker-triggered or owner-requested.

Each order intent has an immutable client ID. Persist it before submission. An external timeout is `UNKNOWN`, not permission to retry. Search broker orders and fills using available identifiers and exact account/instrument/side/quantity/time attributes. If matching is ambiguous, remain in recovery. A replacement is allowed only after the original's final state and remaining quantity are known. At most one unresolved exit intent exists per position; a new quote or duplicate queue message cannot create a second sale.

## Session loss guard and reporting

At each valuation, calculate deposit-adjusted session P&L: current equity minus opening equity minus deposits plus withdrawals. The proposed -2% entry pause uses `opening_equity + cumulative_session_deposits` as its explicit denominator; if that denominator is nonpositive, block entries and report unavailable. Withdrawals do not reduce this denominator. Report this operational guard separately from a time-weighted investment return.

Once the loss guard trips, keep entries paused until the next session and successful reconciliation; a later deposit or rebound does not clear the latch. Stops, target exits, owner closes and protective reconciliation continue. Never fund a losing position automatically.

Report net position outcomes, daily account results and project results separately. A 1% position gain on a 98% allocation is approximately 0.98% account growth before outside operating costs. Include zero-trade days, losing positions, pending fills, drawdown and actual fees. See [operations](./operations.md) for cash-flow-aware performance measurement.

## Interface, invariants and side effects

The planned inputs are an owner policy, research proposal, reconciled account and position, fresh quotes/FX and exchange calendar. Outputs are accepted/rejected plan versions, reserved entry slots, latched exits and durable order intents. [Memory and data](./memory-and-data.md) defines their storage contracts.

All financial calculations use checked decimal arithmetic. No confidence score relaxes the account, cost, quantity, target, freshness or protection checks. Existing orders are reconciled before replacement. The executor alone submits broker orders; this document itself performs no trading.

## Verification

Acceptance cases cover allocation with fees/deposits, confidence band boundaries, target/stop mathematics, target-update races, partial fills, native protection conflicts and unknown submission outcomes. They are listed in [implementation and acceptance](./implementation.md). There is no application or executable test target yet.

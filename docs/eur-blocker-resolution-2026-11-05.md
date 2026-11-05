# EUR blocker resolution, 5 November 2026

The later [month preparation](./eur-demo-month-2026-11.md) supersedes the
checkpoint below: 48 native DEMO fills now reconcile to EUR 4,992.68 flat,
new independently reviewed expected-identity/rule/cost policies cover the finite
6 November–6 December window, and month supervision/reporting repairs are
implemented. The new policies are conditional on fresh session/entry checks;
they do not extend the old reviews or supply activation authority. The final
[machine-readable record](./evidence/eur-blocker-followup-2026-11-05.json) names
the latest tested binary, actual deployment and blocked exact candidate hash.
Native partial entry and a positive real-input paper lifecycle remain unobserved.
The later current-candle work implements the owner's choice of verified IBKR
SMART TRADES, preserving Yahoo/Alpha historical/news roles and the exclusive
type-1 executable-book path. Native fresh candle access is verified; it does
not supply either missing lifecycle. Two independent source/replay findings
were repaired. See the month handoff and machine-readable record for the new
exact candidate, deployed SHADOW state and retained validation.
The remainder of this document preserves the earlier 12:21 UTC checkpoint.

Work continues on `implement/eur-first-run`; all original uncommitted files were
checkpointed before edits. The original candidate
`a412cfbbaa9ea0c7be5d7b79df7c8ae938d3aa5d1482c9d6905bdf6b5ffb363b`
is preserved. The owner authorized implementation, bounded DEMO experiments and
an independent reviewer, but did not authorize activation of a final release.

Only Trading 212 **DEMO account 52211463** was used. Preflight at 12:06:56 UTC
confirmed EUR 4,995.86, no holdings and no pending orders. The final fresh
snapshot at **12:21:09 UTC** confirms **EUR 4,995.85, no holdings and no pending
orders**. At most one acquired holding existed at a time. SHADOW v10 services are
active; the corrected prospective candidate binary is not installed or activated.
LIVE remains disabled. The one-time continuation schedule is disabled
with no next run. No recurring work was created.

Identity expiry was checked immediately: **5 November 2026, 19:08:04 UTC
(20:08 CET)**. It remains current for this bounded continuation; no renewal or
extension is claimed. The new release cannot outlive that identity or any
shorter acceptance/owner authority.

The quote policy remains [IBKR SMART live type-1 timestamped bid/ask](./eur-data-sources.md),
without executable-quote fallback. YFinance and Alpha Vantage retain historical,
available-news and additional-research roles. X remains metadata-only.

## Actual fills and new stop-fill evidence

There are now **44 actual DEMO fills**, comprising 33 Airbus `AIRp_EQ` /
`NL0000235190` and 11 ASML `ASMLa_EQ` / `NL0010273215`. All match a preceding
exact-contract SMART callback received before the broker fill, with live type 1
and positive sizes. Maximum observed fill-to-side divergence is **2.482006 bps
for Airbus** and **1.524856 bps for ASML**; maximum quote age is five and two
seconds. These observations are distinct from accepted future tolerances.

Complete raw wallet arithmetic is **EUR 4,999.07 - 3.22 = 4,995.85**. Wallets
report EUR, FX rate 1 and tax arrays. Fractional gross-to-wallet differences
include currency rounding; the responses do not disclose a separate commission
field. Spread, divergence, P&L and modeled future reserves remain separate.

Two additional tiny Airbus stop experiments each used 0.01 shares. The first
used an ask-derived stop and a 1.5-second delay before DELETE; cancellation won
and a separate market exit flattened the account. That observation does not
prove a stop fill. The second used an explicitly controlled immediate-trigger
STOP and a 2.5-second delay before DELETE:

- Entry order `57550618853` filled at 12:13:36 UTC.
- Native STOP `57550618859` filled before exact DELETE returned HTTP 404 at
  12:13:42.779 UTC.
- The driver reconciled the STOP as terminal FILLED and the position as flat.
  It sent **no additional market sell**.

This is real native broker evidence with controlled trigger/timing, not a
synthetic fill or a claim of a simultaneous network race. Final flat verification
arrived at 12:13:46.430 UTC. The driver retained its conservative exposure-deadline
reason; this run does not supply a working-stop confirmation latency pass.
The raw exact stop-fill/cancellation/flat/quantity chain passes the strengthened
verifier separately from overall protocol acceptance.

## Native partial entry remains unobserved

Earlier Airbus and ASML depth watches each reached their five-minute limits
without dispatch. The new read-only SMART capture observed an Airbus ask of two
shares at 12:12:03 UTC, worth EUR 402.70. This materially changed displayed depth
and justified one further bounded Airbus watch, with a EUR 2,500 maximum entry
notional and a five-minute opportunity limit. No qualifying book recurred in
that watch; it ended without authority creation, an order or a holding.

**No native partially filled entry exists.** No remainder-cancellation and
acquired-partial protection chain can therefore be accepted. Manually reduced
fractional positions and synthetic reducer scenarios remain separate evidence.
The complete failed watch log is preserved. No repeated or larger experiment
was used to manufacture a partial fill.

## Instrument rules and scoped models

Real-price probes corrected the old one-share inference to a **EUR 1 entry
trade-value minimum**, rounded up on quantity steps AIR 0.0001 and ASML 0.00001.
AIR at EUR 198.40 rejected 0.0050 and accepted/cancelled 0.0051; ASML at EUR
1,287.52 rejected 0.00077 and accepted/cancelled 0.00078. Finer quantity precision
was rejected. The AIR residual-value rejection and successful full liquidation,
and fractional native STOP/cancellation/full exits for both instruments, remain
retained with their exact scope. Full-position protection/liquidation below the
entry notional minimum must remain possible.

Trading 212 accepted tested prices with nine and ten fractional decimals; one
terminal STOP rounded its price and failed strict reconciliation. A universal
broker minimum tick remains unproven. The **EUR 0.01 application grid** is an
explicit `VALIDATED_APPLICATION_GRID` policy, bounded to AIR EUR 190-210 and
ASML EUR 1,250-1,350. IBKR minTick does not establish Trading 212 precision.

The forward cost proposal remains five bps plus EUR 0.01 reserve per leg,
with no FX. Its proposed release scope requires at least EUR 500 entry notional
and an exit/entry notional ratio at most 1.05 for each instrument. Including one
cent of conservative rounding per leg, modeled wallet costs fit a 15 bps ceiling
within that scope. This excludes spread, divergence and trading P&L. Tiny DEMO
capability probes are outside that proposed forward cost scope. Independent
component decisions and their hashes are recorded in the final evidence.

## Independent review and implementation corrections

An authorized separate reviewer assessed the original exact candidate and
reported **REQUEST CHANGES**, seven actual findings. Its report, raw-evidence
checks and synthetic negative controls are retained in the private review
artifacts. The negative controls are not broker evidence. The implementation
addresses the findings as follows:

1. Paper release acceptance requires the hashed complete capsule and a positive
   invocation of the bundled binary's credential-free `replay paper-day-check`.
   The replay reconstructs the registered Donchian signal and deterministic
   risk plan from completed bars, SMART callbacks, account and exact costs.
   The ledger binds each book to retained raw live-type/contract callbacks and
   checks ordering, prior exit triggers, protection model and EUR cash. Flags alone cannot satisfy acceptance.
2. Partial-entry acceptance requires a scoped native BUY with a partial acquired
   quantity, terminal remainder cancellation before STOP dispatch, actual
   acquired-quantity native protection and the broker first-fill clock. A
   separate exact native stop-fill/cancellation case checks flat reads and
   prevents overselling the remainder.
3. Expired binding recovery permits full-position regular-session market SELL
   using historical binding shape and fresh account/catalog/available-position
   checks. New entries and priced orders retain current-review requirements.
4. The shared entry gate checks price-dependent quantity minima and both entry
   and planned STOP against reviewed price bands before BUY dispatch.
5. The exact forward cost model is a required artifact, bound to runtime input,
   durable risk result and paper ledger. Modeled cost ceilings are checked over
   explicit per-instrument notional scopes, which also constrain execution.
6. Quote acceptance requires separate exact ticker/ISIN divergence limits and
   unique complete coverage of retained fills. A global limit cannot replace
   instrument-specific decisions.
7. Running execution checks the earliest real manifest, identity, acceptance,
   cost-model and owner expiry on every loop and after quote capture; expiry
   latches an exit without requiring restart. Entry transport deadlines are
   capped by that earliest authority expiry, including across broker reads.

The final independent report distinguishes resolved implementation findings,
bounded component acceptance and missing external evidence. It cannot approve
an incomplete paper lifecycle or missing native partial proof.

## Real-input paper and prior failed runs

The fresh capture has **454 completed five-minute bars per instrument**. Both
registered strategy assessments remain **NO_BREAKOUT**. There is no analyst
selection or risk plan. The actual forward-paper command and credential-free
offline paper check both remain **PAPER_DAY_BLOCKED**. No strategy parameters,
prices or source inputs were changed to force a signal. A positive synthetic
unit fixture remains an implementation test, not a successful real-input day.

Original failures remain intact: omitted terminal TIF, the high-precision STOP,
slow protection/late confirmation, fractional residual rejection and all exact
cleanup records. Earlier positive bounded native working stops, cancellation,
market exits, separate-process restart, controlled external reservation STOP
rejection and controlled application acknowledgement loss retain their original
classifications. Controlled faults do not prove a natural provider outage or
unassisted unknown-ID recovery. Unknown POSTs are never blindly retried.

## Release state

See [execution and release](./eur-execution-and-release.md) for the exact bundle
contract. The [final evidence record](./evidence/eur-blocker-followup-2026-11-05.json)
contains the new manifest hash, independent decisions, validation and verifier
output. The old candidate, original source checkpoint, failed runs and all raw
streams remain preserved under `var/eur-session-20261105/`.

Native partial-entry/remainder/protection evidence and a genuine positive
registered-strategy paper lifecycle remain external release gates. Owner final
activation approval must bind the resulting exact complete manifest; broad
implementation or bounded DEMO authorization cannot replace it. A blocked
bundle is not activation-ready even if its hash receives owner approval.

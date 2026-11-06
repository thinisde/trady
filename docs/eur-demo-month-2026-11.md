# EUR DEMO month preparation

The owner subsequently changed the limit to **30 stock entries per UTC day**,
with **unlimited protective STOPs and exit order changes**. The
[Nov6 policy follow-up](./eur-demo-entry-policy-2026-11-06.md) and its evidence
record contain the latest implementation, review and installed pins. The
one-entry observations below describe the earlier authority and remain preserved.

The requested window is **6 November 2026 00:00 UTC through 6 December 2026
00:00 UTC**: thirty calendar days on Trading 212 **DEMO 52211463**. Research
uses real market inputs. LIVE remains disabled and is never activated after a
timer, a month, or an optimization result. A later LIVE release needs its own
evidence and explicit approval.

The month candidate is **blocked from activation** while native partial-entry
evidence and a genuine positive registered-strategy paper lifecycle are absent.
The independent reviewer must assess the complete resulting release, and the
owner must approve its exact manifest hash. Engineering tests, capability trades
and a blocked manifest cannot supply these missing gates. See the
[current evidence record](./evidence/eur-blocker-followup-2026-11-05.json).

The authorized Nov6 capability continuation made one full-entry round trip,
finished physically flat at EUR 4,994.19, and changed cash by EUR +1.51. Its two
fills are capability evidence within the month window, separate from strategy
performance. The then-active one-entry Nov6 allowance was consumed. The owner
has since authorized the 30-entry policy above. General DEMO stays disabled
pending missing evidence, the reviewed policy release and exact-hash approval; capability
attribution is required before strategy economics can pass. The full account
cash audit includes historical legacy FX rather than assuming every old fill
was EUR-to-EUR. See the [Nov6 continuation](./eur-evidence-continuation-2026-11-06.md).

## Prepared behavior

The SHADOW coordinator evaluates immutable fifteen-minute research cycles over
completed five-minute bars. A durable cursor resumes an unfinished analyst
request across bucket changes; expired requests receive an explicit terminal
result without another paid call. The latest completed candle must belong to
the current primary session and be at most two intervals plus fifteen seconds
old. A no-breakout result is retained and the next bucket may assess new real
bars. Strategy parameters and signals are never forced.

The owner selected verified IBKR SMART TRADES for current candles on
5 November. The read-only helper retains exact contract bindings, native
coverage, candle and receipt timestamps; Yahoo/Alpha remain historical/news
research without candle fallback. Candle source settings and the bridge hash
are bound into replay and the exact release. This source change supplies data,
not DEMO entry authority.

`trady eur demo-run` discovers unresolved account intents before considering a
new entry. An unfinished intent retains its original exact recovery bundle.
The common gate and `protocol-drive` enforce one position, the release's daily entry cap,
no overnight holding, current primary calendar, exact listing and reviewed
price/cost limits. Both manual and supervised entry paths enforce the finite
month window and the same source-runtime dependencies. The primary flatten
deadline, month end and earliest actual approval/evidence expiry cap entry
authority. Recovery and risk-reducing exits remain available after entry expiry.

Before BUY, one raw broker catalog supplies both the exact listing/schedule
check and the transport metadata cache. The account and live SMART book refresh
after catalog warming. The entry transport deadline is capped by the provider
quote timestamp plus ten seconds. Authority contents and trusted root ownership
are checked at the actual POST boundary. Protection and liquidation do not
inherit the entry quote's short expiry.

Only the selected raw listing, its observation time and the immutable record
hash are retained with a prepared protocol. A short restart may seed that
one-listing cache while it is less than 540 seconds old. Fresh account,
positions and exact order/history reconciliation still precede action. A
working stop is preserved when liquidation prerequisites fail; near-expiry
catalogs refresh before stop cancellation. Existing database fencing permits
takeover after a dead connection and excludes a still-live backend. UNKNOWN
submissions are never blindly retried. These are tested mechanisms, not a
guaranteed broker/network protection latency.

The SHADOW coordinator defers its broker reads while a protocol needs execution.
The month collector uses the same account fence and runs only while physically
flat and outside the entry session. After 19:00 UTC it retains at most one
successful report per day; month end attempts a final report. Report failures
are retained as deferred evidence. No Codex recurring task is created.

The report joins native account-wide order/fill history, scoped lifecycle
observations, transaction/dividend pages and flat cash snapshots. Pagination is
bounded; truncation, missing lifecycle fills, extra untracked fills or unresolved
cash events prevent economic acceptance. A cash match alone does not prove
complete trading history. Private raw files remain separate from the sanitized
weekly handoff. Capability experiments before the month are excluded from month
strategy performance. Missing investment-return statistics remain null.

Optimization retains the current policy (`KEEP`). The existing weekly offline
handoff includes native report hashes and coverage status. Proposals require
independent offline validation and separate release approval; no automatic
parameter, model, identity, price-band, cost or LIVE promotion is implemented.

## Activation handoff

The [service template](../services/runtime/trady-eur-demo-month.service) and
[environment template](../services/runtime/eur-demo-month-release.env.example)
are preparation artifacts. The DEMO service remains disabled. Existing SHADOW
services can collect genuine inputs while the release is blocked.
The prepared service disables systemd start-limit lockout and backs repeated
failures off from one to sixty seconds in five steps. This preserves restart
attempts without claiming recovery within the protection deadline.

After all missing evidence has passed review, assemble a new bundle and run
`scripts/verify-eur-release.py verify --bundle DIRECTORY`. Present that exact
hash for owner approval. The root activation helper rechecks the original
private approval and all evidence before creating a read-only receipt at
`/etc/trady/eur-approvals/HASH/authority.json`, readable by the executor group.
The receipt cannot extend any original expiry or approve another hash. No
receipt is issued for a blocked bundle.

Only then install the exact DEMO runtime, matching immutable bundle and release
environment file, and enable the prepared service. Do not repurpose the disabled
one-time continuation schedule. Operator recovery uses the original bundle and
`protocol-drive --intent ORIGINAL_ID --exit-reason OWNER_EXIT`; ambiguous orders
require an exact broker ID and recorded review reference.

## Current observations and limits

A native two-day IBKR capture at 16:10 UTC returned 200 raw bars per instrument.
Conservative normalization retained 199 completed bars through 16:05 UTC, about
301 seconds old at receipt. Independent re-derivation matched both snapshots.
The later diagnostic was stale at evaluation and produced no strategy signal;
it is not a successful paper run. The broker flatten deadline had passed before
integration validation finished, so no after-deadline entry or forced signal
was used. Current bars are available through the existing IBKR access. The
separate native Alpha intraday probe returned a premium-access restriction and
zero bars; no subscription was changed.

The earlier Yahoo month-profile capture contains **489 completed five-minute bars
per instrument**. At its 15:04 UTC decision, the newest candle closed at 14:45
UTC. Both inputs were rejected as stale; no analyst selection, risk plan or
positive paper lifecycle was created. The forward attempt and credential-free
replay remain blocked. The earlier 470-bar no-breakout observation is retained
separately. The IBKR integration addresses current-candle access; an eligible
genuine strategy signal and its complete real-input paper lifecycle remain
unobserved.

Native read-only validation also caught Trading 212's requirement that
transaction-history `cursor` and `time` be supplied together or both omitted.
The failed time-only request and successful unfiltered request are retained.
The collector now starts unfiltered and applies the month cutoff locally.
Account-wide order history was read to an explicit end over two pages and
94 rows. These endpoint checks are capability evidence, not month performance.

The retained 5 November capability evidence contains **48 actual DEMO fills**,
each matched to a preceding timestamped live type-1 SMART book. The two newest
passive LIMIT entries both filled completely and received native STOPs before
separate-process exits. They do not prove partial entry. The native controlled
STOP-fill-before-delayed-cancellation observation remains distinct from a
simultaneous network race. All failed and controlled runs remain preserved.

The fresh read-only snapshot at 14:40:43 UTC is flat with no pending orders and
**EUR 4,992.68**. Across all retained fills, EUR 4,999.07 minus EUR 6.39 equals
EUR 4,992.68. These capability costs and P&L are not a strategy backtest or an
investment return. Independent component acceptance and the latest paper result
are recorded in the final evidence file.

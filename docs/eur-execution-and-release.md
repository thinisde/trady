# EUR execution and release

The [Nov6 entry-policy follow-up](./eur-demo-entry-policy-2026-11-06.md) records
the owner's 30-entry daily cap, unlimited STOP/exit changes, physical-account
fencing and reviewed capability attribution. That change does not supply any
missing native/paper evidence or exact-hash activation approval.

The [bounded month handoff](./eur-demo-month-2026-11.md) documents the prospective
6 November–6 December DEMO supervisor, immutable intraday research cycles,
short-restart catalog recovery, account-wide economic audit and root-authored
read-only approval receipt. The DEMO service remains disabled until all evidence
and final exact-hash owner approval pass this verifier.

The EUR source policy is [IBKR SMART Live bid/ask with YFinance and Alpha
Vantage research](./eur-data-sources.md). SHADOW remains mutation-free. The
explicit DEMO protocol command is separate from the scheduled SHADOW services.

## Decision workflow

The coordinator checks the current exact catalog identity, primary calendar and
IBKR book. Verified completed IBKR SMART TRADES bars feed the existing
Donchian/ATR research baseline under the 5 November owner source instruction.
Only deterministic survivors with a signal reach the pinned DeepSeek
analyst. Unavailable Yahoo news and X metadata cannot become invented news or
unapproved model content.

The executor writes an immutable scoped request; the analyst service reads it
using its own DSN and model credential. Results and unknown-after-restart model
attempts are durable. A missing analyst response produces a waiting checkpoint.
Bounded actual analyst completion messages or provider error codes are retained
privately, with sanitized failure diagnostics, so an invalid result remains
inspectable. This does not retry a failed cycle or authorize a model change.
After a valid selection, the executor obtains a new IBKR book, then calculates
quantity, entry, stop, target, costs and deadline in Rust. Missing reviewed order
rules or EUR costs block risk calculation. Selection alone cannot grant entry.
Daily source capture waits for the broker calendar and primary-session open,
so a midnight process does not freeze the day's research slots prematurely.
The baseline is research-only until separately accepted in the release.

`shadow.current_bars_config` binds the read-only IBKR candle helper and its
SHA-256. The current source uses two days of regular-session five-minute TRADES.
A candle must align with its interval and close before request time, native
coverage end and the latest native candle opening; a successor is required to
prove the last interval completed. Yahoo and Alpha remain historical/news
research sources without current-candle fallback. Their earlier source capsules
remain preserved. `NO_BREAKOUT` is a valid deterministic assessment, so it does not
invoke the analyst or create a risk plan merely to exercise the positive path.

`shadow.risk_config`, when supplied, names a `ReviewedCosts` JSON document:
`review_reference`, `reviewed_at`, `valid_until`, `entry` and `exit`. The latter
use the existing `EntryCosts` and `ExitCosts` contracts. The reviewed universe
supplies exact broker rules. No default costs or guessed precision are imported.

New risk plans retain their entry/exit `cost_model`. Validation recomputes
acquisition cost, stop loss, target net return and reward-to-risk from prices,
quantity and those inputs, and checks currency and policy revision. Entry also
binds the plan's cost inputs to the release's accepted forward model. Entry
snapshots must also stay within the calculator's 98% spendable-cash ceiling.
Older plans without reproducible economics remain readable for historical inspection
but cannot authorize a new entry. Existing protocol contexts remain recoverable.

The earlier pre-month SHADOW sensitivity file, used at the 5 November
12:21 UTC checkpoint, had no independent cost acceptance. Its `ReviewedCosts`
configuration type alone did not satisfy the release gate. That historical
scope is retained in the [real-session report](./eur-blocker-resolution-2026-11-05.md).

The current bounded month uses a separately independently accepted forward
model: five bps plus EUR 0.01 reserve per leg, no FX, at least EUR 500 entry
notional and exit notional at most 1.05 times entry notional. The modeled
round-trip wallet-cost ceiling is 15 bps, including conservative rounding.
This is a scoped sensitivity allowance, not a guarantee of future broker fees;
spread, divergence and trading P&L remain separate. Exact model and acceptance
hashes are bound in the month candidate. Missing native partial and paper
evidence still prevent activation.

Instrument bindings can include `value_rules`: `minimum_trade_value`,
`minimum_remaining_position_value`, `price_floor` and `price_ceiling`.
The entry quantity floor is price-dependent and rounded up to `quantity_step`.
The currently observed EUR 1 trade-value rule does not imply a one-share floor.
Entry and priced-order validation enforce the accepted price range and grid.
Full-position market liquidation and protection must not be rejected merely
because their notional is below the entry-value minimum. The partial-entry
remainder is cancelled before protecting the actual acquired amount.
Before BUY dispatch, the common gate checks the price-dependent entry floor and
both entry and planned native STOP against the reviewed band. An expired
binding permits only full-position regular-session market liquidation after
fresh account, current catalog and exact available-position checks. New entries
and priced orders still require a current binding.

## Durable execution

```text
trady eur protocol-drive --config EXACT_DEMO_CONFIG --bundle RELEASE_DIRECTORY \
  --owner-approval PRIVATE_APPROVAL --intent UNIQUE_INTENT --snapshot ORDER_GATE_SNAPSHOT
```

The initial snapshot must match a selection and risk plan already produced by
the durable research workflow. The command verifies the running binary, runtime,
broker configuration, helper and accepted instrument binding against the bundle,
then applies the common account, universe, quote, session, risk and daily-limit
gate. The independent acceptance artifacts supply the otherwise missing release
and sequential-protection authority. An `enabled` flag cannot supply it.

The driver reserves the daily slot and persists UNKNOWN before transport. It
never retries an unknown POST. It cancels an entry remainder after partial fills,
waits for exact terminal history, then protects only acquired quantity. It
recognizes protection from an exact working sell STOP with matching price, size
and TIF. A cancellation acknowledgement alone cannot establish cancellation.
Target, deadline and exposure exits wait for terminal prior orders and recheck
sellable quantity. Stop cancellation races subtract actual stop fills before an
ordinary market exit. An exact-ID ambiguity requires operator review.

The loop captures fresh IBKR books while protected and latches the target when
the bid reaches it. Native stops remain broker-side. Their LTP trigger, execution
gaps and network latency remain weaker than atomic attached protection. No
bounded latency guarantee is established by passing the synthetic tests.

Restart with the same `--intent`; the journal supplies its original context and
broker binding. `--exit-reason OWNER_EXIT` latches a risk-reducing exit.
After an operator identifies an ambiguous submission, add `--recover-leg ENTRY`
(or `PROTECTION`/`EXIT`), `--broker-order-id EXACT_ID` and
`--review-reference REVIEW_ID`. The reducer records that review; the adapter
then verifies the exact order shape, holdings and history before any replacement.

An interrupted DELETE has a separate recovery path. A missing cancellation
acknowledgement, or an order still unresolved ten seconds after its cancellation
marker, returns `recovery_action=REQUIRE_OPERATOR` with the exact leg and ID.
Restart still reconciles first; a late terminal observation clears that condition.
After reviewing the native order, use `--retry-cancel-leg ENTRY` (or `PROTECTION`),
`--broker-order-id EXACT_ID` and `--review-reference REVIEW_ID` to authorize one
further cancellation attempt. This cannot change the broker ID or retry a POST.
The attempt requires a fresh complete broker observation and liquidation
prerequisites; another ambiguous attempt requires another explicit review.

`--once` records one checkpoint for supervised diagnostics; it does not run the
remaining lifecycle. An expired release cannot start another entry; recovery
retains the original release binding and latches an exit. LIVE remains rejected.
An already running process checks the earliest manifest, identity, owner,
acceptance and forward-cost expiry on every loop and again after quote capture.

Raw pending, positions and bounded history responses, including fill prices and
wallet fees, are committed as `protocol_broker_observation` before their reducer
inputs. This preserves costs for subsequent reconciliation rather than declaring
missing P&L or fees to be zero.

History completeness is scoped to the current exact order IDs: distinct native
fill quantities must equal each order's cumulative filled quantity. Missing
orders or incomplete totals block reconciliation and return the explicit
`EXACT_ORDER_HISTORY_COVERAGE_EXHAUSTED` operator action; malformed or conflicting
fill IDs reject the observation. A restart can obtain a new proof. The
eight-page bound does not require scanning unrelated old ticker history; this
proof does not establish account-wide economic-history completeness.

Native execution quantities are signed: BUY fills must be positive and SELL
fills negative before their magnitudes enter the ledger. Native whole-second
fill clocks receive only the existing one-second dispatch precision allowance.
Complete multi-leg views use one `ObservedBatch` reconciliation after all exact
legs update, avoiding a false holding conflict while applying an exit or a
partially filled working STOP. A locally rejected zero-filled leg without a
native ID does not require invented native history. Unknown no-ID legs remain
blocked. Historical individual `Observed` records retain their original replay
semantics; repaired source does not rewrite an earlier native journal.
Unchanged protected holdings retain their raw broker observations without
duplicating reducer events. Migration 21 and replay remove the former 10,000-event
ceiling, so older long journals can still reconcile and flatten.

The exposure clock starts at the broker's first fill timestamp, retained as an
optional journal field for compatibility with existing records. Scoped entry
confirmation can inherit an account-wide view at most ten seconds old while
refreshing the exact entry, its complete fill history and actual holdings. Its
scope is explicit; it cannot claim a new complete pending-order scan. Exact
working STOP detail and a fresh holding provide fast protection confirmation.
Their raw responses are stored in `protocol_protection_confirmation` before the
corresponding reducer event. Late confirmation latches an exit rather than
erasing an exceeded exposure deadline. Read pacing and short-lived identity
caching avoid redundant calls within broker rate-limit buckets. Recovery warms
the instrument catalog before cancelling existing protection.

## Exact release bundle

`scripts/verify-eur-release.py assemble --spec SPEC --out NEW_DIRECTORY` copies
the named files and writes a SHA-256 manifest. The spec carries `owner_id`,
`account_id`, `builder`, `created_at`, `valid_until` and an `artifacts` role/path
map. Paths are relative to the spec. `verify --bundle DIRECTORY` reports blockers
and exits nonzero until every prerequisite passes.

Required artifacts include the exact binary, runtime, activation helper, broker
and IBKR configurations, current crosswalk, source policy, instrument rules,
broker history, the exact forward EUR cost model, cost acceptance, quote
compatibility, protocol acceptance,
paper-day acceptance and independent review. The independent review must bind
the runtime, binary, helper and all acceptance artifacts by their hashes. Every
acceptance identifies account 52211463, demo environment, independent reviewer,
review time, expiry and separately hashed underlying observations.

An IBKR-current-candle runtime also requires exact `current_bars_config` and
`current_bars_bridge` artifacts. SHADOW and prospective DEMO must select the
same source. Positive paper acceptance must bind those roles, the runtime and
IBKR quote configuration; every frozen provider configuration must equal the
corresponding bundled artifact. A self-consistent older Yahoo capsule cannot
satisfy a release configured for IBKR current candles.

Instrument acceptance must bind quantity and tick rules to the intended order
price range. It must include price-dependent `value_rules` and identify a tick
as `VALIDATED_APPLICATION_GRID` or `BROKER_PUBLISHED_MINIMUM_TICK`. The current
cent grid is a proposed application constraint; it does not establish a universal
broker minimum. Quote acceptance requires BUY and SELL samples for both exact EUR
ISINs, live SMART type 1, preceding quotes at most ten seconds old, spread at most
20 bps and a reviewed divergence ceiling at most 20 bps. These are verifier
ceilings, not a claim that compatibility has passed. Each exact ticker/ISIN has
its own accepted limit; duplicate or omitted fills fail coverage. Protocol acceptance requires
broker evidence for partial fill, rejected stop, cancellation race, unknown
submission and restart. The partial-entry check requires a retained native BUY
order marked `PARTIALLY_FILLED`, or cancelled with a positive fill below its
ordered quantity. Case flags, manually reduced holdings and synthetic tests
cannot fill that slot. Exact scoped raw history must also prove terminal
remainder cancellation before an acquired-quantity STOP, its working native
shape, holding and fill-clock latency. A separate stop-fill cancellation case
requires exact native STOP fills, DELETE timing, terminal flat reads and no
oversell. Paper acceptance requires successful selection, research,
risk, protection, exit and reviewed cost
reconciliation with a flat ending account.

An owner approval file must be root-owned, private, unexpired and contain
`release_sha256` equal to the exact manifest SHA-256, `decision: APPROVE`, matching
`owner_id`, `broker_account_id: 52211463`, `environment: demo`,
`live_enabled: false`, `approved_at` and `valid_until`. Broad implementation
authorization is preserved separately; it is not relabelled as approval of an
unidentified release.

`activate --bundle DIRECTORY --owner-approval FILE --out NEW_RECEIPT` writes a
receipt only after all evidence and that approval pass. It never starts services
or submits orders. The explicit protocol command re-verifies the same inputs.

## Real-input forward paper

```text
trady eur forward-paper --config SHADOW_CONFIG --stream FRESH_IBKR_CAPTURE_DIRECTORY \
  --out NEW_PRIVATE_DIRECTORY --exit-after-seconds 60
```

The command requires a durable actual analyst selection and Rust risk plan
already recorded for the SHADOW run. It exports the immutable source capsule
before deciding whether a paper lifecycle can start. A missing signal,
selection or risk plan produces a blocked summary without simulated fills.

The bounded command has no broker transport. It samples fresh, exact-contract
SMART type-1 books, enters at the ask within the risk-plan limit, observes a
paper bid-trigger stop once per second and exits at the bid for the stop,
target, flattening deadline or configured operational time bound (1–900
seconds). The ledger identifies `simulation: true`, zero broker mutations and
`PAPER_BID_TRIGGER_ON_ONE_SECOND_OBSERVATIONS`. It does not claim to reproduce
Trading 212's native LTP stop or fill queue. The configured entry/exit cost
rates and uncertainty reserves reconcile to conservative cent-rounded cash.

Ledger verification binds the account snapshot, selection, plan, instrument,
costs, event sequence, book times, protection/exit reason and ending EUR cash.
A positive result writes a new capsule with the ledger. The offline
`replay paper-day` command validates that ledger against the source capsule;
the fifteen synthetic protocol scenarios alone cannot produce a paper-day
pass. A positive paper ledger still needs independent review of its real
inputs, modeling assumptions and costs before release acceptance.

The verifier opens the hashed paper input archive and runs the bundled binary
with `replay paper-day-check` in an empty environment. This command validates
the complete source capsule, strategy, selection, risk, ledger, books and EUR
cash without credentials, providers or PostgreSQL persistence. Stage flags
cannot substitute for a positive capsule. The normal `replay paper-day` command
still persists its report in PostgreSQL.

The accepted forward cost model is a separate required artifact. Its exact
content must match the runtime risk input, durable risk result and paper ledger.
Cost acceptance defines per-instrument minimum entry notional and maximum exit
notional ratio. The verifier checks fixed/rate/reserve costs plus a cent of
conservative rounding per leg against the declared ceiling; execution rejects
plans outside that reviewed scope. This modeled wallet-cost ceiling excludes
spread, execution divergence and trading P&L, which remain separate.

Forward paper retains a hashed capsule of the watcher contract and live-type
callbacks plus every raw bid/ask callback used by its ledger. Positive replay
requires exact callback membership and preceding type 1 for each book, in
addition to numerical and clock checks. Entry transport deadlines are capped
by the earliest accepted authority expiry so broker reads cannot carry a BUY
past that boundary.

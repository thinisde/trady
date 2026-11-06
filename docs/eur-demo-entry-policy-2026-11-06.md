# EUR DEMO entry policy, 6 November 2026

The owner's final correction authorizes **30 stock entries per UTC day**, with
**no daily quota on protective STOPs or exit order changes**. It supersedes the
earlier one-entry policy and the intervening proposal to count every broker
placement. One acquired holding at a time, the reviewed risk and price rules,
the finite 6 November–6 December DEMO window, and all release gates remain.

Policy `eur_intraday_30_entries_v2` uses `max_daily_entries: 30` and
`max_daily_exits: null`. A slot is reserved before a BUY can be dispatched;
cancelled, rejected and unknown entry attempts conservatively retain their
slot. A preparation retired without any submission consumes no new slot.
STOP placement/replacement/cancellation, exits and original-intent recovery do
not consume entry slots. Broker pacing, exact-ID reconciliation and sellable
quantity checks still apply to every applicable order.

The physical DEMO account **52211463** has one shared counter across logical
account names and owners. Migration 23 retains historical reservations, fences
the physical broker reader/executor across aliases, blocks new entry behind an
unresolved physical intent, and consumes each immutable research cycle once.
An UNKNOWN submission remains a blocker after its process dies. A live database
session keeps the physical lock even after its logical lease expires.

The old normal/precision probe journals remain unchanged and nonterminal.
Only independently reviewed exact-head native cleanup facts can exclude them
from current admission. Their exact cleaned heads remain permanently read-only;
expiry blocks new entry again without making an old recovery writable. Read-only
historical reporting can retain those facts after entry authority expires.

The Python emergency cleanup no longer contains broker HTTP mutations. It
rejects completed or previously attempted scopes before credentials/transport
and routes an unfinished original experiment to the same fenced Rust exit
driver. That path requires its original durable journal/context, rejects a
terminal or externally cleaned scope and any competing unresolved intent, and
cannot create another BUY. Ambiguous submissions still require exact-ID review.

The month audit keeps capability fills, matched strategy fills and unattributed
cash movements separate. The Nov6 capability round trip's **EUR +1.51** remains
capability activity, not strategy return. The latest retained flat balance was
**EUR 4,994.19**. A complete account-wide history and an observed opening balance
preceding the first period movement are both required; matching net cash alone
cannot establish coverage. The runtime collector retains up to 256 pages of
50 lifetime order rows and reports truncation as incomplete. This exceeds the
2,790 rows for 31 days of 30 entry/STOP/exit cycles and leaves room for retained
history and thousands of replacements; no finite collector guarantees coverage
of an unlimited number of future changes.

Attribution proof is self-contained: retained source bytes/hashes must match
their parsed projections; account-wide pagination must end explicitly; each
completed leg must match its original protocol and exact native identity,
quantity, type and status. Cleanup quantities balance within the same exact
instrument. Every captured current-period fill must be assigned to a genuine
strategy order or a declared reviewed capability activity. Pending review,
controlled fault tests and manually cleaned journals do not become native
partial/protection acceptance.

Executable quotes remain exact-contract IBKR SMART live type-1 bid/ask with
positive sizes and age at most ten seconds. Current candles remain the pinned
read-only IBKR SMART TRADES source with completion, coverage and freshness
checks. Yahoo/Alpha remain historical/news/research, and X remains metadata-only.
AIR 190–210 and ASML 1,250–1,350 bands and the application cent grid are unchanged.

The implementation/deployment and independent review status is recorded in the
[current policy evidence](./evidence/eur-demo-entry-policy-2026-11-06.json).
The [morning continuation](./eur-evidence-continuation-2026-11-06.md) remains a
closed historical run. Both continuation schedules stay disabled. The separate
bounded manual follow-up watched five minutes for a materially thinner native
opportunity (displayed ask size at most one share), with one attempt and EUR
2,500 maximum. Its 384 native book observations contained no qualifying book.
The test failed explicitly with no entry dispatched; no authority or protocol
head was created and the physical day count remained one. The final account
was flat at EUR 4,994.19. This supplies no partial-fill evidence and did not
reuse the morning marker, create a schedule or activate ordinary DEMO.

Automatic DEMO remains blocked until a genuine broker partial BUY, terminal
remainder cancellation and protection of its acquired quantity are observed,
and the unchanged registered strategy completes a successful real-input paper
lifecycle plus credential-free replay. The simultaneous stop-fill/cancellation
race remains unobserved; the earlier controlled observation keeps its original
scope. A complete independent release review and final owner approval of the
new exact manifest are still required. No approval receipt or automatic LIVE,
strategy or model promotion is created by this policy change.

The independently accepted implementation is installed in SHADOW as
`eur_month_thirty_entries_v4`, with migration 23 and all three SHADOW services
restored on the exact reviewed binary. The evidence record contains the full
binary, source, runtime and operation hashes. Validation passed 678 workspace
tests, 72 Python tests, strict Clippy and disposable PostgreSQL/CLI checks.

An account-reader handoff returned an actual HTTP 429 before the following
reconciliation succeeded. Its diagnostic is retained; the raw HTTP body was
not exported and is not reconstructed. Operational handoffs require at least
5.2 seconds after the final account-summary read before another reader starts.
The account fence does not establish a cross-process provider pacing guarantee.

The two retained new-deployment research cycles (12:15 and 12:30 UTC) used
155 and 156 completed current IBKR candles per instrument respectively. Both
returned `NO_BREAKOUT` for AIR and ASML, with no selection or risk plan. No
paper attempt, model response, fill or positive ledger was manufactured. The
bounded paper observer ended at 12:35 UTC; its retained result distinguishes
absence of a valid plan from a successful paper lifecycle.

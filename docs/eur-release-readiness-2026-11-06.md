# EUR release readiness, 6 November 2026

The owner's full-send request authorized gathering the missing evidence and
preparing the release. It did not identify or approve a release manifest.
Automatic DEMO remains disabled and LIVE remains disabled. Earlier runs, sealed
bundles, unsuccessful observations and exact-head cleanup evidence are preserved.

The existing uncommitted implementation was preserved on
`release/eur-readiness-20261106`. It includes the reviewed 30-entry policy,
physical account fencing, exact native signed-fill reconciliation, atomic
multi-leg observations and self-contained capability attribution. These changes
passed independent source review; passing software checks does not establish
native partial-entry or real-input paper acceptance.

## New native evidence

A fresh, read-only, exact-contract IBKR SMART capture observed occasional ASML
asks of one share. One new bounded manual experiment used the existing thin-depth
probe, with one BUY attempt, at most EUR 2,500, one holding and the unchanged
ASML EUR 1,250–1,350 price band. The SHADOW executor's broker reader was paused
for this test and restored after the final reader cooldown. No schedule or
ordinary DEMO activation was created.

At 13:37:42 UTC the qualifying SMART ask was EUR 1,296 with size one. Trading 212
filled **all 1.92901 shares** of LIMIT BUY **57600344785**, fill **57600344787**.
The full-quantity STOP **57600344791** at EUR 1,283.04 was confirmed working at
13:37:53.910241882 UTC, 5.910241882 seconds after the broker's whole-second
first-fill timestamp. This observed timestamp difference is not a network
latency guarantee.

The separate exit process encountered a retained metadata HTTP 429 and local
pacing delays while the STOP remained working. It cancelled that exact STOP,
observed terminal cancellation, then submitted MARKET SELL **57600344814**,
fill **57600344816**. The durable protocol reached `FLAT` at
13:38:57.638926538 UTC. Its conservative terminal reason records
`BROKER_FILL_PROTECTION_DEADLINE_EXCEEDED` during the exit observation interval;
it does not establish a failed initial STOP confirmation or a latency guarantee.
There was no extra BUY or sell beyond the acquired quantity.

The entry and exit match preceding positive-size SMART type-1 books. Absolute
divergence was approximately 1.54321 bps for the BUY and zero for the SELL,
within the existing ASML limit. The probe does **not** establish a native partial
BUY, partial remainder cancellation, smaller-quantity protection or a new
simultaneous STOP-fill/cancellation race. Displayed SMART depth does not describe
Trading 212's OTC execution queue.

## Cash and attribution

The subsequent coordinated read-only audit retained all 100 lifetime history
rows over two pages, ending explicitly, with 56 distinct fills. A fresh native
snapshot at 13:40 UTC found no holdings or pending orders and EUR **4,993.42**.
The new round trip changed cash by **EUR −0.77**, which is capability activity
and is excluded from strategy performance. It consumes the second physical
entry reservation of this UTC day under the shared 30-entry policy.

The initial supervisory snapshot invocation used an unsupported output option
and failed before broker access. The corrected coordinated audit above supplies
the observed final account state; the failed invocation remains retained.

A new self-contained attribution candidate adds these two fills to the prior
two Nov6 capability fills, retaining both original externally cleaned heads.
The actual binary's offline checker reported
`CAPABILITY_PROOF_VALIDATED_PENDING_REVIEW`: four capability fills, two cleanup
heads, no provider calls and no broker mutations. Prior accepted registries are
not edited or relabelled as covering the new activity.

The independent Sol review subsequently approved the source with no findings
and accepted this exact pending registry and evidence pair after 356 evidence
checks. A separate reviewed registry now passes
`REVIEWED_CAPABILITY_ATTRIBUTION_VALID` in an empty environment. It has not been
installed. Acceptance covers the captured four capability fills and two original
cleanup heads through 13:40:26.191842563 UTC, with validity through 6 December;
it does not certify future coverage or complete month economics. The net Nov6
capability cash change is EUR +0.74. A fresh transaction/dividend audit remains
part of the separate complete month reconciliation.

## Paper and release gates

A bounded observer watches new immutable results from the installed unchanged
registered strategy. It may attempt one 60-second forward-paper lifecycle only
after a genuine durable analyst selection and Rust risk plan exist. The current
13:30 UTC cycle returned `NO_BREAKOUT` for both instruments, with 168 completed
current IBKR candles each. No paper entry, analyst response, risk plan or
positive ledger was manufactured. Later observations will be recorded in the
machine-readable follow-up.

Current native API documentation was fetched through Context7. The cancellation
API documents acceptance of a cancellation request, not guaranteed cancellation;
the retained experiment therefore uses exact terminal native history before
selling. Documentation cannot replace an observed partial fill.

Validation observed in this session passed 678 Rust workspace cases, 43
disposable PostgreSQL/CLI cases and 72 Python cases, plus formatting, locked
workspace checking, strict all-targets Clippy, canonical assets and local links.
The ignored native experiment was invoked separately with explicit DEMO scope.

Private original bytes, request/response journals, failed commands, source
inventory, account audit and both attribution versions are retained under
`var/eur-release-readiness-20261106`. Public status is in
[the release-readiness evidence record](./evidence/eur-release-readiness-2026-11-06.json).
The prior sealed manifest
`d09df2264cbb3bb13fb4fc8ad3e5de9b449348bfdb0446a021b68b09ee31fc73`
is preserved. No activation-ready replacement manifest or owner approval
receipt is created while required native partial/protection evidence,
successful real-input paper replay and complete exact-release review are absent.

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

The bounded observer finished at 15:50:01 UTC after ten immutable cycles from
13:30 through 15:45. ASML produced a genuine signal at 14:15 and AIR at 15:15,
but both analyst attempts failed. The other eight cycles returned `NO_BREAKOUT`
for both instruments. No cycle produced a durable valid selection/risk plan,
so the observer started no forward-paper lifecycle. No strategy parameter,
signal, analyst response, risk plan or ledger was manufactured.

The actual retained diagnostics identify the same implementation defect in
both failed cycles: the analyst adapter rejected a captured quote carrying
`entry_authorized: false` merely because the field existed. The source repair
accepts an absent marker or the exact boolean `false`; `true`, `null` and other
values remain rejected. The application continues to wrap tool results with
its own `entry_authorized: false`. A loopback HTTP regression reproduced the
original failure, then passed after the fix, including successful correlated
tool-result continuation and rejection of claimed/malformed authority. Failed
native requests remain immutable and were not retried. The fixed candidate was
retained separately during these observations. The approved SHADOW installation
below subsequently installed those bytes; a successful real-input paper
lifecycle on the repaired source remains required.

The final recorded 15:45 inputs were exported without another provider call.
Credential-free `paper-day-check` replay exited 1 with `PAPER_DAY_BLOCKED`:
no executable candidate, valid real-input paper ledger or strategy/risk plan.
Fifteen passing synthetic protocol scenarios did not turn that replay green.

## Owner-authorized 98% balance probe

The owner's later entry authorization superseded the EUR 2,500 probe cap with
**98% of fresh spendable EUR cash**. It applies to a new bounded manual
experiment, not the closed morning authority. The wrapper requires a separate
explicit balance-sizing record and exclusive attempt marker. The original
continuation and ordinary release approvals are not extended by this record.

The new snapshot contained EUR 4,993.42, giving a conservatively rounded
EUR **4,893.55** cap. Fractional sizing at the fresh EUR 1,279.60 limit produced
**3.82428 ASML shares**, limit notional **EUR 4,893.548688**, rounded down to the
reviewed quantity step. Nominal share price did not impose a whole-share limit.
The existing exact identity, price-band, quote, protection and physical account
fences still applied.

LIMIT BUY **57650117604**, fill **57650117606**, filled the whole requested
quantity at 15:51 UTC. STOP **57650117614**, EUR 1,266.81 for the full quantity,
was confirmed at 15:51:25.320597159 UTC, 6.320597159 seconds after the native
whole-second first-fill timestamp. The separate process observed that exact
STOP cancelled before MARKET SELL **57650117645**, fill **57650117647**.
The protocol ended `FLAT`; its conservative exit-observation deadline reason
is preserved. No partial entry or smaller-quantity protection was observed.

The corrected supervisory native snapshot succeeded, and the later full
account-wide audit reached the explicit end of **103 rows, three pages and 58
distinct fills**. At 15:54 UTC the account was flat at **EUR 4,997.24**. This
probe changed cash by **EUR +3.82**, still capability activity. Both fills match
preceding positive-size live SMART books: zero BUY divergence and approximately
3.12452 bps absolute SELL divergence. The physical UTC-day entry count is three.
The SHADOW reader was restored and both read-only quote streams are closed.

A separate self-contained six-fill attribution candidate preserves the prior
four-fill acceptance and all old source bytes. It includes this new round trip,
the complete refreshed history and the two original cleanup heads. The second
independent Sol review accepted the exact six-fill candidate after 958 checks,
resolved the analyst defect and approved the current source delta with no
findings. The separately stamped six-fill registry passes the credential-free
reviewed checker. Coverage ends at 15:54:32.743817983 UTC; the net Nov6
capability cash change is EUR +4.56. The earlier four-fill registry is not
relabelled as covering later fills, and neither new registry is installed.

Both Sol rounds approved their behavior scope without findings or requested
code changes. The second round covers the newly observed analyst repair and
owner-authorized cash sizing. Review usage was not exposed. These source and
attribution approvals do not approve a complete release or activate DEMO.

## Approved SHADOW installation

After source PR #4 merged as `8c690c2`, the owner explicitly approved the
prepared persistent SHADOW binary replacement and restart of its three services.
The install completed at 16:44 UTC. Analyst, executor and gateway all ran new
PIDs on binary
`d1463d818cda7929f8371367980c3965ecb68725547bd9883334f7d9c1fb83e3`,
with no restarts during the 35-second stability check. The original binary is
retained privately for rollback.

The runtime remains SHADOW with configuration hash
`be611c54084c3efd5dd9363d90e883c38837722221434ffa96aa1696f4d97f46`.
The install changed no configuration, credential, migration or capability
registry. General DEMO remains disabled/inactive and LIVE remains disabled.
Post-install application output reports waiting for the next primary session;
this is operational startup proof, not a repaired positive paper lifecycle.
The [installation evidence](./evidence/eur-shadow-install-2026-11-06.json)
records the exact running pins and original private receipt hash. Earlier
observations keep their actual producing bytes and timestamps.

Current native API documentation was fetched through Context7. The cancellation
API documents acceptance of a cancellation request, not guaranteed cancellation;
the retained experiment therefore uses exact terminal native history before
selling. Documentation cannot replace an observed partial fill.

Final validation observed in this session passed 680 Rust workspace cases, 43
disposable PostgreSQL/CLI cases and 73 Python cases, plus formatting, locked
workspace checking, strict all-targets Clippy, canonical assets and local links.
The ignored native experiments were invoked separately with explicit DEMO scope.

Private original bytes, request/response journals, failed commands, source
inventory, account audit and both attribution versions are retained under
`var/eur-release-readiness-20261106`. Public status is in
[the release-readiness evidence record](./evidence/eur-release-readiness-2026-11-06.json).
The prior sealed manifest
`d09df2264cbb3bb13fb4fc8ad3e5de9b449348bfdb0446a021b68b09ee31fc73`
is preserved. No activation-ready replacement manifest or owner approval
receipt is created while required native partial/protection evidence,
successful real-input paper replay and complete exact-release review are absent.

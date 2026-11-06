# EUR evidence continuation, 6 November 2026

The subsequent [30-entry policy follow-up](./eur-demo-entry-policy-2026-11-06.md)
contains the latest installed SHADOW pins and bounded manual-watch results.
This morning continuation and its original producing bytes remain historical
evidence; it has not been reopened.

This is the owner's single authorized 08:15–10:00 UTC continuation, with no new
experiment after 09:40 UTC. The task disabled itself before provider or broker
activity; both continuation schedules are disabled. No replacement schedule,
general DEMO activation, LIVE access or owner approval receipt was created.

The initial workspace was clean on `main` at `5319b527`; its tree matched
`implement/eur-first-run` at `a32e332`, so work continued on the requested
branch without discarding changes. The owner's later hardening commits and all
prior sealed bundles remain intact. Private evidence is retained under
`var/eur-evidence-20261106-0815`, starting with exclusive `START.json` and
`ENTRY_ATTEMPT.json` guards.

## Actual native result

The account was verified flat at EUR 4,992.68 before mutations. The existing
identity observation remains valid until 6 November 17:23:08.013199 UTC; no old
review was extended. Native catalog, calendar and exact SMART contracts were
refreshed. The competing SHADOW broker reader was paused for the probe and
restored afterwards.

One genuinely new AIR opportunity showed a live type-1 SMART ask of EUR 197.84
with size 10. The single LIMIT BUY requested 12.6364 shares, EUR 2,499.985376
limit notional. Trading 212 filled the entire quantity. SMART displayed depth
does not describe the Trading 212 OTC queue: this is **not native partial-entry
evidence**, and no second BUY was attempted.

| Native order | Actual result |
| --- | --- |
| BUY 57600336356; fill 57600336358 | Full 12.6364 shares |
| STOP 57600336362 | Full acquired quantity, EUR 195.87; working STOP confirmed |
| Exact STOP cancellation | Terminal cancellation observed before exit POST |
| MARKET SELL 57600336378; fill 57600336380 | Full exit; physical account flat |

The STOP confirmation timestamp was 4.919244496 seconds after the native
whole-second first-fill timestamp. This is an observed timestamp difference,
not a network latency guarantee. A native metadata 429 and subsequent local
pacing delays occurred during exit-process catalog warming while the STOP
remained working. The failures and original process logs are retained.

Both new fills matched preceding timestamped SMART books with positive sizes
and live type 1. BUY divergence was 0 bps; SELL absolute/adverse divergence was
1.010203050813213455904636832 bps, within the existing AIR 5 bps limit. Price
bands, cent grid, quantity steps and notional limits were not widened.

The final native account was flat at EUR 4,994.19. The EUR 1.51 cash increase is
capability activity, **not strategy performance**. Complete lifetime order
history contains 97 rows over two pages and 54 distinct fills: four on
3 November, 48 on 5 November and two on 6 November. The EUR 5,000 deposit plus
native wallet movements of EUR -5.81 reconciles to EUR 4,994.19. The historical
3 November AAPL USD/EUR conversions and their disclosed conversion taxes remain
identified as pre-month legacy activity; they were not repeated.

The actual Nov6 BUY consumes this day's physical entry allowance. Ordinary
DEMO deployment remains disabled, cannot add another Nov6 entry, and still
requires reviewed attribution of these capability fills before month strategy
economics can pass. The account-wide audit does not silently import those fills
into the strategy ledger or rewrite old protocol journals.

## Review findings and repairs

Independent review identified and the implementation addressed reused probe
intent/account IDs, widened probe price bands, missing quote expiry at POST,
incomplete SMART identity/size/receipt checks, signed native SELL fill handling,
whole-second fill-clock handling and ambiguous mutation cleanup. The wrapper
now claims one exclusive continuation slot and permits only the thin-depth
partial watch. Mutation markers are synced before dispatch, unknown outcomes
require exact-ID review, and recovery uses the driver's cancellation/history
boundary. Failed runs remain preserved.

Two Nov5 lab journals retain stale nonterminal local states after separately
recorded exact-ID cleanup. Fresh native history verified all six related IDs
terminal and each acquired share sold exactly once. Their original context,
transport, checkpoint and cleanup hashes are bound by an external-cleanup
attestation. That attestation permitted the separate bounded probe; it does not
manufacture reducer `FLAT` states or ordinary-deployment acceptance.

The native run exposed a further application defect: applying ENTRY before EXIT
from the same complete account view briefly compared the new zero holding with
the old exit fill total. It latched `HISTORY_AND_HOLDING_CONFLICT`, then reached
`FLAT` after applying the remaining legs, without an extra order. The original
native journal and finding remain unchanged. The subsequent source repair
applies complete multi-leg observations together, preserves old single-event
replay semantics, and tests full exits, a partially filled working STOP,
genuine ledger conflicts and locally rejected STOPs without native IDs.

The actual native producer is retained separately: test executable
`b26a433a2f74c1fb40f1c319c29f8feeafa856b86f968805cd187f7feff0748b`,
source inventory `31999d70df0ad7f1855171d32f9056bdb820176516f71d8964b123fe6b41172d`,
with SHADOW main binary
`49bd234e88cfcfaf9d207c642b092ad5e5f85af0b405a479f556a8b33c547803`.
Later repairs are not relabelled as the producing bytes for this native run.

The final independently accepted SHADOW source inventory is
`03bb35f1dc22df023c28c34ecee389fba013b21c249582940aa2a563d508f984`.
Its installed binary is
`173a883df22ae55aa586417bf0363c4279893fc126fad1ffb09388b94a3c2f0b`,
runtime configuration
`df9ed598029172b64178daa32074aa7e4d57a8cceaecaeef9840346e12c4c72b`,
deployment `eur_month_batch_hardened_v3`. All three SHADOW service processes
were checked against those running bytes after installation at 09:30 UTC.
General DEMO remains disabled. Validation passed 670 Rust cases, 63 script
cases, formatting and strict workspace/all-targets Clippy. Independent review
accepted the source update for SHADOW only and withheld complete-release
acceptance.

## Real-input paper observation

The 08:15 cycle had 105 completed current IBKR candles per instrument and no
breakout. The first hardened-source 08:45 cycle produced natural registered
strategy signals, but its genuine analyst attempt returned
`ANALYST_FAILED_OR_RESPONSE_INVALID`; no selection or Rust risk plan resulted.
The failed response is immutable. The 09:00 and 09:15 cycles returned
`NO_BREAKOUT`, and their real-input paper exports explicitly remained blocked.
The final-source 09:30 cycle also returned `NO_BREAKOUT` for both instruments,
with 120 completed IBKR candles each. Its paper attempt retained
`DURABLE_REAL_INPUT_SELECTION_AND_RISK_PLAN_REQUIRED`; it created no simulated
entry, positive ledger or replay acceptance.

Current provider documentation and a read-only Go model-catalog response
confirmed the configured DeepSeek model is listed. This does not establish the
cause of the earlier analyst failure. The new source retains bounded actual
analyst completions or provider error codes for future natural cycles, without
replaying a failed request, changing the model or creating a strategy signal.
Any forward-paper lifecycle must use a genuine durable selection/risk plan,
fresh approved SMART stream, a 60-second bound and credential-free replay.
The bounded observer finished at 09:40:01 UTC without a valid current plan or
paper lifecycle. No positive-ledger replay was attempted and no experiment was
started after the 09:40 cutoff. The final coordinated native account check at
09:35:59 UTC again found no holdings/orders and EUR 4,994.19 cash; the SHADOW
reader was restored. No future continuation was created.

## Release boundary

The sealed blocked candidate remains
`d09df2264cbb3bb13fb4fc8ad3e5de9b449348bfdb0446a021b68b09ee31fc73`
at `var/eur-legacy-cleanup-20261105/release-final-reviewed`. It and all earlier
candidates, including `9e0478...`, are preserved. This continuation does not
turn it into an activation-approved release.

Native partial BUY/remainder cancellation/protection of the smaller acquired
quantity and a successful real-input paper lifecycle remain required. Existing
controlled faults, simulations, manually reduced holdings and the historical
STOP-fill-before-DELETE-404 observation keep their original classifications;
this run supplies no new simultaneous STOP-fill/cancellation race observation.
Complete independent release review and exact-hash owner approval remain gates.
No activation-ready replacement bundle is assembled while required evidence is
absent. The current machine-readable status, installed pins and final results
are in [the continuation record](./evidence/eur-evidence-continuation-2026-11-06.json).
The existing sealed bundle's verifier was run again at 09:31 UTC and exited 1
with native-partial, protocol, paper, independent-review and exact-owner-approval
blockers. Its manifest hash was unchanged; no verifier result was bypassed.

# EUR blocker follow-up, 4 November 2026

**5 November correction:** real-price probes show a **EUR 1 minimum trade
value**, not the one-share minimum inferred below from EUR 1 limit requests.
Fractional protection/liquidation and forty actual fill/book matches are now
recorded in the [real-session follow-up](./eur-blocker-resolution-2026-11-05.md).
The scheduled continuation disabled itself at startup. This dated report and
its sealed JSON observations remain historical evidence.

The runtime changes are implemented and deployed in **SHADOW**. The overall
release remains blocked by missing broker-backed acceptance. No LIVE order was
submitted. The current demo account is flat.

The owner explicitly selected [IBKR SMART Live for real-time bid/ask](./eur-data-sources.md),
with YFinance and Alpha Vantage for news where available, historical stock data
and other research queries. This is documented in the root/configuration guides,
the IBKR guide and [machine-readable policy](../config/eur-data-sources.json).
The EUR loader requires the IBKR configuration and rejects quote fallback.

## Implemented and verified

- **Decision and risk:** replaced the fixed blocked result with completed-bar
  strategy assessment, a durable analyst-role request/response, the pinned model,
  a fresh post-research IBKR book and deterministic Rust risk calculation. Model
  credentials remain with the analyst service. Waiting, failed and unknown
  outcomes are distinct. Missing order rules or reviewed EUR costs remain blockers.
- **Execution:** connected the journalled reducer to the Trading 212 adapter
  through the explicit, release-gated `eur protocol-drive` command. It checks exact
  order shape, bounded history, holdings and available sell quantity, preserves
  unknown dispatches, handles cancellation races and records raw economic
  responses. Target/deadline exits and reviewed exact-ID recovery are wired.
  Scheduled SHADOW services do not call the mutation command.
- **Activation:** implemented immutable bundle assembly, per-artifact hash
  verification, evidence coverage/expiry checks, raw fill matching for quote
  samples, independent review coverage and exact-hash private owner approval.
  Failed or missing evidence prevents an activation receipt. See
  [execution and release](./eur-execution-and-release.md) for the contract.
- **Identity:** fresh broker and Yahoo reads revalidated both unchanged exact
  identities. A new review was written without editing the original seal and
  installed in the SHADOW configuration. It expires **5 November at 20:08 CET**.
- **Tests:** 667 Rust workspace tests passed; the 38 ignored integration tests
  then passed against disposable PostgreSQL and CLI fixtures. All 39 Python
  tests, strict Clippy, formatting and asset/link checks passed. Targeted driver,
  HTTP and CLI checks passed after the final changes. A real online pinned-model
  probe passed with explicitly synthetic numerical inputs; it establishes model
  transport/contract behavior, not market or paper-day acceptance.

The [broker probe artifact](./evidence/eur-broker-rule-probes-2026-11-04.json)
records eighteen DEMO validation requests. Eleven accepted unfilled limits were
cancelled by exact ID and verified terminal in history; seven requests were
rejected. No position was acquired.

For both exact EUR instruments the broker reported a **one-share minimum**.
Airbus accepted quantity 1.0001 and rejected five-decimal quantities with an
explicit four-decimal precision error. ASML accepted 1.00001 and rejected six
decimals with an explicit five-decimal error. Fractional buy-limit quantities
above the minimum therefore passed request validation. Fractional sell/protective
fills have not been established. Precision may change with the instrument's
price band and must be revalidated.

Both instruments also accepted a limit price of 1.000000001. This low-price
request acceptance cannot establish the minimum executable tick near the actual
market price. The reviewed identity rows retain unknown execution rules; the
observations are a separate artifact.

## Still required for release

1. Verify the executable price lattice in the intended Airbus/ASML order ranges
   and independently accept the complete instrument rules.
2. Capture broker-backed partial fills, rejected protection after acquisition,
   cancellation races, unknown submissions and restart recovery. The new driver
   regressions and old fifteen synthetic scenarios cannot substitute for these.
3. Capture EUR BUY and SELL fill/book matches for each exact instrument. The
   [compatibility artifact](./evidence/eur-quote-acceptance-pending-2026-11-04.json)
   preserves missing samples. Ten-second age, 20 bps spread/divergence and 50 bps
   round-trip cost ceilings are verifier bounds pending acceptance, not measured
   EUR compatibility.
4. Complete a real-session paper day with successful selection, research, risk,
   protection, exit and reviewed broker costs. The current closed-session run
   and offline replay remain blocked. Yahoo news remains unavailable; X is
   metadata-only without approved content/model processing.
5. Obtain independent acceptance and final owner approval of the exact complete
   bundle hash. The broad implementation grant is preserved; it has not been
   rewritten into an approval of a release whose required evidence is missing.

Primary EUR sessions were already closed during this work. Their next regular
opening is **5 November at 09:00 CET**, subject to the broker calendar. Existing
SHADOW services and scheduling remain active. A one-time continuation in this
conversation is scheduled for **5 November at 09:05 CET (08:05 UTC)** to collect
the remaining session evidence. The continuation disables its own recurring
trigger at startup. Scheduling does not itself establish any acceptance result.

Private deployment backups, scoped source observations, full test logs, model
probe and replay artifacts are retained under `var/eur-completion-20261104/`
and `/var/lib/trady-eur-backups/`. The candidate manifest and exact unresolved
activation blockers are linked from the [final evidence record](./evidence/eur-blocker-followup-2026-11-04.json).

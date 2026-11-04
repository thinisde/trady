# EUR recorded replay and recovery checks

Export a completed SHADOW cycle using the analyst database role:

```sh
trady eur export --config /etc/trady/runtime/eur-shadow.json --out /var/lib/trady-evidence/capture-001
trady replay manifest-check --config config/eur-demo.example.json --manifest /var/lib/trady-evidence/capture-001/manifest.json
trady replay paper-day --config config/eur-demo.example.json --manifest /var/lib/trady-evidence/capture-001/manifest.json --out /var/lib/trady-evidence/replay-001
```

Use a new output directory each time. Captured configurations, raw provider
responses, catalog chunks, normalized quotes and their hashes are frozen.
Provider assertions run offline; only the final report uses PostgreSQL, under
a separate `replay_` account. IBKR quotes are normalized again from their
provider-timestamped callbacks. Replay never connects to IB Gateway or a broker.

The command writes selection, strategy, ledger, restart, reconciliation,
Telegram, capability and summary artifacts. Its order ledger is explicitly
classified as synthetic protocol fixtures. A captured blocked selection remains
blocked: missing risk plans, executable candidates or reviewed costs cannot be
replaced with invented successful trades. `PAPER_DAY_BLOCKED` returns nonzero
after preserving evidence. This is not complete positive paper-day acceptance.

Run the separate deterministic order-protocol regression suite without providers
or a database:

```sh
trady replay protocol-suite --out /var/lib/trady-evidence/protocol-suite.json
python3 scripts/test-postgres.py
```

The reducer emits a submission effect only after its UNKNOWN marker commits.
Replaying that event requests reconciliation or operator recovery instead of
submitting twice. Partial fills require cancellation and final exact-ID history
before protection. Partial exits reconcile remaining holdings before a further
sell. Stops cannot widen, and entry-authority expiry cannot disable a controlled
exit. The PostgreSQL suite checks fencing, atomic one-slot reservation, restart
recovery and protected-table permissions in a disposable database.

These tests do not establish that Trading 212 provides attached/OCO protection,
that separate stop orders bound exposure acceptably, or that a restart-safe
broker transport is approved. The DEMO path remains closed while its real
capability and driver integration are unfinished.

The daily timer starts before approved exchange sessions. An early invocation
returns `EUR_SHADOW_WAITING_FOR_SESSION` and does not consume the daily decision.
The existing executor continues reconciliation and assessment. Completed reports
produce an immutable sanitized daily handoff during the next analyst cycle.
The EUR weekly worker derives coverage from captured calendars, reports missing
economic metrics explicitly and writes `PENDING_REVIEW`. Its credential file
contains only `TRADY_ANALYST_DSN`; it has no broker or Telegram credentials.

Render calendar overrides for the exact approved exchanges with
`scripts/prepare-eur-schedule.py`. Rendering does not install or enable timers.
Verify service units before installation. Keep the legacy sealed weekly research
service separate from `trady-eur-weekly-report.service`.

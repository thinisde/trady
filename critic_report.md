# Stage 7: Design Critique

Target: `docs/designs/trading212-same-day-sequential-protection.md`

## Findings

### Blocker: Alternatives use unsupported author attribution

The alternatives are labeled “Rejected by author” without linked evidence. Use
concrete, verifiable rejection rationale and links, or remove the attribution.
The trade-offs should include measurable complexity, cost, and risk criteria.

### Blocker/Major: The unprotected-holding path is not operationally bounded

If stop placement is rejected or unknown and no valid executable quote exists,
the document says to flatten when safe or alert and remain unresolved. It does
not define an operator SLA, kill switch, manual broker procedure, maximum
exposure or timeout, or restart/market-close behavior for an unprotected live
position. Define mandatory escalation and the allowed overnight outcome.

### Major: No measurable acceptance criteria

Terms such as fresh, stale, paced, bounded, conservative, valid, same-day, and
configured hurdle have no thresholds or metrics. Add quote age, stop-placement
latency, spread/liquidity limits, deadline timezone, API pacing limits, flatness
SLA, recovery/alert SLA, and pass/fail tests.

### Major: Quote compatibility has no proof method

The design rejects feeds that are not proven compatible but defines no venue
mapping, provenance, sampling, divergence criterion, evidence owner, expiry, or
behavior when Trading 212 OTC execution differs from IEX or another feed.

### Major: Partial-fill race remains ambiguous

Canceling the remainder, then protecting the filled quantity, leaves a race for
fills arriving between observation, cancellation, and stop placement. Define one
authoritative order-history reconciliation, the terminal-cancellation rule,
late-fill transitions, and exact-ID protection adjustment.

### Major: Stop semantics are incomplete

The design does not define trigger side, stop-price derivation, stop versus
stop-limit selection, tick/lot constraints, broker status taxonomy, or partial
stop-fill handling. “Open or otherwise accepted” is too broad to prove working
protection.

### Major: Exit cancellation and fill authority are incomplete

A terminal cancellation or fill observation may not settle all in-flight
executions. Define the history/settlement sequence, grace period, timeout, and
residual quantity reconciliation before submitting a sell.

### Major: Same-day and overnight policy can contradict itself

The non-goal disallows overnight holding without protection/reconciliation, but
recovery can remain unresolved past the deadline. Define deadline timezone,
holiday/market-close behavior, next-session handling, and whether owner approval
can permit overnight recovery.

### Major: Recovery mutation is security-sensitive but undefined

The owner-reviewed exact-ID recovery operation lacks an authorization model,
command schema, allowed actions, idempotency rules, audit trail, and dual-control
requirements.

### Major: Accounting claims lack source and formula

The 0.15% FX fee and conservative slippage are unversioned assertions. Bind
them to dated config/evidence and define the exact net-target equation, rounding,
and conflict behavior.

### Major: Success is not measurable

The document explains the business motivation but does not state measurable
outcomes such as supervised cycles completed, zero oversells, reconciliation
rate, protection latency, or recovery resolution rate.

### Additional consistency issues

* “One sell attempt” conflicts with exempt emergency liquidation; define whether
  emergency sells may repeat and how they are counted.
* Retaining `ATOMIC_ATTACHED` as a selectable protection mode conflicts with the
  claim that Trading 212 does not support it. Make it non-selectable for this
  broker or scope it to a capability registry.
* Target limit/market selection, tick/lot rounding, partial target fills, and
  replacement behavior are not deterministic.
* Durable journal schema expansion has no versioning, backfill, restart
  compatibility, or rollout/rollback plan for an in-flight old record.

### Nits

Normalize list-marker style. Replace uncalibrated terms such as fail-closed,
honest, exact, normal, risk-reducing, and fresh with operational definitions.
Claims that the current capability record proves broker behavior should cite an
artifact ID or be stated as observations.

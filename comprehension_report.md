# Stage 6: Comprehension Test

Target: `docs/designs/trading212-same-day-sequential-protection.md`

## What this proposes

The document proposes a one-trade-per-day Trading 212 demo workflow that buys
with a marketable limit, reconciles the actual fill, places a separate native
stop only for filled shares, and sells only after the stop is terminally
cancelled or has filled. A durable journal/state machine records every mutation
before sending it, treats unknown broker responses as recovery, and gates entry
on fresh quote/session/liquidity/cost evidence including two 0.15% FX
conversions. It explicitly supports sequential native stops while leaving atomic
bracket/OCO protection unsupported, with supervised rollout and later autonomy
subject to evidence gates.

## How the existing system works, per this document

The existing application already has durable order roles, a fenced
`TradingState`/execution journal, account observations, partial-fill accounting,
cancellation requests, recovery states, Trading 212 order/observation adapters,
and capability records with expiry/evidence semantics. Trading 212 can submit
ordinary market, limit, stop, and stop-limit orders and report fills, fees, FX,
and pending/history state, but its capability record does not establish atomic
attached protection or a broker-executable quote feed. The proposed work binds
those pieces into a new lifecycle and adds protection metadata, quote/cost gates,
adapter normalization, journal/migration support, tests, and operator
documentation.

## Findings

### Major: Same-day policy values are undefined

The document repeatedly requires a bounded same-day policy, session window,
deadline, configured hurdle, and one session, but never defines values,
timezone, market calendar, or deadline selection. A fresh implementer cannot know
when entry is allowed, when the deadline exit triggers, or what configuration
makes sequential mode eligible.

### Major: Quote compatibility has no testable definition

Venue or execution-compatibility scope is a central authorization condition, but
the document gives no mapping, attestation, measurement, or acceptance rule.
It notes that IEX may differ from Trading 212 OTC execution without saying what
would prove compatibility or what evidence is sufficient.

### Major: Late fills after entry cancellation are unspecified

The flow cancels the entry remainder before protection, then allows later fills
to add protection, but does not define the state transition for a fill after a
cancellation request or acknowledgement. It does not specify an observation
window or whether a late fill creates a new stop request or recovery state.

### Major: Stop acceptance and terminal states are ambiguous

The requirement to observe a stop as open or accepted does not define allowed
broker statuses, the difference between API acknowledgement and a live pending
order, or a timeout. “Terminal broker fill” is also not defined as a stop state.
The implementer cannot know when a target sell is authorized.

### Major: Recovery authority is not defined

The design refers to an owner-reviewed exact-ID recovery operation, alerts, and
optional automatic flattening, but does not define the owner role, command/API,
required evidence, or operator procedure.

### Major: Journal and fence invariants are assumed, not stated

Terms such as fenced `TradingState`, execution journal, account fence, executor
role, and analyst/gateway roles are project-specific. The design assumes their
write and ordering guarantees while changing persistence and migrations, but
does not summarize the contracts a new implementation must preserve.

### Major: Capability and release gate schemas are incomplete

Unsupported versus unverified, independent review hash, capability artifact,
validity window, formula/research/input-service gates, and shadow/paper gates
are named without schemas, owners, or pass/fail semantics. Entry authorization
cannot be implemented or audited from the design alone.

### Major: Daily-limit classification is incomplete

The document excludes protection and emergency liquidation from ordinary buy and
sell limits but does not define classification after a rejected stop, late stop
fill, or partial target. Restart/recovery could block a required risk-reducing
sell or allow an extra strategy sell.

### Major: Fee-aware economics lack a normative formula

The design names FX, spread, charges, slippage, and a hurdle but gives no
formula, rounding rules, precision rules, or conflict handling when broker fees
differ from the configured 0.15% assumption. Different implementers could make
different entry decisions.

### Major: Demo acceptance is not executable

The demo plan lacks the exact account/instrument, stop price, deadline, expected
observations, and pass/fail criteria for cancellation races and ambiguous
responses. The test list lacks expected state assertions for several recovery
branches.

### Nit: The lifecycle lacks a complete state table

The diagram uses BUY intent, `OPEN_PROTECTED`, `RECOVERY_REQUIRED`, terminal,
and flat reconciliation without listing all states, legal transitions, or
persisted fields. Readers must infer how no-fill, stop-filled, and failed-stop
paths terminate.

### Nit: Current interfaces are not linked or summarized

The component list names project-specific files and interfaces such as
`TradingInputs::quote`, `ProtectedOrderRequest`, `CancellationReceipt`, loopback
fixtures, and the integrity verifier without links or a concise current contract
summary. It is unclear which changes are additive and which replace existing
behavior.

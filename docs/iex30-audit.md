# IEX30 code audit

This audit records the focused path after removing the unused IBKR data plane,
European deployment artifacts and generated design/review files.

## Implemented

| Area | Result |
|---|---|
| Universe | trady-domain/src/iex.rs defines one ordered, unique 30-symbol allowlist, aliases and instrument validation. |
| Screening | IEX30 is a supported index ID; the screener requires the exact 30-member set and USD. |
| Universe source | source_id alpaca_iex30 returns the owner-reviewed local IEX30 snapshot with bounded provenance. |
| Alpaca REST | AlpacaDataConfig accepts IEX only; quote/history symbol conversion rejects every other instrument. |
| Alpaca stream client | Authenticated blocking IEX WebSocket client, bounded read timeout, reconnect backoff, exact 30-symbol subscription, quote parsing and shared quote cache. |
| Alpaca stream plan | stream-plan validates the 30-symbol trade/quote subscription and emits auth and subscribe messages for the IEX endpoint. |
| Execution guard | application dispatch and the Trading 212 executor reject non-IEX30 instruments before submission. |
| Watchdog schedule | IexWatchdogConfig validates the same exact list and emits one scheduled freshness tick per symbol. |
| Holding watchdog | autonomous run records fenced stream health, stale-symbol and held-quote observations once per bucket while a plan is active. |
| X budget | the focused registry uses a total 1,900,000 micro-USD ceiling, post/user unit costs, per-call limits and exhaustion stopping. |

## Wired

| Boundary | Wiring |
|---|---|
| Autonomous runtime | RuntimeConfig loads Alpaca IEX and watchdog settings; the executor starts the stream before the session window, uses the shared quote cache, records watchdog observations during holdings and stops after confirmed closure. |
| Analyst and executor | Both AutomaticInputs and MarketInputs route quote reads through Alpaca when enabled. |
| Credentials | Enabled Alpaca data reads TRADY_ALPACA_API_KEY and TRADY_ALPACA_SECRET_KEY through CredentialStore. |
| CLI | alpaca stream-plan, watchdog check/schedule and autonomous check are registered in the main command router. |
| Trading 212 | Demo remains the only executor in the focused config; no Alpaca order method exists. |

## Configured

- autonomous.iex30.demo.json is the single focused runtime template. It is
  disabled until evidence gates are supplied.
- alpaca-iex30.paper.example.json contains the IEX feed, USD currency and
  bounded REST limits.
- iex30.universe.json declares the exact membership and 30-symbol capacity.
- watchdog.iex30.json schedules all 30 symbols at five-minute intervals.
- research.json contains the reviewed candidate X sources and total
  credit ceiling, but remains disabled until stable user IDs and rights are
  filled in.

## Not implemented or still blocked

| Item | State |
|---|---|
| Long-lived Alpaca WebSocket client | Implemented with authenticated IEX subscription, quote cache, timeout, bounded reconnect and lifecycle shutdown. |
| Executor watchdog loop | Implemented for active holdings; each bucket persists stream/quote health through the fenced journal. |
| Alpaca credentials | Required in the private credential store before an enabled runtime can start. |
| Trading 212 demo identity | Bound to demo account 52211463 (EUR) and verified against the provider summary on 2026-11-03; the example profile is no longer a placeholder. |
| Input service and universe authority | Placeholder endpoint, owner identity and source rights intentionally block readiness. |
| Model, formula and capability evidence | The demo account now has dated cash/instrument observations plus explicit unsupported native-attachment and executable-quote results in [the capability evidence artifact](./evidence/trading212-demo-capability-2026-11-03.json). The model/formula gates remain empty and autonomous check remains blocked. |
| X identity and rights | Stable numeric user IDs, ownership/coverage approvals and source rights are intentionally blank; no X request is made while the registry is disabled. |
| Live execution | Not enabled by this request. The selected path is Trading 212 demo execution only. |

## Removed

- IBKR Rust adapter, CLI, sidecar, service files, validation script and config.
- European runtime/cold-start templates and the European universe example.
- Generated EGM briefs, review reports, hypotheses and superseded implementation
  plans that described the removed deployment scopes.
- Dated demo, drift, provider-research, crosswalk and simulated-entry records.
- The first-live-demo research command and its unused model/news/FinBERT profiles.
- Obsolete pending deployment manifests and optional market-intelligence design
  folders that were not loaded by the IEX30 runtime.

The repository still contains generic research and historical documentation
used by shared tests and offline tooling. Those modules are outside the IEX30
runtime because the focused config does not load or authorize them.

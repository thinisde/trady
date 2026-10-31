# Market expectations versus realized surprise (proposed)

Status: new feature design, 2026-10-31. Documentation only: no consensus feed, event study, expected-return calibration or production authority is implemented.

## Purpose and distinction

The [financial-data layer](../free-financial-data.md) already plans earnings observations and warns that consensus surprise requires timestamped expectation data. The **new capability** here is an event-centric **expectations decomposition and market-pricing model**: separate what was forecast, what was reported, what the market plausibly priced before the release, and what price did after the release. It must not claim to know a true unobservable priced-in expectation.

Use cases: earnings revenue/EPS beats that nevertheless disappoint, CPI vs survey consensus, FOMC decision and communication surprises, guidance revisions against prior company range, and peer spillovers. Only event types with defensible comparable estimates get a numerical surprise.

## Event and benchmark contracts

An immutable EventExpectation contains event id/issuer, type, actual release timestamp, source availability, consensus provider, analyst-count/dispersion if available, estimate vintage timestamp, accounting currency/units/period and adjustment definition. ActualObservation contains primary filing/release link, reported value, revisions and availability timestamp. MarketReaction contains pre-event reference quote/bar, post-event trading-session windows, benchmark control, corporate actions, FX and trading halts.

~~~json
{
  "schema_version": 1,
  "event_id": "earnings:issuer_42:2026Q3",
  "metric": "diluted_eps",
  "period": "2026-Q3",
  "unit": "USD/share",
  "estimated_value": "1.30",
  "estimate_available_at": "2026-10-29T18:00:00Z",
  "reported_value": "1.42",
  "report_available_at": "2026-10-30T20:10:00Z",
  "coverage": {"analysts": 12, "rights_id": "licensed-consensus-v1"},
  "reaction": null
}
~~~

Synthetic fixture, not a real issuer. Require estimate **frozen before** release. Null reaction means the reaction was not yet observed.

## Calculations and uncertainty

- **Raw surprise:** actual minus consensus, with signed sign convention, magnitude and unit. **Normalized surprise:** (actual − consensus) / max(abs(consensus), scale_floor) for metrics where this is meaningful; do not divide by near-zero EPS or compare EPS definitions without adjustment. Standardized unexpected earnings may use analyst-estimate dispersion or historical surprise volatility only if sufficient data are present.
- **Expectation drift:** estimate revisions in explicitly earlier windows. Avoid hindsight recomputation from today's analyst data.
- **Event abnormal return:** realized event-window stock total return minus selected matched benchmark/sector return for the same tradable interval. This is an event-study proxy, not the causal effect of the news.
- **Pre-event positioning proxy:** abnormal pre-release cumulative returns and options-implied move where licensed. Label as *pricing proxy*, not percentage already priced in. The model may fit conditional reaction against expectations, positioning, macro regime and liquidity in a historical sample; it cannot observe the market's true beliefs.
- Handle after-close releases, pre-market releases, simultaneous news, confounding announcements, trading halts, splits, holidays and FX with explicit quality flags. Event time must precede reaction data; market close does not imply the event was publicly known.
- Outcome categories can be surprise-up/reaction-up, surprise-up/reaction-down, surprise-down/reaction-up, surprise-down/reaction-down, and inconclusive. Never map an earnings beat deterministically to BUY.

## Architecture and consumer access

The existing research source adapter supplies filings and macro releases. Introduce an optional licensed ConsensusEstimateSource and an EventStudyService under the Rust analyst boundary. Event artifact store versions expectation, actual and reaction independently; backtests reconstruct their availability chronology. A read-only research tool event.surprise_check returns raw surprise, normalized definition, latest report, vintage, price-reaction window, confounders and abstention. The LLM may explain contrasts; only an approved, calibrated downstream feature passes into [prediction fusion](../prediction-fusion.md). No model/tool changes trading policy, stop or live schedule.

## Validation and experimentation

Create point-in-time event panels where analyst-estimate snapshots are legally accessible and timestamps reliable; do **not** backfill current consensus as past consensus. Train on chronologically separated issuers/events, embargo overlapping event windows, and compare to: surprise sign only, price momentum, sector benchmark and Trady's current feature set. Score probability calibration, abnormal-return prediction, net paper profitability, confidence intervals, abstention by event type and early/late availability. Negative or null improvement is a valid result. Disclose consensus availability/cost and event coverage.

## Degraded operation and acceptance tests

- Missing benchmark/consensus, revised actual, stale event clock or unit mismatch -> INCONCLUSIVE, no numerical surprise signal.
- Report correction after the fact -> append revision and invalidate dependent historical reconstruction, not rewrite old prediction.
- Two events released simultaneously -> mark confounded, never attribute all return to one.
- Fixtures: EPS negative-to-positive, zero/negative consensus denominator, revised CPI vintage, post-market release, holiday next session, overlapping product launch, analyst estimate timestamp after event, material guidance mismatch.
- Paper/shadow research only until data rights, baseline comparisons and sealed evaluation pass. No order authority.

## Primary references

[SEC EDGAR data APIs](https://www.sec.gov/search-filings/edgar-application-programming-interfaces); [FRED revision-aware observations](https://fred.stlouisfed.org/docs/api/fred/series_observations.html); related [free financial API catalog](../free-financial-api-catalog.md) and [weekly optimization gates](../weekly-optimization.md).

# Options market intelligence (proposed)

Status: new feature design, 2026-10-31. Documentation only; no options feed, subscription, prediction feature, order permission or deployment exists.

## Purpose and distinction

Add a **read-only derivatives expectations service** that interprets publicly/contractually available equity-options markets as context for Trady's stock decisions. This is NOT options trading: the [v1 policy](../strategy.md) remains long-only, cash-only, one ordinary equity holding and the existing entry/exit limits remain unchanged. Unlike the existing ATR/realized-volatility features in [prediction fusion](../prediction-fusion.md), this design observes prices of options expiring in the future. Its incremental hypothesis: a market-derived estimate of uncertainty may help distinguish price direction from event magnitude.

## Questions and use cases

- How large is the event-window move implied by liquid near-expiry options, conditional on model assumptions, compared with prior realized event moves?
- Has downside skew or term structure changed **before** an earnings release? Is the change different from the broader sector?
- Does the research model predict direction while the option market prices high uncertainty? Annotate the difference; never infer bullishness from high implied volatility.
- When coverage is too thin, does the system explicitly abstain instead of inventing a zero or interpolated signal?

## Data acquisition, contracts and rights

Candidate classes: a licensed options-chain feed with timestamped bid/ask, underlying venue quote, strike, put/call, expiry, contract multiplier, exercise type, volume, open interest and underlying corporate-action adjustment; optional risk-free/dividend curves. Exchange/OCC or licensed distributors must be checked for contract-level use, retention, derived-feature storage and AI processing. **No free or live chain is assumed.** Options chain timestamps and underlying quote timestamps must be aligned.

Proposed Rust boundary:
- trady-adapters: OptionsChainSource read-only adapter, capability/rights record, rate budget and expiry-aware poller.
- trady-domain: OptionsContractKey; OptionsQuoteObservation; ImpliedExpectationAssessment.
- trady-application: validate chain coverage, resolve issuer mapping, persist source revision and as-of evidence.
- trady-policy: pure feature calculations. Do not introduce an order port or reuse broker order credentials.

Suggested immutable observation payload:

~~~json
{
  "schema_version": 1,
  "instrument_id": "internal_equity_id",
  "provider": "licensed_options_provider",
  "underlying_quote_at": "2026-10-30T19:00:00Z",
  "chain_observed_at": "2026-10-30T19:00:01Z",
  "available_at": "2026-10-30T19:00:04Z",
  "expiry": "2026-11-06",
  "currency": "USD",
  "contract_multiplier": 100,
  "contracts": [
    {"right": "call", "strike": "100.00", "bid": "2.40", "ask": "2.60", "volume": 80, "open_interest": 500},
    {"right": "put", "strike": "100.00", "bid": "2.10", "ask": "2.35", "volume": 75, "open_interest": 450}
  ],
  "rights_id": "approved_entitlement_revision"
}
~~~

Values above are synthetic schema fixtures, not live quotes. Raw contracts are optional/rights-dependent; derived features must retain source hashes and coverage.

## Analytical method

1. Align spot/forward, interest/dividend assumptions and option expiration/calendar with the decision's timestamp. Reject crossed, zero-width, stale or materially inconsistent markets; respect tick precision and adjusted contracts.
2. Estimate ATM implied volatility with a documented option-pricing model and solver convergence; report bid/ask IV range where meaningful. Do not equate annualized IV with probability of a specific direction.
3. For exploratory horizons, compute **one-standard-deviation scale** approximately as spot × IV × square-root(year fraction). Label the distribution/model assumption; do not claim it is a guaranteed range or an executable straddle break-even. For event-specific inference compare implied event variance between adjacent expiries only after accounting for calendar and non-event variance; report fragile inferences as unknown.
4. Calculate put/call IV skew at comparable delta, expiry term structure, underlying/sector relative IV rank, and IV-to-realized-volatility ratio with lookback and method recorded. Put/call volume is not a reliable independent directional vote.
5. Keep each calculation's evidence graph separate from filings/news origin and Perpetual label lineage. The same event news reflected in option prices is not an independent second confirmation.

Candidate typed feature: expiry, horizon, ATM IV interval, skew estimate, estimated move interval, relative-to-sector percentile, quote quality, coverage, model uncertainty and age. Missing data must be absent, not neutral.

## Integration and authority

The analyst may request options context for a shortlisted stock or current holding through a **read-only** model tool such as options.expectations. Its structured result uses exactly the same origin, availability and rights records as [financial-data integration](../free-financial-data.md). LLM output summarizes disagreement; it cannot update monetary limits. The fusion engine only consumes newly approved, calibrated numeric features, never raw prose or unvalidated option predictions. Existing required research gates and independent protection remain unchanged.

## Backtest, evaluation and cost

Build historical snapshots where both chain quotes and source availability actually existed, including delisted contracts, contract adjustments and survivorship-free eligible equities. Compare existing price/news baseline against adding one option feature at a time. Assess log loss/calibration for move-size buckets, direction log loss (to detect mistaken inference), conditional returns after spreads/FX, turn-over, abstention and drawdown. Use nested time splits and event-group embargo. Model exchange licensing, per-request cost and missing-chain rates. Promotion requires net positive evidence on independent sealed periods, not a better in-sample chart.

## Degraded mode and acceptance

- Empty/missing chain, disputed entitlement, stale quote, failed volatility solver or ambiguous corporate action -> unavailable; no increased confidence.
- High IV alone -> no BUY, target increase or stop relaxation.
- Known rights expiration -> invalidate downstream model features; existing holding protections continue.
- Tests: put/call parity tolerance with dividends/carry caveats, near-zero time to expiry, crossed quotes, illiquid tails, adjusted multiplier, odd holiday expiry, split, inconsistent timestamps, and duplicate-source lineage.
- Only enable in paper/shadow research after entitlement review, deterministic fixture tests and prospective signal-ablation results.

## Boundaries and references

No actual broker option execution, shorting, leverage, additional holding or automatic strategy promotion. Side effects of a later implementation are licensed read requests and versioned research records only. Reference: [Options Industry Council: volatility and skew](https://www.optionseducation.org/referencelibrary/faq/technical-information), [OIC: option pricing](https://prd-web.optionseducation.org/optionsoverview/options-pricing). Implementation must recheck provider terms and quote availability before deployment.

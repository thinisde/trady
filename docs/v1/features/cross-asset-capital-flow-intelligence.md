# Cross-asset capital-flow and stress intelligence (research implementation)

Status: deterministic research implementation, 2026-10-31; enabled by default for research. [Policy](../../../crates/trady-policy/src/intelligence/cross_asset.rs) aligns sparse publication-aware series, computes compatible beta/correlation with uncertainty, separates primary flows from performance-driven AUM changes, and reports bounded stress/breadth context. Approved normalized inputs and explicitly reviewed contextual entities use the [shared service](./implementation-report.md). Feed provisioning and historical calibration remain separate gates.

## What is new

The current [financial-data integration](../free-financial-data.md) covers macro series and some market context; [weekly optimization](../weekly-optimization.md) mentions cross-asset momentum. This extension constructs a **separate, time-aligned cross-asset state/flow explanation**: ETF creation/redemption flows where licensed, rates and yield curves, credit risk, equity sector relative returns, commodities and FX. The goal is context on risk appetite and funding pressure **without claiming that money moves mechanically from one sector to another**.

Critical accounting distinction: price appreciation does not require equivalent cash inflow; ETF secondary-market trading does not itself imply net primary-market creations; buying and selling sides of every transaction balance. A fall in one sector does not prove the capital went into another sector.

## Data classes and contracts

- ETF fund flows and shares outstanding at publish date, issuer/administrator/source and pricing basis. Identify **primary** fund creations/redemptions separately from exchange traded volume and estimated flow proxies.
- Sector/industry ETF adjusted total returns, constituents and weights as actually effective; avoid present-day constituent reconstruction.
- Government bond yields/term spreads, credit index or ETF yields/spreads, FX spot/rate differential and liquid commodity benchmarks, each with venue, source, observation and revision/vintage. FRED series is context, not a synchronous tick feed.
- Optional fund holdings (reported delayed, frequency/licensing) may improve sector exposure estimation, but holdings disclosure ≠ live capital allocation.

~~~json
{
  "schema_version": 1,
  "as_of": "2026-10-30T20:00:00Z",
  "available_at": "2026-10-31T06:00:00Z",
  "region": "US",
  "series": [
    {"id": "sector_etf_example", "kind": "total_return", "value": "-0.004", "period": "1d", "unit": "fraction"},
    {"id": "treasury_curve_example", "kind": "yield_spread", "value": "0.002", "unit": "fraction"},
    {"id": "fund_flow_example", "kind": "primary_net_flow", "value": null, "unit": "USD"}
  ],
  "rights_id": "approved-source-set-v1"
}
~~~

Synthetic data. Different frequencies cannot be combined in a single instantaneous timestamp without a documented last-known-value policy and age limits.

## Feature construction and method

1. Normalize assets to currency, adjustment basis, session calendar and venue; use rolling-availability join, never future macro revisions.
2. Compute relative-sector total return and breadth against market-cap-weighted/index baselines; keep sector leadership separate from claimed capital flow.
3. Estimate realized beta/correlation and short-horizon regime changes using trailing data; uncertainty rises with short samples or structural breaks. Reject singular covariance and overfit regime labels.
4. Where primary fund flows are available, compute net creation/redemption relative to assets and compare with returns and changes in fund size. Explicitly distinguish performance-driven assets-under-management change.
5. Build stress features from credit spread/yield movement, FX shocks, liquidity proxies and cross-market volatility. Give each series its own timestamp and reliability rather than one magic "risk-on" number.
6. Produce CrossAssetRegimeAssessment with sparse feature vector, contributor explanations, stale-data masks, vintage, confidence interval and effective regime horizon. Default to descriptive output, not directional prediction.

## Architecture

trady-adapters implements optional fund-flow and licensed benchmark readers; reuse FRED/price adapters when suitable. trady-application::cross_asset joins time-specific observations. trady-policy::cross_asset_features creates deterministic features with bounded numerical methods. Analyst tool macro.cross_asset_context returns provenance-aware state; Perpetual or other fusion may receive only model-approved and historically validated features. No asset other than an allowed ordinary stock becomes tradable. Crypto, bonds and ETFs remain **context only** unless a separate future owner-approved change is made to v1.

## Experimental evaluation

Compare Trady's existing ATR/market-regime baseline with yield/credit only, ETF-relative-performance only, verified primary fund flow only, and the combined model. Use publication-lag-aware historical samples, rolling windows, nested chronological tests, and regimes including high-rate and low-liquidity transitions. Evaluate incremental conditional return forecast, Brier/log loss, downside-tail calibration, trading costs and source budget. Check whether a new feature merely restates the stock's own recent momentum; do not count redundant signals twice.

## Risks and acceptance

- ETF quote volume is **not** primary fund flow; reject a field with the wrong semantics.
- A declining bond yield is not universally bullish and not a deterministic equity BUY.
- Delayed flow publication -> unavailable before publication, even if its reporting period ended earlier.
- Currency/timezone/holiday mismatch -> no silent forward fill beyond approved freshness.
- Missing optional series -> degrade with explicit sparse mask; required series absent -> ABSTAIN.
- Stress risk may request fewer *new* entries through a reviewed policy but cannot loosen a holding stop or replace native protection.

## References and dependencies

[FRED/ALFRED observations and vintage handling](https://fred.stlouisfed.org/docs/api/fred/series_observations.html); [Trady's data source catalog](../free-financial-api-catalog.md); [weekly evaluation design](../weekly-optimization.md). ETF primary-flow and credit data vendors require separate procurement and licensing review; no endpoint is implied by this document.

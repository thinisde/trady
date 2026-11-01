# Short-squeeze exposure and crowding intelligence (research implementation)

Status: deterministic research implementation, 2026-10-31; enabled by default for research. [Policy](../../../crates/trady-policy/src/intelligence/short_interest.rs) selects publication-aware position vintages, calculates float ratios and frozen-volume days-to-cover, and reports optional licensed borrow context. It rejects daily short-sale volume as a short-interest substitute. Approved normalized inputs use the [shared service](./implementation-report.md); live subscriptions and calibration remain separate gates. Trady remains long-only.

## New research capability

Use public reported short interest, liquidity and optional licensed borrow-market data to describe whether a candidate equity is exposed to **crowded positioning and difficult exits**. This is not present as a standalone plan in [quantitative prediction](../prediction-fusion.md). A squeeze forecast is necessarily uncertain: reported short positions are snapshots, covering behavior is not directly observable, and high short interest may reflect justified negative fundamentals.

## Eligible data and limitations

- **Reported short interest:** shares short as of the reporting settlement date, date published and source/exchange. As of the referenced FINRA guidance, firms report on a periodic twice-monthly schedule; re-check rules/calendar before deployment. Record publication lag and **do not** call daily short-sale volume short interest.
- **Shares outstanding and free float:** provider method, effective date, exclusions, currency and corporate actions. Free-float estimates vary; attach source version.
- **Market volume/liquidity:** completed consolidated or clearly venue-scoped volume, spread, turnover, halt history, accessible liquidity, availability time.
- **Borrow-market context (optional):** utilization, indicative borrow fee, shares-on-loan, hard-to-borrow status and recall/locate constraints only from explicitly licensed feeds. These are not guaranteed to reflect every broker or short seller.
- **Options pressure (optional):** link [options expectations](./options-market-intelligence.md) only when chain coverage/right are known; do not infer dealer gamma inventory from public chains alone.

## Contract and formulas

Proposed ShortCrowdingObservation fields include issuer, as_of_settlement, published_at, ingested_at, short_shares, float_shares, average_daily_volume_window, borrow_fee, float methodology, source id, adjustment and release vintage.

Candidate features:

1. Short-interest percentage of float = reported short shares / verified float shares, with unit and denominator documented. Missing float => no ratio.
2. **Days to cover (DTC)** = reported short shares / average daily shares traded across a frozen lookback. This is a volume-based *heuristic*, not the number of days shorts must close; it can change rapidly with volume.
3. Change in reported positions over **publication-aware** intervals; no interpolation of hidden daily short interest.
4. Relative gap/volume anomaly and historical volatility state. High volatility may signal risk rather than opportunity.
5. Optional borrow-fee percentile and dispersion if the same licensed methodology is available historically.

~~~json
{
  "schema_version": 1,
  "issuer_id": "issuer_42",
  "short_interest": {
    "settlement_date": "2026-10-15",
    "published_at": "2026-10-26T20:00:00Z",
    "shares_short": "6000000"
  },
  "float": {"shares": "30000000", "effective_date": "2026-10-15"},
  "avg_volume_shares_20d": "1500000",
  "borrow_fee_annualized": null,
  "features": {"short_interest_to_float": "0.20", "days_to_cover": "4.0"},
  "rights_id": "source-revision"
}
~~~

Synthetic fixture. Financial ratios use checked decimal calculations in Rust; do not let floating conversions silently round shares.

## System design

trady-adapters: ReportedShortInterestSource, FloatSource, optional BorrowSource; reuse [financial-data registry](../free-financial-data.md). trady-domain: provenance-typed ShortCrowdingEvidence. trady-application: point-in-time availability, split-adjustment and publication-aware history. trady-policy: pure risk flags, never independent confirmed-catalyst points. Read-only analyst tool short.crowding returns levels, publication lag, uncertainty and bounded explanatory factors.

## Decisions and risk

For a short-only-candidate signal, **no entry** may occur without existing research and forecast gates. For a long holding, squeeze-related rapid price reversals can trigger a request for fresh analysis, but the executor keeps original native protection and limits. Do not automatically raise targets from crowding alone; do not extend five-session holding deadline. Explicitly distinguish a squeeze from a large short interest value.

## Validation

Test different float vendors, share splits, borrow-provider survivorship, publication lag, report corrections and volatility regimes. Train any probabilistic outcome model against original historical publication dates; benchmark momentum/volatility-only models. Measure realized large-move risk calibration, precision, false alerts, after-cost outcomes and missing-coverage bias. Require event-level holdouts to avoid oversampling persistent crowded names.

## Acceptance and failure modes

- Daily short-sale volume substituted for published outstanding short positions -> hard validation error.
- Float <= 0, shares short > plausible issued shares, unknown adjustment -> review/UNAVAILABLE, not silent clipping.
- Zero average volume -> unknown DTC; never infinity interpreted as a score.
- Delayed report arrives after price spike -> cannot explain or predict earlier move in historical replay.
- Borrow fee absent -> absent, no imputation from share volume.
- No additional broker permissions or live promotion. Deploy as paper/shadow research feature only after licensing and prospective evidence.

## References

[FINRA short interest reporting](https://www.finra.org/filing-reporting/regulatory-filing-systems/short-interest); [FINRA distinction between short interest and short-sale volume](https://www.finra.org/sites/default/files/2019-07/information-notice-051019.pdf); [FINRA investor short interest explainer](https://syndication.finra.org/content/short-interest-what-it-what-it-not).

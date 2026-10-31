# Financial statement anomaly and earnings-quality detector (proposed)

Status: new feature design, 2026-10-31. Documentation only; financial-fact collection is separately planned, but no cross-period forensic accounting assessment is implemented by this proposal.

## Purpose and distinct scope

The existing [company intelligence](../company-intelligence.md) and [financial data layer](../free-financial-data.md) plan filing extraction, ratios and issuer research. This extension computes **accounting-consistency anomalies and earnings-quality trends**, not a generic balance-sheet summary. It should flag statements meriting verification, never accuse an issuer of fraud or infer future stock prices directly from an unusual ratio.

Questions: Is reported revenue growing faster than collections? Does net income repeatedly exceed operating cash flow after normal working-capital patterns? Are receivables, inventory or accrued revenue changing faster than comparable industry peers? Has share count diluted owners despite earnings growth? Were prior periods restated or unusual nonrecurring items large?

## Data acquisition and semantics

Use SEC EDGAR company facts, filings and original 10-K/10-Q/8-K disclosures with filing accession, fiscal period, taxonomy tag, units, filing timestamp, acceptance/public availability, amended/latest-as-of lineage and company-specific extensions. The public **companyfacts XBRL** API covers supported financial facts but does not resolve every note or proprietary taxonomy item; for complex facts use linked primary filing excerpts with manual validation.

Reconcile standardized values by **economic concept + label + unit + period + consolidated scope**. Beware cumulative year-to-date cash flow vs quarter-only earnings, fiscal 53-week years, acquisitions/disposals, discontinued operations, banking/insurance accounting, and GAAP/non-GAAP mismatches. Do not calculate an anomaly if comparable semantics cannot be established.

## Candidate mathematical features

- **Accruals proxy:** (net income − operating cash flow) / average assets, only for aligned periods/units, and industry-appropriate interpretation. An alternative working-capital accrual methodology must be separately versioned; don't blend them.
- **Receivables growth gap:** year-over-year percentage change in trade receivables minus revenue growth, provided denominator and acquisitions/currency impact are valid.
- **Cash conversion:** operating cash flow / net income where denominator is materially positive; mark negative/near-zero net income as unsuitable, not extreme score.
- **Inventory-revenue divergence:** inventory change relative to sales growth for relevant product businesses; inventory is not comparable across financial institutions or software providers.
- **Share dilution:** change in split-adjusted diluted weighted-average shares or actual outstanding shares, with explicit denominator and share-based compensation/exercise context. Weighted-average shares and end-period outstanding are distinct.
- **Restatement history:** as-of-publication amendment events and changes in reported amounts; do not assume an amendment is a fraud signal.
- **One-offs:** impairment, restructuring, pension and acquisition items extracted with uncertainty and classification tags; primary disclosure is preferable to LLM speculation.

The initial scoring policy should use sector/industry-matched robust empirical ranks, not hand-tuned universal thresholds. Indicators can conflict and are not independent likelihood ratios.

## Typed contract

~~~json
{
  "schema_version": 1,
  "issuer_id": "issuer_42",
  "fiscal_period": "2026-Q3",
  "comparison_period": "2025-Q3",
  "filing_accession": "synthetic-accession",
  "available_at": "2026-10-30T21:00:00Z",
  "features": [
    {"name": "accruals_to_avg_assets", "value": "0.07", "quality": "COMPARABLE"},
    {"name": "receivables_vs_revenue_growth", "value": null, "quality": "UNRESOLVED_ACQUISITION"}
  ],
  "flag": "REQUIRES_REVIEW",
  "version": "earnings-quality-v1",
  "rights_id": "sec-source-review"
}
~~~

Synthetic fixture; null cannot be filled with a guessed historical value. Store original fact ids, formulas, crosswalk revision, source text offsets and comparison cohort.

## Architecture and reporting

trady-adapters reuses SEC EDGAR transport; trady-domain adds ComparableFinancialFact, AnomalySignal and EarningsQualityAssessment. trady-application::accounting provides period mapping/issuer cohort selection, audit links and invalidations. trady-policy::accounting computes checked decimal ratios; LLM does not generate accounting numbers. A read-only analyst tool issuer.earnings_quality returns flags with fact-to-filing evidence, counterexamples and missingness. Feed a calibrated feature into [prediction fusion](../prediction-fusion.md) **only after** incremental value and time-horizon relevance are established. Restatements should update research but cannot retroactively update the information used at a past trade.

## Testing and scientific validation

Evaluate against baselines of simple profitability, debt, momentum and sector-neutral ratios. Use point-in-time forms, preserve delisted issuers, train/validate chronologically, and cluster uncertainty by issuer/industry to avoid overstating sample size. Distinguish long-horizon accounting quality from Trady's current intraday-to-five-session holding horizon; test the latter explicitly rather than assuming a slow anomaly predicts tomorrow's return. Report false alarms from M&A/seasonality, coverage by industry, calibration for later restatements, and incremental net return. Test adversarial accounting units and duplicate tags.

## Acceptance and safe behavior

- Different cash flow periods / missing taxonomy mapping -> abstain on ratio.
- Corrected/restated figures -> append revision and invalidate derived future assessments; historical replay uses original available filing.
- Low/nonpositive denominator -> NA with reason, not extreme-risk score.
- Bank vs manufacturer cohort -> separate industry model, not shared threshold.
- Single anomalous ratio -> research flag only; no immediate SELL, stop change or assumption of fraud.
- Missing original filing or unsupported source rights -> read-only flag unavailable, existing protection continues.
- Build deterministic fixtures for 53-week fiscal year, 10-Q YTD cash flow, revised 10-K, currency change, acquisition and stock split.

## References and neighboring plans

[SEC EDGAR data APIs](https://www.sec.gov/search-filings/edgar-application-programming-interfaces); [SEC technical specifications](https://www.sec.gov/submit-filings/technical-specifications); [existing company data design](../free-financial-data.md). Any future classification thresholds require prospective evaluation and independent deployment approval.

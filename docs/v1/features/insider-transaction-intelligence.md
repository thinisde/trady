# Insider transaction intelligence (research implementation)

Status: deterministic research implementation, 2026-10-31; enabled by default for research. The [bounded SEC ownership XML parser](../../../crates/trady-adapters/src/intelligence/ownership.rs) normalizes Forms 3/4/5 and amendments. [Policy](../../../crates/trady-policy/src/intelligence/insider.rs) separates reviewed cash transactions from awards/exercises/withholding, deduplicates exact amendment lineages and evaluates distinct-owner purchase clusters. The [shared service](./implementation-report.md) provides approved normalized ingestion and replay. Scheduled SEC collection, notifications and predictive calibration remain separate work; generic SEC access is described in [financial data](../free-financial-data.md).

## Objective

Research whether directors, officers or significant holders have made **discretionary open-market purchases/sales**, and whether those activities provide incremental evidence for a company's longer-horizon fundamentals. Exclude treating an insider trade as guaranteed knowledge of future results or proof of misconduct. Insider activity is often a weak short-horizon signal, so the feature may be useful mainly for screening and risk context.

## Official source and parsing

Use SEC EDGAR Section 16 ownership filings (Forms 3/4/5 and amendments), respecting SEC fair access, structured ownership XML and publication time. **Do not** equate EDGAR companyfacts XBRL with Form 4 transaction records. SEC historical insider transaction datasets are quarterly snapshots and may not be adequate for timely signals; separate historical backfills from live filings monitoring.

The parser must resolve issuer CIK, security/class, reporting-owner identifiers, position title, direct/indirect holdings, non-derivative vs derivative tables, transaction codes, amounts/prices, footnotes, filing datetime, transaction dates, amendments and any plan-related disclosure flags/footnotes. Recognize Form 4 codes such as **P** purchase and **S** sale, plus M exercises and F tax-withholding; do not classify all acquisitions as open-market buys. Transaction reporting latency and possible exemptions require a missing-coverage state.

## Data record and event model

~~~json
{
  "schema_version": 1,
  "issuer_cik": "0000000000",
  "filing_accession": "example-accession",
  "filed_at": "2026-10-30T20:00:00Z",
  "transaction_date": "2026-10-28",
  "reporting_owner_id": "issuer-scoped-pseudonym",
  "security_type": "common",
  "transaction_code": "P",
  "shares": "500",
  "price": "100.00",
  "direct_or_indirect": "direct",
  "plan_annotation": "UNKNOWN",
  "amendment_of": null,
  "rights_id": "sec-public-data-review"
}
~~~

Synthetic fixture. Use immutable accession + transaction-line identity rather than row index alone; multiple rows can represent the same economic transaction. Keep links to original exhibits and footnotes. Preserve any as-filed corrections.

## Research transformations

1. Normalize transaction type: discretionary-looking cash purchase, scheduled/plan-described transaction, option exercise, award, tax/withholding, gift, derivative or unknown. **Never infer intent solely from code or form checkbox**; footnotes can change classification.
2. Compute net open-market buying/selling over independently defined windows with price and splits adjusted, but keep purchases and sales as **separate** totals; offsetting transactions need not represent a neutral decision.
3. Identify issuer-level purchase clusters from **distinct** reporting persons within a predeclared interval, excluding duplicates/amendments and trivial recurring purchases. Cluster count is not source corroboration for an external news claim.
4. Scale dollar value relative to approximate historical owner transactions where permitted; position context is often incomplete, so do not calculate "fraction of personal net worth".
5. Join issuer financial fundamentals and price response with publication-time alignment. Transaction date and filing date differ; only filing availability can inform a public backtest.

Optional InsiderActivityAssessment: reporting period, qualifying event count, value ranges, role breadth, transaction type, dilution/awards caveat, public-source linkage, sampling bias and UNKNOWN flags.

## Trady integration

Extend the planned SEC collector with a new OwnershipFilingParser and InsiderPatternService inside the analyst's Rust process; use [memory and data](../memory-and-data.md) to persist original and amended statements. Add **read-only** tool issuer.insider_activity, bounded by issuer/time/rights. The LLM may summarize purchased shares and verified roles; it cannot claim insider knowledge, change financial limits or count filings as independent bullish news origins. If empirically justified after validation, the fusion engine can consume a versioned low-weight feature with separately calibrated outcomes.

## Experiments

Test on stocks eligible at the original filing time, not today's S&P 500 survivors. Compare open-market buy-only, all-insider, and null baselines; measure incremental future returns and adverse outcomes at the existing 1–5-session holding horizon as well as slower horizons, explicitly marking mismatch. Avoid using post-filing prices/updated footnotes as features. Include issuer/industry fixed effects, blackout/earnings timing, acquisitions and same-owner repeats. Report calibration, coverage, transaction-code confusion and after-cost results.

## Safeguards and acceptance

- Form 4 exercise (M) or tax withholding (F) never counted as an ordinary cash purchase.
- Amended filings supersede only the applicable revision; historic forecasts retain what was knowable then.
- Insider reported trade date before filing is **not** available to earlier backtests.
- Foreign issuers or exempt insiders may have different coverage; missing filings do not prove zero trades.
- Plan annotation UNKNOWN -> remain uncertain.
- Owner-facing interface cannot issue defamation or insider-trading accusations. No automated trades from a filing alone.

## References

[SEC insider transactions datasets](https://www.sec.gov/data-research/sec-markets-data/insider-transactions-data-sets); [SEC ownership XML technical specifications](https://www.sec.gov/submit-filings/technical-specifications); [Investor.gov Forms 3/4/5 overview](https://www.investor.gov/introduction-investing/general-resources/news-alerts/alerts-bulletins/investor-bulletins-69).

# Stock research reports

`trady research-report FILE [--as-of RFC3339]` generates an offline report for
moderate-risk US/EU research over a declared 6–12 month horizon. The input contains
1–100 unique symbols, dated financial references, theses, catalysts and risks.
Run the [dated researched input](./research-us-eu-2026-10-31.input.json):

```sh
cargo run --locked --bin trady -- research-report docs/research-us-eu-2026-10-31.input.json --as-of 2026-10-31T23:59:59Z
```

The strict version-1 JSON input uses `schema_version`, `horizon_months` and
`candidates`. Each candidate has `symbol`, `company`, `region` (`US` or `EU`),
`sector`, `currency`, `financials`, optional `quote` and `trailing_eps`, `thesis`,
`catalysts` and nonempty `risks`. The input file is limited to 1 MiB. Unknown
fields, duplicate JSON keys, duplicate symbols and invalid dates are rejected.

Financials identify `period_start`, `period_end`, `published_at`, `source_url`,
`revenue_millions` and optional `prior_year_revenue_millions`,
`operating_income_millions`, `free_cash_flow_millions`. Monetary values are decimal
strings in the candidate's reporting currency. All current-period financial
amounts must cover the same period and units. Prior-year revenue must cover its
corresponding comparable period. Operating income means the statutory reported
amount; do not silently substitute adjusted or business profit. Free cash flow
can use an issuer's identified non-GAAP measure, or operating cash flow minus
cash capital expenditures; document the definition in the research record.

The report calculates revenue growth, operating margin and free-cash-flow margin.
Financial evidence is stale when publication is over 120 days old or the period
ended over 180 days before the cutoff. Negative growth, nonpositive operating
income or nonpositive free cash flow produces `CAUTION`; missing figures produce
`INCOMPLETE`; complete, positive, current metrics produce `SUPPORTED`. These are
data-screen results and do not measure a stock's expected return or suitability.

An optional quote supplies `price`, `currency`, `observed_at`, `source_url`.
Optional `trailing_eps` supplies `diluted_eps`, `currency`, `period_start`,
`period_end`, `published_at`, `source_url`. Trailing P/E is calculated only from
a positive, current full-year diluted EPS and a quote no older than four calendar
days, with matching currency. Quarterly EPS is not annualized. Future observations
or publications and currency mismatches are rejected; missing or stale valuation
inputs remain visible and produce no P/E.

Output preserves the supplied HTTPS source links and records
`source_verification: SUPPLIED_REFERENCES`: the generator validates the input but
does not fetch or attest to the contents of those references. Keep the research
record alongside the generated JSON. `valuation_review_required` remains true,
even when P/E is available. No target price, order proposal or entry approval is
created. The generator makes no network or database calls.

Verification: `cargo test -p trady-application stock_report --lib --locked`.


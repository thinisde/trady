# US/EU stock research — 31 October 2026

Research profile: moderate risk, 6–12 months, with review dates from **30 April to 31 October 2027**. This is a focused shortlist of liquid companies, not a full-market ranking. Every company source below was published by the 31 October 2026 cutoff.

**Research priorities: Visa and SAP for business quality; Microsoft with a capital-spending condition. Schneider Electric and Sanofi merit follow-up, with incomplete financial inputs in this report.** These priorities are an inference from the evidence, not an assessment that the shares are attractively priced today. Visa's verified closing quote implies a premium earnings multiple; the other candidates still lack complete valuation inputs. No orders or proposals were created.

## Market conditions

The Federal Reserve held its target range at 3.75–4.00% on 28 October and described inflation as elevated. The ECB held rates on 29 October and highlighted higher energy prices and risks to growth. My inference is to favor durable cash generation and pricing power, and to require valuation headroom rather than assume falling rates will lift multiples. Sources: [Federal Reserve statement](https://www.federalreserve.gov/newsevents/pressreleases/monetary20261028a.htm), [ECB statement](https://www.ecb.europa.eu/press/press_conference/monetary-policy-statement/2026/html/ecb.is261029~5c5ffe99e5.en.html).

## Candidates and conditions

| Company | Evidence available at the cutoff | 6–12 month research case | Main risk / next check |
| --- | --- | --- | --- |
| **Visa (V, US)** | FY2026 net revenue $45.903bn versus $40.000bn; operating income $28.110bn. Calculated cash FCF $22.057bn. | Payment-network economics and cash generation; watch payment and cross-border volume through fiscal 2027. | Regulation, litigation and spending sensitivity. At 33.57× trailing EPS, the valuation premium requires review. [Earnings, 27 Oct](https://www.sec.gov/Archives/edgar/data/1403161/000140316126000126/q42026earningsrelease.htm), [issuer quote, 30 Oct](https://investor.visa.com/stock-information/quote-chart/default.aspx). |
| **SAP (SAP, EU)** | Q3 revenue €10.270bn; IFRS operating profit €2.930bn; issuer FCF €1.718bn. Current cloud backlog €24.144bn; approximately €10bn FY FCF outlook. | Recurring enterprise demand and backlog conversion; verify continued cash delivery. | Premium valuation, cloud transition and infrastructure costs. Check current share price, full-year diluted earnings and execution against guidance. [SAP statement, 21 Oct](https://www.sap.com/docs/download/investors/2026/sap-2026-q3-statement.pdf). |
| **Microsoft (MSFT, US)** | Quarter revenue $92.279bn; operating income $45.915bn. Operating cash flow $58.000bn less cash capex $39.367bn gives $18.633bn cash FCF. | Enterprise/cloud demand supports business quality; upside depends on monetizing infrastructure investment. | Cash capex rose from $19.394bn a year earlier. Track cash conversion and depreciation rather than extrapolate revenue growth. [10-Q, 28 Oct](https://www.sec.gov/Archives/edgar/data/789019/000119312526436133/msft-20260930.htm). |
| **Schneider Electric (SU.PA, EU)** | Q3 revenue €11.460bn, up 16.9% organically; upgraded 2026 target. The Q3 release does not provide matching quarterly profit and FCF inputs for this report. | Electrical distribution and automation demand; examine conversion of data-center orders into cash-generating revenue. | Shares also depend on the AI/capital-investment cycle. Obtain current valuation, cash conversion and leverage before upgrading conviction. [Company release, 16 Oct](https://www.se.com/ww/en/assets/pdf/release-Q3-revenues-2026). |
| **Sanofi (SAN.PA, EU)** | Q3 sales €14.444bn and issuer FCF €3.269bn; sales grew 14.3% at constant currency and 16.2% reported. Guidance raised. | Medicine demand provides an alternative to the AI investment theme; monitor Dupixent and launches. | Product concentration, clinical/patent and reimbursement risks. Complete the statutory profit comparison and valuation; business EPS is not IFRS EPS. [Company release, 30 Oct](https://www.sanofi.com/en/media-room/press-releases/2026/2026-10-30-06-30-00-3392028). |

Three of the five names—Microsoft, SAP and Schneider—share exposure to enterprise/AI capital spending. This shortlist is not a diversified portfolio allocation. US holdings also introduce EUR/USD exposure for a euro-based account.

Amazon was deprioritized for this moderate-risk screen because its Q3 release reported trailing cash FCF of **negative $42.4bn** despite strong operating growth. IQVIA was deprioritized because its release reported **3.45× net leverage**. Sources: [Amazon results, 29 Oct](https://ir.aboutamazon.com/news-release/news-release-details/2026/Amazon-com-Announces-Third-Quarter-Results/), [IQVIA results, 27 Oct](https://ir.iqvia.com/press-releases/press-release-details/2026/IQVIA-Reports-Third-Quarter-2026-Results/default.aspx).

## Reproducible calculations

The new report generator produced these results from the [dated input](./research-us-eu-2026-10-31.input.json); its full output is [saved as JSON](./research-us-eu-2026-10-31.json).

| Company / financial period | Calculated reported revenue growth | Operating margin | Cash FCF margin | Data-screen result |
| --- | ---: | ---: | ---: | --- |
| Visa / FY ended 30 Sep 2026 | 14.7575% | 61.2378% | 48.0513% | SUPPORTED |
| SAP / Q3 2026 | 13.1556% | 28.5297% | 16.7283% | SUPPORTED |
| Microsoft / quarter ended 30 Sep 2026 | 18.8045% | 49.7567% | 20.1920% | SUPPORTED |
| Schneider / Q3 2026 | Not calculated | Unavailable | Unavailable | INCOMPLETE |
| Sanofi / Q3 2026 | Not calculated | Unavailable | 22.6322% | INCOMPLETE |

Revenue growth = (current / prior − 1) × 100; margin = same-period income or cash FCF / revenue × 100. The generator did not back-solve prior-year revenue from rounded growth percentages. Visa's comparison is annual; the other rows are quarterly. Cash-flow definitions also differ: Visa and Microsoft use operating cash flow minus cash capex; SAP and Sanofi use their identified non-GAAP/IFRS FCF measures. Cash calculations do not deduct all possible lease-financed investment obligations.

Visa's issuer quote shows a **$413.59 close on 30 October 2026**. With FY2026 GAAP diluted EPS of **$12.32**, the generator calculates **33.5706× trailing P/E**, equivalent to a simple earnings yield of **2.9788%**. That premium makes valuation discipline central to the thesis. Sources: [issuer quote](https://investor.visa.com/stock-information/quote-chart/default.aspx), [annual earnings](https://www.sec.gov/Archives/edgar/data/1403161/000140316126000126/q42026earningsrelease.htm).

Arbitrary P/E assumptions of **20×, 25× and 30×** imply **$246.40, $308.00 and $369.60** respectively at unchanged EPS. This is a multiple-sensitivity calculation, not a fair-value estimate or a price target. The other four names retain missing valuation inputs; no expected returns were generated. Secondary SAP price pages agreed on a closing price, but no dated price was extracted from the issuer/exchange pages, so that quote was not promoted into the report input.

All five rows have `valuation_review_required=true`, and all report-level and candidate-level `entry_authorized` values are false. Visa's P/E does not complete its investment case: leverage, expected earnings durability and upcoming events still require review. Complete missing quotes and trailing earnings for the other names before reassessing valuation and entry conditions.

## Search provenance and limits

Exa search covered three workstreams: macro conditions, broad company discovery, and company financial/risk validation. The 11 searches requested 65 result slots, with duplicate URLs and secondary summaries removed before selecting the primary releases and issuer quote above. The final report relies on central-bank statements, issuer releases and SEC financial statements, not stock-pick listicles. Search coverage is a focused research pass rather than an exhaustive market screen.

The generator validates supplied references and dates; it does not independently fetch or attest to source contents. No licensed API payload was persisted or sent to a model for this report. Source facts were researched from the linked public releases. The [stock report guide](./stock-reports.md) describes validation and missing-data behavior.


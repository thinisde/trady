# Research guide sources and limitations

Checked 2026-10-31. These links support taxonomy, available measurements, disclosure channels and API capabilities. Guide scoring, peer comparisons and rejection rules are **Trady design choices**, not trading advice issued or validated by the linked agencies. Historical examples are identity/materiality context, not current catalysts.

## Taxonomy and general evidence

| Primary source | Use in the guides | Limit to preserve |
|---|---|---|
| [S&P GICS methodology](https://www.spglobal.com/spdji/en/documents/methodologies/methodology-gics.pdf) / [MSCI GICS](https://www.msci.com/indexes/index-resources/gics) | Principal-business classification and hierarchy; 11 sectors/25 industry groups | Pin effective edition and obtain permitted per-company classification data |
| [S&P 500](https://www.spglobal.com/spdji/en/indices/equity/sp-500/) | Index definition and constituent changes | Company count is not a fixed count of tradeable share classes |
| [SEC EDGAR APIs](https://www.sec.gov/search-filings/edgar-application-programming-interfaces) | Filed submissions, company facts, identifiers and original disclosures | Filed facts are period/unit/context dependent; a filing is not independent reporting |
| [Census economic indicators](https://www.census.gov/economic-indicators/) | Retail, manufacturing orders, inventories, services, construction/housing | Nominal, revised and often seasonally adjusted; actual release/observation dates differ |
| [Federal Reserve data](https://www.federalreserve.gov/data.htm) | Rates, credit, bank balance sheets and lending surveys | Surveys and aggregate data are not same-day issuer results |
| [BEA income/outlays information](https://www.bea.gov/news/pio-release-additional-information) | Spending, disposable income, real/nominal distinction | Price, quantity, annualized rates and revisions must remain distinguishable |

## Sector measurement sources

| Sector/group | Original source | Concrete question |
|---|---|---|
| Energy | [EIA Weekly Petroleum Status Report](https://www.eia.gov/petroleum/supply/weekly/) and [natural gas weekly](https://www.eia.gov/naturalgas/weekly/) | Which inventories, product supply, refinery utilization or gas balances changed? |
| Materials | [USGS commodity statistics](https://www.usgs.gov/centers/national-minerals-information-center/commodity-statistics-and-information) | What is actually known about production, supply and mineral exposure? Annual context is not an intraday quote. |
| Industrials/capital goods | [Federal Reserve G.17](https://www.federalreserve.gov/releases/g17/current/) and [Census manufacturing](https://www.census.gov/manufacturing/m3/) | Do orders, production, backlog conversion and capacity support the claim? |
| Transportation | [BTS air-traffic methodology](https://www.bts.gov/browse-statistical-products-and-data/statistical-products/air-traffic-methodology) | Is traffic observed, forecast/revised, seasonally adjusted and economically profitable? |
| Consumer durables/retail/real estate | [Census indicators](https://www.census.gov/economic-indicators/) and [BEA PCE](https://www.bea.gov/data/consumer-spending) | Is sales growth price or volume, and is demand supported by actual orders/sales? |
| Food/input-sensitive businesses | [USDA WASDE](https://www.usda.gov/about-usda/general-information/staff-offices/office-chief-economist/commodity-markets/wasde-report) | What changed in agricultural supply/use assumptions and which business bears the input cost? |
| Drugs/biotechnology | [FDA development process](https://www.fda.gov/patients/learn-about-drug-and-device-approvals/drug-development-process) and [ClinicalTrials.gov](https://clinicaltrials.gov/) | What stage, registered endpoint, comparator, safety signal and regulatory decision exist? Registration is not independent efficacy proof. |
| Health equipment/services | [CMS health-expenditure data](https://www.cms.gov/data-research/statistics-trends-and-reports/national-health-expenditure-data) and [payment-rule notices](https://www.cms.gov/medicare/payment/fee-schedules/physician/federal-regulation-notices) | Who receives revenue and who bears reimbursement/utilization costs? Proposed rules differ from final effective rules. |
| Banks | [FDIC Quarterly Banking Profile](https://www.fdic.gov/quarterly-banking-profile) / [Fed lending survey](https://www.federalreserve.gov/data/sloos.htm) | Are deposit costs, asset quality, loan demand, liquidity and capital improving together? |
| Insurance | [NAIC snapshots and analysis](https://content.naic.org/industry/insurance-industry-snapshots-analysis-reports) | Does underwriting improve separately from investment gains and reserve changes? |
| Technology/semiconductors | [BIS controls](https://www.bis.gov/regulations/ear/744) and issuer/customer filings | Which products/markets are actually affected by a current restriction or customer order? Rules need effective-date verification. |
| Communication services | [FCC reports and research](https://www.fcc.gov/reports-research) and issuer disclosures | What do network, subscription, advertising and competition facts show? |
| Utilities | [EIA Electricity Monthly](https://www.eia.gov/electricity/monthly/) and [FERC reports](https://www.ferc.gov/staff-reports-and-papers) | Does demand translate into regulated returns or merchant margins after fuel, capital and reliability costs? |
| Equity REITs | [Nareit FFO guidance](https://www.reit.com/nareit/advocacy/policy/nareit-ffo-white-paper-and-related-implementation) and property-owner filings | Are comparable FFO, rents, occupancy and refinancing economics supported? FFO is supplemental, not cash flow. |

## Research method and coverage

Research used direct official-page reads, Context7's X API documentation and Exa discovery across taxonomy, economic/regulatory sources, five named people and identity/capability follow-ups. Exa requests covered 90 result slots across 17 searches; slots include duplicates, irrelevant returns and failed restrictions, and are not a count of unique sources read. Only inspected, relevant primary pages inform the specification. X pages unavailable through Exa fetch were read from the official documentation host; a blocked personal-site request was not treated as successful access.

Each topic guide cites the source and adds original analytical questions. Some direct SEC/FCC requests were access-limited and a Census request returned an error; source documentation/indexed official content does not establish a working runtime collector. No complete current constituent dataset, live entitlement, real-time feed or empirical five-person influence ranking was obtained. These become runtime inputs/checks. Account ownership and unresolved handle verification are explicit in [X listeners](./watchdogs-and-x.md); the final five names/priorities are the owner's preference.

Publication, observation and retrieval times remain distinct. Slow official statistics provide context; only newly released data, disclosures or verified current changes create a fresh catalyst. Search snippets, publisher syndication and a person's repost cannot supply independent confirmation of the underlying claim.

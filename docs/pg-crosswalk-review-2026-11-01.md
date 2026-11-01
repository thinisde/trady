# PG identity crosswalk review

Status: **REQUESTED_MAPPING_REJECTED_CORRECT_BROKER_ID_IDENTIFIED**.

The requested historical key remains `PG_US_EQ`; the requested broker key `PGC_US_EQ` is rejected. Public issuer/SEC records match ticker `PG`, venue `XNYS` (New York Stock Exchange), USD, and share-class ISIN `US7427181091`. The current account-bound Trading 212 catalog contains `PG_US_EQ` as Procter & Gamble with that ISIN, while `PGC_US_EQ` is Peapack-Gladstone Financial Corp with ISIN `US7046991078`. The explicit fields are retained and no identifier was silently aliased.

The current broker metadata artifact is `var/identity/t212-demo-metadata-20261101.json` with SHA-256 `03ae2dc6133d2b47f93d511b99e4a313d79c65420f66d146c1d031825f887665`. Its broker catalog `addedOn` value is retained as catalog provenance, not treated as membership effective date. A current membership effective date and a licensed provider content hash are still missing. FMP membership access returned HTTP 402 and Massive quotes returned HTTP 403. The automated independent review rejected the requested PGC mapping and identified the exact PG_US_EQ candidate.

Public identity references: [P&G 2022 Form 10-K](https://us.pg.com/annualreport2022/2022_form_10k.pdf) and [SEC identity table](https://www.sec.gov/Archives/edgar/data/1849336/000184933626000004/xslNPX-INFO-TABLE_X01/votetable_xml.xml). These public checks do not establish deployment rights or the broker mapping.

The machine-readable record is [`pg-crosswalk-review-2026-11-01.json`](./pg-crosswalk-review-2026-11-01.json).

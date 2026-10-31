---
name: company-profile
description: >-
  Investigate a shortlisted company's business, filings, management statements,
  counterparties and risks; persist an attributed company profile before Trady entry.
metadata:
  version: "1.0.0"
---

# Company and management deep dive

Read [company intelligence](../docs/v1/company-intelligence.md). Resolve issuer, listing/share class, CIK, effective classification and official domains through approved tools. Retrieve the previous profile revision; do not merge similar names or substitute a parent for its listed subsidiary.

Read the latest annual/quarterly filings, intervening current reports, earnings release/guidance and catalyst originals. Record business segments, geographic/currency exposure, customer/supplier concentration, cash/debt/dilution and material risks. Keep fiscal periods, units, currency and GAAP/non-GAAP reconciliation with each metric. A source's company-wide outlook may not describe the affected segment.

After choosing the stock, inspect CEO/CFO and relevant product leader statements, earnings Q&A, original interviews and permitted public posts. Search the latest baseline period, all subsequent changes, at least the last 72 hours, and the catalyst's original date. Example query families: `"issuer" "product" guidance availability customer`, `"executive full name" "issuer" interview transcript`, and `"issuer" correction delay cancellation lawsuit`. Use source-specific syntax and official-domain filters; query windows must be explicit.

Separate promise, prototype, trial, approval, shipment, contract and recognized revenue. Ask who pays, how soon, at what margin, with which funding/capacity, and what could invalidate the claim. Compare independent sources and counterparties' statements. If no relevant recent executive statement exists after recorded searches, say so; do not invent one or require an unofficial personal X account.

Use [relationship analysis](./relationship-analysis.SKILL.md) for people/company links. Store `PROFILE_REVIEW` with typed fact drafts, statement claim/evidence references, baseline IDs, material gaps and expected prior revision through `research.store_artifact`. The application resolves entity IDs and validates/merges facts; your requested verification state is not authoritative. Persist rejected or inaccessible findings too, subject to rights.

Return tool-generated profile-review/revision references in `research.focus`. An incomplete material fact, unsupported executive attribution, stale baseline or material unresolved conflict blocks entry. Memory is evidence-backed context, not permission to change trading policy.

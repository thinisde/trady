---
name: source-comparison
description: >-
  Compare material Trady claims across primary and independent sources, detect
  copied reporting, resolve factual differences and retain entry-blocking contradictions.
metadata:
  version: "1.0.0"
---

# Source comparison

## Compare claims, not headline counts

Start from the claim cards and their original items. Build a matrix with one row per material claim and columns for primary evidence, independent support, contradictions, date/metric alignment and unresolved questions. Read the supporting content; agreeing titles alone are insufficient.

Trace each item to its underlying origin. An issuer release, an X link to it, a Google RSS entry and five syndicated copies are one announcement lineage. Different domains are not automatically independent verification. A reporter merely repeating the release adds attribution, not another confirmation of the same facts.

For an entry thesis, require a checked primary source and at least one genuinely independent supporting/reporting lineage, with the relevant distinction recorded. Seek the counterparty, regulator, original filing, separate reporting or other permitted source able to corroborate the disputed facts. Exceptional targets need their stricter confirmation gate as well.

## Reconcile differences explicitly

Align issuer, event time, reporting/update time, quarter, currency, units and metric definitions. Many apparent disagreements are different dates, forecast versus actual, gross versus net, or GAAP versus adjusted figures. Record a resolution only when a cited fact explains the difference.

Give a dated primary correction or newer verified filing precedence over an older copied report when it addresses the same fact. An issuer's promotional forecast remains a forecast even if independently quoted. Do not resolve a material conflict by majority vote, sentiment average or selecting the source with the highest proposed upside.

Use `evidence.compare` for the structured matrix and origin clusters, then inspect the actual records and request missing sources through the approved tools. A comparison tool's summary is not a new independent source. If the origin or factual conflict cannot be resolved within bounds, retain a blocker.

## Final assessment

State agreed facts, interpretation ranges, disconfirmed claims, independent-origin count, primary-verification state, coverage gaps and remaining material contradictions. Separate weak reporting diversity from actual independent corroboration.

Populate JSON `research.source_comparison` and `skill_findings` with claim/item/evidence references. Mark the comparison complete only when mandatory sources were actually read, dates/metrics align and material conflicts are resolved. Otherwise return `BLOCKED` and no entry; route remaining work through [search planning](../search-planning/SKILL.md) or [uncertainty handling](../uncertainty-and-incidents/SKILL.md).

Follow the [confidence and readiness policy](../../docs/v1/strategy.md) and [JSON protocol](../../docs/v1/llm-and-tools.md). Source consensus is an input to research, not a trading command.

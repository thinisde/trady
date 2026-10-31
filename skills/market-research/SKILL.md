---
name: market-research
description: >-
  Scope Trady's pre-market, opening or midday stock research and assemble a
  candidate dossier before entry evaluation. Also use for broad owner market questions.
metadata:
  version: "1.0.0"
---

# Market research

## Establish the decision

Read the current policy, approved instrument universe, actual session/calendar, source health and account/position snapshots. Distinguish a new-entry question from management of an existing holding. If a position exists, route its management through [position review](../position-review/SKILL.md); do not propose a second holding.

Record the question, effective S&P 500/owner/broker universe snapshot, research interval and mandatory coverage. Use [sector selection](../sector-selection.SKILL.md) to compare sectors, then narrow the subsector/branch before ranking stocks. Pre-market work is preparation; opening entry evaluation needs fresh context and a new unexpired proposal. A missed scheduled run does not justify an expired catch-up order.

## Investigate and compare

Use [search planning](../search-planning/SKILL.md) to find primary announcements, independent reporting and counterevidence. Apply [news analysis](../news-analysis/SKILL.md) to material items, then [source comparison](../source-comparison/SKILL.md) to each candidate thesis. X discovery must be supplemented with usable non-X evidence; a copied headline is not an independent source.

Rank candidates by verified materiality, price confirmation, execution economics, current uncertainties and policy fit. Include a credible no-action alternative. Do not rank by the most enthusiastic language, maximum quoted upside or the number of duplicated posts.

For a claimed breakthrough, separate research/prototype news from regulatory clearance, commercial availability, actual orders and earnings impact. Check what was announced now and what was already known. Price reaction and affordability are separate questions from the quality of the invention.

## Finish the dossier

Complete the finalist's [company profile](../company-profile.SKILL.md), [relationship review](../relationship-analysis.SKILL.md) and [watchdog](../holding-watchdog.SKILL.md). Use market/FX/calculators for draft economics and [Perpetual review](../perpetual-review.SKILL.md) for advisory input, then decide the proposed limits and refresh final economics. Request [entry readiness](../entry-readiness/SKILL.md) only when the comparison is complete. Finalize only after all checks pass and confidence is at least 90/100. The ordinary aim remains 1%.

If evidence, source independence, freshness, costs or protection remain unresolved, return the concrete blockers and no entry. Continue useful bounded research within the run allowance; do not replace missing facts with a confident narrative.

## Result

Populate final JSON `research.scope`, the search/claim/comparison artifacts, `skill_findings` and the readiness checklist when applicable. Include ranked candidates and exclusion reasons in the bounded reply. Any final proposal follows [trading policy](../../docs/v1/strategy.md) and the [conversation protocol](../../docs/v1/llm-and-tools.md); this skill never issues an order.

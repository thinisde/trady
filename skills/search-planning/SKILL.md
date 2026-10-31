---
name: search-planning
description: >-
  Plan and refine Trady news, X, RSS and primary-source queries for a company,
  event or unresolved claim, with independent confirmation and counterevidence.
metadata:
  version: "1.0.0"
---

# Search planning

## Form a specific question

Resolve the issuer name, approved instrument ID, exchange, ticker aliases, product/event terms and relevant dates. A ticker such as an ordinary word needs the issuer or cashtag/venue context supported by that source. Separate the original event time from the time a post or article was published.

Start with the claim you need to verify, not a desired trade conclusion. Use three query purposes: original announcement, independent reporting, and credible disconfirmation. Choose the smallest relevant time window that includes the announcement and prior context; widen it when the supposed new event may be old.

## Query in useful stages

1. Locate the primary issuer release, filing or official announcement using the legal name, exact product/event phrase and date. Use a domain filter only where the registered source supports it.
2. Search independent market reporting with issuer plus event terms. Avoid requiring bullish adjectives that filter out criticism.
3. Search counterevidence such as correction, delay, trial limitations, regulatory status, guidance caveats, financing/dilution or a disputed customer claim, choosing terms relevant to the event.
4. Check older coverage to establish novelty and what the market already knew. Review price timing separately through market-data tools.

Illustrative query intents are `issuer + product + announcement date`, `issuer + earnings + guidance + quarter`, and `issuer + product + correction/status`. Do not assume Google-style operators work in X or a news API. The adapter's advertised query schema determines valid filters; unsupported syntax is an error to correct, not an empty-news finding.

Use `news.search`, `rss.search`, `x.search_posts` and registered primary pages as appropriate. Search different reporting origins rather than repeatedly paging one aggregator. An X post URL should first resolve its actual post/context and linked origin; a quote/repost alone is insufficient.

## Refine without chasing confirmation

Inspect the first bounded results. Correct ambiguous issuer mapping, overly broad terms, date mistakes and missing synonyms. Expand one dimension at a time and record why. Stop repeating a query once results are duplicates or provider caps prevent useful coverage.

If one source fails, try a permitted independent path within budget. Record incomplete pagination, inaccessible articles and unavailable date coverage. Exhausting a budget does not establish that the claim is true or uncontested.

## Result

Populate JSON `research.search_log` with source ID, exact query/filters, requested interval, returned item IDs, completeness and unresolved gaps. Pass material items to [news analysis](../news-analysis/SKILL.md) and independent origins to [source comparison](../source-comparison/SKILL.md). Follow the bounded [JSON/tool protocol](../../docs/v1/llm-and-tools.md).

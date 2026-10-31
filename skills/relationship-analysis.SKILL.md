---
name: relationship-analysis
description: >-
  Attribute public praise, criticism, quotes and reposts between people and companies,
  and distinguish those mentions from verified business relationships and price effects.
metadata:
  version: "1.0.0"
---

# Mentions and economic relationships

Use [the company-intelligence graph rules](../docs/v1/company-intelligence.md#directional-mentions-and-economic-links). Resolve the actual speaker, target and quoted speaker. Retrieve original readable context, publication/event dates and permitted locator/excerpt; a search snippet or screenshot alone is insufficient.

Check both directions and both favorable/adverse claims. Query official statements and permitted social search with `"person A" "company B" partnership`, `"company A" "company B" terminate dispute`, and named product/customer aliases. Do not treat absence in a bounded search as proof that no relationship exists.

Separate an economic edge such as `SUPPLIES` from a mention such as `PRAISES`. A compliment proves at most that an attributed compliment occurred. An allegation stays an attributed allegation. For quotes, separate the quotation's author and distributor; for reposts record `REPOSTS` without assuming agreement. Read parent/quoted context, qualify sarcasm/unclear stance and refuse ambiguous entity matches.

Expand at most two hops and ten relevant neighbors initially. For each material link, explain the revenue/cost, regulation, production, reputation or competitive channel and when it could matter. Public co-mention is a candidate association, never enough to create a verified factual edge. Correlation after a post is not proof the speaker caused a price move.

Store `RELATIONSHIP_REVIEW` via `research.store_artifact`, including empty results with search coverage when appropriate. Use the exact directional event/edge fields in [research contracts](../docs/v1/research-contracts.md). Reference existing entity/evidence IDs; request resolution for unknown entities rather than inventing IDs. Dependent assertions expire or become disputed after corrections/deletions.

Return the tool-generated review ID in `research.focus` and material adverse/unclear links in the thesis and watchdog. A found rivalry never authorizes a sale; a material unresolved attribution blocks any entry that relies on it.

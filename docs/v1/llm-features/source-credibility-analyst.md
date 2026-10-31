# Source credibility and correction-history analyst (proposed)

Status: optional research-evidence quality layer. Trady already plans [primary/independent source comparison](../research.md) and the separate [suspected coordination detector](../features/market-manipulation-detector.md). This feature adds **time-dependent source reliability assessments** based on traceable corrections, claim specificity, incentives and category-specific reporting history, without asserting that high-reliability outlets always speak truth.

Every generative assessment is via **OpenCodeGo `deepseek-v4.1-flash`**; no alternate LLM or automatic Pro under [model policy](./model-routing-and-governance.md).

## Objective

Explain **why** an item deserves primary/secondary/unknown evidentiary status, whether it was timely/corrected, and whether multiple articles share an origin. Distinguish factual accuracy, ability to predict a stock move, public influence and legal reliability: a well-sourced regulatory release can still have no predictable price impact.

## Inputs and provenance

For permitted source items: original publisher/author where available, resolved domain/account identity, quote target, rights/expiry, published/first-seen/ingested times, source corrections, claim IDs, backing primary evidence, contextual incentives/disclosures, retractions, independent lineage and market time. Don't attempt to infer demographic/sensitive traits, deanonymize individuals or issue defamatory "fraud" verdicts.

Classify source roles: `PRIMARY_ISSUER`, `PRIMARY_REGULATOR`, `PRIMARY_FILING`, `DIRECT_WITNESS`, `SECONDARY_REPORTING`, `REPOST`, `UNVERIFIABLE`. A press release is primary **for what the issuer said**, not independently proven economic truth.

## Pipeline

1. Flash extracts atomic, falsifiable claims with exact original source identifiers and qualifiers.
2. Rust validates each claim against primary references and available timestamps and checks duplicates/syndication roots. Unsupported source ids are rejected.
3. For sources with adequate legally accessible historic records, compute **source-category-specific** correction/verification statistics with sample sizes and censoring. Small samples -> `INSUFFICIENT_HISTORY`; a 100% record from one case is not a calibrated probability.
4. Flash critiques source incentives and missing context while distinguishing conflict-of-interest disclosures from unsupported accusations.
5. The evidence quality layer emits source-rank/correction-history explanations and `VERIFIED`, `DISPUTED`, `UNVERIFIED`, `RETRACTED` claim states. Verified *claim provenance* does not establish its trading effect.
6. Feed only bounded, independently reviewed evidence-quality penalties into existing research readiness; no extra confidence simply because a favored source name appears. Link relevant coordinated amplification separately.

## Proposed contract

```json
{
  "schema_version": 1,
  "claim_id": "claim-101",
  "source_id": "news-101",
  "source_role": "SECONDARY_REPORTING",
  "original_origin_ids": ["issuer-release-1"],
  "prior_sample_count": 2,
  "historical_accuracy": null,
  "claim_status": "UNVERIFIED",
  "reasons": ["statement lacks primary corroboration"],
  "model_id": "deepseek-v4.1-flash",
  "order_authority": false
}
```

Synthetic; null reflects insufficient evidence, not zero accuracy.

## Architecture and operating budget

Reuse source/correction records in [financial data](../free-financial-data.md) and [news collection](../../news.md), add a SourceCredibilityService with versioned claim-evaluation and published-history windows. Future read-only tools `source.verify_claim`, `source.compare_origins`, `source.credibility_history` return sanitized bounded results. Respect X access/deletions, article retention and third-party LLM rights. Owner account should not become a "source identity" in public outputs. Avoid repeated calls for unchanged claims: use content hashes and periodic correction updates.

## Experimental validation

Benchmark against curated annotation of verifiable claims across topics (not only stocks that moved); measure source attribution precision, duplicated-lineage recall, inappropriate false discrediting, useful abstention, actual correction lag and impacts on research costs. Separate source accuracy metrics from investment profit outcome evaluation and use prospective frozen records. Train no new generative model.

Acceptance: reposts aren't independent; primary company statement isn't independently verified forecast; retracted article marked invalidated; absence of evidence isn't falsity; unknown identity stays unknown; deleted content respects rights; judge-claim source cannot cite its own generated conclusion. Source risk may request a review/entry block but never cause automatic sell.

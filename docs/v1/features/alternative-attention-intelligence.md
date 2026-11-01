# Alternative attention intelligence (research implementation)

Status: deterministic research implementation, 2026-10-31; enabled by default for research and rights-dependent. [Policy](../../../crates/trady-policy/src/intelligence/attention.rs) checks audited entity mappings and compatible query scales, calculates robust median/MAD surprise and source-availability timing, and preserves missingness. Reviewed normalized imports use the [shared service](./implementation-report.md). Direct Google Trends, Wikimedia and Reddit collection adapters and predictive calibration remain separate work.

## Purpose and novelty

Trady already collects news and approved X accounts. This new service measures **public attention change outside news/X**, with explicit separation between interest, sentiment, verified fundamentals and future returns. Search spikes and pageviews can be caused by marketing, scandals, rumors or bot traffic. They are not necessarily positive or pre-news market signals.

Candidate sources:
- **Google Trends:** sampled, normalized 0–100 web UI data; absolute search count cannot be inferred. A limited-access Google Trends API alpha may provide a different consistent-scale contract, but admission/rights are not assumed. Do not unofficially scrape around restrictions.
- **Wikimedia Analytics API:** aggregated pageviews for properly resolved company/brand/topic pages, noting language, page redirects and coverage of automated versus human agents.
- **Reddit (optional):** only after explicit Data API terms/permission approval. Public visibility does **not** grant permission for arbitrary commercial collection, model training or third-party LLM sharing. Aggregate forum attention, not user identities.
- **Other query signals:** only from reviewed providers with durable provenance and usage rights. Avoid marketing vendors that lack reproducible timestamps.

## Entity and signal resolution

Map firm symbol -> CIK/internal issuer -> corporate name/product/brand -> source-specific topic/page/query. A ticker such as CAT or META is ambiguous; require audited resolution and blacklist generic/acronym-only matches. Version name changes, acquisitions, localization and redirects. Preserve separate query IDs for brand vs issuer; never collapse them into the same independent event origin.

## Normalized data

~~~json
{
  "schema_version": 1,
  "issuer_id": "issuer_42",
  "source": "google_trends_ui_reviewed_import",
  "topic_id": "approved_topic_key",
  "region": "US",
  "window_start": "2026-10-01T00:00:00Z",
  "window_end": "2026-10-30T00:00:00Z",
  "available_at": "2026-10-31T08:00:00Z",
  "value": "83",
  "value_semantics": "relative_normalized_index_0_100",
  "sampling_or_revision": "sample-retrieval-id",
  "completeness": "PARTIAL",
  "rights_id": "approved-manual-import"
}
~~~

Synthetic schema fixture, not evidence of API entitlement. Never compare two 0–100 values produced from incompatible queries/windows as if they shared an absolute scale.

## Feature method

1. Normalize local source timestamps to UTC and explicit bucket boundaries; respect provider publication/cache lag.
2. Estimate within-query changes using robust rolling median/MAD where enough history exists; attach variance across independently obtained permitted samples where practical.
3. Identify attention spikes, sustained attention, attention decay, abnormal cross-language/page concordance and disagreement with verified news. Preserve ambiguity and data censoring.
4. Validate source-specific activity: Google sampling noise/zeros, Wikimedia bots/page moves, Reddit API deletions/moderation and forum coverage. Do not infer sentiment from volume alone.
5. Compare relative lead time to **already verified** original corporate events. If an event was first public before the attention change, do not label the attention feature as an early predictor.
6. Return AttentionAssessment containing source-specific normalized surprises, confidence, event provenance, time availability and a flag for possible publicity manipulation. A coordination flag can be independently linked to [manipulation detector](./market-manipulation-detector.md).

## Runtime and model boundaries

A strictly read-only AlternativeAttentionSource set in trady-adapters, AttentionFeatureService in trady-application and pure normalizers in trady-policy. Use optional analyst tool attention.snapshot, limited to approved issuer/query, retention, rate budget and compliant data. LLM may describe *attention patterns*, not use volume as evidence of bullishness. Existing policy and Perpetual feature training must accept only independently tested and rights-approved features.

## Evaluation

Construct forward-only archives using actual observation/ingestion timestamps: do not reconstruct "what Google Trends would have shown" using today's rescaled series without uncertainty markers. Report coverage, entity precision, lead-vs-lag event timing, false alarms, calibration for event categories and incremental trading value after costs on untouched periods. Controls: news-count-only, X-only, and random query mappings. Account for repeated measurements of same viral story and market-cap bias. Disable underperforming or unavailable sources individually.

## Compliance and quality acceptance

- Scrape rejected/alpha not admitted -> no Google Trends adapter.
- A normalized 100 is **not** 100 searches, 100% market share or positive sentiment.
- Unavailable Reddit authorization -> no collection, no cached unauthorized content, no model forwarding.
- Low-count noise, geography mismatch, topic collision, deleted page and source downtime -> UNKNOWN/partial, never zero.
- Deleted/corrected data invalidates derived features under the existing rights and provenance system.
- New feature cannot trigger a trade or raise a target without independent research and the preexisting full release gates.

## References

[Google Trends sampling/normalization](https://support.google.com/trends/answer/4365533?hl=en); [Google Trends API alpha](https://developers.google.com/search/apis/trends); [Wikimedia Analytics API](https://doc.wikimedia.org/generated-data-platform/aqs/analytics-api/documentation/getting-started.html); [Reddit Data API Terms](https://redditinc.com/policies/data-api-terms). Re-check access terms at deployment.

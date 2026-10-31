# Suspected market-manipulation and coordinated-hype detector (proposed)

Status: new feature design, 2026-10-31. Documentation only. The term **suspected** is mandatory; this system cannot establish criminal intent, identify a trader or certify fraud.

## Purpose and delta from current plan

Trady already deduplicates X/news lineage and checks corroborating sources in [research](../research.md). This extension detects **behavioral coordination and cross-channel promotional anomalies** rather than merely copied articles. Primary objective: reduce false entries triggered by artificially amplified attention and maintain explainable coverage gaps. It must not expose a list of accused individuals or trade on negative allegations.

## Threat model and inputs

Potential indicators: bursts of semantically near-identical messages, narrow creation-age cohorts where lawfully available, synchronized timestamps, repeated outbound URLs, repeated claims across low-independence accounts, unusual engagement ratios and contemporaneous turnover/volatility shocks. These are **weak indicators**, not proof: genuine emergencies, coordinated press releases and live sports/business events can create identical patterns.

Use only approved X/news/other source metadata with valid processing rights. Apply the existing X rights, deletion and correction controls. Never scrape private groups, evade platform controls, deanonymize users or profile sensitive personal traits. Prefer hashed source identifiers with source-scoped retention and aggregated outputs; raw text may be sent to a model only under explicitly reviewed rights.

## Data model

New optional records:
- PromotionEvent: issuer, claim cluster, time window, original source ids, published/first-seen timestamps, channel, observed-versus-covered count, content lineage, provisional status.
- CoordinationSignal: burst surprise, text similarity concentration, common-URL concentration, temporal synchronization, independent-author breadth, optional price/volume divergence and per-feature availability flags.
- ManipulationAssessment: **risk_of_unreliable_attention**, not probability of fraud; version, calibration, uncertainty, supporting counterexamples, threshold policy, expiry and rights reference.

~~~json
{
  "schema_version": 1,
  "issuer_id": "issuer_42",
  "window_start": "2026-10-30T13:00:00Z",
  "window_end": "2026-10-30T13:10:00Z",
  "observed_posts": 120,
  "collection_completeness": "partial",
  "features": {
    "near_duplicate_fraction": 0.82,
    "shared_url_fraction": 0.75,
    "independent_origin_count": 3,
    "volume_zscore": null
  },
  "status": "REVIEW_REQUIRED",
  "version": "coordination-v1",
  "rights_id": "source-rights-v2"
}
~~~

Synthetic fixture. Null volume is not zero. Do not persist unnecessary user identity fields.

## Detection pipeline

1. Join permitted messages to issuer/claim clusters using the existing provenance graph; normalize only allowed fields. Separate retweets, quotations, reposts and primary issuer disclosure.
2. Estimate historical within-source baseline posting rate by time of week/event category. Compute burst scores only where baseline coverage is comparable; record censored windows explicitly.
3. For each cluster, calculate near-duplicate embeddings/lexical hashes, URL reuse, timestamp concentration and effective independent origin count. Do not treat ten posts reusing one wire story as ten corroborations.
4. Attach price/volume deviation **only when** a properly timestamped authorized feed exists; use broad-market and sector controls. Correlation is not proof of manipulation.
5. Ensemble rules produce a bounded suspicion tier (NORMAL, ELEVATED, REVIEW_REQUIRED, UNAVAILABLE) with interpretable feature contributions, contradictory evidence and coverage diagnostics. Avoid assigning personal culpability.
6. The signal may **reduce evidence weight or request review**; it cannot manufacture bearish direction, generate an exit or override a latched stop. Valid independently verified disclosures remain available with their own lineage.

## Rust ownership and tool contract

- trady-adapters reuses existing read-only X/RSS collector.
- trady-application::coordination groups events, stores pseudonymous statistics and attaches source invalidations.
- trady-policy::evidence_quality converts a reviewed assessment into bounded source-reliability adjustment without raising confidence or changing numerical risk caps.
- Read-only tool market.coordination_check returns tier, window, explanation, provenance ids, coverage and uncertainty; no order tool or public accusation.

## Evaluation and guardrails

Manually label **behavioral** positives/negatives from permitted archival samples, including organic earnings/social spikes, real disasters, coordinated journalist publication, repeated syndication and known enforcement case histories with temporal availability verified. Report precision at review threshold, false-positive rate by market-cap/source, recall, time-to-detection and number of legitimate opportunities unnecessarily blocked. Conduct prospective blind review. Prefer high precision to broad speculative accusations. Ablate count-only, text-only and price-only signals; test source outage and deletion cascades.

## Operations and acceptance

- Disabled until rights review and evidence-quality integration pass. Low coverage -> UNAVAILABLE, not NORMAL.
- Rejected ingestion due to platform permissions -> diagnostic only, no substitute scrape.
- A deleted original must retract linked features and preserve a non-content audit marker only where authorized.
- Tests: official filing generates simultaneous independent discussion (not flagged as fraud); one botnet-like repeated claim does not increase independent confirmation; timestamp disorder, source limits, synthetic brigading, and spoofed retweet chains cannot produce positive confidence.
- No automatic public reporting of named accounts. Only authorized owner-facing operational warnings.

## References

[SEC/FINRA pump-and-dump investor alert](https://www.sec.gov/files/investor/alerts/ia_pumpanddump.htm); [SEC social media stock tip scams](https://www.investor.gov/investing-basics/avoiding-fraud/types-fraud/pump-dump-schemes); existing [X monitoring contract](../x-api-monitoring.md). This proposal detects anomalies, not legally established manipulation.

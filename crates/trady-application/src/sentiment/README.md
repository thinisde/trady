# Sentiment assessment boundary

## Overview

Rights-aware projection of normalized research items and exact prediction
correlation. The public module remains `trady_application::sentiment`; the
[v1 design](../../../../docs/v1/finbert/README.md) describes the full evidence flow.

## Interface

- `[mod.rs:SentimentProvider]`: `predict` and configured `model_revision`.
  `predict(&self, documents: &[SentimentDocument]) -> Result<SentimentBatch>`;
  `model_revision(&self) -> &str`.
- `[mod.rs:text_sha256]`: SHA-256 of the exact selected UTF-8 text.
- `[mod.rs:assess_sentiment]`: source projection, provider call and assessment.
- `[mod.rs:validate_probabilities]`: validator re-exported from
  [sentiment policy](../../../trady-policy/src/sentiment/mod.rs).
  `SentimentDocument`, `SentimentBatch` and `SentimentAssessment` are imported
  [domain records](../../../trady-domain/src/sentiment/mod.rs).

## Invariants

`[mod.rs:assess_sentiment]` requires 1–16 distinct, nonempty item IDs and reporting
lineages, each at most 256 bytes. It clones and redacts each source for model use,
accepts case-insensitive `en` or an `en-` prefix followed by hyphen-separated
subtags of 1–8 ASCII alphanumeric characters, and rejects future, expired or
older-than-24-hour observations. Present social metadata must report `missing_context=false`;
absent social metadata is accepted. Source originals remain unchanged. Callers
select instrument-relevant evidence; this function does not verify relevance.

`[mod.rs:assess_sentiment]` selects nonblank full text, otherwise summary,
otherwise title, retaining the selected bytes without trimming; unavailable text
or text longer than 4,096 Unicode scalar values fails. Response schema, model,
configured revision, count, IDs and hashes must match exactly. Invalid
probabilities, duplicate predictions, truncation or provider failures abort the
batch. Evidence identity and timestamps come from the source inputs.
Observation time uses `event_at`, otherwise `published_at`; returned evidence
preserves input order even when predictions arrive in another order. The model
revision must match the supplied provider; this function does not compare it to
the shipped `FINBERT_REVISION` constant. `[mod.rs:validate_probabilities]` checks
components in `[0,1]` with sum tolerance `0.000001`.

## Side Effects

`[mod.rs:assess_sentiment]` calls `predict` once after all source projections
validate, and queries `model_revision` during response validation. The
caller supplies time; this module does not persist observations or authorize an
order. Consumers must recheck freshness at use time.

## Verification

Run `cargo test -p trady-application --test sentiment --locked` from the repository
root. Tests use deterministic providers without loading FinBERT.
Run `cargo test --workspace --locked` for workspace verification.

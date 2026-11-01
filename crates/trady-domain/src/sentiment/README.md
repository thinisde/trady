# Sentiment records

## Overview

Provider-neutral records shared by the FinBERT adapter, application and policy.
The public module remains `trady_domain::sentiment`; see the
[v1 design](../../../../docs/v1/finbert/README.md).

## Interface

- `[mod.rs:SentimentProbabilities]`, `SentimentDocument`, `SentimentPrediction`,
  `SentimentBatch`: request and response records with decimal probabilities.
- `[mod.rs:SentimentEvidence]`, `SentimentAssessment`: correlated observations
  and caller-supplied scoring time.
  `[mod.rs:SentimentDocument::text]` carries the raw request text; predictions
  and evidence retain identity and probabilities without a raw-text field.
  `[mod.rs:SentimentPrediction]` carries `truncated`; evidence carries `lineage_id`,
  `observed_at` and `expires_at` alongside item/text identity and probabilities.
- `[mod.rs:SENTIMENT_VERSION]`, `FINBERT_MODEL`, `FINBERT_REVISION`,
  `MAX_SENTIMENT_ITEMS`, `MAX_SENTIMENT_CHARS`: schema/model identity and bounds.
  Values are version `1`, model `ProsusAI/finbert`, revision
  `0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031`, at most `16` items and `4096`
  characters. These constants do not validate record construction.

## Invariants

`[mod.rs:SentimentProbabilities]`, `SentimentDocument`, `SentimentPrediction`,
`SentimentBatch` and `SentimentEvidence` deny unknown deserialized fields.
`[mod.rs:SentimentAssessment]` supports
serialization only, with public fields. Record construction alone does not
validate permissions, correlation, probability bounds, freshness or authority;
application and policy callers are responsible for those checks.

## Side Effects

None. `[mod.rs:SentimentAssessment]` and the other DTOs/constants define data only.

## Verification

Run `cargo test -p trady-policy -p trady-application --test sentiment --locked`
from the repository root for application and policy contract coverage.
Run `cargo test --workspace --locked` for workspace verification.

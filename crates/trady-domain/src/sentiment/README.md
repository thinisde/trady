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
- `[mod.rs:SENTIMENT_VERSION]`, `FINBERT_MODEL`, `FINBERT_REVISION`,
  `MAX_SENTIMENT_ITEMS`, `MAX_SENTIMENT_CHARS`: schema/model identity and bounds.

## Invariants

Deserializable records deny unknown fields. `SentimentAssessment` supports
serialization only, with public fields. Record construction alone does not
validate permissions, correlation, probability bounds, freshness or authority;
the application and policy enforce those checks.

## Side Effects

None. This module defines records and constants only.

## Verification

Run `cargo test -p trady-policy -p trady-application --test sentiment --locked`
from the repository root for application and policy contract coverage.

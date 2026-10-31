# Sentiment assessment boundary

## Overview

Rights-aware projection of normalized research items and exact prediction
correlation. The public module remains `trady_application::sentiment`; the
[v1 design](../../../../docs/v1/finbert/README.md) describes the full evidence flow.

## Interface

- `[mod.rs:SentimentProvider]`: `predict` and configured `model_revision`.
- `[mod.rs:text_sha256]`: SHA-256 of the exact selected UTF-8 text.
- `[mod.rs:assess_sentiment]`: source projection, provider call and assessment.
- `[mod.rs:validate_probabilities]`: policy validator re-export.

## Invariants

Assessment requires 1–16 distinct item IDs and reporting lineages. It clones and
redacts each source for model use, requires English and complete social context,
and rejects future, expired or older-than-24-hour observations. Source originals
remain unchanged.

Text selection uses nonblank full text, otherwise summary, otherwise title;
unavailable or longer-than-4,096-character text fails. Response schema, model,
configured revision, count, IDs and hashes must match exactly. Invalid
probabilities, duplicate predictions, truncation or provider failures abort the
batch. Evidence identity and timestamps come from the source inputs.

## Side Effects

Calls the supplied provider once after all source projections validate. The
caller supplies time; this module does not persist observations or authorize an
order. Consumers must recheck freshness at use time.

## Verification

Run `cargo test -p trady-application --test sentiment --locked` from the repository
root. Tests use deterministic providers without loading FinBERT.

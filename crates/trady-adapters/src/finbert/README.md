# FinBERT HTTP adapter

## Overview

An explicitly configured HTTP implementation of the application sentiment port.
The [v1 design](../../../../docs/v1/finbert/README.md) defines its wire contract;
[configuration](../../../../config/finbert/README.md) ships disabled.

## Interface

- `[mod.rs:FinBertConfig]`, `FinBertConfig::validate`: endpoint, revision, enabled
  state and bounded request timeout.
- `[mod.rs:FinBertClient::new]`: validates configuration and constructs the client.
- `[mod.rs:SentimentProvider]` implementation: `model_revision` and `predict`.

## Invariants

Construction requires explicit enablement. Configuration requires schema v1, a
40-character hexadecimal revision, a 1–60 second timeout and a root URL without
credentials, query or fragment. HTTP is limited to loopback or `finbert`; remote
endpoints require HTTPS. Redirects and ambient proxies are disabled.

Prediction validates 1–16 unique documents, matching hashes and nonblank texts
of at most 4,096 characters. Responses must be HTTP 200, at most 64 KiB and match
schema, model, revision, document IDs and text hashes, with valid probabilities.
Application assessment separately rejects truncation and enforces source rights.

## Side Effects

Explicit `predict` sends document text to `v1/sentiment` and reads a bounded JSON
response. Client construction neither launches the service nor loads model weights.

## Verification

Run `cargo test -p trady-adapters --test finbert --locked` from the repository root.
Tests use one-shot mock HTTP fixtures and do not launch FinBERT.

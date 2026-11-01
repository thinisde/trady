# FinBERT HTTP adapter

## Overview

An explicitly configured HTTP implementation of the application sentiment port.
The [v1 design](../../../../docs/v1/finbert/README.md) defines its wire contract;
[configuration](../../../../config/finbert/README.md) ships disabled.

## Interface

- `[mod.rs:FinBertConfig]`, `FinBertConfig::validate`: `config_version: u32`,
  `enabled: bool`, `endpoint: String`, `model_revision: String` and
  `timeout_seconds: u64`. Deserialization rejects unknown fields.
- `[mod.rs:FinBertClient::new]`: validates configuration and constructs the client.
- `[mod.rs:SentimentProvider for FinBertClient]`: the application-owned port's
  `model_revision(&self) -> &str` and
  `predict(&self, documents: &[SentimentDocument]) -> Result<SentimentBatch>`.

## Invariants

`[mod.rs:FinBertClient::new]` requires explicit enablement after validation;
`[mod.rs:FinBertConfig::validate]` also accepts disabled configuration.
Configuration requires schema v1, a 40-byte ASCII hexadecimal revision,
a 1–60 second timeout and a root URL without credentials, query or fragment.
HTTP accepts only the hosts `localhost`, `127.0.0.1`, `[::1]` or `finbert`;
remote endpoints require HTTPS. Construction disables redirects and ambient proxies
and bounds connection timeout to the smaller of five seconds and the configured
timeout.

`[mod.rs:FinBertClient::predict]` validates 1–16 documents with unique nonempty
IDs of at most 256 bytes, matching hashes and nonblank texts of at most 4,096
Unicode scalar values. Responses must be HTTP 200, at most 64 KiB and match
schema, model, revision, item count, unique document IDs and text hashes, with
valid probabilities. Response order is preserved; predictions correlate by ID/hash,
not position. Failures return sanitized errors without provider response bodies.
Application assessment separately rejects truncation and enforces source rights.

## Side Effects

`[mod.rs:FinBertClient::predict]` posts schema version and complete document IDs, hashes and text to
`v1/sentiment`, then reads a bounded JSON response. Client construction neither launches the service nor
loads model weights.

## Verification

Run `cargo test --workspace --locked` from the repository root.

Run `cargo test -p trady-adapters --locked` from the repository root.
The package has no `finbert` integration-test target. Adapter verification uses
offline fixtures and does not require launching the model service.

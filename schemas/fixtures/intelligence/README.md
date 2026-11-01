# Synthetic intelligence inputs

## Overview

Ten synthetic feature payloads exercise deterministic calculations without market subscriptions or private account data. They are input bodies, not `FeatureObservation` import envelopes. [Test support](../../../crates/trady-adapters/tests/intelligence_support/mod.rs) wraps them with synthetic scope, reviewed-source fixtures, hashes and evidence.

## Interface

| Feature kind | Payload |
|---|---|
| `OPTIONS` | [options.json](./options.json) |
| `COORDINATION` | [coordination.json](./coordination.json) |
| `EVENT` | [event.json](./event.json) |
| `SHORT_INTEREST` | [short_interest.json](./short_interest.json) |
| `INSIDER` | [insider.json](./insider.json) |
| `CROSS_ASSET` | [cross_asset.json](./cross_asset.json) |
| `ATTENTION` | [attention.json](./attention.json) |
| `ACCOUNTING` | [accounting.json](./accounting.json) |
| `MICROSTRUCTURE` | [microstructure.json](./microstructure.json) |
| `TAX` | [tax.json](./tax.json) |

## Invariants

`[../../../crates/trady-adapters/tests/intelligence_support/mod.rs:input]` deserializes these bodies into the strict version-1 `FeatureInput` contract. Their references and permissions are synthetic test claims and grant no vendor entitlement. The tax fixture contains illustrative values only. Historical fixture clocks are deliberate; changing a timestamp requires reviewing the corresponding publication and availability semantics.

## Side Effects

None from these files. Tests and the offline replay service can load them into process memory; real ingestion requires reviewed source configuration and a validated envelope.

## Verification

Run `cargo test -p trady-adapters -p trady-cli --test intelligence --locked` from the repository root.

# FinBERT client configuration

## Overview

Non-secret configuration for the optional self-hosted sentiment provider. The
[v1 design](../../docs/v1/finbert/README.md) covers later activation and threshold use.

## Interface

- [config.example.json](./config.example.json): version 1 `FinBertConfig`, loopback
  endpoint, pinned model revision and a 30-second request timeout.
- [Rust adapter](../../crates/trady-adapters/src/finbert/README.md): validation and
  explicit client construction.
- [Deployment](../../services/finbert/README.md): Compose service and offline override.

## Invariants

`[config.example.json:enabled]` is false. Copy the example to deployment-owned
configuration and explicitly enable it before scoring. The adapter validates
endpoint shape, revision and timeout; configuration contains no credentials.

## Side Effects

None from this file alone. CLI status reads and validates it without contacting
the service; explicit scoring requires an enabled provider.

## Verification

Run from the repository root:

```sh
cargo run --locked --bin trady -- sentiment status --config config/finbert/config.example.json
```

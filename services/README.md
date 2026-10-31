# Inference services blueprint

## System Overview

Optional inference dependencies called through Rust application ports. They do
not own financial state, policy or broker operations.

## Subdirectories & System Boundaries

- [FinBERT](./finbert/README.md): CPU English text classification, bounded HTTP
  contract, explicit Docker deployment and model-cache configuration.

## Interface

`[finbert/serve.py:Handler]` exposes health and sentiment HTTP endpoints.
The [v1 design](../docs/v1/finbert/README.md) binds these observations to Rust adapters
and conservative threshold constraints.

## Global Invariants

`[finbert/serve.py:__main__]` guards model/listener startup;
`[../config/finbert/config.example.json:enabled]` is false and
`[../services/finbert/compose.yaml:services.finbert.profiles]` makes the service opt-in.
Inference cannot mutate a trading plan or authorize an order.

## Global Side Effects

Later explicit startup can download model artifacts, bind an HTTP listener and
consume CPU/RAM. The service has not been built or launched by this change.

## System Verification

Run `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s services/finbert/tests -v`
and the configuration-only Compose checks in the [component contract](./finbert/README.md).

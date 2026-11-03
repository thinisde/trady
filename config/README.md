# Deployment and fixture configuration

## Overview

The focused deployment uses only `autonomous.iex30.demo.json`,
`alpaca-iex30.paper.example.json`, `trading212.iex30.demo.example.json`,
`iex30.universe.json`, `watchdog.iex30.json` and `research.json`. The other
files are compatibility fixtures for shared offline tests and are not loaded by
the IEX30 runtime.

Secret values use the [central credentials store](../docs/credentials.md), separate
from provider configuration. `credentials init` creates the ignored private
`credentials.local.json`; environment variables override its entries.

## Interface

- [IEX30 Trading 212 demo](./autonomous.iex30.demo.json): focused runtime
  template with Trading 212 demo execution, Alpaca IEX market data and the
  exact 30-symbol owner allowlist. The focused deployment audit is
  [IEX30 audit](../docs/iex30-audit.md).

- [Cold-start demo setup](./cold-start.demo.json): provider-owned account
  discovery, independent OHLCV integrity checks for the fixed IEX30 list,
  shadow-only seed model/formula/cost artifacts and the supervised first-order
  intent gate. It does not contain a fabricated broker account number and it
  never grants entry authority by itself. See the [cold-start guide](../docs/cold-start.md).

- [x-pilot.example.json](./x-pilot.example.json): retained disabled compatibility
  fixture for the X parser tests. It is outside the IEX30 deployment; use
  [research.json](./research.json) for the focused registry.

- [IEX30 universe](./iex30.universe.json): reviewed membership and WebSocket
  capacity declaration.
- [IEX30 watchdog](./watchdog.iex30.json): five-minute quote freshness schedule
  for every symbol in the fixed universe.
- [Long Donchian/ATR v1 spec](./formulas/long-donchian-atr-v1.json) and
  [authority](./formulas/long-donchian-atr-v1.authority.json): pre-registered
  model-free research baseline bound to the IEX30 demo policy. The runtime keeps
  entries paused until prospective shadow and execution-cost evidence is complete.

- [Credential setup READMEs](./credentials/README.md): how to obtain every API
  key/token, database connection string and locally generated secret.

- [credentials.example.json](./credentials.example.json): blank named entries for
  API keys, separate T212 demo/live API key/secret pairs, shared secrets, hashing/signing keys
  and service database credentials.
  Keep the committed example blank; edit only the private initialized copy.

- [Alpaca IEX market data](./alpaca-iex30.paper.example.json): read-only paper
  IEX quote and bounded daily-history configuration for IEX30. The stream starts
  up to 1,800 seconds before the verified session open and stops after holding
  closure.
- [research.json](./research.json): the focused IEX30 X registry consumed by
  `[../crates/trady-application/src/research.rs:ResearchConfig]`. All seven
  sources remain disabled until stable identities, ownership/coverage rights and
  the total $1.90 budget evidence are present. See the [IEX30 guide](../docs/iex30.md).

- [IEX30 Trading 212 demo account](./trading212.iex30.demo.example.json): the
  account block used by the focused runtime; replace its placeholder broker
  account number before enabling autonomous execution.

The remaining JSON examples and service directories are compatibility fixtures
for shared offline tests; they are outside the focused runtime.

## Invariants

The focused runtime keeps entries paused and disabled until the model, input,
formula, capability and broker evidence gates are satisfied. Its placeholder
input endpoint intentionally reports `approved_input_service_unconfigured`.

The compatibility X pilot fixture remains disabled and is not loaded by the
IEX30 runtime. The focused registry supplies the only default X sources and
requires identity, rights and total-budget review before enablement.

`[../crates/trady-application/src/lib.rs:FoundationConfig::validate]` rejects live,
shadow, unsupported versions and unsafe policy in shared fixtures.
`[../crates/trady-adapters/src/trading212/mod.rs:Trading212Config::validate]` requires
explicit environment/account bindings and a bounded timeout; arbitrary URLs and
credentials are not configuration fields.

## Side Effects

None from these files alone. The CLI reads paper configuration; the test runner
mounts the authentication fixture into a new disposable server.
The explicit `t212 snapshot` CLI command can read the configured demo/live account.

## Verification

Run `cargo test -p trady-cli --locked` and `python3 scripts/test-postgres.py` from
the repository root.

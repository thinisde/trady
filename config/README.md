# Deployment and fixture configuration

## Overview

The disabled target template is `eur-demo.example.json`. The
[dated SHADOW review](../docs/evidence/eur-shadow-review-2026-11-04/README.md)
binds real catalog identities and source decisions. The full universe example
still uses illustrative broker IDs and unverified order precision; the review
does not make it an operational deployment. The
IEX30/Alpaca paths remain until EUR replay and database parity evidence pass.

Secret values use the [central credentials store](../docs/credentials.md), separate
from provider configuration. `credentials init` creates the ignored private
`credentials.local.json`; environment variables override its entries.

## Interface

- [EUR Trading 212 demo](./eur-demo.example.json): disabled SHADOW template
  with one entry slot, one open position, 15-minute pre-close flattening, and
  rotation disabled. LIVE is structurally blocked.
- [Illustrative EUR universe](./eur_equity_universe.example.json): example
  ISIN/MIC and Yahoo mappings with placeholder broker IDs. Replace these through
  a complete reviewed registry before the real-input milestone. The Airbus ISIN
  is corrected; quantity and tick values remain illustrative.
- [Reviewed identity crosswalk](../docs/evidence/eur-shadow-review-2026-11-04/crosswalk.json):
  exact Airbus/ASML demo catalog identities with explicitly unknown order rules.
  This research-only document cannot be imported as an orderable universe.
- [Trading 212 EUR demo](./trading212.eur-demo.example.json): account-scoped
  demo transport profile used by the target runtime.

- [Cold-start demo setup](./cold-start.demo.json): provider-owned account
  discovery, independent OHLCV integrity checks for the fixed IEX30 list,
  shadow-only seed model/formula/cost artifacts and the supervised first-order
  intent gate. It does not contain a fabricated broker account number and it
  never grants entry authority by itself. See the [cold-start guide](../docs/cold-start.md).

- [Bound IEX30 demo cold start](./cold-start.iex30.demo.json): the local
  cold-start profile bound to the discovered Trading 212 demo account
  `52211463` (EUR). It records account and metadata evidence only; it does not
  authorize an order or promote the research formula.

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
  The content-addressed [integrity manifest](./formulas/long-donchian-atr-v1.integrity.json)
  binds the authority, formula specification, IEX30 universe, runtime copy and
  registered Donchian implementation. `scripts/verify-long-donchian-integrity.py`
  is the independent CI verifier; a mismatch is a blocking error.

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
- [Yahoo Finance Euronext market data](./yfinance.euronext.example.json):
  illustrative `yfinance-rs` configuration with placeholder broker IDs.
- [Reviewed Yahoo SHADOW bindings](./yfinance.eur-shadow.reviewed.json): exact
  two-instrument demo ticker/ISIN mappings verified against catalog, issuer and
  provider metadata. Reads are enabled; the observed quotes were stale and
  lacked bid/ask, so no executable-quote capability is accepted.
- [research.eur.json](./research.eur.json): enabled official ECB RSS with
  attribution and bounded retention. Airbus RSS is disabled after a recorded
  parser failure. Official Airbus/ECB X identities have stable API IDs; post
  collection and content/model processing remain disabled. Alpha Vantage uses
  the correct credential reference and a reduced budget, but its news source
  is disabled because the intended historical role is not wired. The shared X
  billing ledger is preserved. See the dated review for evidence and rights.

- [IEX30 Trading 212 demo account](./trading212.iex30.demo.example.json): the
  provider-shape example. The focused runtime is bound to the discovered demo
  account in `autonomous.iex30.demo.json`; the checked-in runtime remains
  disabled and entry-paused until the independent release gates pass.

The remaining JSON examples and service directories are compatibility fixtures
for shared offline tests; they are outside the focused runtime.

## Invariants

The EUR template is disabled. The legacy IEX30 profile remains a separate
record-only configuration. The
checked-in native Perpetual artifact is approved only for bounded
advisory/shadow use; the formula authority remains entry-paused and its
prospective-shadow and paper gates remain open. Broker protection and quote
limitations remain explicit unsupported capability evidence. Its placeholder
input endpoint intentionally reports `approved_input_service_unconfigured`.
Account discovery is bound to the real demo account. The dated capability
observation in [`../docs/evidence/trading212-demo-capability-2026-11-03.json`](../docs/evidence/trading212-demo-capability-2026-11-03.json)
verifies cash and exact stock instruments, including a filled USD-listed AAPL
round trip with broker FX conversion, while explicitly recording that atomic
attached protection and executable broker quotes are unsupported.

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

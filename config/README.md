# Deployment and fixture configuration

The target is EUR equities through Trading 212 DEMO. The checked-in
[EUR template](./eur-demo.example.json) is disabled and grants no order authority.
The full [universe example](./eur_equity_universe.example.json) still contains
illustrative IDs/rules. Actual deployment uses separately reviewed identities,
instrument rules and costs described in the
[month handoff](../docs/eur-demo-month-2026-11.md).

The [retired IEX30 runtime](../docs/eur-legacy-retirement-2026-11-05.md) no longer
accepts execution commands. Its configuration files remain inert historical
inputs at their hash-sensitive paths for offline parity and integrity tests.

## EUR and research inputs

- [Source policy](../docs/eur-data-sources.md) and
  [machine-readable selection](./eur-data-sources.json): IBKR SMART live type-1
  bid/ask exclusively for execution and verified IBKR TRADES for current candles.
- [Current-candle example](./ibkr-current-bars.example.json): disabled pinned
  read-only helper, dedicated client ID and two-day regular-session history.
- [SMART example](./ibkr-smart.eur-shadow.example.json): exact quote bindings.
- [EUR SHADOW example](./eur-shadow.example.json): separate research/runtime
  configuration with no authority to activate DEMO.
- [Trading 212 EUR DEMO](./trading212.eur-demo.example.json): account-scoped
  transport configuration.
- [Reviewed Yahoo bindings](./yfinance.eur-shadow.reviewed.json) and
  [illustrative mappings](./yfinance.euronext.example.json): historical research
  and read-only diagnostics, without executable-quote fallback.
- [Research registry](./research.eur-shadow.metadata.json) and
  [EUR research example](./research.eur.json): available news/history with
  explicit source rights and shared request budgets. X stays metadata-only.
- [Original identity review](../docs/evidence/eur-shadow-review-2026-11-04/README.md):
  historical observations, distinct from current session/entry checks.
- [OpenCode](./opencode/README.md), [financial research](./financial-data.json)
  and [optional intelligence](./intelligence.example.json): separate research
  settings; none grant order authority.

## Historical offline fixtures

The following are retained for evidence/replay, not a deployable runtime:

- [IEX30 membership](./iex30.universe.json),
  [old runtime](./autonomous.iex30.demo.json),
  [Alpaca config](./alpaca-iex30.paper.example.json),
  [watchdog config](./watchdog.iex30.json) and
  [broker example](./trading212.iex30.demo.example.json).
- [Generic legacy runtime](./autonomous.example.json) and
  [record-only legacy fixture](./autonomous.demo-no-submit.json).
- [Cold-start example](./cold-start.demo.json) and
  [old bound account fixture](./cold-start.iex30.demo.json).
- [Donchian specification](./formulas/long-donchian-atr-v1.json),
  [authority](./formulas/long-donchian-atr-v1.authority.json) and
  [integrity manifest](./formulas/long-donchian-atr-v1.integrity.json).
  Keep their exact bytes and existing relative paths; the independent
  mathematical/parity checks still consume them.
- [X pilot](./x-pilot.example.json) and [older registry](./research.json):
  compatibility/import fixtures; they are not EUR content-processing approval.
- [Foundation paper config](./paper.json): role/database diagnostic fixture.

`autonomous check|run` rejects the old runtime shapes before credentials.
`services/runtime/prepare.py` emits only disabled EUR SHADOW configuration.
The reserved `alpaca_iex30` provider is rejected before HTTP and never creates
synthetic membership.

## Credentials and effects

Use the [central store](../docs/credentials.md) and
[credential setup](./credentials/README.md). The
[blank credential template](./credentials.example.json) contains names only;
secret values belong in ignored private files or the environment. Historical
credential names do not enable a retired provider.

Configuration files alone perform no network or database action. Explicit
`t212 snapshot` reads only the configured account. EUR entry still requires
a current exact release, complete evidence and separate owner approval.

Validate with `cargo test --workspace --locked`,
`python3 scripts/test-postgres.py` and `python3 scripts/verify-assets.py`.

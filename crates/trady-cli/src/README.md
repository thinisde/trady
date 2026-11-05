# CLI commands and wiring

[main.rs](./main.rs) routes the binary's private modules. The
[EUR service entrypoint](./autonomous.rs) accepts only `autonomous check|run
--config EUR_CONFIG [--once]`. Legacy shapes and retired actions fail before
credential loading. It delegates configured SHADOW work to
[eur_shadow.rs](./eur_shadow.rs); there is no older execution fallback.

The [retirement record](../../../docs/eur-legacy-retirement-2026-11-05.md)
identifies the removed runner, Alpaca/watchdog and account/cold-start paths.
Historical guides and hash-sensitive lab fixtures are retained separately.

## EUR workflow

- [eur.rs](./eur.rs): exact CLI options, config diagnostics, universe/gate
  commands and replay routing.
- [eur_shadow.rs](./eur_shadow.rs): immutable provider observations, analyst
  requests, SHADOW coordination, export capsules and private-owner outbox.
- [eur_decision.rs](./eur_decision.rs): registered-strategy survivors, durable
  analyst selection, refreshed executable books and Rust risk calculation.
- [eur_execute.rs](./eur_execute.rs): exact release/owner/dependency checks and
  durable native protocol transport/recovery.
- [eur_operation.rs](./eur_operation.rs): finite DEMO supervision, session
  preflight and account-wide economic reconciliation.
- [eur_paper.rs](./eur_paper.rs): bounded simulation on actual live SMART books,
  with no broker transport and no manufactured positive lifecycle.
- [eur_replay.rs](./eur_replay.rs): frozen input/source/risk/ledger validation;
  credential-free paper checking cannot replace missing evidence.
- [eur_workflow.rs](./eur_workflow.rs): dated reports and independently reviewed
  offline handoffs.

The analyst owns model credentials, executor owns account/transport credentials,
and gateway owns Telegram delivery. Provider timestamps remain separate from
receipt times. Current IBKR candles and executable type-1 bid/ask are different
inputs; Yahoo/Alpha remain historical/news research without quote fallback.
See [execution and release](../../../docs/eur-execution-and-release.md).

## Shared diagnostics and research

[broker_read.rs](./broker_read.rs) supplies bounded retries for read-only broker
observations. Order submissions never use that retry path. Foundation role
probes and fake-account/database demonstrations remain in `main.rs` to check
storage isolation and unknown-outcome recovery; their output grants no entry.

[news.rs](./news.rs), [financial.rs](./financial.rs),
[intelligence.rs](./intelligence.rs), [stock_report.rs](./stock_report.rs),
[api_health.rs](./api_health.rs) and [yfinance.rs](./yfinance.rs) expose explicitly
configured research or diagnostics. X content rights remain separately gated.

[OpenCode](./opencode/README.md), [Telegram](./telegram/README.md) and
[FinBERT](./finbert/README.md) retain their scoped research interfaces.
[inputs_service.rs](./inputs_service.rs) handles reviewed immutable imports and
bounded loopback serving; [model_lab.rs](./model_lab.rs) handles offline model
evaluation. They do not create an order authority.

[weekly.rs](./weekly.rs) and [release.rs](./release.rs) retain prospective
forecasts, pinned offline helpers, formula/parity bridges and separate
research-release identities. The EUR transport still requires its own exact
bundle, complete evidence and owner approval.

## Validation

Run `cargo test --workspace --locked`, strict Clippy and
`python3 scripts/test-postgres.py`. The retired child-process test target was
removed with its implementation; current EUR routing, packet/model, journal,
protection/recovery and role-isolation checks remain. The ignored native DEMO
probe requires explicit scoped authorization and is never part of ordinary
test execution.

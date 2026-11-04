# Trady

Trady is a Rust trading bot for one reviewed EUR day-trading authority. The
first operational target is the Trading 212 demo account; yfinance-rs supplies
read-only venue-bound quotes and completed bars, while Alpha Vantage, yfinance
news, X, RSS, browser readers, and owner URLs supply timestamped research.
Trading 212 is the only broker mutation path. The runtime permits one open
position, same-day flattening 15 minutes before the verified exchange close,
and a fully checked rotation only after the current position is closed.

The target operator profile is [config/eur-demo.example.json](./config/eur-demo.example.json).
The migration plan and authority boundary are documented in
[docs/trady-bot-implementation-plan.md](./docs/trady-bot-implementation-plan.md).
The old IEX30/Alpaca files remain replay fixtures during migration and are not
the target runtime authority.

## Focused configuration

- [EUR Trading 212 demo profile](./config/eur-demo.example.json)
- [EUR yfinance-rs crosswalk](./config/yfinance.euronext.example.json)
- [EUR/Alpha Vantage research registry](./config/research.eur.json)
- [Trading 212 EUR demo account example](./config/trading212.eur-demo.example.json)
- [Intraday Donchian/ATR formula baseline](./docs/formulas/long-donchian-atr-v1.md)

The target research registry keeps Alpha Vantage active for EUR news sentiment
and requires rights/freshness checks before its items reach ranking or holding
review. X accounts, RSS, browser pages, and owner URLs remain bounded and
untrusted until their own provenance checks pass.

## Operator commands

```sh
cargo run --locked --bin trady -- eur config \
  --config config/eur-demo.example.json
cargo run --locked --bin trady -- eur gate-explain \
  --config config/eur-demo.example.json
cargo run --locked --bin trady -- yfinance quote \
  --config config/yfinance.euronext.example.json --instrument AIR_PA_EQ
cargo run --locked --bin trady -- t212 snapshot \
  --config config/trading212.demo.example.json
```

`eur gate-explain` uses the same application-owned order-readiness gate as the
executor. `eur select` replays mandatory candidate selection without network
or broker access. `yfinance quote` remains read-only. The runtime keeps live
submission disabled until exact quote, protection, and restart-recovery
evidence is approved.

## Account and release gates

Use `t212 snapshot` for a read-only Trading 212 demo account check. An enabled
EUR executor must be constructed with an append-only target journal and, when
Telegram is enabled, a lifecycle outbox before the order gate can mutate the
broker. Those sinks persist research, selection, strategy, order-intent, exit,
and reconciliation records. A weekly Codex review can propose changes but
cannot activate them.

Credentials are loaded from the private credential store or the documented
environment variables. Never place secrets in committed configuration.

## Verification

```sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
git diff --check
```

The legacy IEX30/Alpaca adapters and replay fixtures remain available only for
compatibility tests and migration comparison. They are not the EUR runtime
authority.

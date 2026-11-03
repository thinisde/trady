# Trady

Trady is a Rust trading bot configured for one deployment scope: the fixed
30-symbol IEX30 US-equity universe. Alpaca supplies read-only IEX market data;
Trading 212 demo is the execution account. The order path rejects symbols,
venues and currencies outside that scope before it can reach the broker.

The focused operator guide is [docs/iex30.md](./docs/iex30.md). The complete
implemented, wired, configured and blocked-feature review is
[docs/iex30-audit.md](./docs/iex30-audit.md).

## Focused configuration

- [IEX30 symbol registry](./config/iex30.universe.json)
- [Autonomous Trading 212 demo profile](./config/autonomous.iex30.demo.json)
- [Alpaca IEX data profile](./config/alpaca-iex30.paper.example.json)
- [30-symbol watchdog profile](./config/watchdog.iex30.json)
- [Long Donchian/ATR v1 formula baseline](./docs/formulas/long-donchian-atr-v1.md)
- [IEX30 X/research registry and $1.90 total budget](./config/research.json)
- [Trading 212 demo account example](./config/trading212.iex30.demo.example.json)

The X registry names seven candidate accounts (NVIDIA, AMD, Meta, Tesla,
Reuters, DeItaone and KobeissiLetter), but all sources ship disabled until stable
numeric IDs, ownership/coverage rights and operator approval are recorded. The
ledger reserves before each request and stops at the configured total ceiling.

## Operator commands

```sh
cargo run --locked --bin trady -- autonomous check \
  --config config/autonomous.iex30.demo.json
cargo run --locked --bin trady -- alpaca stream-plan \
  --config config/alpaca-iex30.paper.example.json
cargo run --locked --bin trady -- watchdog check \
  --config config/watchdog.iex30.json
cargo run --locked --bin trady -- watchdog schedule \
  --config config/watchdog.iex30.json
cargo run --locked --bin trady -- news status
```

`autonomous run` starts the authenticated IEX WebSocket up to 30 minutes before
the verified session open, reconnects with bounded backoff, and stops it after a
confirmed zero-holding reconciliation. During a holding it persists watchdog
health once per configured bucket. `alpaca stream-plan` and `watchdog schedule`
remain offline inspection commands.

## Account and release gates

Use `t212 snapshot` for a read-only Trading 212 demo account check. Use
`account cold-start` for the supervised first-order workflow; it remains blocked
until the account identity, input/universe authority, model and formula evidence,
broker capabilities and owner release artifacts are present. No shipped file
authorizes live trading.

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

The remaining shared adapters, policy modules and offline labs are retained only
for compatibility tests and supervised research. They are not loaded by the
focused IEX30 runtime.

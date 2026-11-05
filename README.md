# Trady

Trady is a Rust research and execution system for EUR equities. The first
operational milestone is a real-input SHADOW run with
PostgreSQL and Telegram. That SHADOW milestone has completed with blocked
selection; a successful paper day and demo acceptance remain incomplete.
The checked-in EUR example is disabled, uses SHADOW, and cannot submit orders.
LIVE is rejected. DEMO remains blocked on native partial-entry/protection
evidence, a positive real-input paper lifecycle, complete independent acceptance
and exact-hash owner approval. Scoped quote/rule/cost components are accepted.

The target operator profile is [config/eur-demo.example.json](./config/eur-demo.example.json).
The migration plan and authority boundary are documented in
[docs/trady-bot-implementation-plan.md](./docs/trady-bot-implementation-plan.md).
The obsolete IEX30/Alpaca runtime, private worker and old account/preflight CLI
have been [retired](./docs/eur-legacy-retirement-2026-11-05.md). Shared offline
parity fixtures and historical journals remain. The
[current follow-up](./docs/eur-blocker-resolution-2026-11-05.md) records the
real DEMO fills, protocol fixes, cash reconciliation and remaining acceptance.
The [bounded DEMO month handoff](./docs/eur-demo-month-2026-11.md) prepares
6 November–6 December operation and offline optimization without automatic LIVE
activation or model promotion.

## Focused configuration

The owner's EUR data-source policy (4–5 November 2026) is **IBKR SMART Live
for real-time bid/ask** and **verified IBKR SMART TRADES for current five-minute
strategy candles**. YFinance and Alpha Vantage supply historical stock research,
news where available and other research queries.
Missing or stale IBKR books block entry; Yahoo or Alpha Vantage prices cannot
replace them. See [source policy](./docs/eur-data-sources.md).

- [EUR Trading 212 demo profile](./config/eur-demo.example.json)
- [EUR yfinance-rs crosswalk](./config/yfinance.euronext.example.json)
- [EUR/Alpha Vantage research registry](./config/research.eur.json)
- [Trading 212 EUR demo account example](./config/trading212.eur-demo.example.json)
- [Intraday Donchian/ATR formula baseline](./docs/formulas/long-donchian-atr-v1.md)

The EUR crosswalk contains illustrative broker IDs and is not an approved
catalog snapshot. The research registry lists example sources; it has no
reviewed X account registry. Provider collection configuration does not establish
quote compatibility, content rights, or release acceptance.

## Operator commands

```sh
cargo run --locked --bin trady -- eur config \
  --config config/eur-demo.example.json
cargo run --locked --bin trady -- gate explain \
  --config config/eur-demo.example.json
cargo run --locked --bin trady -- universe check \
  --config config/eur-demo.example.json
cargo run --locked --bin trady -- yfinance quote \
  --config config/yfinance.euronext.example.json --instrument AIR_PA_EQ
cargo run --locked --bin trady -- t212 snapshot \
  --config config/trading212.demo.example.json
```

`gate explain --snapshot FILE` evaluates the executor's application-owned gate
against the supplied order inputs. Without a snapshot it returns configuration
blockers and `SNAPSHOT_UNAVAILABLE`. The nested `eur gate-explain` spelling is
an alias. `eur select` replays candidate selection; `replay manifest-check`
checks recorded input hashes offline and does not certify a paper-day replay.
`yfinance quote` remains read-only.

## Account and release gates

Use `t212 snapshot` for a read-only Trading 212 account check. EUR journal
writes use database leases and fencing; acknowledgements reload their exact
reserved intent from PostgreSQL after restart. The target outbox atomically
publishes lifecycle events and deliveries. Its gateway primitives are tested
and are connected to the long-running EUR coordinator/gateway. Interrupted sends
remain ambiguous instead of being retried blindly. A weekly Codex review may
propose changes and cannot activate them.

Credentials are loaded from the private credential store or the documented
environment variables. Never place secrets in committed configuration.

## Verification

See the [5 November codebase audit](./docs/codebase-audit-2026-11-05.md) for the
latest fixes, regression coverage and migrations 21–22.

```sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
git diff --check
```

The current binary rejects legacy runtime configurations before credentials.
Historical IEX/formula fixtures remain only where shared mathematical replay,
database compatibility or retained evidence still consumes them.

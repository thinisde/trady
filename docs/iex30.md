# IEX30 deployment

Trady's focused deployment uses one fixed US-equity universe, Alpaca's IEX
market-data plane and Trading 212 demo execution.

## Universe

The runtime accepts exactly these 30 symbols:

META, MU, NVDA, AMD, SNDK, GOOGL, BKNG, AMZN, AAPL, MSFT, INTC, SHOP, STX,
AVGO, TSM, CSCO, LRCX, VLO, TSLA, ORCL, BAC, WDC, JPM, GOOG, LITE, GEV, AZO,
DELL, WBD and CRM.

The canonical list is in [trady-domain/src/iex.rs](../crates/trady-domain/src/iex.rs)
and [iex30.universe.json](../config/iex30.universe.json). The screening
adapter rejects a different count, symbol or index ID. Both Alphabet share
classes remain separate instruments.

## Runtime path

- Execution account: Trading 212 demo, configured by
  [autonomous.iex30.demo.json](../config/autonomous.iex30.demo.json).
- Market data: Alpaca Basic, IEX only, configured by
  [alpaca-iex30.paper.example.json](../config/alpaca-iex30.paper.example.json).
- Quote routing: the autonomous analyst and executor prefer Alpaca IEX. The
  old IBKR and SIP paths are removed from this deployment.
- Order gate: the application dispatcher and Trading 212 executor reject every
  instrument outside the IEX30 list before an order request is sent.
- Watchdog: [watchdog.iex30.json](../config/watchdog.iex30.json) emits one
  five-minute freshness check for each symbol while the autonomous runtime holds
  a position. Health is persisted through the executor fence.

The runtime template is deliberately disabled until the owner supplies the
Trading 212 demo account identifier, Alpaca credentials, input-service
evidence, approved model and formula artifacts, capability evidence and
reviewed owner allowlist. The check command reports those missing gates; it
does not invent readiness.

## Alpaca stream

The command trady alpaca stream-plan --config config/alpaca-iex30.paper.example.json
prints the provider endpoint, the exact 30-symbol trade/quote subscription and
the authentication/subscription JSON. Alpaca's documented IEX endpoint is
wss://stream.data.alpaca.markets/v2/iex.

The autonomous runtime starts the authenticated stream up to 30 minutes before
the verified session open, reconnects with bounded backoff, and shares its IEX
quote cache with the executor. It stops the socket after broker reconciliation
reports zero holdings and no active exit order. REST quote/history calls remain
bounded and read-only fallbacks.

## Formula baseline

The current registered research baseline is
[`long_donchian_atr_v1`](./formulas/long-donchian-atr-v1.md): a 20-session
long-only breakout with Wilder ATR(14), a 0.10 ATR entry allowance, a 1.50 ATR
stop and a 1.50R target. Its authority is model-free and does not consume
FinBERT or LLM fusion inputs. The package includes development, holdout, parity
and retrospective shadow-proxy reports. The 2026 proxy has material drawdown,
so the runtime remains disabled and entry-paused pending prospective shadow and
paper execution-cost evidence.

## X monitoring

The focused registry is [research.json](../config/research.json).
It is disabled until account IDs, rights and owner approval are recorded. The
initial watch set is:

| Source | Handle | Purpose |
|---|---|---|
| x_iex_nvidia | [nvidia](https://x.com/nvidia) | NVIDIA company announcements |
| x_iex_amd | [AMD](https://x.com/AMD) | AMD company announcements |
| x_iex_meta | [Meta](https://x.com/Meta) | Meta company announcements |
| x_iex_tesla | [Tesla](https://x.com/Tesla) | Tesla company announcements |
| x_iex_reuters | [Reuters](https://x.com/Reuters) | broad market and company headlines |
| x_iex_deitaone | [DeItaone](https://x.com/DeItaone) | fast market-moving headlines |
| x_iex_kobeissi | [KobeissiLetter](https://x.com/KobeissiLetter) | macro and market context |

The source list is a bounded starting point. Add an account only after
verifying its stable X user ID, ownership/coverage rights and relevance to at
least one IEX30 symbol or its macro risk. Do not spend credits on profile
lookups when a reviewed stable user ID is already present in the config.

The budget is a total remaining balance of 1,900,000 micro-USD ($1.90), not a
recurring allowance. The unit rates follow [X's developer pricing](https://developer.x.com/):
5,000 micro-USD per returned post and 10,000 micro-USD per returned user. The
config reserves 1,750,000 micro-USD for ordinary post reads and 150,000
micro-USD for support reads, stops at the ceiling and records the exhausted
state. With seven accounts polled every
three days for a 30-day cycle and at most five posts per account, the maximum
planned post spend is about $1.75. Keep the sources disabled until their
stable IDs and rights are filled in.

The watchdog config validates and materializes one freshness tick per symbol
for the current five-minute bucket. During a holding, `autonomous run` records
one fenced health observation per bucket, including stream connectivity,
reconnects, held-symbol quote age and stale IEX30 symbols. The CLI schedule
remains useful for offline inspection.

## Operator commands

    cargo run --locked --bin trady -- autonomous check \
      --config config/autonomous.iex30.demo.json
    cargo run --locked --bin trady -- alpaca stream-plan \
      --config config/alpaca-iex30.paper.example.json
    cargo run --locked --bin trady -- watchdog schedule \
      --config config/watchdog.iex30.json

All three commands are offline configuration checks. They do not place an
order, contact Alpaca or consume X credits.

# EUR data sources

Owner instructions, 4–5 November 2026: use **IBKR SMART Live data for real-time
bid/ask quotes** and **verified IBKR SMART TRADES for current five-minute
strategy candles**. Keep **YFinance and Alpha Vantage** for historical stock
research, available news and additional queries.

The executable-quote path must use timestamped IBKR `tickByTickBidAsk` books
with `marketDataType=1`, exact EUR ISIN/conId/listing bindings, positive sizes
and the configured ten-second freshness bound. SMART is a composite source;
its compatibility with Trading 212 fills must be evidenced per instrument.
IBKR remains a read-only data connection. Trading 212 remains the execution
broker for the approved demo account.

There is no fallback to YFinance, Alpha Vantage, last trades, delayed/frozen
IBKR data, position valuations or receipt-time timestamps for real-time
bid/ask. An unavailable or stale IBKR book blocks entry and is retained as a
typed observation. Closed primary sessions remain closed regardless of SMART
extended-hours availability.

The current-candle bridge uses the existing read-only Gateway and official SDK,
exact EUR conId/ISIN/SMART/primary-exchange bindings, two days of five-minute
regular-session `TRADES` and a separate client ID. Native candle timestamps,
history coverage and receipt times remain separate. Completion is capped by
request time, native coverage end and the latest native candle's opening time;
the last candle is withheld until a succeeding candle establishes completion.
Missing, malformed, stale or incomplete IBKR candles block a candidate. There
is no fallback to Yahoo or Alpha candles. Historical trade bars do not carry
the executable book's type-1 or bid/ask authority.

YFinance supplies historical OHLCV research; its news endpoint is used only
when available. Alpha Vantage supplies bounded historical cross-checks and
additional supported research queries through the shared request budget. The
5 November native Alpha intraday request returned an explicit premium-access
restriction and zero bars; this is an access failure, not a successful data
observation. Neither provider's outage may be filled with invented news or
prices. Official RSS can supply separately reviewed news. X remains
metadata-only until its content storage and model-processing rights are approved.

The retained Yahoo research path uses five days of intraday history with at
most 2,048 completed bars and six months for daily history. It cannot supply
current strategy candles when the IBKR candle source is configured. IBKR raw
and normalized candles, exact source settings and the bridge hash are retained
in paper inputs. Replay re-derives normalization and rejects a change back to
the old candle source or a substituted provider configuration.

In the bounded month profile, a newly downloaded last completed candle must
also belong to the current primary session and be at most two bar intervals
plus fifteen seconds old at signal evaluation. The same predicate is replayed
for paper acceptance; signal expiry also caps the final entry transport deadline.

This source selection is an owner preference. It does not establish an
exchange entitlement, Trading 212 price equivalence, order precision,
protection capability or release acceptance. Those checks remain separate.

Configuration: [IBKR source](../config/ibkr-smart.eur-shadow.example.json),
[current-candle bridge](../config/ibkr-current-bars.example.json),
[machine-readable owner policy](../config/eur-data-sources.json),
[EUR SHADOW](../config/eur-shadow.example.json),
[YFinance bindings](../config/yfinance.eur-shadow.reviewed.json),
[research registry](../config/research.eur-shadow.metadata.json).
Implementation and observed feed behavior are described in
[IBKR quotes](./ibkr-shadow-quotes.md).

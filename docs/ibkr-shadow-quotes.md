# IBKR SMART quotes in EUR SHADOW

The [owner's source policy](./eur-data-sources.md) requires IBKR SMART Live
for real-time bid/ask. YFinance and Alpha Vantage remain research providers.

The local `ibg-controller` session returned **live SMART bid/ask** for Airbus
and ASML on 4 November 2026. Both contracts resolved by exact ISIN in EUR.
The [assessment](./evidence/ibkr-smart-eur-assessment-2026-11-04.json) records
the contract IDs, provider callbacks, SDK version and capture hashes.

The connection uses the existing read-only Gateway on `127.0.0.1:4002`.
The Gateway's paper trading mode is separate from market-data type: this
session reported type **1 (live)** for both instruments. No Gateway login,
restart, subscription purchase, or IBKR order is required by the quote bridge.

The disabled [source example](../config/ibkr-smart.eur-shadow.example.json)
pins the exact contracts and the [read-only probe](../scripts/probe-ibkr-smart.py).
Its Python SDK comes from the official IBKR 10.50.02 archive, whose SHA-256 is
recorded in both files. Install that SDK in `/opt/trady/ibkr-sdk`, and copy the
probe to `/opt/trady/scripts/probe-ibkr-smart.py` before using the source.
The bridge inherits no executor environment or credentials. Its outgoing
message allowlist permits connection setup, contract discovery, quote requests,
server time and cancellation of its own data subscriptions.
Capture files use `/var/lib/trady-executor/eur-shadow`, owned by the executor's
systemd state directory. Root-owned deployment/recovery archives are separate.

For a reviewed SHADOW deployment, set `shadow.ibkr_config` to the installed
source configuration. Include that file in the configuration hash and use a
new deployment ID. Under the 5 November owner instruction, the separate
[current-candle bridge](../scripts/capture-ibkr-current-bars.py) supplies verified
IBKR SMART TRADES bars; Yahoo and Alpha remain historical research sources.
There is no silent fallback
to a delayed or untimed quote when IBKR is unavailable.

The adapter requires a live `marketDataType` callback and a timestamped
`tickByTickBidAsk` event. It checks conId, ISIN, EUR currency and primary
exchange against the reviewed binding. It rejects empty or crossed books,
missing size, future timestamps and changed identities. An old book retains
its original time and becomes STALE under the configured ten-second limit.

Raw callbacks and normalized quotes are separate PostgreSQL observations.
`eur export` includes both, and `replay paper-day` recalculates the normalized
quotes from the frozen callbacks without connecting to Gateway. The approved
primary exchange calendar controls market state and same-day deadlines;
extended SMART trading hours do not extend the bot's approved session.

SMART is a composite venue source. Trading 212 quote compatibility, order
precision and protection/recovery acceptance are still required for DEMO.
Adding this source does not open those gates, and LIVE stays blocked.

The candle bridge permits only connection setup, contract discovery, server
time, historical-data requests and cancellation. It has no broker credentials
or order API. Its timestamps and native coverage are independently checked;
historical bars never substitute for the live type-1 executable book. See the
[source policy](./eur-data-sources.md) for completion and freshness rules.

Primary API references: [market-data types](https://www.interactivebrokers.com/docs/tws-api/doc/md/delayed-data),
[timestamped BidAsk callbacks](https://www.interactivebrokers.com/docs/tws-api/doc/md/live-ticks),
[contract discovery by ISIN](https://www.interactivebrokers.com/docs/tws-api/contracts/contracts-specified-by-cusip-figi-or-isin),
and [historical trade bars](https://www.interactivebrokers.com/docs/tws-api/doc/md/historical-bars).

# IBKR SMART quotes in EUR SHADOW

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

For a reviewed SHADOW deployment, set `shadow.ibkr_config` to the installed
source configuration. Include that file in the configuration hash and use a
new deployment ID. Yahoo remains available for completed bars and research;
Alpha Vantage remains a historical cross-check. There is no silent fallback
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

Primary API references: [market-data types](https://www.interactivebrokers.com/docs/tws-api/doc/md/delayed-data),
[timestamped BidAsk callbacks](https://www.interactivebrokers.com/docs/tws-api/doc/md/live-ticks),
and [contract discovery by ISIN](https://www.interactivebrokers.com/docs/tws-api/contracts/contracts-specified-by-cusip-figi-or-isin).

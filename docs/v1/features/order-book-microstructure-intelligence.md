# Order-book microstructure and entry-quality intelligence (research implementation)

Status: deterministic research implementation, 2026-10-31; enabled by default for research. The [bounded book builder](../../../crates/trady-adapters/src/intelligence/book.rs) handles snapshots/deltas, gaps, recovery epochs, depth capacity and freshness. [Policy](../../../crates/trady-policy/src/intelligence/microstructure.rs) computes displayed-depth metrics, compatible dynamics and elapsed-window markouts. The [shared service](./implementation-report.md) exposes approved research context. Level 2 entitlements, exchange subscriptions, route validation and predictive calibration remain separate gates.

## Purpose and difference from existing execution

Trady already plans fresh bid/ask checks, tick-size validation, bounded limit orders and slippage in [trading policy](../strategy.md). This extension models **displayed liquidity dynamics and price-impact risk** using licensed per-venue depth-of-book data. It is primarily an **execution-quality** research input, not a standalone forecast of long-term direction.

Questions: Is a planned limit entry inside a thin quote, despite a tight top-of-book spread? Did visible depth disappear near the opening auction? Is order imbalance stable across repeated snapshots or a brief cancellation? Can expected fill probability and adverse selection be modeled before submitting within existing policy bounds?

## Source and market-structure limits

Candidate feeds: Nasdaq TotalView or other licensed venue/order-book products for the intended listed equities; exchange opening/closing imbalance products when covered. Verify real-time redistribution, derived analytics, recording/replay, tick timestamps and fees. For Trading 212, validate actual execution venue and order routing separately: **a Nasdaq-only book does not represent consolidated US liquidity, hidden orders, off-exchange execution or the broker's precise fill probability**. Aggregating different venues requires compatible quote conditions and timestamps.

The incoming chain may be sequence-numbered order/event updates or independent book snapshots. If snapshots only, do not reconstruct individual cancellations/trades or guaranteed queue position. Market data from a few seconds ago may be useless for trading decisions; set a stricter *verified* freshness policy than ordinary research data and remain unavailable if infrastructure cannot meet it.

## Core features and equations

Given top N valid levels, calculate:
- Bid/ask spread = best ask − best bid, in ticks and basis points of mid-price. Reject crossed/locked markets unless labeled venue state supports handling.
- Depth-weighted imbalance = (sum bid size − sum ask size) / (sum bid size + sum ask size). Zero denominator -> NA. This describes **displayed** depth on the selected venue.
- Microprice approximation at best level = (best ask × best bid size + best bid × best ask size) / (best bid size + best ask size). A modeling estimate, not actual next trade.
- Book slope and cumulative notional capacity within a fixed number of ticks or basis points; respect depth truncation and cancellations.
- Changes in imbalance, spread, depth persistence, quote update intensity and realized post-signal adverse selection over a registered window.
- Optional opening auction indicative price and imbalance, never silently blended with continuous trading quotes.

All features include selected venue, number of levels, feed timestamp, receive timestamp, clock skew, book version and derived-cost model.

## Contract

~~~json
{
  "schema_version": 1,
  "instrument_id": "equity_42",
  "venue": "XNAS",
  "exchange_time": "2026-10-30T14:30:01.010Z",
  "received_at": "2026-10-30T14:30:01.025Z",
  "sequence": 95002,
  "bids": [{"price": "100.00", "size": "800"}],
  "asks": [{"price": "100.01", "size": "450"}],
  "depth_levels": 1,
  "book_state": "VALID_PARTIAL_DEPTH",
  "rights_id": "licensed-level2-v1"
}
~~~

Synthetic fixture. Tradeable size/price are instrument precision-bound, and this sample cannot imply unseen levels.

## Engineering design

Create isolated BookFeedReader and SequenceBookBuilder in trady-adapters with per-symbol ring buffers, bounded memory, monotonic sequence enforcement, gap detection and replayable snapshots. The Rust analyst receives delayed/permitted summarized data for research; executor may receive **only** independently validated short-lived price-quality metrics through an approved one-way interface with no LLM/tool round trip or privilege escalation. Persist sampled/historical data only if vendor rights allow. A pure trady-policy::microstructure library can compute standardized depth metrics and evidence quality, but order placement remains in the existing executor.

Microstructure can decline a marginal new entry or improve a permitted limit price **inside the already-approved range**, with full revalidation and no order chasing. It cannot force a second entry, ignore a native stop, move prices beyond strategy limits or create a market-order fallback.

## Evaluation and service budget

Backtest with actual timestamped L2 histories and venue-specific replay assumptions; candle OHLC cannot reproduce intrasecond queue dynamics. Model feed latency, processing latency, limit order placement/cancel restrictions, spreads, fees, partial fills, hidden liquidity and unknown queue position. Compare current best-bid/ask baseline against added imbalance/depth features under realistic broker fills. Report implementation shortfall, fill rate, adverse selection, missed fills, feature decay and total data/infrastructure cost. On a 12-core/24GB VPS, cap subscribed symbols/depth and test packet-loss behavior before deploying.

## Acceptance and incidents

- Sequence gap, stale feed, expired entitlement, clock drift or mismatched venue -> invalidate book; do not pretend an empty book means no liquidity.
- Unknown routing -> research only; no execution-quality authority.
- Quote update with zero sizes, locked book, crossed quotes, negative/invalid price, out-of-order snapshot -> reject or explicitly label as auction/halt state.
- Elevated imbalance with zero verified predictive value -> no trade signal.
- Data outage during holding -> native protective orders remain independent and service does not cancel exits.
- Initial release paper/replay only, then shadow comparison with execution-quality metrics; independently verify against actual broker fills.

## Primary references

[Nasdaq TotalView depth data](https://www.nasdaq.com/products/data/equities/nasdaq-totalview/depth-data); [Deutsche Börse open order book](https://live.deutsche-boerse.com/en/know-how/handeln/offenes-orderbuch). Actual provider rights, quote coverage and Trading 212 order capabilities remain unverified.

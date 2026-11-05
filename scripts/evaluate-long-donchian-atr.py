#!/usr/bin/env python3
"""Evaluate the pre-registered long Donchian/ATR baseline on frozen IEX30 bars.

The evaluator is deliberately simple and auditable. It never searches parameters,
uses future bars for signal construction, or treats the output as release approval.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import statistics
from datetime import datetime, timezone
from decimal import Decimal, getcontext

# Match rust_decimal's 28-digit precision for the independent reference.
getcontext().prec = 28

SYMBOLS = [
    "META", "MU", "NVDA", "AMD", "SNDK", "GOOGL", "BKNG", "AMZN", "AAPL", "MSFT",
    "INTC", "SHOP", "STX", "AVGO", "TSM", "CSCO", "LRCX", "VLO", "TSLA", "ORCL",
    "BAC", "WDC", "JPM", "GOOG", "LITE", "GEV", "AZO", "DELL", "WBD", "CRM",
]
ZERO = Decimal("0")
ONE = Decimal("1")
ENTRY_ALLOWANCE_ATR = Decimal("0.10")
STOP_ATR = Decimal("1.50")
TARGET_R = Decimal("1.50")
ROUND_TRIP_COST = Decimal("0.0020")  # 20 bps, pre-registered conservative proxy.


def dec(value: str | int | float) -> Decimal:
    return Decimal(str(value))


def canonical(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def digest(value: object) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def read_bars(root: pathlib.Path, symbol: str) -> list[dict]:
    parts = []
    for path in [root / f"{symbol}.json", root / "2026" / f"{symbol}.json"]:
        if path.exists():
            parts.extend(json.loads(path.read_text()).get("bars", []))
    by_time = {row["observed_at"]: row for row in parts}
    return [by_time[key] for key in sorted(by_time)]


def true_range(previous_close: Decimal, row: dict) -> Decimal:
    return max(
        dec(row["high"]) - dec(row["low"]),
        abs(dec(row["high"]) - previous_close),
        abs(dec(row["low"]) - previous_close),
    )


def atr_series(bars: list[dict], window: int = 14) -> list[Decimal | None]:
    result: list[Decimal | None] = [None] * len(bars)
    ranges = [true_range(dec(bars[i - 1]["close"]), bars[i]) for i in range(1, len(bars))]
    if len(ranges) < window:
        return result
    atr = sum(ranges[:window], ZERO) / Decimal(window)
    result[window] = atr
    for i, current in enumerate(ranges[window:], start=window + 1):
        atr = (Decimal(window - 1) * atr + current) / Decimal(window)
        result[i] = atr
    return result


def simulate_symbol(symbol: str, bars: list[dict]) -> list[dict]:
    atrs = atr_series(bars)
    trades = []
    occupied_until = -1
    for index in range(35, len(bars) - 1):
        if index <= occupied_until or atrs[index] is None:
            continue
        prior_high = max(dec(row["high"]) for row in bars[index - 20:index])
        close = dec(bars[index]["close"])
        atr = atrs[index]
        if close <= prior_high:
            continue
        next_bar = bars[index + 1]
        next_open = dec(next_bar["open"])
        entry_ceiling = close + ENTRY_ALLOWANCE_ATR * atr
        if next_open > entry_ceiling:
            trades.append({"symbol": symbol, "signal_at": bars[index]["observed_at"], "status": "NON_FILL_GAP"})
            occupied_until = index + 1
            continue
        entry = next_open
        stop = entry - STOP_ATR * atr
        target = entry + TARGET_R * STOP_ATR * atr
        exit_price = None
        exit_status = None
        exit_index = None
        for current_index in range(index + 1, min(len(bars), index + 6)):
            row = bars[current_index]
            opened = dec(row["open"])
            high = dec(row["high"])
            low = dec(row["low"])
            if opened <= stop:
                exit_price, exit_status = opened, "STOP_GAP"
            elif opened >= target:
                exit_price, exit_status = opened, "TARGET"
            elif low <= stop and high >= target:
                exit_price, exit_status = stop, "STOP_SAME_BAR_FIRST"
            elif low <= stop:
                exit_price, exit_status = stop, "STOP"
            elif high >= target:
                exit_price, exit_status = (opened if opened >= target else target), "TARGET"
            if exit_price is not None:
                exit_index = current_index
                break
        if exit_price is None:
            exit_index = min(len(bars) - 1, index + 5)
            exit_price = dec(bars[exit_index]["close"])
            exit_status = "EXPIRY"
        gross = exit_price / entry - ONE
        net = gross - ROUND_TRIP_COST
        trades.append({
            "symbol": symbol,
            "signal_at": bars[index]["observed_at"],
            "entry_at": bars[index + 1]["observed_at"],
            "exit_at": bars[exit_index]["observed_at"],
            "entry": str(entry),
            "stop": str(stop),
            "target": str(target),
            "atr": str(atr),
            "status": exit_status,
            "gross_return": str(gross),
            "net_return": str(net),
        })
        occupied_until = exit_index
    return trades


def metrics(trades: list[dict]) -> dict:
    returns = [dec(row["net_return"]) for row in trades if "net_return" in row]
    if not returns:
        return {"closed_trades": 0, "status": "NO_TRADES"}
    equity = ONE
    peak = ONE
    max_drawdown = ZERO
    for value in returns:
        equity *= ONE + value
        peak = max(peak, equity)
        max_drawdown = max(max_drawdown, (peak - equity) / peak)
    return {
        "closed_trades": len(returns),
        "target_trades": sum(row["status"] == "TARGET" for row in trades),
        "stop_trades": sum(row["status"].startswith("STOP") for row in trades),
        "expiry_trades": sum(row["status"] == "EXPIRY" for row in trades),
        "non_fill_gap_signals": sum(row["status"] == "NON_FILL_GAP" for row in trades),
        "mean_net_return": str(sum(returns, ZERO) / Decimal(len(returns))),
        "median_net_return": str(Decimal(str(statistics.median(returns)))),
        "win_rate": str(Decimal(sum(value > ZERO for value in returns)) / Decimal(len(returns))),
        "compounded_return": str(equity - ONE),
        "maximum_drawdown": str(max_drawdown),
    }


def period_trades(trades: list[dict], start: str, end: str) -> list[dict]:
    return [row for row in trades if start <= row["signal_at"][:10] <= end and "net_return" in row]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--history-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    all_trades = []
    manifest = {}
    for symbol in SYMBOLS:
        bars = read_bars(args.history_dir, symbol)
        manifest[symbol] = {
            "bars": len(bars),
            "first": bars[0]["observed_at"] if bars else None,
            "last": bars[-1]["observed_at"] if bars else None,
        }
        all_trades.extend(simulate_symbol(symbol, bars))
    dataset_hash = digest(manifest)
    periods = {
        "development": period_trades(all_trades, "2024-01-01", "2024-12-31"),
        "holdout": period_trades(all_trades, "2025-01-01", "2025-12-31"),
        "shadow_proxy": period_trades(all_trades, "2026-01-01", "2026-11-03"),
    }
    report = {
        "schema_version": 1,
        "formula_id": "long_donchian_atr_v1",
        "universe": "IEX30",
        "dataset_hash": dataset_hash,
        "data_contract": "Alpaca Basic IEX daily regular-session bars",
        "cost_model": {"round_trip_bps": "20", "status": "PROXY_UNTIL_FILLED_COST_EVIDENCE"},
        "signal_contract": {
            "breakout_window": 20,
            "atr_method": "WILDER",
            "atr_window": 14,
            "minimum_warmup_bars": 35,
            "entry_allowance_atr": "0.10",
            "stop_atr_multiple": "1.50",
            "target_r_multiple": "1.50",
            "maximum_hold_sessions": 5,
            "same_bar_rule": "STOP_FIRST",
        },
        "periods": {name: {"metrics": metrics(rows), "trade_count_including_non_fill": len(rows)} for name, rows in periods.items()},
        "parameter_search": {"searched": False, "candidate_count": 1, "multiple_testing_log": "none"},
        "status": "RESEARCH_ONLY_COST_AND_SHADOW_EVIDENCE_INCOMPLETE",
        "limitations": [
            "Alpaca Basic IEX bars do not establish live execution impact or queue position.",
            "The 20 bps round-trip cost is a pre-registered proxy until a paper fill supplies costs and slippage.",
            "The 2026 period is retrospective shadow proxy data, not a prospective shadow release.",
            "The report does not claim profitability or production readiness.",
        ],
    }
    (args.output_dir / "dataset-manifest.json").write_text(json.dumps({"symbols": manifest, "dataset_hash": dataset_hash}, indent=2) + "\n")
    (args.output_dir / "evaluation-report.json").write_text(json.dumps(report, indent=2) + "\n")
    (args.output_dir / "trades.json").write_text(json.dumps(all_trades, indent=2) + "\n")
    print(json.dumps({"dataset_hash": dataset_hash, "periods": report["periods"], "status": report["status"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

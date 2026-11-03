#!/usr/bin/env python3
"""Replay the registered Donchian baseline into an immutable ledger.

The replay is deliberately independent from the Rust calculator.  It consumes
the content-addressed local bar snapshot, records rejected signals and data
quality exceptions explicitly, and maintains a chronological portfolio equity
ledger separate from trade-level statistics.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import platform
from datetime import datetime, timezone
from decimal import Decimal, getcontext
from typing import Any

getcontext().prec = 28

FORMULA_ID = "long_donchian_atr_v1"
BREAKOUT_WINDOW = 20
ATR_WINDOW = 14
MINIMUM_WARMUP = 35
ENTRY_ALLOWANCE_ATR = Decimal("0.10")
STOP_ATR = Decimal("1.50")
TARGET_R = Decimal("1.50")
MAX_HOLDING_SESSIONS = 5
ROUND_TRIP_COST = Decimal("0.0020")
RISK_FRACTION = Decimal("0.02")
# The registered policy limits each entry by both initial risk and cash
# allocation.  The old replay used only the risk budget, which could create a
# position whose notional exceeded the cash-only portfolio gate when the stop
# distance was small.
CASH_FRACTION = Decimal("0.02")
ZERO = Decimal("0")
ONE = Decimal("1")


def decimal(value: Any) -> Decimal:
    return Decimal(str(value))


def canonical(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=False, separators=(",", ":")).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def file_hash(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path: pathlib.Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def true_range(previous_close: Decimal, row: dict[str, Any]) -> Decimal:
    return max(
        decimal(row["high"]) - decimal(row["low"]),
        abs(decimal(row["high"]) - previous_close),
        abs(decimal(row["low"]) - previous_close),
    )


def atr_series(bars: list[dict[str, Any]]) -> list[Decimal | None]:
    result: list[Decimal | None] = [None] * len(bars)
    ranges = [true_range(decimal(bars[i - 1]["close"]), bars[i]) for i in range(1, len(bars))]
    if len(ranges) < ATR_WINDOW:
        return result
    atr = sum(ranges[:ATR_WINDOW], ZERO) / Decimal(ATR_WINDOW)
    result[ATR_WINDOW] = atr
    for index, current in enumerate(ranges[ATR_WINDOW:], start=ATR_WINDOW + 1):
        atr = (Decimal(ATR_WINDOW - 1) * atr + current) / Decimal(ATR_WINDOW)
        result[index] = atr
    return result


def validate_bar(row: dict[str, Any]) -> str | None:
    try:
        open_price = decimal(row["open"])
        high = decimal(row["high"])
        low = decimal(row["low"])
        close = decimal(row["close"])
    except (KeyError, ValueError, ArithmeticError):
        return "MALFORMED_OHLC"
    if min(open_price, high, low, close) <= ZERO:
        return "NONPOSITIVE_OHLC"
    if high < max(open_price, close) or low > min(open_price, close):
        return "OHLC_ORDER_INVALID"
    return None


def load_symbol(root: pathlib.Path, symbol: str) -> tuple[list[dict[str, Any]], list[dict[str, Any]], dict[str, Any]]:
    """Load and validate a symbol without silently dropping malformed rows."""
    source_files: list[dict[str, Any]] = []
    raw_rows: list[dict[str, Any]] = []
    quality: dict[str, Any] = {
        "malformed_rows": 0,
        "duplicate_timestamps": 0,
        "conflicting_duplicates": 0,
        "unordered_rows": 0,
        "partial_rows": 0,
    }
    for path in [root / f"{symbol}.json", root / "2026" / f"{symbol}.json"]:
        if not path.exists():
            continue
        document = read_json(path)
        rows = document.get("bars", []) if isinstance(document, dict) else []
        source_files.append({
            "path": str(path),
            "sha256": file_hash(path),
            "bytes": path.stat().st_size,
            "rows": len(rows),
            "provider": document.get("provider") if isinstance(document, dict) else None,
            "feed": document.get("feed") if isinstance(document, dict) else None,
        })
        if not isinstance(rows, list):
            quality["partial_rows"] += 1
            continue
        raw_rows.extend(row for row in rows if isinstance(row, dict))

    by_time: dict[str, dict[str, Any]] = {}
    for row in raw_rows:
        timestamp = row.get("observed_at")
        if not isinstance(timestamp, str):
            quality["partial_rows"] += 1
            continue
        error = validate_bar(row)
        if error:
            quality["malformed_rows"] += 1
            continue
        if timestamp in by_time:
            quality["duplicate_timestamps"] += 1
            if by_time[timestamp] != row:
                quality["conflicting_duplicates"] += 1
        by_time[timestamp] = row
    bars = [by_time[key] for key in sorted(by_time)]
    for previous, current in zip(bars, bars[1:]):
        if previous["observed_at"] >= current["observed_at"]:
            quality["unordered_rows"] += 1
    quality["raw_rows"] = len(raw_rows)
    quality["valid_bars"] = len(bars)
    quality["first"] = bars[0]["observed_at"] if bars else None
    quality["last"] = bars[-1]["observed_at"] if bars else None
    return bars, source_files, quality


def exit_event(
    bars: list[dict[str, Any]],
    entry_index: int,
    entry: Decimal,
    stop: Decimal,
    target: Decimal,
) -> tuple[int, Decimal, str] | None:
    for holding_session in range(1, MAX_HOLDING_SESSIONS + 1):
        index = entry_index + holding_session - 1
        if index >= len(bars):
            return None
        row = bars[index]
        opened = decimal(row["open"])
        high = decimal(row["high"])
        low = decimal(row["low"])
        if opened <= stop:
            return index, opened, "GAP_THROUGH_STOP"
        if low <= stop and high >= target:
            return index, stop, "STOP_SAME_BAR_FIRST"
        if low <= stop:
            return index, stop, "STOP"
        if high >= target:
            return index, opened if opened >= target else target, "TARGET"
        if holding_session == MAX_HOLDING_SESSIONS:
            return index, decimal(row["close"]), "FIFTH_HOLDING_SESSION_CLOSE"
    return None


def simulate_symbol(symbol: str, bars: list[dict[str, Any]], hashes: dict[str, str]) -> list[dict[str, Any]]:
    atrs = atr_series(bars)
    records: list[dict[str, Any]] = []
    occupied_until = -1
    for index in range(MINIMUM_WARMUP, len(bars)):
        current = bars[index]
        atr = atrs[index]
        base = {
            "symbol": symbol,
            "signal_at": current["observed_at"],
            "valid_bar_count": index + 1,
            "authority_sha256": hashes["authority_sha256"],
            "code_sha256": hashes["implementation_sha256"],
            "dataset_hash": hashes["dataset_hash"],
        }
        if index <= occupied_until:
            records.append({
                **base,
                "event_type": "signal",
                "signal_decision": "BLOCKED_OPEN_POSITION",
                "decision_reason": "SYMBOL_POSITION_ALREADY_OPEN",
            })
            continue
        if atr is None:
            records.append({
                **base,
                "event_type": "signal",
                "signal_decision": "REJECTED",
                "decision_reason": "ATR_UNAVAILABLE",
            })
            continue
        prior_high = max(decimal(row["high"]) for row in bars[index - BREAKOUT_WINDOW:index])
        signal_close = decimal(current["close"])
        threshold = signal_close + ENTRY_ALLOWANCE_ATR * atr
        if signal_close <= prior_high:
            records.append({
                **base,
                "event_type": "signal",
                "signal_decision": "NO_SIGNAL",
                "signal_close": str(signal_close),
                "previous_20_bar_high": str(prior_high),
                "atr": str(atr),
            })
            continue
        signal = {
            **base,
            "event_type": "signal",
            "signal_decision": "SIGNAL_ELIGIBLE",
            "signal_close": str(signal_close),
            "previous_20_bar_high": str(prior_high),
            "atr": str(atr),
            "gap_threshold": str(threshold),
        }
        if index + 1 >= len(bars):
            records.append({
                **signal,
                "signal_decision": "REJECTED",
                "decision_reason": "MISSING_NEXT_REGULAR_SESSION_OPEN",
                "next_open": None,
            })
            continue
        next_bar = bars[index + 1]
        next_open = decimal(next_bar["open"])
        signal["next_open"] = str(next_open)
        if next_open > threshold:
            records.append({
                **signal,
                "signal_decision": "REJECTED",
                "decision_reason": "OPENING_GAP_ABOVE_ALLOWANCE",
            })
            occupied_until = index + 1
            continue
        entry = next_open
        stop_distance = STOP_ATR * atr
        stop = entry - stop_distance
        target = entry + TARGET_R * stop_distance
        if stop <= ZERO or target <= entry:
            records.append({
                **signal,
                "signal_decision": "REJECTED",
                "decision_reason": "INVALID_TRADE_PLAN",
                "entry": str(entry),
                "stop": str(stop),
                "target": str(target),
            })
            continue
        exit_result = exit_event(bars, index + 1, entry, stop, target)
        risk_quantity = RISK_FRACTION / stop_distance
        cash_quantity = CASH_FRACTION / entry
        quantity = min(risk_quantity, cash_quantity)
        risk_budget = quantity * stop_distance
        notional = quantity * entry
        entry_record = {
            **signal,
            "event_type": "position",
            "signal_decision": "ACCEPTED",
            "decision_reason": "OPEN_WITHIN_INCLUSIVE_ALLOWANCE",
            "entry_at": next_bar["observed_at"],
            "entry": str(entry),
            "stop": str(stop),
            "target": str(target),
            "stop_distance": str(stop_distance),
            "target_distance": str(target - entry),
            "quantity": str(quantity),
            "risk_budget": str(risk_budget),
            "notional": str(notional),
            "risk_quantity": str(risk_quantity),
            "cash_quantity": str(cash_quantity),
            "cost_model": {"round_trip_bps": "20", "round_trip_fraction": str(ROUND_TRIP_COST)},
        }
        if exit_result is None:
            records.append({
                **entry_record,
                "position_status": "UNRESOLVED_MISSING_EXIT_BAR",
                "decision_reason": "MISSING_HOLDING_BAR",
                "exit_at": None,
                "exit": None,
                "exit_reason": None,
                "gross_return": None,
                "cost_amount": None,
                "net_return": None,
            })
            occupied_until = len(bars) - 1
            continue
        exit_index, exit_price, exit_reason = exit_result
        gross = exit_price / entry - ONE
        net = gross - ROUND_TRIP_COST
        records.append({
            **entry_record,
            "position_status": "CLOSED",
            "exit_at": bars[exit_index]["observed_at"],
            "exit": str(exit_price),
            "exit_reason": exit_reason,
            "holding_session": exit_index - index,
            "gross_return": str(gross),
            "cost_amount": str(quantity * entry * ROUND_TRIP_COST),
            "net_return": str(net),
        })
        occupied_until = exit_index
    return records


def closed(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [row for row in records if row.get("position_status") == "CLOSED" and row.get("net_return") is not None]


def trade_metrics(trades: list[dict[str, Any]], cost: Decimal = ROUND_TRIP_COST) -> dict[str, Any]:
    returns = [decimal(row["gross_return"]) - cost for row in trades]
    if not returns:
        return {"closed_trades": 0, "status": "NO_TRADES"}
    return {
        "closed_trades": len(returns),
        "mean_gross_return": str(sum((decimal(row["gross_return"]) for row in trades), ZERO) / Decimal(len(returns))),
        "mean_net_return": str(sum(returns, ZERO) / Decimal(len(returns))),
        "total_cost_fraction": str(cost * Decimal(len(returns))),
        "target_trades": sum(row["exit_reason"] == "TARGET" for row in trades),
        "stop_trades": sum(row["exit_reason"].startswith("STOP") or row["exit_reason"] == "GAP_THROUGH_STOP" for row in trades),
        "expiry_trades": sum(row["exit_reason"] == "FIFTH_HOLDING_SESSION_CLOSE" for row in trades),
        "win_rate": str(Decimal(sum(value > ZERO for value in returns)) / Decimal(len(returns))),
        "arithmetic_mean_uses_closed_trade_net_returns": True,
        "cost_applied_once": True,
    }


def portfolio_ledger(trades: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    ordered = sorted(trades, key=lambda row: (row["exit_at"], row["symbol"], row["signal_at"]))
    equity = ONE
    peak = ONE
    max_drawdown = ZERO
    events: list[dict[str, Any]] = []
    for sequence, trade in enumerate(ordered, start=1):
        before = equity
        net = decimal(trade["net_return"])
        notional = decimal(trade["notional"])
        equity += notional * net
        peak = max(peak, equity)
        drawdown = (peak - equity) / peak if peak > ZERO else ZERO
        max_drawdown = max(max_drawdown, drawdown)
        events.append({
            "sequence": sequence,
            "event_type": "portfolio_exit",
            "symbol": trade["symbol"],
            "signal_at": trade["signal_at"],
            "exit_at": trade["exit_at"],
            "exit_reason": trade["exit_reason"],
            "net_return": trade["net_return"],
            "equity_before": str(before),
            "equity_after": str(equity),
            "mark_to_market_drawdown": str(drawdown),
            "open_position_count_at_exit": sum(
                1 for other in trades
                if other["entry_at"] <= trade["exit_at"] and other["exit_at"] >= trade["exit_at"]
            ),
        })
    return events, {
        "starting_equity": str(ONE),
        "ending_equity": str(equity),
        "compounded_return": str(equity - ONE),
        "maximum_drawdown": str(max_drawdown),
        "chronological_equity_ledger": True,
    }


def write_chain(path: pathlib.Path, records: list[dict[str, Any]]) -> dict[str, Any]:
    if path.exists():
        raise RuntimeError(f"append-only ledger already exists: {path}")
    previous = None
    first = None
    with path.open("x", encoding="utf-8") as output:
        for sequence, original in enumerate(records, start=1):
            record = dict(original)
            record["sequence"] = sequence
            record["previous_record_sha256"] = previous
            record["record_sha256"] = digest(record)
            previous = record["record_sha256"]
            first = first or previous
            output.write(json.dumps(record, ensure_ascii=False, sort_keys=False) + "\n")
        output.flush()
        try:
            import os

            os.fsync(output.fileno())
        except OSError:
            pass
    try:
        path.chmod(0o600)
    except OSError:
        pass
    return {"records": len(records), "first_record_sha256": first, "last_record_sha256": previous}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--history-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--root", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)

    authority_path = root / "config/formulas/long-donchian-atr-v1.authority.json"
    spec_path = root / "config/formulas/long-donchian-atr-v1.json"
    universe_path = root / "config/iex30.universe.json"
    implementation_path = root / "crates/trady-policy/src/limits/donchian.rs"
    authority = read_json(authority_path)
    spec = read_json(spec_path)
    universe = read_json(universe_path)
    symbols = universe["symbols"]
    hashes = {
        "authority_sha256": file_hash(authority_path),
        "spec_sha256": file_hash(spec_path),
        "universe_sha256": file_hash(universe_path),
        "implementation_sha256": file_hash(implementation_path),
    }
    all_records: list[dict[str, Any]] = []
    raw_files: list[dict[str, Any]] = []
    quality: dict[str, Any] = {}
    for symbol in symbols:
        bars, files, symbol_quality = load_symbol(args.history_dir, symbol)
        raw_files.extend({**file, "symbol": symbol} for file in files)
        quality[symbol] = symbol_quality
        all_records.extend(simulate_symbol(symbol, bars, hashes | {"dataset_hash": "PENDING"}))
    stable_manifest = {
        "schema_version": 1,
        "source": "local_content_addressed_snapshot",
        "raw_files": raw_files,
        "quality": quality,
        "universe": {
            "id": universe["universe_id"],
            "symbols": symbols,
            "effective_date": "NOT_DECLARED_BY_SOURCE",
            "membership_limitation": "The frozen current IEX30 list is replayed backward; this is not a point-in-time membership study.",
        },
        "session": {
            "timezone": "America/New_York",
            "bar_contract": "regular_session_daily",
            "adjusted_prices": "ALPACA_DAILY_BAR_CONTRACT",
            "split_and_dividend_treatment": "source_contract_not_reconstructed",
        },
        "query": {
            "request_parameters": "local snapshot; no mutable API query at replay time",
            "query_timestamp": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        },
        "parser": {"python": platform.python_version(), "script_sha256": file_hash(pathlib.Path(__file__))},
        "cost_model": {"round_trip_bps": "20", "round_trip_fraction": str(ROUND_TRIP_COST), "applied_once": True},
        "position_sizing": {
            "risk_fraction": str(RISK_FRACTION),
            "cash_fraction": str(CASH_FRACTION),
            "method": "minimum_of_initial_risk_and_cash_allocation",
            "leverage": False,
        },
    }
    dataset_hash = digest({"raw_files": raw_files, "quality": quality})
    hashes["dataset_hash"] = dataset_hash
    all_records = [
        {**record, "dataset_hash": dataset_hash}
        for record in all_records
    ]
    ledger_path = output / "trade-ledger.jsonl"
    chain = write_chain(ledger_path, all_records)
    trades = closed(all_records)
    unresolved_positions = [
        {
            "symbol": row["symbol"],
            "signal_at": row["signal_at"],
            "decision_reason": row.get("decision_reason"),
        }
        for row in all_records
        if row.get("position_status") == "UNRESOLVED_MISSING_EXIT_BAR"
    ]
    events, portfolio_metrics = portfolio_ledger(trades)
    portfolio_path = output / "portfolio-ledger.jsonl"
    portfolio_chain = write_chain(portfolio_path, events)
    (output / "input-manifest.json").write_text(json.dumps({**stable_manifest, "hashes": hashes, "dataset_hash": dataset_hash}, indent=2) + "\n")

    periods = {
        "2024": [row for row in trades if row["signal_at"].startswith("2024-")],
        "2025": [row for row in trades if row["signal_at"].startswith("2025-")],
        "2026": [row for row in trades if row["signal_at"].startswith("2026-")],
    }
    diagnostics: dict[str, Any] = {
        "months": {},
        "symbols": {},
        "exit_reasons": {},
        "accepted_opening_gaps": sum(row.get("signal_decision") == "ACCEPTED" for row in all_records),
        "rejected_opening_gaps": sum(row.get("decision_reason") == "OPENING_GAP_ABOVE_ALLOWANCE" for row in all_records),
        "missing_next_open_signals": sum(
            row.get("decision_reason") == "MISSING_NEXT_REGULAR_SESSION_OPEN" for row in all_records
        ),
        "data_quality_exceptions": sum(
            item.get("malformed_rows", 0)
            + item.get("conflicting_duplicates", 0)
            + item.get("partial_rows", 0)
            for item in quality.values()
        ),
        "largest_wins": sorted(
            [{"symbol": row["symbol"], "signal_at": row["signal_at"], "net_return": row["net_return"]} for row in trades],
            key=lambda row: decimal(row["net_return"]), reverse=True,
        )[:10],
        "largest_losses": sorted(
            [{"symbol": row["symbol"], "signal_at": row["signal_at"], "net_return": row["net_return"]} for row in trades],
            key=lambda row: decimal(row["net_return"]),
        )[:10],
        "maximum_consecutive_losses": 0,
        "overlapping_position_count": 0,
        "volatility_buckets": {},
    }
    loss_streak = 0
    atr_values = sorted(decimal(row["atr"]) for row in trades)
    for row in trades:
        loss_streak = loss_streak + 1 if decimal(row["net_return"]) < ZERO else 0
        diagnostics["maximum_consecutive_losses"] = max(diagnostics["maximum_consecutive_losses"], loss_streak)
        overlap = sum(
            other["entry_at"] <= row["entry_at"] < other["exit_at"]
            for other in trades
        )
        diagnostics["overlapping_position_count"] = max(diagnostics["overlapping_position_count"], overlap)
        atr = decimal(row["atr"])
        percentile = sum(value <= atr for value in atr_values) / len(atr_values)
        bucket = "LOW" if percentile <= 0.33 else "MEDIUM" if percentile <= 0.66 else "HIGH"
        diagnostics["volatility_buckets"].setdefault(bucket, []).append(row)
    for row in trades:
        month = row["signal_at"][:7]
        diagnostics["months"].setdefault(month, []).append(row)
        diagnostics["symbols"].setdefault(row["symbol"], []).append(row)
        diagnostics["exit_reasons"][row["exit_reason"]] = diagnostics["exit_reasons"].get(row["exit_reason"], 0) + 1
    for key in ["months", "symbols"]:
        diagnostics[key] = {name: trade_metrics(rows) for name, rows in diagnostics[key].items()}
    diagnostics["volatility_buckets"] = {
        name: trade_metrics(rows) for name, rows in diagnostics["volatility_buckets"].items()
    }
    diagnostics["cost_sensitivity"] = {
        f"{bps}_bps": trade_metrics(trades, Decimal(bps) / Decimal(10000))
        for bps in (0, 20, 40)
    }
    diagnostics["cost_sensitivity"]["measured_paper_cost"] = {
        "status": "MISSING_PAPER_EVIDENCE",
        "passed": False,
    }

    report = {
        "schema_version": 1,
        "formula_id": FORMULA_ID,
        "status": "RESEARCH_REPLAY_COMPLETE"
        if not diagnostics["data_quality_exceptions"] and not unresolved_positions
        else "RECONCILIATION_BLOCKED",
        "authority_sha256": hashes["authority_sha256"],
        "spec_sha256": hashes["spec_sha256"],
        "universe_sha256": hashes["universe_sha256"],
        "implementation_sha256": hashes["implementation_sha256"],
        "dataset_hash": dataset_hash,
        "ledger": {"trade": chain, "portfolio": portfolio_chain},
        "trade_metrics": trade_metrics(trades),
        "unresolved_positions": unresolved_positions,
        "portfolio_metrics": portfolio_metrics,
        "periods": {name: trade_metrics(rows) for name, rows in periods.items()},
        "diagnostics_2026": diagnostics,
        "limitations": [
            stable_manifest["universe"]["membership_limitation"],
            "Alpaca Basic IEX bars do not establish queue position or live market impact.",
            "A missing holding bar remains unresolved and is never converted into a zero return.",
            "The 20 bps cost is a pre-registered proxy until paper fills provide measured costs.",
        ],
    }
    (output / "replay-report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "dataset_hash": dataset_hash, "closed_trades": len(trades), "portfolio": portfolio_metrics}, indent=2))
    return 0 if report["status"] == "RESEARCH_REPLAY_COMPLETE" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, KeyError) as exc:
        print(json.dumps({"status": "REPLAY_FAILED", "error": str(exc)}))
        raise SystemExit(1)


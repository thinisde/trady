#!/usr/bin/env python3
"""Compare the independent Python baseline with the Rust Donchian bridge."""

from __future__ import annotations

import argparse
import importlib.util
import json
import pathlib
import subprocess
from datetime import datetime, timedelta, timezone
from decimal import Decimal, InvalidOperation

PARITY_TOLERANCE = Decimal("1e-25")


def load_evaluator(path: pathlib.Path):
    spec = importlib.util.spec_from_file_location("donchian_evaluator", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load evaluator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def values_equal(left, right) -> bool:
    if isinstance(left, dict) and isinstance(right, dict):
        return left.keys() == right.keys() and all(values_equal(left[key], right[key]) for key in left)
    if isinstance(left, list) and isinstance(right, list):
        return len(left) == len(right) and all(values_equal(a, b) for a, b in zip(left, right))
    if isinstance(left, str) and isinstance(right, str):
        try:
            return abs(Decimal(left) - Decimal(right)) <= PARITY_TOLERANCE
        except InvalidOperation:
            return left == right
    return left == right


def rust_bridge(request: dict) -> dict:
    rust = subprocess.run(
        ["cargo", "run", "--quiet", "--locked", "-p", "trady-policy", "--example", "donchian_bridge"],
        input=json.dumps(request), text=True, capture_output=True, check=False,
    )
    if rust.returncode:
        raise RuntimeError(rust.stderr.strip())
    return json.loads(rust.stdout)


def synthetic_bars(count: int = 35, *, close: str | None = None, high: str | None = None) -> list[dict]:
    start = datetime(2026, 1, 1, 21, tzinfo=timezone.utc)
    bars = []
    for index in range(count):
        value = Decimal(100 + index)
        bars.append({
            "observed_at": (start + timedelta(days=index)).isoformat().replace("+00:00", "Z"),
            "open": str(value),
            "high": str(value + 1),
            "low": str(value - 1),
            "close": str(value),
        })
    if count >= 35 and close is not None:
        bars[-1]["close"] = close
        bars[-1]["open"] = str(Decimal(close) - 1)
        bars[-1]["low"] = str(Decimal(close) - 2)
        bars[-1]["high"] = high or str(Decimal(close) + 1)
    return bars


def boundary_cases() -> list[dict]:
    cases = []
    under_warm = rust_bridge({"bars": synthetic_bars(34), "entry": "100"})
    cases.append({"name": "34_valid_bars_no_signal", "rust": under_warm, "passed": under_warm["signal"] is None})

    eligible = rust_bridge({"bars": synthetic_bars(35, close="200"), "entry": "200"})
    cases.append({"name": "35_valid_bars_signal_evaluated", "rust": eligible, "passed": eligible["signal"] is not None})

    excluded_high = rust_bridge({"bars": synthetic_bars(35, close="200", high="500"), "entry": "200"})
    cases.append({
        "name": "current_high_excluded_from_channel",
        "rust": excluded_high,
        "passed": excluded_high["signal"] is not None and Decimal(excluded_high["signal"]["breakout_level"]) < Decimal("500"),
    })

    equality_bars = synthetic_bars(35)
    prior_high = max(Decimal(row["high"]) for row in equality_bars[-21:-1])
    equality_bars[-1]["close"] = str(prior_high)
    equality_bars[-1]["open"] = str(prior_high - 1)
    equality_bars[-1]["low"] = str(prior_high - 2)
    equality_bars[-1]["high"] = str(prior_high + 1)
    equal = rust_bridge({"bars": equality_bars, "entry": str(prior_high)})
    cases.append({"name": "close_equal_channel_no_signal", "rust": equal, "passed": equal["signal"] is None})

    signal = eligible["signal"]
    threshold = Decimal(signal["entry_ceiling"])
    for name, opening, expected in [
        ("entry_gap_exact_boundary_accepted", threshold, "ACCEPTED"),
        ("entry_gap_above_boundary_rejected", threshold + Decimal("0.01"), "REJECTED"),
        ("entry_gap_below_signal_close_accepted", Decimal("199"), "ACCEPTED"),
    ]:
        value = rust_bridge({"bars": synthetic_bars(35, close="200"), "entry": "200", "next_open": str(opening)})
        cases.append({"name": name, "rust": value, "passed": value["entry_decision"]["decision"] == expected})
    missing = rust_bridge({"bars": synthetic_bars(35, close="200"), "entry": "200", "evaluate_entry": True})
    cases.append({
        "name": "missing_next_open_rejected",
        "rust": missing,
        "passed": missing["entry_decision"]["reason"] == "MISSING_NEXT_REGULAR_SESSION_OPEN",
    })

    plan = rust_bridge({"bars": synthetic_bars(35, close="200"), "entry": "200"})["plan"]
    stop = Decimal(plan["stop"])
    target = Decimal(plan["target"])
    same_bar = {
        "observed_at": "2026-02-01T21:00:00Z", "open": "200", "high": str(target + 1),
        "low": str(stop - 1), "close": "201",
    }
    exit_value = rust_bridge({"bars": synthetic_bars(35, close="200"), "entry": "200", "exit_bars": [same_bar]})
    cases.append({"name": "stop_wins_same_bar", "rust": exit_value, "passed": exit_value["exit"]["reason"] == "STOP_SAME_BAR_FIRST"})

    gap_bar = {
        "observed_at": "2026-02-01T21:00:00Z", "open": str(stop - 1), "high": str(stop),
        "low": str(stop - 2), "close": str(stop - 1),
    }
    gap_exit = rust_bridge({"bars": synthetic_bars(35, close="200"), "entry": "200", "exit_bars": [gap_bar]})
    cases.append({"name": "open_below_stop_is_gap_stop", "rust": gap_exit, "passed": gap_exit["exit"]["reason"] == "GAP_THROUGH_STOP"})

    target_gap = {**same_bar, "open": str(target + 1), "high": str(target + 2)}
    target_exit = rust_bridge({"bars": synthetic_bars(35, close="200"), "entry": "200", "exit_bars": [target_gap]})
    cases.append({"name": "target_at_open_precedes_later_stop", "rust": target_exit,
                  "passed": target_exit["exit"]["reason"] == "TARGET"
                  and Decimal(target_exit["exit"]["executable_price"]) == target + 1})

    fifth_bars = [
        {"observed_at": f"2026-02-0{index}T21:00:00Z", "open": "200", "high": "201", "low": "199", "close": "200.5"}
        for index in range(1, 6)
    ]
    expiry = rust_bridge({"bars": synthetic_bars(35, close="200"), "entry": "200", "exit_bars": fifth_bars})
    cases.append({"name": "fifth_holding_session_closes", "rust": expiry, "passed": expiry["exit"]["reason"] == "FIFTH_HOLDING_SESSION_CLOSE"})
    return cases


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--history-dir", type=pathlib.Path, required=True)
    parser.add_argument("--evaluator", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    args = parser.parse_args()
    evaluator = load_evaluator(args.evaluator)
    cases = []
    for symbol in ["NVDA", "AAPL", "SNDK"]:
        bars = evaluator.read_bars(args.history_dir, symbol)
        atrs = evaluator.atr_series(bars)
        selected = None
        for index in range(35, len(bars)):
            if atrs[index] is None:
                continue
            prior_high = max(evaluator.dec(row["high"]) for row in bars[index - 20:index])
            close = evaluator.dec(bars[index]["close"])
            if close > prior_high:
                selected = index
                break
        if selected is None:
            continue
        request = {
            "bars": [
                {key: row[key] for key in ("observed_at", "open", "high", "low", "close")}
                for row in bars[: selected + 1]
            ],
            "entry": str(bars[selected]["close"]),
        }
        rust_value = rust_bridge(request)
        # The Python reference uses the same registered constants as the Rust spec.
        atr = atrs[selected]
        prior_high = max(evaluator.dec(row["high"]) for row in bars[selected - 20:selected])
        stop_distance = evaluator.STOP_ATR * atr
        target_distance = evaluator.TARGET_R * stop_distance
        python_value = {
            "formula_id": "long_donchian_atr_v1",
            "signal": {
                "signal_at": bars[selected]["observed_at"],
                "signal_close": str(evaluator.dec(bars[selected]["close"])),
                "breakout_level": str(prior_high),
                "atr": str(atr),
                "entry_ceiling": str(evaluator.dec(bars[selected]["close"]) + evaluator.ENTRY_ALLOWANCE_ATR * atr),
                "stop_distance": str(stop_distance),
                "target_distance": str(target_distance),
            },
            "plan": {
                "entry": str(evaluator.dec(bars[selected]["close"])),
                "risk_unit": str(stop_distance),
                "stop": str(evaluator.dec(bars[selected]["close"]) - stop_distance),
                "target": str(evaluator.dec(bars[selected]["close"]) + target_distance),
                "reward_to_risk": str(evaluator.TARGET_R),
            },
        }
        rust_normalized = {
            "formula_id": rust_value["formula_id"],
            "signal": rust_value["signal"],
            "plan": rust_value["plan"],
        }
        cases.append({"symbol": symbol, "python": python_value, "rust": rust_normalized, "equal": values_equal(python_value, rust_normalized)})
    boundaries = boundary_cases()
    passed = bool(cases) and all(case["equal"] for case in cases) and all(case["passed"] for case in boundaries)
    report = {
        "schema_version": 1,
        "formula_id": "long_donchian_atr_v1",
        "scope": "SIGNAL_PLAN_ENTRY_GAP_AND_EXIT_BOUNDARIES",
        "cases": cases,
        "case_count": len(cases),
        "boundary_cases": boundaries,
        "boundary_case_count": len(boundaries),
        "passed": passed,
        "numeric_tolerance": str(PARITY_TOLERANCE),
        "status": "PARITY_PASS" if passed else "PARITY_FAIL",
        "limitations": ["This report covers deterministic signal, unrounded plan, opening-gap and daily-bar exit rules; broker costs, tick rounding and live order lifecycle remain separate checks."],
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "case_count": len(cases)}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())

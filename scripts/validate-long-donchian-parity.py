#!/usr/bin/env python3
"""Compare the independent Python baseline with the Rust Donchian bridge."""

from __future__ import annotations

import argparse
import importlib.util
import json
import pathlib
import subprocess
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
        rust = subprocess.run(
            ["cargo", "run", "--quiet", "--locked", "-p", "trady-policy", "--example", "donchian_bridge"],
            input=json.dumps(request), text=True, capture_output=True, check=False,
        )
        if rust.returncode:
            raise RuntimeError(rust.stderr.strip())
        rust_value = json.loads(rust.stdout)
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
    passed = bool(cases) and all(case["equal"] for case in cases)
    report = {
        "schema_version": 1,
        "formula_id": "long_donchian_atr_v1",
        "scope": "SIGNAL_AND_UNROUNDED_TRADE_PLAN",
        "cases": cases,
        "case_count": len(cases),
        "passed": passed,
        "numeric_tolerance": str(PARITY_TOLERANCE),
        "status": "PARITY_PASS" if passed else "PARITY_FAIL",
        "limitations": ["This report covers the signal and unrounded plan; broker costs, tick rounding and order lifecycle remain separate checks."],
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "case_count": len(cases)}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())

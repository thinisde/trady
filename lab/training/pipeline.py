#!/usr/bin/env python3
"""Fetch genuine bars and prepare research-only, purged native-model datasets.

Historical barrier labels are hypothetical observations, never actual fills,
net returns, point-in-time availability attestations, or deployment approvals.
"""
import argparse
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
PURPOSE = "RESEARCH_ONLY_HISTORICAL_REPLAY"
FEATURES = ["return_1", "return_5", "return_20", "volatility_20",
            "range_20", "volume_ratio_20", "distance_sma_20"]
LIMITATIONS = [
    "Retrospectively downloaded split-adjusted daily bars; original historical availability is unverified.",
    "Current five-stock selection introduces selection/survivorship bias; results do not generalize to an historical S&P 500 universe.",
    "Hypothetical next-open entry and gross 1% target/1% stop over five bars; no actual fills, FX, fees or net payoff labels.",
    "Same-bar target/stop conflicts are excluded, which can bias the retained sample.",
    "Chronological holdout is an offline diagnostic, not independent production evaluation or prospective shadow evidence.",
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def encode(value):
    return (json.dumps(value, indent=2, allow_nan=False) + "\n").encode()


def publish(path, value):
    """Create private artifacts without replacing prior runs."""
    path = Path(path)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    data = encode(value)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    return digest(data)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("provider redirects are forbidden")


def validate_bars(document, symbol, start, end):
    if document.get("status") != "ok" or document.get("meta", {}).get("symbol") != symbol:
        raise ValueError("provider status or symbol mismatch")
    if document["meta"].get("currency") != "USD" or document["meta"].get("interval") != "1day":
        raise ValueError("unexpected provider currency or interval")
    values = document.get("values", [])
    if not 100 <= len(values) < 5000:
        raise ValueError("insufficient or possibly truncated history")
    rows = []
    seen = set()
    for value in values:
        date = dt.date.fromisoformat(value["datetime"])
        if date in seen or not start <= date <= end:
            raise ValueError("duplicate or out-of-range bar")
        seen.add(date)
        numbers = {k: float(value[k]) for k in ["open", "high", "low", "close", "volume"]}
        if not all(math.isfinite(v) for v in numbers.values()):
            raise ValueError("nonfinite bar")
        if not (0 < numbers["low"] <= min(numbers["open"], numbers["close"])
                <= max(numbers["open"], numbers["close"]) <= numbers["high"]
                and numbers["volume"] >= 0):
            raise ValueError("inconsistent OHLCV")
        rows.append({"date": date.isoformat(), **numbers})
    return sorted(rows, key=lambda row: row["date"])


def fetch(args):
    selection = json.loads(Path(args.selection).read_text())
    instruments = selection["instruments"]
    if not 1 <= len(instruments) <= 5:
        raise ValueError("at most five selected instruments per bounded fetch")
    start, end = dt.date.fromisoformat(args.start), dt.date.fromisoformat(args.end)
    if not start < end < dt.datetime.now(dt.timezone.utc).date():
        raise ValueError("request must end before today")
    out = Path(args.out).resolve()
    if out.exists():
        raise ValueError("output directory already exists; select a new run")
    key = os.environ.get("TRADY_TWELVE_DATA_API_KEY")
    if not key:
        store = Path(os.environ.get("TRADY_CREDENTIALS_FILE", ROOT / "config/credentials.local.json"))
        key = json.loads(store.read_text())["credentials"]["TRADY_TWELVE_DATA_API_KEY"]
    if not isinstance(key, str) or not key.strip():
        raise ValueError("Twelve Data credential is unavailable")
    opener = urllib.request.build_opener(NoRedirect())
    results = []
    for index, instrument in enumerate(instruments):
        symbol = instrument["symbol"]
        if not symbol.isascii() or not symbol.isalpha() or not 1 <= len(symbol) <= 8:
            raise ValueError("invalid selected symbol")
        if index:
            time.sleep(13)  # At most five requests per minute; no paid fallback.
        params = {"symbol": symbol, "interval": "1day", "start_date": args.start,
                  "end_date": args.end, "outputsize": 5000, "order": "asc", "adjust": "splits"}
        url = "https://api.twelvedata.com/time_series?" + urllib.parse.urlencode({**params, "apikey": key})
        request = urllib.request.Request(url, headers={"Accept": "application/json"}, method="GET")
        try:
            with opener.open(request, timeout=25) as response:
                raw = response.read(4_000_001)
            if len(raw) > 4_000_000:
                raise ValueError("provider response exceeds bound")
            document = json.loads(raw)
            bars = validate_bars(document, symbol, start, end)
        except urllib.error.HTTPError as error:
            raise ValueError(f"provider HTTP {error.code}; response details suppressed") from None
        except (urllib.error.URLError, TimeoutError):
            raise ValueError("provider transport failed; credential-bearing URL suppressed") from None
        received_at = dt.datetime.now(dt.timezone.utc).isoformat()
        # Store parsed original content, not the credential-bearing request URL.
        saved_hash = publish(out / "raw" / f"{symbol}.json", document)
        results.append({"instrument_id": instrument["instrument_id"], "symbol": symbol,
                        "received_at": received_at, "request": params,
                        "response_sha256": digest(raw), "saved_document_sha256": saved_hash,
                        "raw_document": f"raw/{symbol}.json", "bars": bars})
        print(json.dumps({"symbol": symbol, "bars": len(bars), "first": bars[0]["date"],
                          "last": bars[-1]["date"], "status": "FETCHED_FOR_RESEARCH"}), flush=True)
    publish(out / "history.json", {"schema_version": 1, "purpose": PURPOSE,
                                    "provider": "Twelve Data", "limitations": LIMITATIONS,
                                    "instruments": results})


def features(bars, index):
    window = bars[index - 20:index + 1]
    closes = [row["close"] for row in window]
    volumes = [row["volume"] for row in window[:-1]]
    mean_volume = statistics.mean(volumes)
    if mean_volume <= 0:
        return None
    returns = [math.log(closes[j] / closes[j - 1]) for j in range(1, len(closes))]
    current = closes[-1]
    result = [current / closes[-2] - 1, current / closes[-6] - 1,
              current / closes[0] - 1, statistics.pstdev(returns),
              statistics.mean((row["high"] - row["low"]) / row["close"] for row in window[-20:]),
              window[-1]["volume"] / mean_volume, current / statistics.mean(closes[-20:]) - 1]
    return result if all(math.isfinite(x) for x in result) else None


def barrier_label(future):
    """Use observable open before high/low; never guess same-bar ordering."""
    entry = future[0]["open"]
    target, stop = entry * 1.01, entry * 0.99
    for bar in future:
        if bar["open"] >= target:
            return 1.0, "target_first"
        if bar["open"] <= stop:
            return 0.0, "stop_first"
        hit_target, hit_stop = bar["high"] >= target, bar["low"] <= stop
        if hit_target and hit_stop:
            return None, "ambiguous"
        if hit_target:
            return 1.0, "target_first"
        if hit_stop:
            return 0.0, "stop_first"
    return 0.0, "expiry_without_target"


def prepare(args):
    history_path = Path(args.history).resolve()
    history_bytes = history_path.read_bytes()
    history = json.loads(history_bytes)
    if history["purpose"] != PURPOSE:
        raise ValueError("history purpose mismatch")
    partitions = {"train": [], "calibration": [], "holdout": []}
    counts = {name: 0 for name in ["ambiguous", "missing_features", "outside_windows"]}
    windows = {"train": ("2016-01-01", "2021-12-01"),
               "calibration": ("2022-02-01", "2023-12-01"),
               "holdout": ("2024-02-01", "2100-01-01")}
    for instrument in history["instruments"]:
        raw_path = (history_path.parent / instrument["raw_document"]).resolve()
        if not raw_path.is_relative_to(history_path.parent):
            raise ValueError("source path escapes history directory")
        raw = raw_path.read_bytes()
        if digest(raw) != instrument["saved_document_sha256"]:
            raise ValueError("raw source hash mismatch")
        request = instrument["request"]
        bars = validate_bars(json.loads(raw), instrument["symbol"],
                             dt.date.fromisoformat(request["start_date"]),
                             dt.date.fromisoformat(request["end_date"]))
        if bars != instrument["bars"]:
            raise ValueError("normalized bars differ from saved source")
        for i in range(20, len(bars) - 5):
            xs = features(bars, i)
            if xs is None:
                counts["missing_features"] += 1
                continue
            future = bars[i + 1:i + 6]
            label, outcome = barrier_label(future)
            if label is None:
                counts["ambiguous"] += 1
                continue
            row = {"instrument_id": instrument["instrument_id"],
                   "date": future[0]["date"], "feature_end": bars[i]["date"],
                   "feature_start": bars[i - 20]["date"], "label_end": future[-1]["date"],
                   "features": xs, "label": label, "outcome": outcome}
            partition = next((name for name, (start, end) in windows.items()
                              if start <= row["feature_start"] and row["label_end"] < end), None)
            if partition is None:
                counts["outside_windows"] += 1
            else:
                partitions[partition].append(row)
    for name, rows in partitions.items():
        rows.sort(key=lambda row: (row["date"], row["instrument_id"]))
        if len(rows) < 100 or min(sum(r["label"] == v for r in rows) for v in [0, 1]) < 20:
            raise ValueError(f"insufficient class support in {name}")
    for left, right in [("train", "calibration"), ("calibration", "holdout")]:
        if max(r["label_end"] for r in partitions[left]) >= min(r["feature_start"] for r in partitions[right]):
            raise ValueError("partition feature/label overlap")
    common = {"schema_version": 1, "purpose": PURPOSE, "deployment_eligible": False,
              "history_sha256": digest(history_bytes), "feature_names": FEATURES,
              "feature_schema_version": "daily_replay_features_v1",
              "label_version": "gross_barrier_1pct_5bars_v1", "limitations": LIMITATIONS}
    out = Path(args.out)
    fit_hash = publish(out / "fit.json", {**common, "partitions": {k: partitions[k] for k in ["train", "calibration"]}})
    holdout_hash = publish(out / "holdout.json", {**common, "partitions": {"holdout": partitions["holdout"]}})
    summary = {"schema_version": 1, "purpose": PURPOSE, "deployment_eligible": False,
               "windows": windows, "fit_sha256": fit_hash, "holdout_sha256": holdout_hash,
               "excluded": counts, "partitions": {k: {"rows": len(v), "positives": sum(r["label"] == 1 for r in v),
               "first": v[0]["date"], "last": v[-1]["date"]} for k, v in partitions.items()},
               "limitations": LIMITATIONS}
    publish(out / "dataset-report.json", summary)
    print(json.dumps(summary, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    f = sub.add_parser("fetch")
    f.add_argument("--selection", default=str(ROOT / "config/deployment.demo-selection.json"))
    f.add_argument("--start", default="2016-01-01")
    f.add_argument("--end", required=True)
    f.add_argument("--out", required=True)
    p = sub.add_parser("prepare")
    p.add_argument("--history", required=True)
    p.add_argument("--out", required=True)
    args = parser.parse_args()
    try:
        (fetch if args.command == "fetch" else prepare)(args)
    except Exception as error:
        # Never echo provider bodies, URLs, credentials, or arbitrary exception text.
        print(json.dumps({"status": "FAILED", "error_type": type(error).__name__,
                          "message": "Research pipeline failed; no approval or fallback was created."}), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

# QuantConnect LEAN and Rust bridge

## Overview

`[bridge.py:RustFormulaBridge]` connects the offline engine replay to the pinned
Rust calculator; `[run.py:main]` validates prepared development-fold jobs.

## Interface

- `[bridge.py:RustFormulaBridge]` verifies an exact Rust executable and invokes
  `trady weekly formula-bridge --file -`, which calls the real `calculate_limits`.
  The runtime method requires an ACCEPTED reply and returns the same full limits.
- `[bridge.py:main]` exposes `bridge.py --fixture --binary ABSOLUTE_TRADY --sha256 HASH`,
  rejects duplicate keys, JSON floats and nonstandard constants, then forwards the
  request to Rust. Rust owns FORMULA/DECIMAL tag validation; the adapter accepts
  ACCEPTED, DECIMAL and REJECTED replies and preserves their complete contents.
  This exercises the same adapter subprocess/wire path for the proof parity
  corpus; it does not itself run the LEAN engine or supply an independent formula.
- `[BridgeAlgorithm.py:TradyFormulaReplay]` consumes `/lab-input/replay.json`,
  executes the formula in the actual engine, tracks orders and writes measured
  equity to `/lab-output/trady-replay.json`.
- `[run.py:main]` exposes `run.py --lock FILE` as a development helper. Its fixed lock names
  `engine_executable:{path,sha256}`, pinned `dependencies`, `mode:"OFFLINE"`,
  `dataset_hash`, `timeout_seconds`, and `trials:{input_hash:{jobs:[...]}}`.
  Each job names fixed `arguments`, `working_directory` and a fresh `output`.
  One job is required per frozen development fold.
- `[BridgeAlgorithm.py:TradyFormulaReplay.initialize]` requires replay configuration
  with `mode:"OFFLINE"`, `data_kind:"REAL"`,
  `opens_at`, `closes_at`, `opening_cash`, `symbol`, `rust_binary`, `rust_sha256`
  and exact timestamp-keyed formula `inputs`. Engine job preparation must pin
  this file, every data file, fee/slippage/fill models, engine assembly and driver.

## Invariants

`[run.py:main]` requires OFFLINE development jobs with pinned dependencies and
absent prior outputs. Network/container isolation and trusted lock preparation
are deployment requirements. `[BridgeAlgorithm.py:TradyFormulaReplay.on_data]`
requires formula timestamps to match engine events; initialization checks frozen
cash within 0.000001 of engine cash. Missing observations yield cash or a blocked
fold. `[run.py:fold_metrics]` checks REAL data, expected sample count, finite
positive equity and first/last timestamps; it does not check intermediate
timestamp ordering or continuity. Open holdings are valued but never counted as
closed wins (`[BridgeAlgorithm.py:TradyFormulaReplay.on_end_of_algorithm]`).

`[BridgeAlgorithm.py:TradyFormulaReplay.on_end_of_algorithm]` explicitly reports
`broker_equivalence_verified:false`. Sibling
protective orders are a simulation with cancellation races; invalid short
holdings abort the replay. Faithful Trading 212 fills, costs, holding deadlines,
partial fills and all execution stresses still require an independent model and
validator. This bridge cannot alone establish economic release feasibility.

## Side Effects

`[bridge.py:RustFormulaBridge.fixture]` runs the pinned calculator with a 4 MiB
request bound and ten-second subprocess timeout. `[run.py:main]` runs the offline
engine; `[BridgeAlgorithm.py:TradyFormulaReplay.on_end_of_algorithm]` writes measured
results. Execute inside a prepared isolated lab image with read-only engine,
algorithm, input and data mounts, bounded writable output, and no network or
Docker socket. A deployment must supply a real pinned engine and rights-approved
historical data; this checkout does not include or claim a completed backtest.

## Verification

Run `python3 -B -m unittest discover -s lab/tests -v` for measurement/fail-closed
gates. Run the prepared engine jobs through `run.py --lock FILE` to obtain actual
development evidence. An unprepared candidate or missing dependency fails.

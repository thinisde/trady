# QuantConnect LEAN and Rust bridge

## Overview

`[bridge.py:RustFormulaBridge]` connects the offline engine replay to the pinned
Rust calculator; `[run.py:main]` validates prepared development-fold jobs.

## Interface

- `[bridge.py:RustFormulaBridge]` verifies an exact Rust executable and invokes
  `trady weekly formula-bridge --file -`, which calls the real `calculate_limits`.
  The runtime method requires an ACCEPTED reply and returns the same full limits.
  `[bridge.py:RustFormulaBridge.__init__]` checks an absolute regular executable
  and exact SHA256 at construction. Each `fixture`/`fixture_wire`/`calculate` call hashes a
  freshly opened non-symlink executable into a private memfd, seals those exact
  bytes against writes/growth/shrinkage, and executes the descriptor through
  `/proc/self/fd` with `pass_fds`. It rechecks the source after execution before
  using the reply. Source changes block the result; replacement between the hash
  and execution cannot run unverified bytes.
- `[bridge.py:main]` exposes `bridge.py --fixture --binary ABSOLUTE_TRADY --sha256 HASH`,
  decodes bounded UTF-8 and forwards the original JSON document through
  `[bridge.py:RustFormulaBridge.fixture_wire]`. Rust owns duplicate-key, JSON
  grammar, FORMULA/DECIMAL tag and typed schema validation. Forwarding preserves
  exponent, signed-zero and Unicode escape lexemes that Python decoding and
  reserialization could normalize. The adapter accepts
  ACCEPTED, DECIMAL and REJECTED replies and preserves their complete contents.
  This exercises the same adapter subprocess/wire path for the proof parity
  corpus; it does not itself run the LEAN engine or supply an independent formula.
  `[bridge.py:cli]` exits 0 for a valid semantic `REJECTED` reply as well as
  ACCEPTED/DECIMAL replies; `[bridge.py:RustFormulaBridge.calculate]` additionally
  requires ACCEPTED limits for runtime calculation.
- `[bridge.py:FormulaInputRejected]` identifies input parse/schema failures.
  `[bridge.py:cli]` reports these with exit 2, empty stdout and the exact prefix
  `LEAN formula fixture PARSE_REJECTED: `. Other exceptions use exit 1 and
  `LEAN formula fixture BLOCKED: <exception type>`.
- `[BridgeAlgorithm.py:TradyFormulaReplay]` consumes `/lab-input/replay.json`,
  executes the formula in the actual engine, tracks orders and writes measured
  equity to `/lab-output/trady-replay.json`.
  `[BridgeAlgorithm.py:TradyFormulaReplay.on_end_of_algorithm]` writes
  `schema_version`, `data_kind`, `equity`, `closed_trades`, `open_holdings`,
  `unfilled_entry` and `broker_equivalence_verified`.
- `[run.py:main]` exposes `run.py --lock FILE` as a development helper. Its fixed lock names
  `engine_executable:{path,sha256}`, pinned `dependencies`, `mode:"OFFLINE"`,
  `dataset_hash`, `timeout_seconds`, and `trials:{input_hash:{jobs:[...]}}`.
  Each job names fixed `arguments`, `working_directory` and a fresh `output`.
  One job is required per frozen development fold.
  Its reply is `{schema_version:1,input_hash,payload:{folds:[...]}}`. Each fold
  contains `interval`, decimal-string `mean_log_return` and `max_drawdown`,
  `closed_trades` and `complete` (`[run.py:fold_metrics]`, `[run.py:main]`).
- `[BridgeAlgorithm.py:TradyFormulaReplay.initialize]` requires replay configuration
  with `mode:"OFFLINE"`, `data_kind:"REAL"`,
  `opens_at`, `closes_at`, `opening_cash`, `symbol`, `rust_binary`, `rust_sha256`
  and exact timestamp-keyed formula `inputs`. Engine job preparation must pin
  this file, every data file, fee/slippage/fill models, engine assembly and driver.

## Invariants

`[run.py:main]` requires OFFLINE development jobs with pinned dependencies and
absent prior outputs. Network/container isolation and trusted lock preparation
are deployment requirements. `[BridgeAlgorithm.py:TradyFormulaReplay.on_data]`
requires formula timestamps to match engine events and checks frozen cash within
0.000001 of engine cash before calculation for an eligible request.
Missing observations yield cash or a blocked
fold. `[run.py:fold_metrics]` checks REAL data, expected sample count, finite
positive equity and first/last timestamps; it does not check intermediate
timestamp ordering or continuity. Open holdings are valued but never counted as
closed trades (`[BridgeAlgorithm.py:TradyFormulaReplay.on_end_of_algorithm]`).
`[run.py:main]` reads `MAX+1` stdin bytes without a separate oversized-input
rejection. Its subprocess output checks and those in
`[bridge.py:RustFormulaBridge.fixture_wire]` apply after captured output has been
allocated; the outer runner supplies the overall process/resource budget.

`[bridge.py:RustFormulaBridge._executable_snapshot]` requires Linux executable
memfd support, kernel file seals, procfs and descriptor inheritance. Missing or
denied support blocks execution; there is no path-based execution fallback.
Every use revalidates the original file, including its regular/executable type
and content. Read-only deployment mounts remain required for the engine, replay,
data and other dependencies; snapshotting the calculator does not pin those
separate artifacts. Refresh prepared dependency hashes after source updates.

`[bridge.py:RustFormulaBridge.fixture_wire]` raises `FormulaInputRejected` from
an inner Rust result only for exit 1, empty stdout and an exact allowlisted
`trady: ` diagnostic: `invalid JSON or duplicate object key`, `trailing JSON data`
or `input schema mismatch`. Source integrity is rechecked before classification.
Missing files, timeouts, other execution errors, signals and unrelated
`RuntimeError` messages remain generic failures; they cannot become parser
evidence. Local oversized or invalid UTF-8 documents also have explicit input
rejection. The finite wire checks are not a formal refinement proof.

`[BridgeAlgorithm.py:TradyFormulaReplay.on_end_of_algorithm]` explicitly reports
`broker_equivalence_verified:false`. Sibling
protective orders are a simulation with cancellation races; invalid short
holdings abort the replay. Faithful Trading 212 fills, costs, holding deadlines,
partial fills and all execution stresses still require an independent model and
validator. This bridge cannot alone establish economic release feasibility.

## Side Effects

`[bridge.py:RustFormulaBridge.fixture_wire]` runs the pinned calculator with a 4 MiB
request bound and ten-second subprocess timeout. `[run.py:main]` runs the offline
engine; `[BridgeAlgorithm.py:TradyFormulaReplay.on_end_of_algorithm]` writes measured
results. Execute inside a prepared isolated lab image with read-only engine,
algorithm, input and data mounts, bounded writable output, and no network or
Docker socket. A deployment must supply a real pinned engine and rights-approved
historical data; this checkout does not include or claim a completed backtest.

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `python3 -B -m unittest discover -s lab/tests -v` for measurement/fail-closed
gates. Run the prepared engine jobs through `run.py --lock FILE` to obtain actual
development evidence. An unprepared candidate or missing dependency fails.

`test_lean_integrity.py` verifies repeated runtime/fixture checks, source mutation
before and during execution, symlink/missing/nonregular replacements, kernel seal
enforcement and descriptor cleanup on sealing failures. These use temporary
synthetic executables; `test_lean_wire_adapter.py` separately checks the actual
Rust binary's unchanged wire replies.

[test_lean_bridge_errors.py](../tests/test_lean_bridge_errors.py) checks exact
parse/schema classification, missing-file/timeout/wrapped-signal failures,
integrity precedence, the CLI exit/diagnostic distinction and original wire
lexeme preservation. Run it with
`python3 -B -m unittest discover -s lab/tests -p 'test_lean_bridge_errors.py'`.

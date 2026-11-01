# Offline optimization tools

## Overview

These are executable bridges and validators, not completed research evidence.
No historical dataset, calibrated predictor, engine binary or
production approval is bundled.

## Subdirectories & System Boundaries

- [Historical research training](./training/README.md) owns bounded provider
  collection and offline binary-label preparation; its artifacts remain
  research-only.
- [LEAN replay](./lean/README.md) owns the engine replay and sealed Rust
  calculator bridge.
- [Lean kernel project](./proofs/README.md) owns pinned arithmetic proofs and
  finite executable parity checks.
- [Independent sealed evaluator](./evaluator/README.md) owns frozen-record
  acceptance and the isolated Unix worker boundary.
- `tests/` contains test-only fixtures and is intentionally outside the
  component-contract tree.

## Interface

- [Historical research training](./training/README.md) downloads real daily bars,
  prepares purged research-only partitions and uses the native Perpetual example.
  Its binary hypothetical barrier labels and offline scores are not execution
  records, four-outcome forecasts or deployment approval.

- `[reconcile.py:main]` exposes `reconcile.py --export FILE` to select one exact
  cutoff from `trady weekly export`. `[reconcile.py:reconcile]` recomputes canonical
  SHA256 for every supplied record; export authenticity, independent
  reconciliation and immutable storage remain deployment responsibilities.
  `[reconcile.py:canonical]` supplies sorted, compact UTF-8 JSON serialization
  with `ensure_ascii=False` for those hashes.
  Stdin requires `stage:"reconcile"`, `input_hash` and
  `manifest:{account_id,mode,cutoff_at}`. The export contains `account_id`, `mode`
  and `reconciliations`; each entry has `hash` and
  `content:{cutoff_at,report,unresolved_items}`. The selected report contains
  `unresolved_count`. Stdout is `{schema_version:1,input_hash,payload:report}`
  (`[reconcile.py:main]`, `[reconcile.py:reconcile]`).
- `[evaluate.py:__main__]` launches the evaluator client/direct TEST_ONLY interface;
  `[evaluator_worker.py:main]` launches the separate worker with required
  `--lock`, `--socket` and `--state-directory` arguments.
- [LEAN replay](./lean/README.md) invokes the real engine and a hash-verified,
  sealed-memfd Rust calculator, then records measured replay results. It reports
  `broker_equivalence_verified:false`
  (`[lean/BridgeAlgorithm.py:TradyFormulaReplay.on_end_of_algorithm]`).
- [Lean kernel project](./proofs/README.md) audits its declared pinned reference
  semantics and finite Lean/Rust/adapter parity, with 26 registered theorems
  (`[proofs/bindings.py:THEOREMS]`). Universal Rust refinement remains unproved.
- [Independent sealed evaluator](./evaluator/README.md) computes actual acceptance
  from frozen records and supports a separate private Unix worker identity.
  TEST_ONLY forces computed gates false (`[evaluator/run.py:evaluate]`);
  worker startup requires mandatory Linux seccomp isolation
  (`[evaluator/isolation.py:isolate_worker]`).
- `tests/test_helpers.py` verifies reconciliation and measured-return gates.

## Invariants

Missing tools, source rights, labels, complete fold valuations or exact input
bindings block the run (`[evaluator/run.py:evaluate]`, `[proofs/check.py:main]`).
Open holdings remain unrealized and pending forecasts
remain pending. A helper success never grants release or order authority.
`[reconcile.py:reconcile]` requires matching account/mode and exactly one identical
cutoff, checks all record hashes, and requires falsy unresolved fields on the
selected record. `[reconcile.py:main]` initially bounds export size to 4 MiB,
requires the reconcile stage and copies the supplied `input_hash` without
recomputation. Its ordinary `json.loads` is not the sealed evaluator's strict
parser; reading at most `MAX+1` request bytes has no separate length rejection.
Evaluator launchers report caught exception classes and exit 1; reconciliation
uses a fixed error message (`[evaluate.py:__main__]`,
`[evaluator_worker.py:__main__]`, `[reconcile.py:__main__]`).

## Side Effects

`[reconcile.py:main]` reads stdin/export data and writes its result envelope.
`[lean/run.py:main]` and `[proofs/check.py:main]` spawn pinned offline programs;
`[evaluator/transport.py:consume]` persists exposure latches and
`[evaluator/run.py:write_artifact]` writes diagnostics. Run those offline helpers
under the isolated lab identity, without internet, broker credentials, production
release credentials or the Docker daemon socket.

The separate `[training/pipeline.py:fetch]` collection command reads a Twelve Data
credential, performs bounded HTTPS GETs with a 13-second interval between symbols,
and writes original provider documents and normalized `history.json`. Its
`prepare` command and the native fit/tune/evaluate executable operate offline.
The [training contract](./training/README.md) describes their exclusive local
artifacts and research-only scope. Collection must not receive broker or release
credentials.

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `python3 -B -m unittest discover -s lab/tests -v` and
`python3 -B -m unittest discover -s lab/training -v`. The linked native trainer
uses `cargo test --locked -p trady-adapters --example train_candidate`.
Runtime integration uses
`trady weekly run --config FILE`, with five reviewed `HelperCommand` entries.
The evaluator's source data, policy, identity and tool pins must be prepared by
deployment. The proof worker owns the exact scope of the current formula
proof/parity project. Missing required evidence produces `BLOCKED`.

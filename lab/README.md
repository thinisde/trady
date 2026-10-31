# Offline optimization tools

## Overview

These are executable bridges and validators, not completed research evidence.
No historical dataset, calibrated predictor, engine binary or
production approval is bundled.

## Interface

- `[reconcile.py:main]` exposes `reconcile.py --export FILE` to select one exact
  cutoff from `trady weekly export`. `[reconcile.py:reconcile]` recomputes canonical
  SHA256 for every supplied record; export authenticity, independent
  reconciliation and immutable storage remain deployment responsibilities.
- `[evaluate.py:__main__]` launches the evaluator client/direct TEST_ONLY interface;
  `[evaluator_worker.py:main]` launches the separate worker with required
  `--lock`, `--socket` and `--state-directory` arguments.
- [LEAN replay](lean/README.md) invokes the real engine and Rust calculator.
- [Lean kernel project](proofs/README.md) audits its declared pinned reference
  semantics and executable parity scope.
- [Independent sealed evaluator](evaluator/README.md) computes actual acceptance
  from frozen records and supports a separate private Unix worker identity.
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
`[evaluator/run.py:write_artifact]` writes diagnostics. Run under the isolated lab identity, without internet, broker
credentials, production release credentials or the Docker daemon socket.

## Verification

Run `python3 -B -m unittest discover -s lab/tests -v`. Runtime integration uses
`trady weekly run --config FILE`, with five reviewed `HelperCommand` entries.
The evaluator's source data, policy, identity and tool pins must be prepared by
deployment. The proof worker owns the exact scope of the current formula
proof/parity project. Missing required evidence produces `BLOCKED`.

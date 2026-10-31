# Offline weekly lab adapters

## Overview

The deployment owns these helper configurations. Candidate parameters are data;
they do not select executables, argv, paths, secrets or release tools.

## Interface

- `[mod.rs:LabConfig]`, `[mod.rs:HelperLab]` configure and implement the
  application `WeeklyLab` port; `HelperLab::{new,config,verify_candidate}`
  expose construction, configuration and candidate verification.
- `[runner.rs:HelperCommand]`, `PinnedFile`, `HelperOutput` and `Response<T>`
  define the fixed argv protocol. `[runner.rs:MAX_INPUT_BYTES]` and
  `MAX_OUTPUT_BYTES` are 4 MiB; `bytes_hash` computes SHA256 and `trusted_file`
  checks configuration-file metadata. Requests and responses carry `schema_version=1` and `input_hash`.
- `[mod.rs:WeeklyLab for HelperLab]` reconciliation returns `ReconciledWeek`;
  `[mod.rs:DevelopmentPayload]` contains `folds`.
  Proof payloads contain scope, source/safety/toolchain hashes, theorem names,
  audited axioms and a witness count. Parity payloads contain actual Rust/Lean
  result pairs. Evaluation payloads use `IndependentEvaluation`; transport
  reports are stamped from the executed process, overriding supplied headers.
- Deployment composes `[mod.rs:HelperLab::evaluate]` with
  `lab/evaluate.py --socket FIXED_SOCKET --worker-uid FIXED_UID`, backed by
  the separately isolated `lab/evaluator_worker.py` and its
  root-owned sealed lock. The lab cannot read sealed data or worker state.

## Invariants

- `[runner.rs:PinnedFile::validate]` requires absolute regular files with exact
  SHA256 pins, at most 64 MiB, no final-component symlink and, on Unix, root
  ownership without group/other write permission. Resolve interpreter symlinks
  before pinning.
- `[runner.rs:HelperCommand::run_linux]` clears the inherited environment and
  supplies only fixed `PATH`/`LANG`, without a broker or database argument.
  `[runner.rs:HelperCommand::validate]` rejects the exact executable basenames
  `sh`, `bash`, `dash`, `zsh`, `env`, `sudo`, `su` and argument tokens
  `-c`, `--command`, `-e`, `--eval`. Only listed dependencies are pin-checked;
  deployment must include interpreter scripts and review every fixed argument.
- `[runner.rs:HelperCommand::run]` requires Linux. `run_linux` uses `prlimit`
  to bound CPU, address space, open files, processes, file size and
  core dumps; pipes, messages and elapsed time are bounded. A service cgroup is
  required to contain descendants, including programs that detach themselves.
- `[runner.rs:persist_artifact]` stores content-addressed stdout/stderr and receipts with
  exclusive creation, file/directory fsync and hash validation on retries.
- `[mod.rs:HelperLab::verify_candidate]` requires bound proof/parity hashes,
  theorem/axiom checks and 1–10,000 equal Rust/Lean result pairs.
  Scope `ARITHMETIC_CORE` cannot satisfy `FORMULA_V1` verification.
  The bundled full reference uses the theorem inventory in
  [bindings.py](../../../../lab/proofs/bindings.py). `required_theorems` must retain
  that reviewed inventory; `protected_safety_hash` is its combined safety-definition
  digest, rather than the hash of the earlier integer-only file. The
  [proof contract](../../../../lab/proofs/README.md) documents exact pin generation
  and the remaining translation/compiler trust boundary.
- `[mod.rs:HelperLab::evaluate]` validates run/dataset/plan binding; the
  production CLI places durable holdout claim/finish accounting around
  evaluation. Direct `HelperLab::evaluate` is an adapter call, not that boundary.

## Side Effects

`[runner.rs:HelperCommand::run_linux]` spawns fixed `/usr/bin/prlimit` and the
configured helper; `[runner.rs:kill_group]` also spawns fixed `/bin/kill`.
These fixed system programs are not `PinnedFile` entries. Execution uses pipe
threads and terminates process groups. `[runner.rs:persist_artifact]` writes
content-addressed stdout, stderr and receipts, with fsync and retry validation;
artifacts can remain after later payload validation fails. Pin validation reads
configured tools/dependencies. Database writes belong
to the CLI's fenced store wrapper. Helper filesystem/network isolation also
depends on the reviewed lab service/container; argv restrictions are not a
filesystem sandbox.

## Verification

Run `cargo test -p trady-adapters --test weekly_helpers --test shadow_recording --locked`.
Timeout, descendant cleanup, output/input limits, environment isolation,
tampering and misbound responses are exercised with explicitly synthetic tools.

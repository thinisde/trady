# Evaluator remediation: independent Goldfish verification

Date: 2026-10-31. Workspace: `/thinisde/trady`.

## Result

No actionable functional or documentation finding was identified in the requested remediation scope. Severity: none. Minimal fix: none.

The Unix-socket isolation, diagnostic-directory ownership/mode enforcement, and per-call calculator hash-to-execution integrity repairs are supported by the inspected implementation. This is a fresh-context source and test review, not an independent execution of the parent's reported 171-test result.

## Scope and method

- Read both supplied EGM standards in full: `/root/.codex/plugins/cache/egm/egm/0.1.0/docs/codex.md` (117 lines) and `skills/egm-bootstrap/references/templates.md` in that plugin (68 lines).
- Read all eight required implementation/unit/README files in full and both targeted test files in full. The required implementation and service unit total 687 lines; the two targeted tests total 385 lines.
- Supplemental source inspection covered `common.py:require/canonical/sha/valid_hash/strict_json/pinned_json`, both evaluator entrypoints, `BridgeAlgorithm.py:initialize/on_data`, and `test_lean_wire_adapter.py`, plus focused call-site searches. Supplemental excerpts total 176 lines.
- No parent history or memory was retrieved. No agents, services, external requests, builds, or test suites were launched. Repository files were not edited. This report is the only requested output artifact.
- Existing root-owned immutable dependencies, sealed-data preparation and deployment ownership/mount requirements were treated as the established trust boundary. No general deployment redesign or production refinement proof was imposed.

## Verification evidence

### PostgreSQL Unix socket isolation

`isolation.py:worker_process` (23–51) runs first in `transport.py:serve` (80–94). It requires Linux and a single existing thread, rejects inherited sockets/rings and other nonstandard descriptors, and limits connected standard-output/error sockets to the systemd journal endpoint. Ordinary stdio and the explicitly constrained root-owned read-only regular descriptors remain permitted.

`isolation.py:DENIED_SYSCALLS/isolate_worker` (20, 54–103) denies socket creation, socket pairs, outbound `connect`, and all three `io_uring` entrypoints with EPERM. It requires successful libseccomp setup, no-new-privilege, thread synchronization, load and local connection/socket self-tests. The actual IPC listener is bound before filter installation; lock and sealed artifact access follow successful installation. Listener `accept` and response writes remain available. Inherited descriptor checks and ordinary `recv` in `transport.py:receive` also avoid an existing DB connection or ancillary-descriptor import path through this protocol.

`trady-evaluator.service:SystemCallArchitectures/SystemCallFilter/SystemCallErrorNumber` (28–30) independently denies native outbound connects and io_uring. The worker's own socket/connect filter does not rely on PostgreSQL's filesystem location. `RestrictAddressFamilies=AF_UNIX` alone is not used as the database-isolation argument.

Read test evidence: `test_kernel_blocks_reachable_unix_database_socket_but_accepts_ipc` uses a synthetic PostgreSQL-named endpoint, attempts filesystem/abstract connects and new UNIX/IPv4/IPv6 sockets, checks actual io_uring syscall errors and synchronization to an existing thread, and still exchanges IPC ping/pong. The remaining `WorkerIsolation` tests cover inherited sockets/stdin, existing threads, unavailable isolation, failed self-tests and rejection before sealed input access. These are actual subprocess/kernel test bodies, not merely assertions about a configuration string. They were inspected, not rerun.

### Existing diagnostic directories and private report reuse

`run.py:diagnostics_directory` (205–241) rejects relative and parent-traversal paths, opens components relative to held directory descriptors with `O_DIRECTORY|O_NOFOLLOW`, checks ancestor ownership/write conditions, and requires final effective-UID ownership with `stat.S_IMODE(...) == 0o700`. The equality includes special permission bits. Existing unsafe directories are rejected without chmod/chown repair. Missing components are created with requested mode 0700.

`run.py:write_artifact` (244–267) creates hash-addressed reports exclusively relative to the validated descriptor and requires exact 0600. Reuse opens without following symlinks or blocking on FIFOs, then checks regular-file type, effective-UID owner, exact 0600, one link and identical bounded contents. File/directory fsync is present for new reports. Successful context exit also checks the directory identity and current descriptor ownership/mode. A pathname replacement cannot redirect the write into the replacement directory.

`transport.py:serve` (91–94) validates the diagnostic directory before `pinned_json` opens sealed datasets. The direct TEST_ONLY helper still reaches the same checked report writer; production direct-lock evaluation is rejected by `run.py:main` (285–286).

Read test evidence: `DiagnosticsBoundary` covers unsafe existing permission combinations including special bits, foreign ownership, relative/traversal/symlinked paths, public or multiply linked report reuse, symlink/FIFO reuse, and a directory replacement during report creation. `test_unsafe_diagnostics_configuration_blocks_before_sealed_artifact_access` checks startup ordering and absence of permission repair.

### Hash-to-execution integrity on every formula call

`bridge.py:RustFormulaBridge.calculate` (61–65) always delegates to `fixture`; `BridgeAlgorithm.py:TradyFormulaReplay.on_data` (50–59) uses that method for eligible formula requests. Constructor-only verification is no longer the execution boundary.

`bridge.py:RustFormulaBridge._verified_binary` (26–43) opens the source with `O_NOFOLLOW|O_NONBLOCK`, requires a regular executable, hashes the opened bytes, optionally copies those same bytes, and checks source metadata consistency plus the expected digest. `_executable_snapshot` (45–59) performs that copy for each call, applies write/growth/shrinkage/seal seals and verifies them, then yields the descriptor with cleanup in `finally`.

`fixture` (67–85) executes `/proc/self/fd/<descriptor>` with `pass_fds`; the source pathname is only argv[0], not the executable selected by the kernel. Replacing that pathname after copying therefore cannot select unverified program bytes. The original source is checked again before the response is accepted. Failed copying, verification or sealing cannot fall back to executing the source path.

Read test evidence: `FormulaExecutableIntegrity` checks repeated `calculate`/`fixture` verification, mutation before execution, symlink/nonregular/missing replacement, actual execution of the sealed trusted snapshot while the source is replaced with a marker-writing program, kernel rejection of snapshot writes, and descriptor cleanup on sealing failure. `test_lean_wire_adapter.py` separately compares fixture/CLI replies against an actual configured Rust binary when available; this review did not execute that binary or infer a completed LEAN backtest.

### EGM contracts

All three READMEs contain the component template's Overview, Interface, Invariants, Side Effects and Verification sections, with concrete file/symbol references. Their ordinary relative Markdown links resolve locally.

The relevant claims agree with the inspected symbols: evaluator isolation before sealed reads and no weaker worker test mode (`lab/evaluator/README.md:33–43,287–294`); exact diagnostic ownership/mode and descriptor-relative reuse (`291–294,395–417`); per-call sealed execution, Linux prerequisites and no path fallback (`lab/lean/README.md:10–19,53–59`); and unit plus worker isolation with no credentials (`services/runtime/README.md:18–27,118–130`). The LEAN contract separately identifies engine/data/dependency preparation and broker-equivalence limitations instead of presenting calculator snapshotting as proof of those properties.

Historical checkpoint counts and previous-review statements in the evaluator README were not adopted as independent verification evidence. The current remediation claims were assessed against source and test bodies.

## Required-file read fingerprints

SHA256 fingerprints identify the exact bytes reviewed; line references above apply to these versions.

```text
lab/evaluator/isolation.py                       103  c77600d47744fe5cc5faa3df38d72925c459b15ff0ce50d3e1688ecf17eb1928
lab/evaluator/transport.py                       118  ae6e554e9668cd014685c55c130690d827598b5fafecf4d4a211a0672de38455
lab/evaluator/run.py                             307  b29f79a6a69289880f42d66c4b4441525d6ed6abb85c029391bf584c146abdc2
lab/lean/bridge.py                               116  9cac09b42bb3e184d32695f3320b1b7dd326d18e87c2b633471c38a2afbc37fc
services/runtime/trady-evaluator.service          43  a8935543585c367432f291bd6acfb91c23257af54f63dee9a8901a486fb3efc5
lab/evaluator/README.md                          482  614788c4728a1365eab12f6ec5fb47b28f1ae05debc595a8aaeb618a4768dd98
lab/lean/README.md                                90  cded35d89ae1b8737f07a9427c2b6145166fc302b179ab10d18f23623f149527
services/runtime/README.md                       163  571b7ccb819b45446fa37ac62a95c16d4cc80128b6409392d5547f7c23313457
lab/tests/test_evaluator_security.py             284  dcfc7d4357bb5f5a910c3c676fd525df25c23cc0b34533df0bc4f7a54b9153d0
lab/tests/test_lean_integrity.py                  101  149bd2a7810c18babc01ff15cf0bed2fa6cc17af543b608a6be516dd79b164e9
EGM docs/codex.md                                117  a7390390c233ece29a2ee7508a8d328b6b4ee200f649417c097687c76f014ad0
EGM skills/egm-bootstrap/references/templates.md   68  f49793727cccbd72afdc4a0569a1d8e8138e943d2fc9b0df772b09deaca80437
```

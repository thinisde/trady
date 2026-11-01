# Weekly lab, shadow/release and isolated deployment handoff

This is a historical checkpoint. Subsequent evaluator isolation, executable
integrity and storage fixes are recorded in the
[audit remediation](../../docs/audit-2026-10-31/remediation.md), with new
verification results and pins.

Status: scoped implementation is stable, including the concrete sealed evaluator follow-up.
Existing uncommitted work was preserved;
no commit/push, installed service/timer, applied firewall, broker call or paid API
was performed. Root owns main/autonomous integration, migration registration,
Cargo manifests, cluster test harness and verified holding persistence.

## Goldfish repairs: latest stable scope

All reported round-one through round-five findings are repaired. These repairs change only
`lab/evaluator/{common.py,observations.py,predictions.py,statistics.py,run.py}`, the existing
`lab/tests/test_sealed_evaluator.py`, new
`lab/tests/{test_evaluator_counterexamples.py,test_acceptance_boundaries.py,test_evaluator_identity_scope.py,test_evaluator_lifecycle.py,test_evaluator_outcome_identity.py}`, and scoped evaluator/test/handoff
contracts. No Rust, migration, CLI, deployment unit or public protocol changed.
Fresh independent EGM verification found no actionable findings in the payoff
identity and session boundary repairs. Root completed the consolidated
drift/completion report. The completed root
integration/proof checks are recorded in
[the root completion report](../../docs/reports/2026-10-31_IMPLEMENTATION.md) and
[the proof handoff](../../lab/proofs/IMPLEMENTATION.md); no frozen proof source,
adapter or pin was changed for these evaluator repairs.

Latest round-five changed paths are `lab/evaluator/{observations.py,predictions.py}`,
`lab/tests/{test_evaluator_lifecycle.py,test_evaluator_outcome_identity.py}`, `lab/evaluator/README.md`,
`lab/tests/README.md` and this handoff. No new CLI hook, library dependency or
deployment action is required; existing helper dependencies must be re-pinned
before a later deployment.

- Validated financial values now become exact integer fractions. Equity/cash
  identities, stressed cost products, sector/probability sums, continuous totals,
  payoff subtraction, return numerators, parameter differences and project-cost
  hurdles cannot round away cancellation residuals. Exact terminating financial
  report totals use a context-independent decimal serializer. Report projections
  and statistical work may use floats; deterministic gates retain fractions.
- Model/transform fit completion must precede forecast generation, then recording
  and decision. A fit completed at 08:59 cannot support a forecast generated at
  08:01 and recorded at 08:02 for a 10:00 decision. An exact completion/generation
  boundary remains valid.
- Approved exchange sessions bound real labels, training payoff observations,
  every supplied closed candidate/neighbor trade (even without a closed-reference
  entry), and covered shadow closes. Entry is session one: three sessions end at
  session three's close; session four is too late. Forecast horizons also respect
  the candidate's holding bound. Calendar coverage through the declared expiry
  is required even for an early/pending label; weekdays and a seconds-only budget
  do not supply missing future sessions.
- Before calendar/account processing or inference, every registered candidate
  and neighbor must fit the frozen label horizon and bootstrap block, regardless
  of whether it has any trades. The maximum registered holding horizon and label
  horizon both bound the minimum block length.
- Conditional payoff means and session-block resampled means/quantiles,
  probability-weighted return, downside, uncertainty, complexity and utility use
  exact fractions. An independent margin is subtracted before the strict cash
  hurdle comparison, even when all three penalty weights are zero. The reported
  0.1/0.2/-0.3/0 mixture therefore has exactly zero utility and fails entry.
- Empirical Brier/prior deterioration, ECE and reliability bins, class frequency
  shift, Brier drift, session coverage and realized-minus-conditional payoff mean
  retain exact counts/probabilities/payoffs through acceptance. Log-loss alone
  uses approximate logarithms and a separate conservative upper margin.
- Continuous compounded drawdown, empirical CVaR95, worst session loss, turnover
  and exposure retain exact cash-flow-adjusted returns and observations through
  absolute and champion-relative gates. Values just beyond a cap cannot pass
  because their floating diagnostic projection equals the cap. Exact inclusive
  threshold equality remains valid. Rational diagnostics serialize numerator
  and denominator strings, including nonterminating ratios.
- `benchmark_parameters` and `benchmark_complexities` must have exactly the
  three frozen benchmark IDs, with object parameters and bounded integer
  complexities. Candidate, neighbor and benchmark identities must be disjoint
  before merging or reading observations. Extra candidate target 0.09 or neighbor
  horizon 5 keys cannot override frozen target 0.04 or horizon 3, even with
  internally matching execution receipt hashes and no neighbor trades.
- Source-dataset look accounting excludes valid reads of unrelated datasets.
  Current-dataset exposure remains global: zero or one exactly bound read is
  allowed; duplicates or changed run/candidate/input fail. All read IDs, scope
  hashes and timestamps remain validated. Cumulative trial correction still
  includes failed related account/mode/dataset attempts and the frozen ceiling;
  unrelated trial scopes do not alter that correction or planned-look factors.
- The nearby map audit additionally closes compound strategy/scenario and
  model/regime/horizon diagnostic collisions, so a later cohort cannot hide an
  ineligible one. Duplicate source record identities already fail through
  `unique`; approved model/transform maps retain separate frozen namespaces.
  Multiple actual forecasts in one dependence-group/session/scenario now
  contribute to an exact residual mean before diagnostic correlation, rather
  than selecting the last input row. Group IDs require bounded nonempty strings.
- Every actual trade now passes shared OPEN/CLOSED lifecycle validation before
  valuation/inference: exact evaluated strategy/scenario, real source identity,
  covered exchange entry and `entry <= recorded <= cutoff`. CLOSED also requires
  `entry < exit <= recorded` and covered exit. OPEN has no realized exit. These
  checks cover candidate, neighbor, benchmark and unreferenced rows; cash cannot
  trade assets. Shadow records use the same clocks/state checks independent of
  coverage, plus program/session binding and CLOSED holding/profit validation.
- Candidate trades require the original ENTER forecast. Any optional neighbor/
  benchmark forecast link uses the same source/model/fit/generation/recording
  clock and exact strategy/scenario binding. Entry must follow decision and
  precede expiry strictly, within the declared calendar forecast horizon, for
  OPEN and CLOSED alike. Valid OPEN holdings retain mark-to-market returns and
  report `open_trades`; they cannot supply `closed_trades`. Benchmark carry-in
  holdings are allowed, and no hypothetical OPEN close/holding-age cap is added.
- Model/regime/horizon cohorts register before pending labels are skipped. A
  pending-only ENTER cohort is visible with zero matured samples and fails its
  own calibration gate. All matured cohorts retain existing calibration gates,
  including ABSTAIN observations. A pending-only ABSTAIN cohort is diagnostic,
  with `required_for_acceptance:false`. Aggregate support and conditional-payoff
  checks remain mandatory. Null pending payoff/label values remain unscored;
  supported cohorts may mix matured/pending outcomes and the same-version refit
  keeps its statistical cohort. Cohort-key collision checks also cover pending
  cohorts, preventing censored rows from hiding an ambiguous diagnostic identity.
- Calibration history sorts by decision time, recording time and immutable
  prediction ID before its older/newer split. A reproduced equal-time export
  ordering counterexample now retains identical drift and acceptance under
  reversed input order.
- Payoff/sample IDs are export references. Original outcome identity is the label
  source hash, exchange session and normalized label start/end. All sample rows
  with that identity must agree on event and original availability/recording,
  even if the original payoff is absent. The payoff context adds regime, horizon,
  currency, scenario and execution/cost conventions. Conflicting exact financial
  or payoff-timing facts for one original/context fail even when unselected.
  An estimator rejects repeated original outcomes before counts, priors, means,
  conditional bounds or utility. Equivalent aliases selected singly across
  forecasts/refits share their canonical cache/multiplicity group; each forecast
  still validates its own fit membership and clocks. Distinct original sources/
  causal windows and separately validated contexts remain legal, with support
  required separately and no claim of statistical independence.
- Actual/shadow entries now consistently use `[open, close)` for OPEN and CLOSED,
  including unreferenced and benchmark carry-in records. The old OPEN-at-close
  fixture now uses 15:59. Inclusive open, inclusive close for exits, after-hours
  recording, legal carry-in, supplied valuations and unrealized/closed counts are
  preserved; no holding-age restriction or hypothetical OPEN close is added.
- TEST_ONLY still forces all five public gates false. Counterexample resealing
  preserves the same source dataset identity. The immutable holdout latch and
  private fixed-request worker protocol are unchanged. Re-pin the updated
  evaluator module files when preparing a deployment; the formula/LEAN bridge
  source pins are unchanged by this repair.

`python3 -B -m unittest discover -s lab/tests -v` passes **154 tests**, with no skips,
including 19 round-one, 24 round-two, 22 round-three, 34 round-four and 30 round-five
counterexample/boundary tests. The latest subset is
`python3 -B -m unittest discover -s lab/tests -p test_evaluator_outcome_identity.py -v`
(30 passed). Before editing source, in-memory TEST_ONLY probes reproduced both
findings and the favorable-alias variant: 12 originals failed a 13-per-event
minimum, but one renamed row per event reported 13 and enabled all private gates.
A favorable alias changed exact utility from `1/6000` to `27/52000`, crossing the
`0.0003` hurdle and enabling all private gates. The aligned 16:00 entry passed OPEN
but failed CLOSED; 15:59 passed. All returned public gates stayed false. Three
regression methods then failed against the pre-repair code; the final 30-test
subset and 154-test suite pass without skips. Controls include sample-ID aliases,
omitted/unselected original facts, exact decimal/timezone equivalence, cached fit
membership, legitimate original/context reuse, unsupported separate contexts,
all actual/shadow trade scopes and both states, inclusive exit/open boundaries,
microseconds before close, carry-in and unchanged supplied valuations.

The prior lifecycle/cohort subset is
`python3 -B -m unittest discover -s lab/tests -p test_evaluator_lifecycle.py -v`
(34 passed). Five new probes failed before repair: expired OPEN, future entry/
recording, future recording alone, the distinct approved-model/fit pending ENTER
cohort and equal-time forecast export ordering. Legal OPEN/calendar/recording
boundaries, benchmark carry-in, unreferenced/shadow controls and supported
mixed/null-pending cohorts pass.
The prior identity/scope subset is
`python3 -B -m unittest discover -s lab/tests -p test_evaluator_identity_scope.py -v`
(22 tests). Both benchmark override probes reproduced without rejection before
repair; the two unrelated historical reads reproduced sequential-look exhaustion.
Legal exact maps and unrelated history now preserve all five private gates while
every public TEST_ONLY gate stays false. The round-two subset
command is `python3 -B -m unittest discover -s lab/tests -p test_acceptance_boundaries.py -v`
(24 passed). All three round-two probes reproduced as failing tests before the
repair. The suite also covers the repeated 1000/1004 fabricated
equity probe, valid huge cancellation with exact accumulated totals, a 1e-54
stressed-cost residual, cash-flow adjustment, sector/exposure/probability residuals,
conditional and realized payoff subtraction, a payoff just beyond its exact
domain, pre-fit generation, legal fit/session endpoints, overlong labels/trades,
neighbor/shadow closes, pending labels and missing future calendar coverage.
Results also remain identical at Decimal precision 3/28/70 with different rounding
modes and Inexact/Rounded traps enabled. Syntax/whitespace checks passed for the
changed Python sources. These are synthetic correctness tests; no historical
backtest, provider call, production acceptance or live action is claimed.

### Round-five source fingerprints

Aggregates use SHA256 over lexicographically sorted `relative-path SHA256\n`
entries (one space and an actual trailing newline per entry). The nine-file
evaluator source scope is `lab/evaluate.py`, `lab/evaluator_worker.py` and all
seven `lab/evaluator/*.py`, including `__init__.py`:
`d40b9d4c3450a5b1cc99df57153a1c95af0825ea6264db92e35f5a8982a53845`.

| Changed Python source | SHA256 |
| --- | --- |
| `lab/evaluator/observations.py` | `af20427262d74c7a72934f42f5eecc82195f867d68e6131fb92da3789e4787b5` |
| `lab/evaluator/predictions.py` | `8da942cda0e09324918cdc88fcacd0148a0cc7eae5417353d79b481b7d232576` |
| `lab/tests/test_evaluator_lifecycle.py` | `7cd7532a2e55274f7b2a00c3add03e69e88a032a55a97a755ddcc281becafb74` |
| `lab/tests/test_evaluator_outcome_identity.py` | `06482e914f7da968fdb88a855a2a0e8ef0cb737dd7e8346b3289030efb61e76e` |

The 186 repository-visible Rust/SQL/Lean files, `lab/lean/bridge.py`, all
`lab/proofs/` files and proof-pin filenames (excluding `target` and `.git`)
retained their pre-edit aggregate:
`c0d78cba9573b12c0591e78b7c0c9d1c70d99f1c2812dbf04da43a6e964333d8`.
The implementation handoff message records the seven changed-file aggregate
before final review status notes, avoiding a self-referential document hash.
Root's final consolidation changes documentation only; the Python and protected
source fingerprints above remain current.

## Changed paths

- `migrations/0000_lab_roles.sql`: administrator-only restricted login roles.
- `migrations/0006_weekly_lab.sql`: lab leases, frozen state, cumulative campaigns
  and dataset budgets, trial registry, consumed holdouts, immutable forecast/label
  and reconciliation ledgers, content-addressed release evidence, audited
  CANDIDATE → VERIFIED → SHADOW → APPROVED → ACTIVE transitions and executor-only
  ACTIVE resolution. Explicit CONNECT grants account for migration 0001's revoke.
- `crates/trady-storage/src/weekly.rs`, one `pub mod weekly` export in `src/lib.rs`:
  `PgWeeklyStore`, shared-session `PgHoldoutLedger`, `PgForecastLedger`,
  `PgReleaseStore`, `PgActiveReleaseReader`, typed candidate/export records.
- `crates/trady-adapters/src/weekly_lab/{mod.rs,runner.rs,README.md}` and narrow
  module export: root-owned pinned helpers, argv-only execution, cleared
  environment, bounded stdin/stdout/stderr/deadlines, Linux process/resource
  limits, process-group cleanup and private content-addressed evidence/receipts.
- `crates/trady-adapters/src/shadow.rs` and narrow module export: network-free
  `ExecutionBroker`, durable idempotent proposal recording, explicit real/fixture
  source markers, no order/cancellation acknowledgement, private 0700/0600 files.
- `crates/trady-cli/src/{weekly.rs,release.rs}`: concrete CLI integration.
- `crates/trady-adapters/tests/{weekly_helpers.rs,shadow_recording.rs}`,
  `crates/trady-storage/tests/weekly_durable.rs`,
  `crates/trady-cli/tests/weekly_cli.rs`: targeted synthetic boundary tests.
- `lab/reconcile.py`, `lab/lean/{bridge.py,BridgeAlgorithm.py,run.py}`:
  read-only reconciled export helper and actual offline LEAN/Rust bridges.
- `lab/evaluate.py`, `lab/evaluator_worker.py`, `lab/evaluator/`: concrete sealed
  numerical evaluator, private Unix transport and durable exposure latch. Complete
  schemas, algorithms and policy settings are in `lab/evaluator/README.md`.
- `lab/proofs/{lean-toolchain,lakefile.lean,TradyProof.lean,Main.lean,Audit.lean,
  check.py,pin.py,TradyProof/Safety.lean}`: executable pinned kernel proof and
  proof/parity project. The completed FORMULA_V1 reference and exact trust
  boundary are maintained in `lab/proofs/README.md` and its final handoff.
- `lab/tests/{test_helpers.py,test_sealed_evaluator.py,sealed_fixture.py,
  test_lean_wire_adapter.py}` and scoped lab README contracts.
- `services/runtime/`: separate analyst/executor/gateway/lab systemd units, weekend
  timer, fixed UID/IPC groups, default-deny per-UID IPv4/IPv6 egress allowlists,
  disabled complete-config preparation helper and deployment contracts.
- Narrow additive Interface/Invariants/Side Effects/Verification entries in the
  three changed crate `src/README.md` files preserve existing prose.

## Exact integration hooks

Main uses `mod weekly; mod release;`, dispatches
`output(weekly::run(&args[1..])?)` and `output(release::run(&args[1..])?)`, and
appends both HELP strings. Root has implemented main dispatch and owns help text.

`trady weekly check|run --config FILE` reads a root-owned `RunConfig` containing
`schema_version`, `worker_id`, `lease_seconds`, `initial:WeeklyState`, and
`helpers:LabConfig`. Run executes the real application `run_weekly`, with durable
claim/finish accounting around sealed evaluation. Configure a lease longer than
two maximum helper deadlines plus 30 seconds. Each `HelperCommand` specifies
`tool_id`, pinned `executable`, fixed `arguments`, pinned `dependencies`, absolute
`working_directory`, private absolute `artifact_directory`, `timeout_seconds`,
`cpu_seconds`, `memory_bytes`, and `max_output_bytes`.

Other weekly commands:

```text
trady weekly export --account ID --mode paper|shadow|live --cutoff RFC3339
trady weekly forecast --account ID --mode paper|shadow|live --file FILE
trady weekly bridge --file FILE|-
trady weekly arithmetic-bridge --file FILE|-
trady weekly formula-bridge --file FILE|-
```

The real formula bridge calls `trady_policy::limits::calculate_limits`; it does
not calculate substitute prices. The arithmetic bridge supplies deterministic
integer fixtures for the Lean executable, and is explicitly not full formula
parity.
`formula-bridge` uses the proof worker's strict typed `Request` and
`proof_bridge::evaluate(&request)`, returning exact normalized full ACCEPTED,
DECIMAL or REJECTED replies. The LEAN runtime adapter uses this same bridge.
Its third-output fixture hook is:

```text
python3 -B lab/lean/bridge.py --fixture --binary /absolute/trady --sha256 HASH
```

It takes the tagged request on stdin and emits the same complete wire reply.
It exercises the actual adapter path; it does not run the LEAN engine.
The stable adapter source SHA256 is
`d59ac4dab9e1310b93e21b017bfd14043b264bfda98c2a35c7bfb82fc8df9619`.
The proof worker owns its lock updates and final source/theorem/parameter binding.
Use the final stripped executable and exact pins recorded in
[the completed proof handoff](../../lab/proofs/IMPLEMENTATION.md). Production
helper dependencies require a prepared binary no larger than the runner's
64 MiB pinned-file cap. The earlier 256,247,920-byte debug-binary checks below
are historical and do not describe the completed pinned proof run.

Production `helpers.evaluate` is the fixed command:

```text
/usr/bin/python3.14 -B /opt/trady/lab/evaluate.py --socket /run/trady-evaluator/evaluator.sock --worker-uid 13005
```

Use the host's real resolved root-owned interpreter path, not a symlink. Pin
Python, the entrypoint and all evaluator modules in `dependencies`. Configure
its private artifact directory and existing runner limits. The separate
`trady-evaluator.service` reads only its root-owned sealed lock/data, has no
credentials or provider/DB route, and shares an IPC group with the lab. The lab
cannot read its sealed files or 0700 state. PostgreSQL still owns the outer durable
claim/finish boundary; the worker additionally consumes an immutable source-data
exposure latch before calculation, surviving failure/restart/lock replacement.
`--lock` directly is restricted to TEST_ONLY. No service was installed or started.

Release commands:

```text
trady release candidate --file FILE
trady release show|formula --id ID
trady release verify --candidate FILE --helper-config FILE
trady release evidence --kind calendar|session|readiness --file FILE
trady release approve --candidate FILE --valid-until RFC3339
trady release advance --id ID --revision N --state VERIFIED|SHADOW|APPROVED|ACTIVE --evidence FILE
```

Credentials stay in the existing credential store: `TRADY_LAB_DSN`,
`TRADY_VERIFIER_DSN`, `TRADY_SHADOW_DSN`, `TRADY_RELEASE_OWNER_DSN`,
`TRADY_RELEASE_DSN`. A verifier helper must return complete `FORMULA_V1`,
`evidence_class:PRODUCTION`, exact source and actual root-owned pinned
proof/parity/evaluation content plus all independent gates. Test/demo evidence
is rejected by the database as well. Owner and releaser are distinct roles.

Runtime may use
`PgActiveReleaseReader::connect(executor_dsn)?.resolve_active(&account,mode)`;
the executor has EXECUTE only on `release.current_active(text,text)`, with no
release mutation grant. Root integrates this function directly through the
existing executor session. The exact runtime gate mapping is in the deployment
README. Existing holdings bypass the new-entry gate to preserve management.

Shadow uses `ShadowBroker::new(directory, ExecutionObservation, ShadowSource)`
and `record_proposal(&BrokerOrderRequest) -> Result<String>`. It implements the
current `ExecutionBroker` port. `observe` returns supplied observed facts;
`submit_order`/`cancel_order` record suppression and return Unsupported. Root's
separate shadow cycle calls `record_proposal` without marking an execution intent
sent. It has no broker client, credential or network field.

Deployment uses `autonomous analyst-service` over
`/run/trady-analyst/analyst.sock`, analyst-owned parent 0750 and socket 0660,
with only analyst/executor in the IPC group. Executor mode never needs analyst
credentials when the socket is configured. Units/configuration are artifacts
for a later explicit deployment; none was installed or started.

## Verification performed

- `cargo check -p trady-cli`: passed with root CLI wiring present.
- `cargo test -p trady-adapters --test weekly_helpers --test shadow_recording --
  --test-threads=1`: 4 passed, including private modes, restart/idempotence,
  timeout with descendants, input/output caps, secret isolation and tampering.
- `cargo test -p trady-cli --test weekly_cli`: 2 passed, including real CLI
  floor/ceil/cap boundaries, strict formula bridge requests, exact decimal wire
  normalization, zero-divisor and unknown-field rejection.
- `python3 -B -m unittest discover -s lab/tests -v`: 154 passed after all five repair rounds,
  covering numerical metrics, cost stress, forecast/payoff support, availability,
  purging, source/request tampering, search budgets, incomplete shadow weeks,
  persistence, bounded private IPC and the actual LEAN/Rust wire adapter. Every
  evaluator fixture remains TEST_ONLY with all production acceptance gates false.
- Root independently completed the corrected clean proof run: 24 audited
  theorems, 647 exact Lean/production-Rust/adapter cases, ten negatives, 172
  Rust/SQL pins including all eleven SQL files, and eighteen proof pins. See
  [the final proof handoff](../../lab/proofs/IMPLEMENTATION.md) for reproduction,
  final digests, scope and superseded evidence. Earlier 639-case adapter-function
  results and the 55-second debug-binary CLI timeout are historical checkpoints,
  superseded by this completed stripped-binary run.
- Root reports workspace formatting, strict Clippy and all 357 Rust tests
  passed after the final RR regression. All twenty disposable PostgreSQL tests
  passed, including the three `weekly_durable` restart/concurrency, cumulative
  budget/checkpoint/holdout, roles and tampering cases. Those tests remain
  explicitly ignored outside `scripts/test-postgres.py` and require isolation.
  [The root completion report](../../docs/reports/2026-10-31_IMPLEMENTATION.md)
  records the integration commands and scope. These evaluator repairs do not
  change that frozen Rust/SQL/Lean/adapter inventory.
- Python AST syntax and explicit source/document whitespace checks passed for
  the repaired component files. Scoped EGM contracts were checked against their
  real functions and existing file links.
- `systemd-analyze verify`: target binary `/opt/trady/bin/trady` is not installed,
  so this cannot be a deployment-success claim. The initial ignored
  `RuntimeMaxSec` for oneshot units was corrected to `TimeoutStartSec=3600`.

## Remaining real limitations

- The completed proof report defines FORMULA_V1's reference/theorem and finite
  executable-parity scope and trust boundary. It does not establish market-data
  truth, profitability, future broker behavior or universal Rust refinement.
  The QuantConnect engine and deployment data were not run here. Tools require
  actual pinned local dependencies and fail closed without them. Context7 was
  quota-blocked; installed PostgreSQL sources and official Rust/Lean/QuantConnect
  sources were consulted instead.
- No trained calibrated predictor, licensed historical dataset, deployed approved
  helper configuration, approved exchange calendar or
  twenty real future exchange-session recordings is fabricated or bundled.
  These owner/deployment inputs remain mandatory. The actual evaluator algorithm
  is implemented; deployment supplies independently frozen original records,
  their trusted provenance/normalization, policy settings, file/tool pins and
  its separate OS identity. A hash alone does not prove provider or engine truth.
   Original-label uniqueness depends on faithful source/session/window
   normalization; aliases cannot supply weights, but inventing a new source hash
   or falsifying original context is not made true by that hash. The evaluator
   cannot attest provider facts absent trusted original records.
- Conditional payoff/calibration/risk accounting and deterministic gates are
  exact for validated decimal inputs. Performance objective/bootstrap and log
  loss retain floating statistical approximations with independent conservative
  margins. Their dependence assumptions and observed-data scope do not imply
  future profitability. Large exact fractions remain subject to the existing
  worker resource, deadline and diagnostic-output budgets; exhaustion blocks
  evaluation.
- The LEAN adapter performs real replay when prepared, but does not establish
  broker-equivalent partial fills, protection races, holding deadlines or cost
  stresses. It reports this limit and cannot alone supply economic acceptance.
- Shadow is observational/proposal recording, with no simulated fill/performance
  claim. Accepted session attestations require the trusted shadow role, a
  hash-verified approved calendar, complete real observations, no order mutation
  and twenty distinct prospective sessions. Their real provenance is an
  independent ingestion/attestation boundary, not inferred from a local label.
- OS cgroups/read-only mounts/firewall policy must contain trusted helpers;
  executable allowlisting alone is not a filesystem/network sandbox. Tools that
  detach escape process groups, so the supplied service KillMode/control-group
  boundary remains required.
- This change does not deploy the topology, set passwords, configure route/IP
  allowlists, enable the timer, activate a release or authorize live trading.

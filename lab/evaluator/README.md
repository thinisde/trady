# Independent sealed evaluator

## Overview

The implementation computes acceptance from frozen account, forecast and payoff
records. It does not accept a supplied pass verdict, a supplied performance
estimate or code selected by a candidate. It needs deployment data; the bundled
fixtures are explicitly TEST_ONLY and cannot produce production acceptance.

## Interface

`[run.py:evaluate]` returns `(payload, diagnostics)` and `[run.py:reply]` emits the
typed `schema_version`/`input_hash`/`payload` helper envelope.
`[common.py:MAX_BYTES]` bounds artifact JSON to 32 MiB and
`[common.py:MAX_ROWS]` bounds record collections to 100,000 rows; these are
separate from the 4 MiB transport-message limit (`[transport.py:receive]`).

Production uses a separate Unix worker identity. The reviewed helper command is:

```text
/usr/bin/python3.14 -B /opt/trady/lab/evaluate.py --socket /run/trady-evaluator/evaluator.sock --worker-uid 13005
```

The worker entrypoint, supplied as an unstarted deployment unit, is:

```text
/usr/bin/python3 -B /opt/trady/lab/evaluator_worker.py --lock /etc/trady/evaluator/lock.json --socket /run/trady-evaluator/evaluator.sock --state-directory /var/lib/trady-evaluator/exposures
```

`evaluate.py --lock ABSOLUTE_FILE` supports direct TEST_ONLY checks. It rejects
PRODUCTION locks. The worker reads only its root-owned lock and three pinned
root-owned input files. The lab identity cannot read those files or worker state.
`[transport.py:serve]` requires a dedicated Linux process. Before reading the lock
or sealed inputs, `[isolation.py:worker_process]` rejects existing threads and
inherited sockets/rings; root-owned read-only regular descriptors and ordinary
stdio are allowed, with connected stdout/stderr restricted to systemd's journal.
After binding its listener, `[isolation.py:isolate_worker]` installs a mandatory
libseccomp filter across all threads and descendants: new sockets, socket pairs,
outbound `connect`, and all three `io_uring` syscalls return EPERM. Unsupported
architectures terminate the process. Accepted connections still support the
existing bounded request/reply protocol. Startup uses only local synthetic
sockets to check the denial; missing Linux/procfs, `libseccomp.so.2`, filter/TSYNC
support or a failed self-test blocks the worker. There is no TEST_ONLY bypass.
The client verifies the socket and parent owner against the fixed worker UID.
IPC uses bounded four-byte length-prefixed JSON, a 4 MiB message limit and
120-second socket deadlines. The outer Rust runner adds a wall/CPU/memory/output
budget. Configure its timeout above the expected evaluation duration and no
greater than the deployment's reviewed budget.

The example uses this checkout's resolved Python path; select and pin the actual
root-owned interpreter on the deployment host. A `/usr/bin/python3` symlink is
not accepted as a `HelperCommand` executable. Pin the resolved executable, both entrypoints and every evaluator module
as `HelperCommand` dependencies. Do not pass a candidate-controlled lock, socket,
UID, working directory, file path, script or command. The worker has no database,
provider, broker, owner or release secret. The production CLI still performs its
transactional PostgreSQL holdout claim/finish around this helper call.

The input is the Rust `HelperLab` evaluate request: `schema_version:1`,
`stage:"evaluate"`, `input_hash`, `manifest`, `plan`, `trial`, `run_id`, `plan_hash`
and `parameters_hash`. Rust computes the last three identities from the actual
typed objects. The lock also binds the canonical entire request, so changing its
contents while retaining those identity strings is rejected.

All input financial values are decimal **strings**, with at most 28 significant
digits and scale 28. Timestamps require an explicit offset. Integers are bounded;
JSON floats, duplicate keys/identities and nonfinite values are rejected. Input
files use canonical JSON: sorted keys, UTF-8, no insignificant whitespace,
`ensure_ascii=False`, `allow_nan=False`. `[common.py:canonical]` is the implementation.

After validating a decimal spelling, `[common.py:number]` returns its exact integer
fraction. All monetary identities, cost products, sector/probability sums,
cash-flow-adjusted returns, payoff means and utility, empirical calibration,
hard risk limits, parameter steps and project-cost hurdles use exact rational
arithmetic through their acceptance comparisons. They do not use the ambient
Decimal arithmetic context. Monetary report totals use an exact terminating
base-ten serializer, including totals longer than an individual input. Exact
ratios in diagnostics use `{numerator,denominator}` strings, including
nonterminating decimal means. Existing numerical report fields are floating
point projections for inspection; acceptance never reads those projections.

The deployment-owned lock has these required fields:

| Field | Meaning |
| --- | --- |
| `schema_version`, `evidence_class` | `1`; `PRODUCTION` or `TEST_ONLY` |
| `candidate_frozen_at`, `sealed_at` | Explicit candidate/policy/holdout freeze chronology |
| `request_sha256` | SHA256 of canonical complete helper request |
| `bindings` | Exact `input_hash`, `run_id`, `plan_hash`, `parameters_hash`, `dataset_hash`, `candidate_id`, `account_id`, `mode` |
| `policy` | Frozen evaluator settings below |
| `artifacts` | `dataset`, `search`, `shadow`, each `{path,sha256}`; absolute regular non-symlink file, root owner, no group/other write |
| `diagnostics_directory` | Absolute worker-owned directory with exact mode 0700; no symlink components or `..` |

The dataset artifact is a normalized evaluation bundle. Its content hash is
separate from `manifest.dataset_hash`: that latter hash is SHA256 of the frozen
`source_manifest`, shared across all strategies and attempts on the same original
data. A different strategy/output file must never create a fresh holdout identity.
The administrator must preserve that source identity when exporting the durable
search/holdout ledger. `source_manifest` has `schema_version:1`, nonempty unique
`input_ledger_hashes` and `normalization_hash`, all real SHA256 identities.
The evaluator checks those identities against the run; it does not establish the
truth of a provider or engine receipt merely from a hash string.

The dataset fields below are checked by `[run.py:evaluate]`,
`[observations.py:calendar_index]`, `[observations.py:validate_fits]`,
`[observations.py:validate_trades]`, `[observations.py:equity_series]` and
`[predictions.py:score_predictions]`:

| Field | Required record schema |
| --- | --- |
| `schema_version`, `evidence_class`, `source_manifest` | Version, immutable evidence class and source identity above |
| `calendar.sessions` | `session_id`, `opens_at`, `closes_at`, `phase:TRAIN/OUTER/SHADOW`, `regime`; OUTER adds `fold_id`, SHADOW adds `week_id` |
| `calendar.weeks` | `week_id`, complete `session_ids`, explicit `closes_at`, including future/incomplete shadow weeks |
| `approved_models`, `approved_transforms` | Version/identity to exact content hash maps, bound to run components `models` and `calibrators` |
| `samples` | `sample_id`, `session_id`, `outcome`, `label_source_hash`, `feature_start_at`, `feature_end_at`, `available_at`, `recorded_at`, `label_start_at`, `label_end_at`, `label_available_at`, `label_recorded_at` |
| `fits` | `fit_id`, `model_hash`, `transform_hash`, `fitted_at`, `outer_fold_ids`; separate `train`, `calibration`, `development`, each with `opens_at`, `closes_at`, nonempty disjoint `sample_ids` |
| `neighbors` | Unique `strategy_id`, exact complete `parameters`, differing by one predeclared step from the frozen candidate |
| `equity` | One complete run for every candidate/neighbor/benchmark × stress scenario; schema below |
| `trades` | `trade_id`, `strategy_id`, `scenario_id`, `state`, `entry_at`, `exit_at`, `recorded_at`, `source_hash`; candidate trades also bind `prediction_id`, `scenario_hash` |
| `forecasts` | Original normalized forecast and matured/pending label schema below |
| `payoff_observations` | Original matured training payoff schema below |

Each equity run has `strategy_id`, `scenario_id`, `currency`, `complexity_count`,
`spread_multiple`, `latency_ms`, `execution_spec`, `execution_receipt_hash` and
ordered `sessions`. The execution spec is exactly `{engine_hash,dataset_hash,
strategy_id,parameters,scenario_id,cost_multiple,spread_multiple,latency_ms}` and
its SHA256 must match the receipt hash. Engine identity matches run component
`engine`. Candidate/neighbor parameters must match their actual frozen objects;
benchmark parameters/complexity match the frozen policy.

`benchmark_parameters` and `benchmark_complexities` must each have exactly the
three `benchmark_ids` keys, with no extra candidate/neighbor entry or omitted
benchmark. Parameter values are objects; complexities are integers in `[0,10000]`.
The candidate, all registered neighbors and all benchmarks occupy disjoint
strategy identities. These checks precede merging the maps and reading any
calendar/account observations, so receipts cannot override frozen parameters.

Every session row has `session_id`, `opening_equity`, `closing_equity`,
`boundary_cash_flow`, `gross_pnl`, `fixed_cost`, `variable_cost_unscaled`,
`available_at`, `recorded_at`, empty `unresolved_order_ids`, nonempty
`source_receipts`, `start_snapshot_hash`, `end_snapshot_hash`, `exposure_fraction`,
`turnover`, `orders_attempted`, `entry_fills`, `closed_trade_ids`,
`sector_gross_pnl` and `sector_exposure_fraction`. Sector maps must sum exactly to
gross P&L and exposure. Snapshot hashes and equity connect across every session
and fold. Costs are charged exactly once:

```text
closing = opening + boundary_cash_flow + gross_pnl
          - fixed_cost - variable_cost_unscaled * frozen_cost_multiple
return = (closing - boundary_cash_flow - opening) / opening
```

Cash flows are boundary flows under this convention. Intraday cash flows need
an independently normalized ledger before this input can be used. All strategies
must share opening capital, currency and flow dates/amounts. Cash has no asset
exposure, turnover, fills, closed trades or execution costs. No equity reset,
missing session, unresolved order or open trade counted as closed is accepted.

`validate_trades` applies `trade_lifecycle` to every supplied actual trade,
including OPEN, unreferenced and benchmark rows. Each trade must belong to an
evaluated strategy/scenario, have a valid source hash and use state OPEN or
CLOSED. Every actual/shadow entry uses `opens_at <= entry_at < closes_at`,
independent of trade state, coverage, references or benchmark carry-in. A CLOSED
exit may equal its session close. Actual entry lies in a covered session, and
`entry_at <= recorded_at <= cutoff`; recording may occur after market close.
CLOSED additionally requires `entry_at < exit_at <= recorded_at` and a covered
exit session. OPEN has absent/null `exit_at`; an apparent realized exit is an
inconsistent state. Cash cannot contain asset trades. Missing/unknown lifecycle
facts block evaluation rather than being skipped.

Validated OPEN holdings remain in the actual mark-to-market equity path and are
reported separately as `open_trades`. Only matching CLOSED references contribute
to `closed_trades`. Unreferenced CLOSED rows still undergo lifecycle and relevant
holding checks, but do not invent a reported closure. Benchmark carry-in holdings
may precede the outer period and retain their frozen benchmark holding contract;
no candidate holding-age limit or hypothetical close is imposed on an OPEN row.

Every forecast has `prediction_id`, `strategy_id`, `session_id`, `model_fit_id`,
`model_hash`, `model_version`, `transform_hash`, `regime`, `horizon_sessions`,
`scenario_hash`, `execution_convention_hash`, `currency`, `source_hash`,
`dependence_group_id`, `observed_at`, `available_at`, `generated_at`, `recorded_at`,
`decision_at`, `valid_until`, `decision:ENTER/ABSTAIN`, `probabilities`,
`payoff_sample_ids`, `label_end_at`, `outcome`, `label_available_at`,
`label_recorded_at`, `label_source_hash`, `opening_equity`,
`realized_gross_profit`, `realized_fixed_cost`, `realized_variable_cost`.
The probability map has exactly TARGET_FIRST, STOP_FIRST, EXPIRY and NON_FILL,
summing to one without renormalization. Pending `outcome:null` is counted pending
and never scored; unavailable label/result fields, including realized payoff
amounts, may be null. The declared `label_end_at` remains explicit so horizon and
future-calendar checks still run. Original source hashes
must resolve to the immutable forecast ledger through the trusted data export;
this schema is normalization, not a replacement for original Prediction DTOs.

Candidate trades require their original ENTER forecast. Neighbor/benchmark trades
may omit a model forecast; any supplied link is validated rather than ignored.
`forecast_context` checks source, approved model/fit/transform, generation and
recording order, decision calendar and probability partition before scoring or
entry use. Every linked OPEN/CLOSED entry must match its strategy/scenario and
satisfy `decision_at <= entry_at < valid_until`, within the declared calendar
forecast horizon. Non-benchmark CLOSED exits retain their applicable holding
bounds. Marking a trade OPEN cannot bypass the forecast's fit, recording or expiry.

Every payoff observation has `observation_id`, `sample_id`, `session_id`, `event`,
`gross_profit`, `fixed_cost`, `variable_cost`, `opening_equity`, `source_hash`,
`available_at`, `recorded_at`, `regime`, `horizon_sessions`, `currency`,
`scenario_hash`, `execution_convention_hash`, and
`cost_convention:"EXECUTABLE_PRICES_PLUS_RESIDUAL"`. These are actual matured
account-unit outcomes from the fitted model's **training** partition. Event,
session and label hash must match the original sample. Payoffs from calibration,
development, future or outer labels cannot train the estimator. Scenario,
execution convention, currency and horizon must match the forecast being scored.

`observation_id` and `sample_id` are export references, not independent outcomes.
`outcome_identity` uses the original `label_source_hash`, exchange `session_id`
and normalized `label_start_at`/`label_end_at`. All samples with that identity
must agree on outcome and original label availability/recording, even when their
payoff row is omitted. Feature windows and renamed references cannot create
additional realized outcomes. Distinct original sources or distinct causal
windows within one source remain distinct; their truth must be established by
the trusted original-ledger normalization, not by a new hash alone.

`payoff_identity` adds the estimator's regime, horizon, currency, scenario,
execution convention and cost convention. All exported aliases for the same
outcome/context must agree on exact gross profit, costs, opening equity and
payoff availability/recording. Contradictory aliases fail even if unselected.
One estimator may reference each original outcome/context once; repeated aliases
are rejected before support counts, priors, means, bootstrap bounds or utility.
Selecting an equivalent alias singly across forecasts/refits is allowed and
shares the same estimator cache and multiplicity group. Every forecast still
checks its own fit membership, causal clocks and context. Separately validated
contexts may reuse originals with different context-specific payoffs; each
requires its own support and cannot borrow another context's samples. Reuse is
not a claim of statistical independence; session-block dependence is retained.

The search artifact has `schema_version`, `evidence_class`, `trials`,
`holdout_reads`. Each trial has `trial_id`, `candidate_id`, `parameters_hash`,
`run_id`, `plan_hash`, `status:REGISTERED/FAILED/COMPLETE`, `account_id`, `mode`,
`dataset_hash`, `registered_at`. Include every related historical attempt,
including failures. Each read has `read_id`, `dataset_hash`, `run_id`,
`candidate_id`, `input_hash`, `consumed_at`. Zero current reads means PostgreSQL
has just claimed it outside the export; one exact current read is allowed;
another candidate/previous exposure is rejected. Look accounting counts only
reads of the current original source dataset. Valid history for other datasets
does not consume this inference's look allowance. Read identities are globally
unique; every read still needs valid dataset/input hashes and a nonfuture
timestamp. Any current-dataset read must match the exact run, candidate and input,
even if it came from another account/run. Trial correction separately includes
every related account/mode/dataset attempt and the frozen cumulative ceiling;
the predeclared planned-look correction is unchanged. The worker adds a durable
exclusive source-dataset exposure latch before computation; changing a lock
cannot erase that latch, even after process restart or evaluation failure.

The shadow artifact has `schema_version`, `evidence_class`, `candidate_id`,
`program_started_at`, `observations`, `trades`. Each observation has `session_id`,
`first_observed_at`, `last_observed_at`, `recorded_at`, `data_kind:REAL`,
`order_mutations:0`, at least two distinct `observation_hashes`, `market_data_hash`.
Each trade has `trade_id`, `candidate_id`, `source_hash`, `state`,
`entry_session_id`, `exit_session_id`, `entry_at`, `exit_at`, `recorded_at`,
`net_profit`. Only prospective, fully covered approved calendar weeks and matured
closed trades count. Release additionally requires its separate twenty-session
and owner-scope gates; evaluator week counts do not replace that lifecycle.
All supplied shadow trades undergo the same lifecycle validation regardless of
coverage or closure. Entry must follow program start and match its actual session;
exit session/state must agree. OPEN does not provide a realized profit/closure.
Every CLOSED trade must have a finite net profit and obey the candidate holding
horizon; only covered closures contribute to the reported shadow count.

Required policy settings are frozen by run component `sealed_evaluator_policy`:

| Group | Keys |
| --- | --- |
| Identity | `schema_version`, `frozen_at`, `account_currency`, `benchmark_ids` (champion, cash, simple_baseline), `benchmark_parameters`, `benchmark_complexities` |
| Stress | `stress_cases`: `scenario_id`, `cost_multiple`, `spread_multiple`, `latency_ms`; base cost 1 and at least one cost ≥2 required |
| Inference | `block_sessions`, `bootstrap_repetitions`, `planned_looks`, `alpha`, `numeric_margin`, `annual_sessions` |
| Boundaries | `max_label_horizon_sessions`, `max_label_horizon_seconds`, `embargo_seconds`, `publication_latency_seconds`, `maximum_forecast_age_seconds` |
| Support | `minimum_calibration_samples`, `minimum_calibration_sessions`, `minimum_calibration_samples_per_event`, `minimum_payoff_samples_per_event`, `minimum_forecast_coverage`, `reliability_bins` |
| Risk | `maximum_drawdown_increase`, `maximum_cvar_increase`, `maximum_worst_loss_increase`, `maximum_turnover_increase`, `maximum_drawdown`, `maximum_exposure`, `maximum_session_return`, `maximum_payoff_abs_return` |
| Forecast quality | `maximum_brier_deterioration`, `maximum_log_loss`, `maximum_ece`, `maximum_class_shift`, `maximum_brier_drift`, `maximum_payoff_mean_error` |
| Economics | `utility_risk_weight`, `utility_uncertainty_weight`, `utility_complexity_weight`, `cash_hurdle`, `project_cost_account_currency` |
| Sensitivity | `neighborhood_steps`, `neighborhood_maximum_objective_drop` |

Thresholds are owner-reviewed experiment settings, not recommendations. A complete
executable schema example is [the synthetic fixture](../tests/sealed_fixture.py).
It is TEST_ONLY and must never be installed as production data.

## Invariants

- `[transport.py:serve]` binds the IPC listener before
  `[isolation.py:isolate_worker]` denies new sockets and outbound connections.
  It reads no lock, dataset, search or shadow file until installation and local
  self-tests pass. Workers run separately from clients, with no weaker test mode.
- `[run.py:diagnostics_directory]` requires evaluator ownership and exact mode
  0700 for new and existing outputs. Descriptor-relative access, no symlink
  traversal and identity checks protect writes/reuse; an unsafe directory is
  rejected without changing its owner or permissions.
- `[observations.py:validate_fits]` requires separate train/calibration/development
  intervals purged and embargoed for
  the frozen outcome horizon plus publication delay. Feature/label availability,
  fitting time and original forecast recording must precede their actual use.
  The complete model/transform fit must exist at forecast **generation**, before
  recording and decision, even if generation occurs before the exchange opens.
- `[observations.py:holding_span]` and `[observations.py:trade_lifecycle]` bind
  outcome labels, matured training payoffs, candidate/neighbor closed trades and
  covered shadow trades to actual approved calendar sessions. The entry session
  is session one: a three-session horizon ends at the third session's close, not
  after three additional sessions. Endpoints must lie within covered exchange
  sessions. Actual/shadow entry excludes the session close in both OPEN and
  CLOSED states; exits include the close. The declared forecast horizon is bounded
  by the candidate holding limit and frozen label policy; coverage through the declared
  expiry is required even for an early or pending label. Missing future sessions
  are not inferred from weekdays or a seconds-only duration.
- `[statistics.py:objective]` matches the application: median fold mean log return, less
  maximum fold drawdown, fixed-index IQR instability and complexity penalties.
  For sorted fold means, the IQR is
  `means[3*(n-1)//4] - means[(n-1)//4]`.
  Session returns include idle sessions and actual costs, without a second
  subtraction. Equal-length contiguous outer folds retain the continuous account.
- `[statistics.py:paired_bounds]` moves paired circular session blocks for all strategies and stresses together;
  blocks never cross folds and cover the registered holding/label horizon.
  Before calendar/account observations or inference, `validate_policy` checks
  every plan candidate and every registered neighbor, including those with no
  closed trades. The label horizon must cover their longest holding horizon;
  the block must cover that horizon and the frozen label horizon.
  One-sided basic bootstrap bounds use a Bonferroni family of cumulative trial
  ceiling × planned looks × benchmarks × stress cases × candidate/neighbors.
  A fold needs two blocks; adjusted tails need at least five bootstrap draws.
  Insufficient resolution is BLOCKED, with no invented confidence bound.
- `[predictions.py:score_predictions]` derives conditional payoffs with
  session-block resampling of actual training outcomes.
  Original causal outcome/context identities, rather than export IDs, determine
  sample uniqueness, estimator caching and payoff-group correction. Renamed
  aliases cannot increase counts, empirical prior weights or favorable means.
  Two tails × four events × payoff groups × trials × looks determine correction.
  Group sums, resampled conditional means, empirical quantiles, probabilities,
  downside, uncertainty and frozen complexity penalties remain exact fractions.
  Missing support leaves utility unavailable and fails support gates; the scorer
  does not rewrite an ENTER decision to ABSTAIN. Each ENTER requires
  `utility - numeric_margin` to exceed the cash hurdle strictly, even with all
  utility penalty weights zero.
  Exact utility equal to the hurdle or the guarded hurdle does not pass.
- `[predictions.py:score_predictions]` compares exact multiclass Brier against the exact empirical
  training-count prior; reliability bin assignment, ECE, older/newer class
  frequencies and Brier drift also remain rational. Session coverage and the
  mean realized-minus-conditional payoff error use exact ratios and thresholds.
  Forecasts sort by decision time, recording time and immutable prediction ID
  before history splits; equal-time exports cannot change drift acceptance.
  Cohorts retain separate support/calibration gates. Log loss uses floating
  logarithms with an independent `+ numeric_margin` upper guard; zero likelihood
  is unavailable and fails. RMSE and paired residual correlations are diagnostic.
- `[predictions.py:score_predictions]` registers each model/regime/horizon cohort before pending labels
  are skipped. Each ENTER cohort requires its own matured calibration support,
  even if all its outcomes are pending. Matured cohorts retain the existing
  calibration gate, including ABSTAIN observations. A pending-only ABSTAIN cohort
  remains visible but is not independently required for entry acceptance.
  Aggregate calibration, event/session floors, conditional-payoff support and
  positive ENTER utility remain mandatory. Null pending labels/payoffs do not
  supply scores, missing outcome classes or calibration samples.
  Cohort diagnostics expose `forecasts`, `enter_forecasts`, `abstain_forecasts`,
  `pending_labels`, `conditional_payoff_support` and `required_for_acceptance`;
  `samples` counts matured labels only. A refit with the same model version,
  regime and horizon keeps the existing statistical cohort.
- `[run.py:evaluate]` and `[predictions.py:score_predictions]` reject strategy/scenario
  and model/regime/horizon diagnostic keys that collide when
  encoded into their existing `|`-joined form. Ambiguity fails explicitly, so
  a later cohort cannot overwrite an earlier ineligible cohort. Other record
  maps retain unique source identities and tuple keys; approved-model/transform
  maps have separate namespaces and remain frozen by their component hashes.
  Dependence group IDs are nonempty strings up to 160 characters. Multiple
  forecasts within a dependence-group/session/scenario contribute to its exact
  residual mean before paired correlation; input order never selects a last row.
- `[statistics.py:deterministic_risk]` and `[run.py:evaluate]` compare actual
  drawdown, CVaR95, worst loss, turnover and
  exposure with the champion and frozen limits under every stress. Neighborhood
  bounds must remain positive. Economics requires supported calibrated positive
  entry utility and adjusted improvement above the frozen hurdle plus project
  cost divided by original account capital.
  `deterministic_risk` compounds exact session returns for continuous wealth and
  peak drawdown, uses the exact worst `ceil(session_count / 20)` empirical returns
  for CVaR95, and compares risk/cap/deterioration limits before projecting floats.
  Exact threshold equality is allowed for these non-strict risk and calibration
  caps; even a smaller-than-float residual beyond the cap fails.
- `[run.py:evaluate]` computes all five `IndependentEvaluation` booleans. TEST_ONLY forces all
  five false while preserving numerical diagnostics. Caller verdict fields are
  rejected. Separate application sample/trade/shadow floors and release proof,
  verification, owner and ACTIVE-authority gates remain mandatory.
- `[run.py:validate_policy]` validates the frozen numerical margin exactly in `[1e-9,1e-4]`.
  Performance objective/bootstrap calculations use IEEE doubles with an
  independent conservative downward margin; neighborhood objective-drop gates
  include that margin as well. Sharpe/Sortino and log-loss cases with undefined
  denominators or zero likelihood are reported unavailable, not made finite. Block-bootstrap
  inference describes the frozen observations under its stated dependence
  assumptions; it is not a future performance guarantee.

## Side Effects

`[transport.py:consume]` creates an immutable dataset exposure latch before
evaluation; retries with identical frozen bindings are allowed, while a different
candidate/request cannot reuse the exposure. `[run.py:write_artifact]` writes
SHA256-addressed diagnostic reports using exclusive 0600 files with file/directory
fsync. `[run.py:diagnostics_directory]` opens every path component without
following symlinks, creates missing directories with requested mode 0700, and
requires the final directory's effective-UID ownership and exact mode 0700,
including for existing directories. Unsafe modes/owners are rejected without
chmod/chown repair. Ancestors must be root/evaluator-owned and either nonwritable
by others or sticky; descriptor-relative writes cannot be redirected by replacing
the validated path. A changed directory identity/mode rejects the result.
Reused reports must be identical, evaluator-owned, singly linked regular 0600
files; symlinks, FIFOs and other file types reject. The umask must preserve owner
permissions. `[transport.py:serve]` validates the diagnostics directory at startup
and reads fixed sealed files only after installing isolation; state stays private
to the evaluator identity. It
accepts only the independently frozen request; no data-selection/peek endpoint
exists. The client sends/receives bounded Unix IPC and emits only the typed helper
reply. Both paths have no provider calls, order mutations or database handle.
The worker's kernel filter prevents connecting to PostgreSQL Unix sockets at any
filesystem or abstract address, independently of socket directory permissions.
It also refuses inherited connected database descriptors; accepted lab IPC uses
ordinary `recv`, so ancillary file descriptors are not imported.

`[observations.py:equity_series]` and `[run.py:evaluate]` produce return/risk/fill
metrics, gross sector attribution and a separate `unallocated_execution_costs`
total; costs are not allocated to sectors. Diagnostics also include descriptive regime means, all paired
bounds, forecast diagnostics and exact pre-class gates. The parent Rust runner
replaces transport report headers with observed process evidence and stores the
actual stdout/receipts. A trusted auditor can resolve the worker diagnostic hash
from that original stdout; lab access to sealed diagnostics is not required.

## Verification

Run `cargo test --workspace --locked` from the repository root for workspace verification.

Run `python3 -B -m unittest discover -s lab/tests -v` (154 tests at the round-five
repair checkpoint). Tests cover numerical
results, continuous equity/costs, snapshot/sector/forecast trade links, purged
availability, pending labels, search correction, exposure persistence, tampering,
socket identity/framing, private output modes and TEST_ONLY exclusion. Temporary
IPC fixtures run under `/tmp/trady-*`; no deployment unit is installed or started.
`test_evaluator_counterexamples.py` additionally reproduces the Goldfish money
cancellation, pre-fit forecast and excessive-session counterexamples, with exact
cost/sector/probability/payoff regressions and legal boundary cases. Financial
results stay identical with Decimal precision 3/28/70, different rounding modes
and Inexact/Rounded traps enabled.
`test_acceptance_boundaries.py` adds 24 regressions for static candidate/neighbor
horizons, exact utility with independent margins, all hard risk/deterioration
caps, exact prior/count calibration, reliability bins, coverage and payoff-error
thresholds. Its first three counterexamples failed against the pre-repair code
and pass after repair. Nonterminating exact diagnostics remain JSON-safe.
`test_evaluator_identity_scope.py` adds 22 tests for the two benchmark overrides,
legal/extraneous/omitted benchmark maps, aliasing, receipt binding, compound-key
collisions, order-independent grouped residuals, dataset-scoped look budgets,
current-exposure mismatches and unchanged cumulative trial correction. Its first
three probes failed against the pre-repair code.
`test_evaluator_lifecycle.py` adds 34 tests for expired/future OPEN entries,
actual/shadow/unreferenced trade states, source/calendar/recording clocks,
forecast links and legal unrealized holdings. It covers pending-only ENTER
model/regime/horizon cohorts, the distinct-fit/model reproduction and matured
control, supported mixed cohorts, ABSTAIN policy, null unscored results, missing
class/conditional support, pending cohort-key collisions and equal-time export
ordering. Four lifecycle/cohort probes and the calibration tie-order probe failed
before repair. The earlier round-four checkpoint passed 124 tests.
`test_evaluator_outcome_identity.py` adds 30 tests covering renamed payoff/sample
support and weighting, conflicting original facts with omitted/unselected rows,
equivalent decimal/timezone encodings, cache/fit membership, legitimate distinct
sources/windows and reuse across forecasts/refits/separate estimator contexts.
Actual/shadow OPEN/CLOSED tests cover exclusive close entry, inclusive open/exit
boundaries, microseconds before close, after-hours recording, carry-in and
unchanged valuation/closure semantics. Before source edits, three regression
methods failed on the support, favorable-weight and entry-state reproductions.
The exact utility probe changed `1/6000` to `27/52000` solely by aliasing; aliases
now reject. The final 154-test checkpoint has no failures or skips. Fresh
independent EGM verification found no actionable findings in the payoff identity
and session boundary repairs.
The executable pin tests require root-owned temporary files and skip explicitly
when run without the ability to create those files.

`test_evaluator_security.py` covers exact existing-directory modes/ownership,
symlink/rename races, private report reuse, a synthetic PostgreSQL-named Unix
socket, native/abstract outbound denial, `io_uring` denial, thread synchronization,
inherited sockets/stdio and startup failure before sealed-file access. It uses
only subprocess-local filters and temporary fixtures under
`/tmp/trady-remediation-20261031/evaluator/`. The existing worker IPC regression
also uses a subprocess so it exercises the same irreversible filter without
restricting the test client. No real database is opened.

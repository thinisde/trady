# Offline helper tests

## Interface

`test_helpers.py:OfflineHelpers` exercises reconciliation and fold measurement
with explicit synthetic inputs.
`test_sealed_evaluator.py` checks actual numerical diagnostics, causal accounting,
availability, purging, uncertainty/search correction, immutable exposure and
private bounded Unix IPC. `test_lean_wire_adapter.py` exercises the real pinned
Rust binary through the adapter; it explicitly skips if the binary is absent.
`test_evaluator_counterexamples.py` exercises Goldfish's money cancellation,
future-fit and excessive-session probes, including exact cost products, sector
and probability sums, payoff subtraction, cash-flow returns, context independence,
positive timing/session boundaries and required future calendar coverage.
`test_acceptance_boundaries.py` checks all registered holding horizons before
observations, zero-penalty and weighted exact utility, independent numerical
margins, exact drawdown/CVaR/worst-loss/exposure/turnover limits, Brier/prior/ECE/
class-shift/drift boundaries, session coverage and realized payoff mean error.
`test_evaluator_identity_scope.py` checks exact benchmark maps and disjoint
strategy names, frozen receipt parameters, compound diagnostic collisions,
grouped residual order independence, source-dataset look accounting, immutable
read scope and cumulative trial correction.
`test_evaluator_lifecycle.py` checks shared OPEN/CLOSED causality across actual,
benchmark, neighbor, unreferenced and shadow rows; original forecast entry links;
pending ENTER cohort calibration without scoring censored results; and drift
acceptance independent of equal-time forecast export order.
`test_evaluator_outcome_identity.py` checks original-label/context uniqueness,
alias support/weighting, conflicting or omitted original facts, canonical
encoding, valid source/window/refit/context reuse and exclusive entry at close.

## Invariants

Missing/tampered/unresolved source data and incomplete/non-real valuations fail.
These tests do not claim that a historical engine run or kernel proof occurred.
Every generated evaluator record is TEST_ONLY, which always makes production
acceptance false. A changed lock cannot reclassify the immutable input files.
Exact financial arithmetic uses rational values from validated finite decimal
strings. Tests enable Decimal rounding traps at precision 3/28/70 to ensure no
cash, cost, equity, sector, probability or payoff identity relies on that context.
Deterministic acceptance comparisons retain exact fractions even where their
floating report projections equal the frozen cap. Tests cover both legal
threshold equality and residual violations, exact nonterminating training priors
and payoff means, and JSON-safe numerator/denominator diagnostics.
Candidate/neighbor receipts cannot acquire replacement parameters through a
benchmark key. Legal benchmark maps and unrelated dataset history preserve the
private gates; TEST_ONLY always masks the public gates. Current dataset exposure
is still global across runs and accounts, and failed related trials still count.
OPEN holdings preserve their actual mark-to-market equity and remain outside
closed-trade counts. Closure cannot waive common entry/source/recording checks.
Every ENTER model/regime/horizon cohort needs its own matured support; pending
outcomes cannot hide it or supply missing labels. Legal mixed cohorts and
pending-only ABSTAIN cohorts retain the documented acceptance behavior.
Calibration history uses decision time, recording time and immutable prediction
ID to resolve ties, so reordered exports retain the same acceptance decision.
Renamed payoff/sample references never create independent outcomes. Within an
estimator, each original source/session/label window and context contributes
once; duplicate aliases reject. Conflicting outcome/payoff facts reject even
when unselected or when the original payoff is omitted. Equivalent aliases
selected singly across forecasts share bounds/search correction; each still
needs its own causal fit membership. Distinct original sources/windows and
separate contexts are allowed without borrowing sample support. Every actual/
shadow OPEN/CLOSED entry uses `[open, close)`; exit at close and after-hours
recording remain legal. Valid benchmark carry-in and valuations are preserved.

## Side Effects

Imports local helpers, computes in memory, and runs temporary local IPC and
executable fixtures under `/tmp/trady-*`; `-B` prevents bytecode artifacts.

## Verification

Run `python3 -B -m unittest discover -s lab/tests -v` (154 tests, no skips, at the
round-five repair checkpoint). The acceptance-boundary subset is
`python3 -B -m unittest discover -s lab/tests -p test_acceptance_boundaries.py -v`
(24 tests). The latest identity/scope subset is
`python3 -B -m unittest discover -s lab/tests -p test_evaluator_identity_scope.py -v`
(22 tests). The latest lifecycle/cohort subset is
`python3 -B -m unittest discover -s lab/tests -p test_evaluator_lifecycle.py -v`
(34 tests). The latest original-outcome/entry-boundary subset is
`python3 -B -m unittest discover -s lab/tests -p test_evaluator_outcome_identity.py -v`
(30 tests, no skips). Its support, favorable-alias and close-entry regression
methods failed on the pre-repair source after in-memory reproductions. The
previous 124-test checkpoint is historical. Fresh independent EGM verification
found no actionable findings in the payoff identity and session boundary repairs.
These remain synthetic software regressions.

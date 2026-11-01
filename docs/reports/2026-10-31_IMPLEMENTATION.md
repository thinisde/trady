# Implementation completion and activation evidence

Later implementation fixes and verification are recorded in the
[audit remediation](../audit-2026-10-31/remediation.md). The counts and source
pins below retain this earlier implementation checkpoint.

Updated 2026-10-31. This record covers the implementation requested after the
remaining-work inventory. It preserves the distinction between executable code,
local software checks and independently approved financial evidence.

## Interface

| Work area | Concrete implementation |
| --- | --- |
| Trusted production inputs | [Input service](../../services/inputs/README.md): immutable imports and independent review, explicit exchange calendars, fresh market/cost/research DTOs, authenticated bounded HTTP and same-origin universe credentials. |
| Calibrated four-outcome forecasts | [Model lab](../../crates/trady-application/src/model_lab/README.md): chronological training, separate calibration/validation, sealed evaluation consumption, conditional net payoffs and independently gated production bundles. Binary native advice remains separate. |
| Research, sentiment and scheduling | [Research workers](../../crates/trady-adapters/src/research_jobs/README.md): durable scoped jobs, daily/holding scheduling, original source dependencies, FinBERT assessment persistence, invalidation, LLM information tools and final notifications. |
| Weekly lab and forecast history | [Offline lab](../../lab/README.md): PostgreSQL freezes, cumulative trial/holdout budgets, immutable forecasts/labels/reconciled exports and pinned bounded helper execution. |
| Independent economic evaluation | [Sealed evaluator](../../lab/evaluator/README.md): exact rational account/cost reconciliation, causal model/forecast chronology, static candidate/neighbor and actual calendar holding bounds, paired search-adjusted uncertainty, exact risk/calibration/payoff gates and a private worker with durable dataset-exposure identities. |
| Formula proof and executable parity | [Pinned Lean project](../../lab/proofs/README.md): complete checked-decimal FORMULA_V1 reference, acceptance/protection/revision theorems, concrete cash/FX witnesses and exact production/adapter comparisons, bound to 172 Rust/SQL source files including all 11 migration/role SQL files. |
| Release and shadow recording | [Runtime deployment](../../services/runtime/README.md): role-separated release lifecycle, content-addressed evidence and read-only real-data shadow observations/proposals. Live entry resolves the current ACTIVE release twice and compares exact deployment bindings. |
| Telegram conversations and controls | [Telegram guide](../telegram.md): queued dialogue/research, reports and sessions, durable owner pause and confirmed resume/close requests, replay protection and executor result notifications. |
| Verified holding advice | [Holding repository](../../crates/trady-storage/src/holding_research.rs) and [activation](../../crates/trady-application/src/autonomous/updates.rs): actual reconciled quantity/acquisition cost, immutable native review, current economics and old-plan exit precedence. |
| Runtime isolation and database transport | [Separate service units](../../services/runtime/README.md), private bounded analyst socket, per-identity credentials/resources/network policy, and [verified PostgreSQL TLS](../../crates/trady-storage/src/transport.rs). |

The [operator guide](../autonomous.md), [input commands](../../services/inputs/README.md)
and [lab commands](../../lab/README.md) describe the concrete reviewable paths.
No service, timer, firewall policy or trading deployment was enabled.

## Invariants

- Model output cannot verify source truth, approve a model, advance a release or
  place an order. Source permissions, ownership, original availability and
  independent lineage must resolve through reviewed deployment records.
- TEST ONLY data and artifacts cannot receive production model/release approval.
  Fixture probabilities and returns establish software behavior only.
- Owner controls are scoped to the authenticated private owner/account/policy.
  Resume and close confirmations bind the current plan/holding/quantity and are
  rechecked by the executor. Durable latches survive restart; deployment pauses
  override resume. A close completes only after flat reconciliation, no active
  plan and terminal outstanding broker orders.
- Native holding updates cannot change the broker stop, extend deadlines,
  loosen trailing rules or undo an exit. Actual remaining entry costs and current
  exit charges/FX are assessed; fresh quote/model/dossier checks run at activation.
  Fresh research controls the nominal confidence ceiling; unavoidable target
  tick rounding cannot authorize a larger nominal fraction. Profitable holding
  stops require explicitly evaluated signed support in the native model.
- Persisted sentiment retains its original scoring time and source/model/claim
  dependencies. Entry and holding activation recheck them transactionally.
  Scores expire after fifteen minutes; adverse probabilities reduce authority,
  while favorable sentiment grants no extra authority.
- Live authority includes exact source/calendar/model/formula bindings. Missing,
  changed or expired ACTIVE evidence blocks entry while existing-position
  reconciliation, protection and liquidation remain available.
  External analyst replies bind the actual persisted native model artifact hash
  to that release. Private socket checks cover every ancestor and the Linux peer UID.
- Independent retention maintenance continues during analysis. Holding-update
  copies use locked source deadlines, track later shortening/deletion and retain
  only immutable identity hashes after cleanup. Source MVCC publication forces
  stale repeatable-read writers to retry before they could miss a new copy.
- Shadow recording has no mutation client and invents no fills or performance.
  Twenty actual prospective calendar sessions remain an elapsed evidence gate.
- Evaluator financial reconciliation, conditional utility, empirical calibration
  and hard risk comparisons retain exact fractions through acceptance. Numerical
  report projections cannot round away threshold violations. Floating performance
  inference and log loss use independent conservative margins; the entry utility
  margin remains active even when all utility penalty weights are zero.
- Candidate, neighbor and benchmark identities are disjoint. Exact benchmark
  maps cannot replace frozen candidate/neighbor receipt parameters; compound
  diagnostic identities cannot overwrite another cohort. Look-budget accounting
  uses the current original source dataset while cumulative related trial and
  global current-dataset exposure checks remain enforced.
- OPEN and CLOSED trades share entry, recording, source and cutoff checks.
  Actual and shadow entries use inclusive session open and exclusive close;
  exits at close and legal carry-in remain supported. Candidate entries must
  precede forecast expiry regardless of closure. OPEN
  holdings retain unrealized valuations and cannot satisfy closed-trade floors.
  Pending ENTER cohorts remain visible and require their own calibration support;
  pending outcomes remain unscored. Calibration ordering is deterministic.
- Payoff support and weighting use original source/session/label-window identity
  within each estimator context. Renamed observations or training samples cannot
  supply another outcome; conflicting aliases fail even when unselected. Legal
  reuse across forecasts, refits and separate contexts remains supported.

## Side Effects

The implementation adds local files, database migrations and explicitly invoked
commands. Verification uses synthetic data, loopback services and disposable
PostgreSQL. Explicit production operation would access approved providers,
databases and Telegram; an enabled executor can submit/cancel broker orders.
No broker, paid provider, external recipient or live deployment was contacted
while implementing these changes.

## Verification

Formatting and strict workspace Clippy pass. Workspace tests pass with 357 tests;
the 20 database scenarios ignored there pass separately through the disposable
PostgreSQL runner. All nine migration sources remained unchanged during that run.
The database tests include real concurrency, stale repeatable-read snapshots,
sentiment revocation, pending-entry close controls and scoped retention cleanup.
The six FinBERT and 154 lab Python tests pass, including 129 evaluator
counterexample/boundary regressions across five repair rounds. Root independently
reran all 154 lab tests with no skips and stable source/test fingerprints.
Independent clean Lean verification passed all 24
theorem audits, 647 exact three-way cases and ten negative checks. The corrected
source lock covers 172 Rust/SQL files, including all eleven repository-level SQL
files, and eighteen proof sources. An actual migration-tamper negative rejects
changed SQL before Lean build artifacts change. The independent clean run is
retained at `/tmp/trady-formula-proof-stable.SEReAo/root-sql-bound-evidence`;
[the proof handoff](../../lab/proofs/IMPLEMENTATION.md) records pins, reproduction
commands and superseded evidence.

Independent source reviews found no remaining findings in the final holding
retention/snapshot fix or the agreed FORMULA_V1 proof scope. The final evaluator
review read all nine production files and the required contracts/tests and found
no actionable findings in the payoff identity and session boundary repairs. It
independently passed 64 focused tests, eighteen lifecycle boundary cases and six
TEST_ONLY probes; every successful synthetic result retained false public gates.
The requested [modified-contract drift audit](2026-10-31_DRIFT_REPORT.md) covers
all 61 modified README contracts. Eight documentation gaps across six READMEs
were corrected against current source, with no unresolved drift. Asset checks cover the lab and
service documentation as well as the existing documentation tree: eleven
canonical JSON assets, 52 skill records, 163 Markdown files and 1,113 local links.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/verify-assets.py
python3 scripts/test-postgres.py
python3 -B -m unittest discover -s services/finbert/tests -v
python3 -B -m unittest discover -s lab/tests -v
```

## Activation dependencies and trust boundaries

Deployment must supply licensed financial history and current provider exports,
verified calendars/funding/costs, independently approved fitted artifacts and
actual account-specific capabilities. Real sealed/prospective financial
performance, twenty shadow sessions and owner live authorization are not
manufactured by fixture tests.

Lean/lake 4.19.0 is prepared locally. The proof scope is the complete FORMULA_V1
reference with retained safety obligations and finite exact executable parity.
Translation, parsers, compilers, build provenance and host integrity remain the
documented trust boundary; universal Rust/limb refinement is not a completion gate.
No theorem establishes market-data truth, profitability or guaranteed fills.

QuantConnect LEAN and licensed engine data were not run here. The real replay
adapter is implemented; broker-equivalent partial fills, protection races,
deadlines and stressed execution costs still need independently validated
deployment evidence. Evaluator fixtures remain TEST_ONLY and cannot satisfy a
production acceptance gate. Helpers, OS isolation, approved data and real
prospective sessions must be prepared before an explicit activation.

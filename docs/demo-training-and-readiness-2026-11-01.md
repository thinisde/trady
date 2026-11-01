# Trady training and demo order readiness — 1 November 2026

**The research model is tuned and fitted. Trady is not ready for autonomous demo
order entries.** The selected model did not beat the validation baseline, and
five deployment prerequisites remain missing. No broker order was submitted,
cancelled or filled during this work. This report supersedes earlier readiness
conclusions for the inspected configuration; it does not grant deployment approval.

## Implemented repairs

- The [native trainer](../crates/trady-adapters/examples/train_candidate.rs) now
  supports `tune`. It evaluates a fixed six-setting grid across three purged
  expanding windows inside the original training partition. Each fold has
  separate fitting, calibration and validation data. The selected settings are
  refitted on the original train/calibration partitions. Claims and reports are
  exclusive local files, and the candidate pins its tuning report by hash.
- Native JSON export now sorts object keys before hashing. Two runs originally
  produced different byte hashes for structurally identical models because of
  randomized internal map ordering. Ordered export removes that variability;
  native reload still checks identical calibration predictions. A regression
  checks different metadata insertion orders.
- [Configuration checks](../crates/trady-cli/src/autonomous.rs) now report
  `entry_capabilities_unverified` and identify each missing, unsupported or
  expired cash-account, instrument, protective-order and quote capability.
  These match the policy's unconditional entry requirements. Fractional support
  remains conditional on actual sizing. This change does not alter executor
  position management or confer broker permissions.
- The final [drift report](./2026-11-01_DRIFT_REPORT.md) reviewed all 45 updated
  README contracts and found no remaining document/source divergence.

## Actual tuning result

The input is the existing real historical dataset described in the
[original training report](./demo-training-setup-2026-11-01.md). No new provider
download or paid subscription was needed. The ordered feature schema and gross
1% target/stop, five-bar binary research labels are unchanged.

The grid is budgets 0.25, 0.5 and 1.0, each with iteration limits 8 and 32.
Seed is 42 and each fit uses two threads. Selection minimizes equally weighted
mean validation log loss, with Brier breaking ties. The fold windows are fixed:

- 2019 validation: 2,947 fitting rows, 472 calibration rows, 1,014 validation rows.
- 2020 validation: 4,148 fitting rows, 506 calibration rows, 840 validation rows.
- 2021 validation: 5,267 fitting rows, 470 calibration rows, 985 validation rows.

Entire feature/label intervals are purged across each boundary. The outer
calibration partition is not used for setting selection. No new predictions or
scores were produced for the previously exposed 2024–2026 holdout.

Selected settings: **budget 0.25, eight iterations**. Final fitting used 6,943
training rows, produced eight trees and calibrated on 2,021 separate rows.

Across validation folds, mean Brier is **0.2503565616**, versus the prevalence
baseline's **0.2503458379**. Mean log loss is **0.6939110661**, versus baseline
**0.6938391705**. Lower is better for both metrics. The selected model wins in
2019 and 2020 and loses in 2021; it has **no average validation advantage**.

The final calibration scores improve over prevalence, but that partition fitted
the calibration parameters and is not independent confirmation. The saved status
is `NO_WALK_FORWARD_ADVANTAGE`, `tuning_complete=true`,
`measured_validation_advantage=false`, `deployment_eligible=false`.

Final private artifacts are under
`var/research-training/2026-11-01-walk-forward-canonical/model/`.
They include `tuning-claim.json`, `tuning-report.json`, `fit-claim.json`,
`native-model.json` and `candidate.json`. There is no holdout claim or report in
that directory. Earlier runs were retained: the grid and fitted parameters stayed
the same while report wording and deterministic export were verified.

Native artifact SHA-256:
`cbff93e1a653565662f85cea0e472d1d6e8c62a69adebd1b8e41ba4b22bc1480`.
Tuning report SHA-256:
`cae60facb22167b41ab967792067c0c259bf9ee279ded4cf941ea40d2789ef60`.

Integrity verification matched these hashes to the candidate metadata and matched
the fit input to its original hash. Reusing the final output directory was
rejected without changing its tuning report. The original fit, holdout and prior
native artifact hashes remain unchanged.

This daily-replay binary model does not implement the approved native runtime
feature/label contract. The runtime also needs a separate four-outcome prediction
model. Real reconciled fills, non-fills, fees, FX, slippage and source/execution
provenance are absent from the daily bars; those records cannot be manufactured
from OHLCV to clear either deployment gate. Profitability, prospective acceptance
and independent production evaluation have not been established.

## Fresh local and broker checks

At **2026-11-01 03:13:36 UTC**, a read-only Trading 212 demo snapshot succeeded:
**EUR 5,000.00 available, zero positions, zero pending orders**. It bound the
configured demo account and returned `entry_authorized=false`. The request used
the demo credential pair and demo origin. It did not test order permissions,
protective-order acceptance, executable quotes or actual fills.

The official [Trading 212 API documentation](https://docs.trading212.com/)
distinguishes the demo and real-money origins. Its availability does not prove
this account's trading capabilities. The current UTC date is Sunday; a fresh
open exchange session has not been supplied. The runtime must validate the
actual session at entry time. See the official
[NYSE timetable](https://www.nyse.com/markets/hours-calendars).

The configured local analyst, executor and gateway `--once` probes each completed
with `entry_authorized=false`. These are foundation probes, not an autonomous
trade. FinBERT's local health endpoint returned HTTP 200 and `status=ready`.

The current no-submit configuration was checked and run once with an empty
inherited environment, a minimal PATH and an absent credential-store path:

```sh
target/debug/trady autonomous check --config config/autonomous.demo-no-submit.json
target/debug/trady autonomous run --config config/autonomous.demo-no-submit.json --once
```

The check returned `configuration_ready=false`, `approved_models=0`,
`entry_controls_enabled=true`, `mode=shadow`, `order_submission_policy=record_only`
and the five blockers below. The run returned `shadow_blocked`, **cycles=0** and
**order_mutations=0**. Its preflight exactly matched the check. The configuration
branch exits before credential loading or network-client construction. Network
traffic was not separately captured.

Audit:
`/tmp/trady-demo-no-submit-20261101/shadow-journal/cycle_63eee88965e40b080f2faab6f93903d5bd55fd77bdbb5d2c7d3bcf2ec0a25ed3.json`.
Its content SHA-256 matched its filename. Configuration SHA-256 remains
`7d3821ca7cefe7bb73a9fbcd73dd53e74307a8bbad306d707540e8f6fe30848a`.

## Remaining prerequisites for actual demo entries

1. **`approved_model_registry_empty`**: supply a genuinely approved native model
   manifest and exact compatible artifact. The tuned daily research artifact
   remains ineligible. Supply reconciled execution data for the distinct
   four-outcome model and a fresh independent evaluation set.
2. **`approved_input_service_unconfigured`**: supply a reviewed HTTPS service,
   source permissions, independent reviewer identities, account bindings and
   fresh imports for executable quotes, costs/FX, calendars and prediction facts.
   The example service registry has no sources, reviewers, accounts or model.
3. **`universe_source_disabled`**: supply the actual constituent/classification
   provider, current membership, broker mappings and permitted storage/processing
   rights. The five-stock allowlist is configured; it does not replace this feed.
4. **`entry_capabilities_unverified`**: supply current real evidence for
   `cash_account`, `instrument`, `protective_orders` and `quotes`. All four fields
   currently lack usable evidence. A successful account read alone does not
   establish all of these capabilities.
5. **`approved_formula_unavailable`**: supply a compatible active formula and
   prediction authority with genuine proof/parity scope, independent evaluation
   and required prospective evidence. The existing proof harness self-test is
   not formula release approval.

The current file is explicitly a shadow recorder. Actual Trading 212 demo
submissions additionally require a reviewed `paper` deployment bound to the same
demo broker account and EUR base currency, using those approved inputs/artifacts
and skill approval covering that deployment. The existing skill review covers
shadow research only. Credentials remain in the private store. Real-money live
deployment has not been requested or enabled.

After the owner asked me to obtain the evidence independently, I queried the
actual demo account's historical-orders API. At **2026-11-01 03:24:42 UTC**, after
matching the account ID and currency against its summary, the bounded GET of
`/api/v0/equity/history/orders?limit=50` returned HTTP 200, **zero items** and
`nextPagePath=null`. The provider response SHA-256 is
`27b7b5f9bc84c7902bc64d8355876e523cb1cb2ae7cf98b01373d33bd49b22c4`.
There were two GET requests and no order mutations; there is no next page hiding
older fills in this account's returned history.

I also inspected all application schemas in the existing local demo database.
Exact row counts were zero for every execution table, including order intents,
account snapshots and holding observations. Weekly forecasts, labels,
reconciliations, holdouts, trials and runs were empty. Release candidates,
evidence, events and active releases were empty. Database queries were read-only.
The locally available provider histories contain market bars, not the account's
missing executions or independent release approvals.

Fresh bounded diagnostics for Twelve Data, Finnhub, Massive and Tiingo each
returned HTTP 200 with the expected probe payload at 03:26 UTC, using one GET
per provider. These confirm working diagnostic endpoints, not executable quote
entitlements or source-rights approval. The private artifact area contained only
the known research datasets/candidates, with no approved bundle or execution
export. The separately configured lab DSN names a Unix socket absent in this
environment; its endpoint is not a source of existing records here. Offline
training does not depend on that connection. A deployment of the weekly lab
still needs its own correctly provisioned restricted connection.

No usable execution export or approved bundle was found in those sources. Owner
authorization to work on demo entries does not create historical executions or
independent-evaluation records. The remaining gates were not removed or marked
approved to make the status appear ready.

## Verification

- Final workspace tests passed: **533 passed, zero failed**, with 34 opt-in
  database tests skipped by that command. The new capability CLI regressions
  are included in that final run.
- All 34 opt-in database integration tests passed on a fresh disposable
  PostgreSQL container through `python3 -B scripts/test-postgres.py`. Its role
  probes, credential rotation and endpoint authentication checks also passed.
  The disposable container was removed; the existing demo database was retained.
- Native trainer: seven tests passed, including holdout rejection before a
  claim, complete interval purging and deterministic metadata serialization.
- Python training pipeline: seven tests passed. Lab/evaluator: 178 tests passed.
  Credential tooling: 29 tests passed. The lab suite initially exceeded a
  40-second caller timeout; the complete rerun passed in 45.682 seconds.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --all -- --check` and `git diff --check` passed.
- Final model/report/input bindings and exclusive output reuse were verified.
  No additional evaluation against the observed holdout was performed.
- Final documentation checks passed: 55 contract directories match the pruned
  production tree, all four worker indexes validate against the EGM schema,
  and every resulting contract was read. Asset verification passed for 11
  canonical JSON assets, 52 runtime skill records, 208 Markdown files and
  1,548 local links. `git diff --check` passed after the documentation updates.
  Behavioral suites were not repeated for documentation-only worker edits.

## EGM scope

Root discovery initially paused upon discovering 16 subdirectories. The
invoked EGM bootstrap skill at
`/root/.codex/plugins/cache/egm/egm/0.1.0/skills/egm-bootstrap/SKILL.md`
says, "If you discover more than 15 subdirectories, you MUST immediately request
user approval" before full-tree execution or worker spawning.
The owner approved full-tree execution. Resumed BFS found 105 subdirectories;
test-only and non-code pruning retains exactly **55 code/parent directories**
over 185 production source files. All four workers completed:

| Worker scope | Contracts | README edits | Completed fresh scouts | Production files |
| --- | ---: | ---: | ---: | ---: |
| Application and CLI | 12 | 10 | 20 | 43 |
| Domain, policy and storage | 12 | 11 | 19 | 50 |
| Adapters and native trainer | 16 | 11 | 19 | 41 |
| Lab, services, migrations, scripts and archived audit SQL | 12 | 10 | 13 | 51 |

The 52 worker contracts plus the repository, Rust workspace and documentation
indexes cover the complete retained tree without duplicate or missing paths.
There were 42 worker README edits and three parent compilations; ten contracts
were verified unchanged. The new audit README documents the existing read-only
SQL. Parent readback covered all contracts, including unchanged ones, and checked
sections, source-reference files, verification commands, relative links and
Markdown syntax. All **71 scouts** completed with no unresolved source reads.
Storage scouts also read the 13 SQL dependencies covered by the operational
worker. Documentation workers preserved functional files and ran no broker or
provider requests.

The evidence preserves one procedural deviation: domain declaration discovery
incidentally emitted two inline test-module declaration/import lines. They were
excluded from analysis before production visibility was rechecked. The adapter
worker recorded 37 initial read/scan failures and their successful retry evidence;
none was counted as completed coverage before resolution.

Parent corrections linked the actual dated FinBERT build/inference evidence,
removed stale service-status wording and rolled up the separate research trainer
and two runtime model contracts. Existing architectural notes were retained.
Full-tree README bootstrapping and its required correctness checks are complete.
No commits, staging or publication were performed. These documentation results
do not change the model result or clear any demo-entry prerequisite above.

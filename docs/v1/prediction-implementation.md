# Prediction and weekly optimization implementation record

Updated 2026-10-31. This record distinguishes executable components from the
[prediction design](./prediction-fusion.md) and
[weekly requirements](./weekly-optimization.md). No trained four-outcome model,
historical performance result, Lean 4 proof, QuantConnect LEAN backtest or production
activation was produced by this implementation.

## Executable subset

| Component | Implemented behavior | Evidence |
|---|---|---|
| [Domain predictions](../../crates/trady-domain/src/prediction.rs) | Versioned source-specific payloads; observation/availability/generation/expiry, horizon, scenario, uncertainty, calibration and lineage validation | Pure policy prediction tests exercise semantic, chronology and partition rejection |
| [Fusion and utility](../../crates/trady-policy/src/prediction/README.md) | One approved calibrated scenario forecaster plus required LLM/FinBERT/quantitative gates; explicit abstention, dependence diagnostics, exploratory ablation, four-way net utility and cash selection | Nine synthetic-fixture tests cover probability semantics, missing/stale/unsupported inputs, expired results, objective/normalization compatibility, scoring and purging |
| [Financial limits](../../crates/trady-policy/src/limits/README.md) | Approved trend/reversal parameter envelope; checked entry tick, ATR stop, cost/FX-aware risk-bounded largest lot and net target; original holding deadline and final grid/inequality checks | Four deterministic tests cover actual equations, both fee legs, FX, stale/unverified evidence, no feasible entry and extreme decimal precision |
| [Application bindings](../../crates/trady-application/src/optimization/README.md) | Formula/policy/input/full-authority hashes, deterministic replay, unchanged current economics, required calibrated prediction utility and FinBERT overlay | Three focused tests cover revision/input binding, quantity tampering, quote refresh, expiry and missing prediction authority; no release-evidence resolver is supplied |
| [Historical evaluation](../../crates/trady-policy/src/prediction/history.rs) | Original four-class Brier scoring, pending/censored labels, regime counts, chronological ordering/embargo and training-label purge eligibility | Prediction fixtures preserve original outputs and reject changed label versions/scenarios |
| [Weekly runner](../../crates/trady-application/src/optimization/weekly.rs) | Frozen manifests/gates/splits/policy; bounded parameter trials; original resume budget; full-run helper hashes; keep/shadow/approval-required/blocked recommendations | Six local store/lab-fixture tests cover no self-authorization, deduplication, interruption, budgets/fencing, reused evidence and deposits |
| [Local developer skill](../../.agents/skills/trady-weekly-optimization/SKILL.md) | Offline reconciliation, cumulative prediction diagnosis, hypothesis registration, chronological experiments, verification and reporting instructions | Skill metadata validation and local-link checks; separate from the 52 runtime skills |

The lower-level Rust financial function returns `Result<CalculatedLimits>`;
fusion returns `Ready`/`Abstain`, and the selector returns `Candidate`/`NoTrade`.
These implement the separate passes of the design's conceptual
`AcceptedCalculation | RejectedCalculation` interface. They grant no order
authority. `FormulaProposal::validate` requires exact financial scenario and
horizon binding before accepting predictive utility. A separately configured
runtime must also validate skills, sources, account state, calendar, capabilities,
permissions, protection and broker reconciliation.

The initial solver uses a single approved formula revision. `select_scenario`
supports a bounded set of externally generated feasible scenarios; it does not
learn ensemble weights or automatically search arbitrary model/code changes.
The weekly runner's executable candidate domain is formula parameters. The
broader model/calibrator/fusion/feature experiments remain specified work for
trusted offline helpers.

## Trust boundaries and remaining work

All domain records and configuration require validation after deserialization.
Matching model/report IDs, booleans and hashes establish structural correlation;
they do not authenticate artifacts or prove historical independence. Deployment
must resolve original immutable forecasts, real calibration/equity/cost/payoff
records and release evidence through trusted stores. Conditional net outcomes
are supplied estimates, including stop gaps, non-fill and expiry economics;
their relationship to actual execution must be validated by that evaluator.

The baseline bounds observation, availability and generation age with the
approved fusion age cap. Completed formula bars must be at most one configured
bar interval old; a carried daily bar across a holiday may conservatively
abstain. Calendar-aware source validity and actual exchange-session counting
remain trusted collector responsibilities. The formula copies a supplied
calendar-resolved deadline; it does not derive exchange calendars.
Calculation expiry is capped by quote, account, FX and completed-bar freshness;
an unchanged quote refresh cannot extend the original assessment's lifetime.

`WeeklyStore` and `WeeklyLab` are ports. No production weekly database adapter,
reconciliation exporter, immutable prediction ledger, trial/holdout-consumption
store, process allowlist runner, external deadline enforcement, report-signature
resolver, LEAN bridge/driver or Lean project is shipped here. Unit fixtures
demonstrate the specified protocol; they are not durable production services.
Elapsed-time checks run between helper calls; helpers need their own resource
limits. A persisted blocked decision is terminal for that frozen run.

No historical observations were invented. In particular, the current native
binary Perpetual assessment cannot satisfy the complete four-way forecast and
payoff requirements. A later independently trained, calibrated and approved
model/service is required. Synthetic fixtures are labeled and used only for
software checks; their probabilities, returns and reports are not measurements.

The release lifecycle remains `CANDIDATE -> VERIFIED -> SHADOW -> APPROVED -> ACTIVE`.
This code creates research recommendations and validates supplied formula
authority. It does not implement deployment promotion or grant Codex broker,
owner-policy, verification-definition or live-credential authority. Structural
changes require independent review, and current holdings keep their existing
protection and deadlines.

## Repository verification

The required repository checks are:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/verify-assets.py
```

All four required checks passed on a clean snapshot of the scoped change:
148 Rust tests passed, five isolated PostgreSQL tests remained ignored, and the
asset validator checked 11 canonical JSON assets and 52 runtime skill records.
The snapshot also corrects a pre-existing moved-key error in shortlist rejection
symbol lookup; screening policy is unchanged. Unrelated concurrent work is
excluded from that verification and commit.

Formatting, strict Clippy, Cargo tests and asset/link validation are local
software checks. PostgreSQL integration tests marked ignored by the workspace
suite require the separate isolated database runner; they are not exercised by
the pure prediction/weekly tests. Lean 4 and QuantConnect LEAN are separate
dependencies and their proof/backtest checks have not run. No scheduler, service,
container, broker connection or strategy activation is started by these checks.

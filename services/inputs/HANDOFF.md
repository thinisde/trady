# Inputs and model implementation handoff

The scoped code tree is stable for root formatting, Clippy and full workspace
checks. No add/commit/push, paid API, provider fetch, live trading or deployment
startup was performed. Loopback HTTP ran only inside tests. No Cargo manifest,
main dispatch or migration was edited by this worker.

## Changed paths

- `crates/trady-adapters/src/inputs_service/{mod.rs,store.rs,calendar.rs,validate.rs,http.rs,tests.rs,README.md}`:
  bounded authenticated HTTP service, durable immutable imports/reviews,
  rights/lineage/source/scope/freshness checks, explicit exchange calendars,
  actual DTO routes and approved-model inference.
- `crates/trady-application/src/model_lab/{mod.rs,fit.rs,inference.rs,journal.rs,tests.rs,README.md}`:
  chronological supplied observations; regularized four-outcome softmax and
  conditional payoff models; separate temperature/error calibration;
  validation/evaluation diagnostics; durable sealed holdout consumption;
  exact-calculation inference and independent production approval gates.
- `crates/trady-cli/src/inputs_service.rs`, `crates/trady-cli/src/model_lab.rs`:
  executable tooling with `HELP` and `pub fn run(args: &[String]) -> Result<Value>`.
- `crates/trady-cli/tests/inputs_model_pipeline.rs`: actual CLI end-to-end test.
- `services/inputs/{README.md,config.example.json,HANDOFF.md}`: operator guide,
  intentionally unconfigured example and this integration handoff.
- `crates/trady-adapters/src/lib.rs`, `crates/trady-application/src/lib.rs`:
  narrowly added module exports only; other agents' declarations preserved.
- The three crates' `src/README.md` contracts: narrow interface additions only,
  preserving existing prose and other components' contracts.

## CLI hooks and routes

Composition root hooks, already observed in the shared checkout:

```rust
mod inputs_service;
mod model_lab;
// Within dispatch:
"inputs-service" => output(inputs_service::run(&args[1..])?),
"model-lab" => output(model_lab::run(&args[1..])?),
```

Include both modules' `HELP` constants in root help output. Commands and all
strict flag sets are listed in `services/inputs/README.md`. No new dependency or
migration is required. The server credential is `TRADY_INPUTS_SERVICE_TOKEN`,
loaded through the existing central credential loader; the autonomous client
uses the same bearer value under `TRADY_TRADING_INPUTS_TOKEN`.

All five original HTTP routes use the actual Rust DTOs:
`session/{account}`, `quote/{instrument}`, `costs/{instrument}`,
`research/{account}`, `predictions/{account}`. The universe export route is
`universe/SP500`. Listener binds loopback behind an approved TLS reverse proxy.

Root added `HttpUniverseSource::with_bearer` and same-origin input-service bearer
forwarding, plus central credential registration. This follow-up preserves those
changes. Credentials must not enter URLs or be forwarded across unapproved origins.

## Research worker integration

Use `ImportStore::{import,review}` and the strict public `SourceImport`/
`SourceReview` records. Staging is not verification: exact content hashes must
be separately reviewed by an allowlisted independent local operator/verifier.
Publish actual originals/evidence first, finalized dossier artifacts second,
then the actual `AnalystPacket`, with every artifact ID in import dependencies.
Source ownership groups and original lineages must remain distinct for independent
confirmation; a different URL does not prove independence.

Publish `ImportPayload::Forecast(Box::new(ForecastImport { account_id,
observation }))` for the exact market snapshot. `ForecastObservation` carries
ordered model features and original research/FinBERT/quantitative observations,
availability, generation, expiry and lineages. Its lineage dependencies must
already resolve. The HTTP prediction operation binds the observations to the
actual authoritative Rust calculation and final horizon; the worker need not
invent a future calculation hash.

The current v1 fusion/domain source slot for a complete forecast is named
`PERPETUAL`. The four-outcome baseline uses a distinct approved model revision in
that existing wire slot; the native binary Perpetual adapter remains separate.
No conversion from a binary output to the other outcomes occurs. Configure the
exact approved model versions, `ApprovedModel::{report_id,calibrator_id,
estimation_id}` and actual cost-model hash in `FormulaAuthority.prediction`.

Holding builders may read `InputsService::forecast_observation` and resolve
fresh source/artifact imports. They must obtain quantity, acquisition economics,
exit costs and original plan from root's executor-attested `VerifiedHolding`
and use its `holding_` binding. This service's current `PredictionRequest` is
entry-specific; it does not fabricate holding contexts or position economics.

## Verification completed

Using `CARGO_TARGET_DIR=/tmp/trady-inputs-worker-build` and offline Cargo:

- `cargo test -p trady-application model_lab::tests --offline`: **6 passed**.
- `cargo test -p trady-adapters inputs_service::tests --offline`: **6 passed**.
- `cargo test -p trady-cli --test inputs_model_pipeline --offline`: **1 passed**.
- `cargo check -p trady-cli --offline`: passed with the root dispatch present.
- `cargo clippy -p trady-application -p trady-adapters --tests --offline -- -D warnings`:
  passed. Root owns the final formatting/full workspace checks.

The last broader CLI Clippy run was blocked by another component's
`trady-storage/src/research_jobs.rs::publish_verified_packet` argument-count lint;
that file was left untouched. Root should rerun its full checks after all workers
finish. The extra `>` in `inputs_service/http.rs::send` is fixed, and subsequent
targeted checks/tests passed.

The CLI test fits/evaluates explicit TEST ONLY supplied observations, imports and
reviews real fixture bytes through the actual executable, serves strict market
and session DTOs, then produces wrapped offline scenario inference with four
predictions and exact calculation binding. It proves that the same test model
cannot be production-bundled or loaded. Adapter tests separately run authenticated
loopback HTTP, including duplicate headers, oversized/unfinished bodies and the
absence of remote import/review capabilities.

## Goldfish follow-up fixes

All three reported input-service defects are fixed. This follow-up changes
`inputs_service/{mod.rs,store.rs,validate.rs,tests.rs,README.md}` and the local
operator/handoff documentation. At root's subsequent request, a single missing
closing parenthesis in `financial/mod.rs` Retry-After parsing was also fixed.
Root's universe bearer integration and credential registration are preserved.
The research fixture now includes
`sentiment_reference: None` for root's defaulted atomic sentiment integration.

- Secondary evidence uses maximum matching between original lineages and reviewed
  ownership groups. Copies cannot count twice across secondary publishers, and
  several originals from one owner cannot inflate independence. Regressions also
  cover overlapping graphs where separate unique counters overcount or greedy
  deduplication would undercount a valid set.
- Review publication checks active-index capacity before publishing either file.
  Lookup independently bounds active entries and total scans. Import/review/prune
  share an OS-owned file lock that releases on close/crash, preventing concurrent
  writers from bypassing capacity. Reviews remove expired index entries; explicit
  prune streams expired legacy indexes even when their cumulative size exceeds
  lookup limits. Directory removals are synchronized and identity/hash tombstones
  survive cleanup. Regression tests use a reduced private test capacity to cross
  repeated fill/prune cycles and recover an over-limit legacy directory, then
  demonstrate a fresh valid import/review/lookup and rejection of recycled IDs.
- Costs staging and reads require both fee/uncertainty currencies to match the
  configured account currency. FX must point into that currency with a positive,
  current observation. Regressions cover configuration/currency mismatch, reversed
  or same-currency pairs, zero rates, stale/future FX and FX expiry while the cost
  observation itself remains fresh.

Focused verification: `cargo test -p trady-adapters --lib inputs_service::tests
--offline` passes **12 tests**, including six new regressions. The earlier
financial parser/test-module compile blockers have been resolved; this suite was
rerun successfully afterward. No broad retest or global formatting/Clippy run was
performed for this follow-up; root owns those final checks. The scoped tree is
stable for them. See the [holding economics handoff](./HOLDING_ECONOMICS_HANDOFF.md)
for the subsequent holding and adverse-sentiment corrections.

## Real remaining limitations and gates

- Actual licensed financial history, current provider/export producers, verified
  calendars/funding/costs and independently checked source truth must be supplied.
  This worker purchased or activated none. The import/review/inference paths are
  implemented, but no approved production model or financial performance exists.
- Local review is a privileged filesystem/operator attestation. It is not a
  cryptographic source signature, remote ownership challenge or automatic legal
  rights/factual verifier. Generic model JSON has no verified-import payload type.
- Production model approval requires actual independently reviewed dataset,
  prospective shadow and owner-approval documents, exact candidate/evaluation
  hashes and a deployment-owned bundle pin. Fitting never creates those approvals.
- TEST ONLY artifacts are permanently rejected by production. The end-to-end
  inference success is explicitly wrapped offline output; production rejection is
  an intentional evidence gate, not a fake success claim.
- Original feature/label availability and source/execution hashes must be checked
  against actual records. The model does not reconstruct historical LLM forecasts,
  infer fill ordering from ambiguous bars or manufacture missing conditional costs.
- Calibration error is empirical. Serial market data do not establish independent
  trials or guaranteed uncertainty coverage. Independent holdout scoring does
  not refit the frozen candidate/calibrator. Out-of-range inputs and unseen
  regimes/horizons abstain; prospective drift monitoring/review remains required.
- Continuous producers must supply new independently verified quotes/costs within
  the 60-second observation window. Operator/producer automation is external;
  freshness cannot be renewed by reimporting old observations.
- License cleanup is an explicit `inputs-service prune` operation and must run
  even when serving is stopped. A crashed evaluation-journal lock requires operator
  inspection before recovery; holdout claims are never silently reset.
- HTTPS termination, OS identity/filesystem isolation and external process
  supervision remain deployment responsibilities. No startup is implied by this
  implementation or handoff.

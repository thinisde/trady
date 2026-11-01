# Native Perpetual advisory inference

## Overview

This module loads deployment-owned immutable binary classifiers and performs native Rust inference. Advice describes whether a persisted scenario's executable net target is reached before its stop or deadline. It cannot set limits, size orders, train models, approve deployments or submit trades.

The exact dependency is pinned in `Cargo.toml` as `perpetual = "=3.0.0-rc.2"`; the adapter exports the matching runtime/source/format identifiers through `[mod.rs:RUNTIME_VERSION]`, `[mod.rs:RUNTIME_SOURCE_REVISION]` and `[mod.rs:MODEL_FORMAT]`. Models must be exported for this pinned format and are rejected when their artifact metadata does not match those constants.

## Interface

- [`mod.rs:ApprovedArtifact`](./mod.rs): deployment-supplied `ModelManifest` and exact serialized model bytes. This type is absent from tool arguments.
- [`mod.rs:NativePerpetualAdvisor::from_approved_artifacts`](./mod.rs): construct the immutable registry and fail startup on invalid manifests or artifacts. An empty registry returns `UNAVAILABLE` when assessed.
- [`mod.rs:NativePerpetualAdvisor::model_count`](./mod.rs): inspect the loaded model count.
- [`mod.rs:NativePerpetualAdvisor::assess`](./mod.rs): implement the provider-neutral `PerpetualAdvisor` port with one-row native inference.
- [`mod.rs:RUNTIME_VERSION`, `RUNTIME_SOURCE_REVISION`, `MODEL_FORMAT`, `MAX_ARTIFACT_BYTES`](./mod.rs): exact runtime, source, serialization format and artifact limit.
- `crates/trady-domain/src/perpetual.rs:AssessArguments`, `ScenarioBinding`, `ScenarioSnapshot`, `MarketObservation`, `FeatureSnapshot`, `AssessmentContext`, `ModelManifest`, `Assessment`, `AssessmentRecord`, `PerpetualReview`: provider-neutral inputs, provenance and advisory outputs.
- `crates/trady-application/src/perpetual.rs:PerpetualRepository`, `PerpetualAdvisor`, `AdvisoryOutput`, `assess`, `validate_record`, `validate_review_for_scenario`, `record_review`: persistence and final-scenario validation boundary.

The composition root constructs `NativePerpetualAdvisor` from reviewed deployment artifacts. A repository resolves actual economics, market, profile, screening and evidence records. The supplied PostgreSQL adapter verifies persisted entry calculations and research bindings; holding references additionally resolve current executor-attested quantity, acquisition cost, exit charges and active plan. The trusted feed may supply dossier/features, but it must not supply deployment approval or select model bytes. Tool arguments contain only persisted reference IDs.

Call `trady_application::perpetual::assess(&mut repository, &advisor, &arguments, now)` to evaluate and persist advice. Before an entry or target raise, call `validate_review_for_scenario(&mut repository, &review, &final_scenario, &current_market, now)`. `final_scenario` is the actual `ScenarioBinding`, including entry, stop, target, costs, horizon, deadline, currency and all reference IDs. Research-only failures can be recorded with `record_review`.

## Invariants

- `[crates/trady-domain/src/perpetual.rs:ModelManifest::validate]` requires `APPROVED`, current validity, dataset/version provenance, chronological training/calibration/validation windows with a purge covering the maximum label horizon, a recorded learning budget/seed, sufficient calibration samples, an acceptable declared Brier score and evaluated scenario support. Deployment must resolve and approve actual validation reports; serialized metadata alone does not prove their existence. The PostgreSQL repository joins persisted local feature and evidence references.
- `[mod.rs:NativePerpetualAdvisor::from_approved_artifacts]` accepts at most 32 models, checks exact runtime/source/format, requires nonempty UTF-8 JSON bytes within the 16 MiB per-artifact limit, verifies SHA-256 before deserialization and rejects duplicate model revisions. Registry construction is an operator-controlled boundary; the serialized approval field does not authenticate its author.
- `[mod.rs:validate_booster]` supports numeric binary `LogLoss` models with 1–1024 trees and at most 65,536 total nodes. It rejects malformed graphs, depth above 64, nonfinite weights, invalid feature indices, categorical splits and unsupported embedded calibration/regression payloads.
- `[mod.rs:validate_booster]` requires native metadata keys `trady.feature_schema_version`, `trady.feature_names` and `trady.label_version` to match the manifest. `trady.feature_names` is the JSON array of ordered feature names.
- `[crates/trady-domain/src/perpetual.rs:AssessmentContext::validate]` requires a fresh persisted scenario, positive entry/stop below target, positive executable quote/FX, unique finite features and nonempty evidence lineage. Entry scenarios also require `stop < entry`; position scenarios may have break-even/profitable stops relative to the current cost-derived entry. Every source obeys `observed_at <= available_at <= feature as_of`; feature `as_of` equals scenario `calculated_at`, and deadline equals that time plus the stated horizon.
- `[crates/trady-domain/src/perpetual.rs:ScenarioSupport]` stop bounds are signed `(entry - stop) / entry` distances. Nonpositive bounds require `supports_positions`; a positive-only manifest remains unsupported for profitable stops even when it permits positions. No absolute-value conversion or implicit model promotion changes a trained range. Broader holding support requires separately evaluated and approved artifacts; the fixtures here are TEST ONLY.
- `[crates/trady-domain/src/perpetual.rs:ModelManifest::check_features]` enforces exact feature count/order, individual source freshness, instrument/horizon/target/stop/cost/position support and evaluated feature bounds. Label-free standardized-distance warnings and rejections return `ABSTAIN` without numerical predictions. They do not demonstrate unchanged realized predictive quality.
- `[mod.rs:NativePerpetualAdvisor::assess]` requires exactly one supported model. Missing, expired, unsupported or ambiguous models return `UNAVAILABLE`. Native `predict_proba(..., false, false)` supplies classification probabilities; this pinned version averages fold probabilities. Offline Platt calibration transforms the native probability with a stable logit/sigmoid path. `[mod.rs:calibrated_probability]` clamps probability endpoints before the logit;
  output is converted to `Decimal` through 16 fractional digits. Raw logits are
  never relabeled as probabilities.
- `[mod.rs:NativePerpetualAdvisor::assess]` returns an error for an invalid
  assessment ID, wrong native prediction shape or calibration/representation
  failure. Invalid contexts instead return `UNAVAILABLE` advice.
- `[mod.rs:NativePerpetualAdvisor::assess]` bounds successful validity by model TTL, model expiry, scenario/feature/market validity, deadline and every feature-source expiry. Expected returns, intervals and contributions remain absent because this contract has no separately validated models for them.
- `[crates/trady-application/src/perpetual.rs:assess]` validates the loaded context, independently validates adapter output and saves the immutable `AssessmentRecord` before returning it. Persistence failure returns an error.
- `[crates/trady-application/src/perpetual.rs:validate_review_for_scenario]` requires fresh `OK` advice, exact final-scenario equality, current feature validity and quote/FX changes within the manifest's evaluated tolerances. It accepts a reasoned `DISAGREED` review without a universal probability threshold, then saves the review. Normal policy, broker and executor gates remain the caller's responsibility.

## Side Effects

- `[mod.rs:NativePerpetualAdvisor]`: none beyond bounded in-memory model allocation and computation; no filesystem/network access, model mutation or broker calls.
- `[crates/trady-application/src/perpetual.rs:assess]`: calls `PerpetualRepository::load_context` and `save_assessment`; the repository must atomically persist immutable input/output/provenance and reject dangling or differently bound references.
- `[crates/trady-application/src/perpetual.rs:validate_review_for_scenario]` and
  `record_review`: load persisted assessments and call `PerpetualRepository::save_review` after validation. Database schema and implementation belong to the composition/storage layer.

## Verification

- Run from the workspace root: `cargo test --workspace --locked`.
- Run from the workspace root: `cargo test -p trady-domain perpetual --lib --locked`.
- Run from the workspace root: `cargo test -p trady-adapters perpetual --lib --locked -- --test-threads=1`.
- [`tests.rs`](./tests.rs) fits and serializes a real synthetic native classifier, verifies probability/calibration parity and persisted inputs, and exercises corrupt artifacts, unsupported objectives/graphs, missing models, stale/future/nonfinite features, drift, persistence failure and changed final scenarios. The synthetic fixture is test-only and has no financial deployment approval.
- [`holding_economics_tests.rs`](./holding_economics_tests.rs) uses that fitted fixture for final reviewed 1% to 2% holding activation after fresh confidence rises from 75 to 85, explicit signed support for profitable FX stops, and rejection of changed economics/model/drift/research or excessive nominal targets sharing a rounded price. Run `cargo test -p trady-adapters --lib perpetual::tests::holding_economics --locked`.
- Holding sentiment regressions check the exact 0.60 threshold, the ten-point penalty, a separate no-raise veto even at base score 100, no positive-sentiment bonus and live missing-reference rejection. Synthetic references exercise only restrictive policy; actual persistence still requires the fenced journal's atomic dependency check.
- Keep inference in the analyst execution path; the synchronous adapter does not impose a wall-clock worker deadline or run protection/exits. The [CLI analyst child](../../../trady-cli/src/autonomous_worker.rs) supplies process isolation and a 30-second request deadline; the executor uses separate market reads for holdings. Deployment still owns distinct OS/container identities, resource limits, approved artifacts and delayed outcome monitoring. PostgreSQL persists the local input/evidence lineage; external financial-report approval remains a deployment gate.

use super::*;
use std::collections::BTreeMap;
use trady_application::perpetual::{self as application, PerpetualRepository};
use trady_domain::{Currency, InstrumentId};

#[path = "holding_economics_tests.rs"]
mod holding_economics;

fn now() -> DateTime<Utc> {
    "2026-10-31T12:00:00Z".parse().unwrap()
}
fn decimal(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn context(value: f64) -> AssessmentContext {
    let at = now();
    let instrument_id = InstrumentId::new("US_TEST").unwrap();
    let arguments = AssessArguments {
        instrument_id: instrument_id.clone(),
        scenario_calculation_id: "scenario_1".into(),
        market_snapshot_id: "market_1".into(),
        profile_review_id: "profile_1".into(),
        screening_artifact_id: "screening_1".into(),
        position_id: None,
    };
    AssessmentContext {
        scenario: ScenarioSnapshot {
            binding: ScenarioBinding {
                arguments,
                currency: Currency::new("USD").unwrap(),
                entry_price: decimal("100"),
                stop_price: decimal("95"),
                target_price: decimal("110"),
                estimated_cost_fraction: decimal("0.001"),
                horizon_seconds: 3600,
                deadline_at: at + Duration::hours(1),
            },
            calculated_at: at,
            valid_until: at + Duration::minutes(5),
            market: MarketObservation {
                instrument_id,
                snapshot_id: "market_1".into(),
                observed_at: at,
                valid_until: at + Duration::minutes(5),
                executable_price: decimal("100"),
                fx_rate: Decimal::ONE,
            },
        },
        features: FeatureSnapshot {
            snapshot_id: "features_1".into(),
            schema_version: "test_features_v1".into(),
            as_of: at,
            valid_until: at + Duration::minutes(5),
            features: vec![FeatureValue {
                name: "signal".into(),
                value,
                lineage: vec![FeatureLineage {
                    source_id: "licensed_fixture".into(),
                    source_revision: "revision_1".into(),
                    evidence_id: "evidence_1".into(),
                    observed_at: at,
                    available_at: at,
                }],
            }],
        },
    }
}

fn manifest(hash: String) -> ModelManifest {
    let at = now();
    ModelManifest {
        model_revision_id: "test_model_1".into(),
        approval: ModelApproval::Approved,
        artifact_sha256: hash,
        runtime_version: RUNTIME_VERSION.into(),
        runtime_source_revision: RUNTIME_SOURCE_REVISION.into(),
        model_format: MODEL_FORMAT.into(),
        feature_schema: FeatureSchema {
            version: "test_features_v1".into(),
            features: vec![FeatureSpec {
                name: "signal".into(),
                minimum: -20.0,
                maximum: 20.0,
                reference_mean: 0.0,
                reference_stddev: 2.0,
                max_age_seconds: 120,
            }],
        },
        label_version: "test_target_v1".into(),
        target_definition: TARGET_DEFINITION.into(),
        training: TrainingLineage {
            dataset_id: "synthetic_native_test".into(),
            dataset_revision: "revision_1".into(),
            training_data_cutoff_at: at - Duration::days(30),
            calibration_start_at: at - Duration::days(25),
            calibration_end_at: at - Duration::days(20),
            validation_start_at: at - Duration::days(15),
            validation_end_at: at - Duration::days(5),
            purge_seconds: 3600,
            learning_budget: 1.0,
            random_seed: 7,
        },
        validation_report_id: "synthetic_validation_1".into(),
        calibration: ProbabilityCalibration {
            revision_id: "synthetic_platt_1".into(),
            slope: 0.8,
            intercept: -0.2,
            samples: 1000,
            minimum_samples: 100,
            brier_score: 0.15,
            maximum_brier_score: 0.25,
        },
        support: ScenarioSupport {
            instruments: vec![InstrumentId::new("US_TEST").unwrap()],
            minimum_horizon_seconds: 3600,
            maximum_horizon_seconds: 3600,
            minimum_target_fraction: decimal("0.01"),
            maximum_target_fraction: decimal("0.2"),
            minimum_stop_fraction: decimal("0.01"),
            maximum_stop_fraction: decimal("0.1"),
            maximum_cost_fraction: decimal("0.01"),
            supports_positions: false,
        },
        approved_at: at - Duration::days(1),
        valid_until: at + Duration::days(1),
        max_assessment_age_seconds: 60,
        quote_drift_tolerance_fraction: decimal("0.001"),
        fx_drift_tolerance_fraction: decimal("0.0005"),
        drift_warning_z: 3.0,
        drift_rejection_z: 5.0,
    }
}

/// A genuinely fitted native booster, serialized through the pinned Rust API.
/// This synthetic fixture has no financial validation or deployment approval.
fn trained_fixture() -> (Vec<u8>, f64, f64) {
    let values: Vec<f64> = (0..128).map(|i| (f64::from(i) - 63.5) / 32.0).collect();
    let targets: Vec<f64> = (0..128).map(|i| if i < 64 { 0.0 } else { 1.0 }).collect();
    let matrix = Matrix::new(&values, values.len(), 1);
    let mut booster = PerpetualBooster::default()
        .set_objective(Objective::LogLoss)
        .set_budget(1.0)
        .set_seed(7);
    booster.cfg.num_threads = Some(1);
    booster.cfg.iteration_limit = Some(2);
    booster.fit(&matrix, &targets, None, None).unwrap();
    assert!(
        !booster.trees.is_empty(),
        "fixture must contain fitted trees"
    );
    booster.insert_metadata(
        "trady.feature_schema_version".into(),
        "test_features_v1".into(),
    );
    booster.insert_metadata("trady.feature_names".into(), "[\"signal\"]".into());
    booster.insert_metadata("trady.label_version".into(), "test_target_v1".into());
    let sample = [1.0];
    let sample = Matrix::new(&sample, 1, 1);
    let probability = booster.predict_proba(&sample, false, false)[0];
    let raw = booster.predict(&sample, false)[0];
    (booster.json_dump().unwrap().into_bytes(), probability, raw)
}

fn advisor() -> NativePerpetualAdvisor {
    let (bytes, _, _) = trained_fixture();
    let hash = artifact_sha256(&bytes);
    NativePerpetualAdvisor::from_approved_artifacts(
        vec![ApprovedArtifact {
            manifest: manifest(hash),
            bytes,
        }],
        now(),
    )
    .unwrap()
}

#[derive(Default)]
struct Repository {
    context: Option<AssessmentContext>,
    records: BTreeMap<String, AssessmentRecord>,
    reviews: Vec<PerpetualReview>,
    fail_save: bool,
}
impl PerpetualRepository for Repository {
    fn load_context(&mut self, _: &AssessArguments) -> Result<AssessmentContext> {
        self.context
            .clone()
            .ok_or_else(|| Error("scenario does not exist".into()))
    }
    fn save_assessment(&mut self, record: &AssessmentRecord) -> Result<()> {
        require(!self.fail_save, "simulated persistence failure")?;
        require(
            !self.records.contains_key(&record.assessment.assessment_id),
            "duplicate assessment",
        )?;
        self.records
            .insert(record.assessment.assessment_id.clone(), record.clone());
        Ok(())
    }
    fn load_assessment(&mut self, id: &str) -> Result<Option<AssessmentRecord>> {
        Ok(self.records.get(id).cloned())
    }
    fn save_review(
        &mut self,
        review: &PerpetualReview,
        _: &ScenarioBinding,
        _: DateTime<Utc>,
    ) -> Result<()> {
        self.reviews.push(review.clone());
        Ok(())
    }
}

#[test]
fn real_native_fixture_uses_classification_probability_and_offline_calibration() {
    let (bytes, native_probability, raw_logit) = trained_fixture();
    assert!(native_probability > 0.5 && native_probability < 1.0);
    assert!((native_probability - raw_logit).abs() > 0.01);
    let m = manifest(artifact_sha256(&bytes));
    let a = NativePerpetualAdvisor::from_approved_artifacts(
        vec![ApprovedArtifact { manifest: m, bytes }],
        now(),
    )
    .unwrap();
    let c = context(1.0);
    let mut repo = Repository {
        context: Some(c.clone()),
        ..Repository::default()
    };
    let result = application::assess(&mut repo, &a, &c.scenario.binding.arguments, now()).unwrap();
    assert_eq!(result.status, AssessmentStatus::Ok);
    let expected =
        1.0 / (1.0 + (-(0.8 * (native_probability / (1.0 - native_probability)).ln() - 0.2)).exp());
    let actual = result
        .target_hit_probability
        .unwrap()
        .to_string()
        .parse::<f64>()
        .unwrap();
    assert!((actual - expected).abs() < 1e-14);
    assert!(
        (actual - native_probability).abs() > 0.001,
        "calibration must actually be applied"
    );
    assert_eq!(repo.records.len(), 1);
    assert_eq!(repo.records[&result.assessment_id].context, c);
    assert_eq!(result.valid_until, now() + Duration::seconds(60));
    assert_eq!(result.limits_authority, LimitsAuthority::AdvisoryOnly);
    assert!(result.expected_net_return_fraction.is_none() && result.return_interval.is_none());
    assert!(result.top_contributions.is_empty());
    let negative = a
        .assess("negative", &context(-1.0), now())
        .unwrap()
        .assessment;
    assert!(negative.target_hit_probability < result.target_hit_probability);
}

#[test]
fn empty_registry_persists_unavailable_and_blocks_entry() {
    let advisor = NativePerpetualAdvisor::from_approved_artifacts(vec![], now()).unwrap();
    let c = context(1.0);
    let mut repo = Repository {
        context: Some(c.clone()),
        ..Repository::default()
    };
    let result =
        application::assess(&mut repo, &advisor, &c.scenario.binding.arguments, now()).unwrap();
    assert_eq!(result.status, AssessmentStatus::Unavailable);
    assert!(result.target_hit_probability.is_none());
    assert_eq!(repo.records.len(), 1);
    let review = PerpetualReview {
        assessment_id: result.assessment_id,
        influence: AssessmentInfluence::Unavailable,
        reason: "No approved deployment".into(),
    };
    application::record_review(&mut repo, &review, now()).unwrap();
    assert!(
        application::validate_review_for_scenario(
            &mut repo,
            &review,
            &c.scenario.binding,
            &c.scenario.market,
            now()
        )
        .is_err()
    );
}

#[test]
fn invalid_artifacts_are_rejected_at_startup() {
    let (bytes, _, _) = trained_fixture();
    let good = manifest(artifact_sha256(&bytes));
    for fault in [
        "hash",
        "runtime",
        "source",
        "format",
        "approval",
        "calibration",
        "purge",
        "expiry",
        "metadata",
    ] {
        let mut m = good.clone();
        match fault {
            "hash" => m.artifact_sha256 = "0".repeat(64),
            "runtime" => m.runtime_version = "2.3.0".into(),
            "source" => m.runtime_source_revision = "unreviewed".into(),
            "format" => m.model_format = "pickle".into(),
            "approval" => m.approval = ModelApproval::Candidate,
            "calibration" => m.calibration.samples = 0,
            "purge" => m.training.purge_seconds = 0,
            "expiry" => m.valid_until = now(),
            "metadata" => m.feature_schema.version = "wrong_schema".into(),
            _ => unreachable!(),
        }
        assert!(
            NativePerpetualAdvisor::from_approved_artifacts(
                vec![ApprovedArtifact {
                    manifest: m,
                    bytes: bytes.clone()
                }],
                now()
            )
            .is_err(),
            "{fault}"
        );
    }
    let malformed = b"{}".to_vec();
    let m = manifest(artifact_sha256(&malformed));
    assert!(
        NativePerpetualAdvisor::from_approved_artifacts(
            vec![ApprovedArtifact {
                manifest: m,
                bytes: malformed
            }],
            now()
        )
        .is_err()
    );
}

#[test]
fn missing_unsupported_future_stale_and_nonfinite_features_fail_closed() {
    let a = advisor();
    for fault in [
        "schema",
        "order",
        "missing",
        "duplicate",
        "lineage",
        "future",
        "stale",
        "nan",
        "infinity",
        "horizon",
        "target",
        "position",
        "scenario",
        "market",
    ] {
        let mut c = context(1.0);
        match fault {
            "schema" => c.features.schema_version = "unsupported".into(),
            "order" => c.features.features[0].name = "other_feature".into(),
            "missing" => c.features.features.clear(),
            "duplicate" => c.features.features.push(c.features.features[0].clone()),
            "lineage" => c.features.features[0].lineage.clear(),
            "future" => {
                c.features.features[0].lineage[0].available_at = now() + Duration::seconds(1)
            }
            "stale" => {
                let l = &mut c.features.features[0].lineage[0];
                l.observed_at = now() - Duration::seconds(121);
                l.available_at = l.observed_at;
            }
            "nan" => c.features.features[0].value = f64::NAN,
            "infinity" => c.features.features[0].value = f64::INFINITY,
            "horizon" => {
                c.scenario.binding.horizon_seconds = 7200;
                c.scenario.binding.deadline_at = now() + Duration::hours(2);
            }
            "target" => c.scenario.binding.target_price = decimal("200"),
            "position" => c.scenario.binding.arguments.position_id = Some("position_1".into()),
            "scenario" => c.scenario.valid_until = now(),
            "market" => c.scenario.market.valid_until = now(),
            _ => unreachable!(),
        }
        let out = a.assess("failed", &c, now()).unwrap();
        assert_eq!(
            out.assessment.status,
            AssessmentStatus::Unavailable,
            "{fault}"
        );
        assert!(out.assessment.target_hit_probability.is_none(), "{fault}");
    }
}

#[test]
fn warning_rejection_and_out_of_support_drift_never_return_predictions() {
    let a = advisor();
    for (value, drift) in [
        (6.5, DriftStatus::Warning),
        (10.0, DriftStatus::OutOfRange),
        (25.0, DriftStatus::OutOfRange),
    ] {
        let out = a.assess("drift", &context(value), now()).unwrap();
        assert_eq!(out.assessment.status, AssessmentStatus::Abstain);
        assert_eq!(out.assessment.drift_status, drift);
        assert!(out.assessment.target_hit_probability.is_none());
        out.assessment.validate().unwrap();
    }
}

#[test]
fn duplicate_or_ambiguous_models_cannot_be_selected_silently() {
    let (bytes, _, _) = trained_fixture();
    let m = manifest(artifact_sha256(&bytes));
    assert!(
        NativePerpetualAdvisor::from_approved_artifacts(
            vec![
                ApprovedArtifact {
                    manifest: m.clone(),
                    bytes: bytes.clone()
                },
                ApprovedArtifact {
                    manifest: m.clone(),
                    bytes: bytes.clone()
                }
            ],
            now()
        )
        .is_err()
    );
    let mut second = m.clone();
    second.model_revision_id = "test_model_2".into();
    let a = NativePerpetualAdvisor::from_approved_artifacts(
        vec![
            ApprovedArtifact {
                manifest: m,
                bytes: bytes.clone(),
            },
            ApprovedArtifact {
                manifest: second,
                bytes,
            },
        ],
        now(),
    )
    .unwrap();
    assert_eq!(
        a.assess("ambiguous", &context(1.0), now())
            .unwrap()
            .assessment
            .status,
        AssessmentStatus::Unavailable
    );
}

#[test]
fn expired_models_fail_closed_after_successful_startup() {
    let a = advisor();
    let mut c = context(1.0);
    let tomorrow = now() + Duration::days(2);
    c.scenario.calculated_at = tomorrow;
    c.scenario.valid_until = tomorrow + Duration::minutes(5);
    c.scenario.binding.deadline_at = tomorrow + Duration::hours(1);
    c.scenario.market.observed_at = tomorrow;
    c.scenario.market.valid_until = c.scenario.valid_until;
    c.features.as_of = tomorrow;
    c.features.valid_until = c.scenario.valid_until;
    c.features.features[0].lineage[0].observed_at = tomorrow;
    c.features.features[0].lineage[0].available_at = tomorrow;
    assert_eq!(
        a.assess("expired", &c, tomorrow).unwrap().assessment.status,
        AssessmentStatus::Unavailable
    );
}

#[test]
fn final_scenario_binding_quote_fx_and_expiry_are_enforced() {
    let a = advisor();
    let c = context(1.0);
    let mut repo = Repository {
        context: Some(c.clone()),
        ..Repository::default()
    };
    let assessment =
        application::assess(&mut repo, &a, &c.scenario.binding.arguments, now()).unwrap();
    let review = PerpetualReview {
        assessment_id: assessment.assessment_id,
        influence: AssessmentInfluence::Disagreed,
        reason: "Verified counterevidence justifies retaining independently checked limits".into(),
    };
    application::validate_review_for_scenario(
        &mut repo,
        &review,
        &c.scenario.binding,
        &c.scenario.market,
        now(),
    )
    .unwrap();
    assert_eq!(repo.reviews.len(), 1);
    for field in [
        "stop", "target", "entry", "horizon", "cost", "profile", "market",
    ] {
        let mut b = c.scenario.binding.clone();
        match field {
            "stop" => b.stop_price -= Decimal::ONE,
            "target" => b.target_price += Decimal::ONE,
            "entry" => b.entry_price += Decimal::ONE,
            "horizon" => b.horizon_seconds += 1,
            "cost" => b.estimated_cost_fraction += decimal("0.001"),
            "profile" => b.arguments.profile_review_id = "profile_2".into(),
            "market" => b.arguments.market_snapshot_id = "market_2".into(),
            _ => unreachable!(),
        }
        assert!(
            application::validate_review_for_scenario(
                &mut repo,
                &review,
                &b,
                &c.scenario.market,
                now()
            )
            .is_err(),
            "{field}"
        );
    }
    for (quote, fx) in [("100.11", "1"), ("100", "1.0006")] {
        let mut market = c.scenario.market.clone();
        market.executable_price = decimal(quote);
        market.fx_rate = decimal(fx);
        assert!(
            application::validate_review_for_scenario(
                &mut repo,
                &review,
                &c.scenario.binding,
                &market,
                now()
            )
            .is_err()
        );
    }
    assert!(
        application::validate_review_for_scenario(
            &mut repo,
            &review,
            &c.scenario.binding,
            &c.scenario.market,
            now() + Duration::seconds(60)
        )
        .is_err()
    );
    assert_eq!(
        repo.reviews.len(),
        1,
        "failed binding validations must not save a successful review"
    );
}

#[test]
fn persistence_failure_or_wrong_reference_never_returns_success() {
    let a = advisor();
    let c = context(1.0);
    let mut repo = Repository {
        context: Some(c.clone()),
        fail_save: true,
        ..Repository::default()
    };
    assert!(application::assess(&mut repo, &a, &c.scenario.binding.arguments, now()).is_err());
    assert!(repo.records.is_empty());
    let mut wrong = c.scenario.binding.arguments.clone();
    wrong.profile_review_id = "different_profile".into();
    repo.fail_save = false;
    assert!(application::assess(&mut repo, &a, &wrong, now()).is_err());
    assert!(repo.records.is_empty());
}

#[test]
fn stable_calibration_handles_extreme_probabilities_and_rejects_nonfinite() {
    let mut c = manifest("0".repeat(64)).calibration;
    c.slope = 100.0;
    c.intercept = -100.0;
    for p in [0.0, 1.0, 1e-300, 1.0 - f64::EPSILON] {
        let result = calibrated_probability(p, &c).unwrap();
        assert!(result.is_finite() && (0.0..=1.0).contains(&result));
    }
    for p in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(calibrated_probability(p, &c).is_err());
    }
}

#[test]
fn malformed_native_graphs_and_wrong_objectives_are_rejected() {
    let (bytes, _, _) = trained_fixture();
    for fault in [
        "objective",
        "cycle",
        "feature",
        "child",
        "nonfinite",
        "metadata",
    ] {
        let mut booster =
            PerpetualBooster::from_json(std::str::from_utf8(&bytes).unwrap()).unwrap();
        match fault {
            "objective" => booster.cfg.objective = Objective::SquaredLoss,
            "cycle" => {
                let n = booster.trees[0].nodes.get_mut(&0).unwrap();
                n.is_leaf = false;
                n.left_child = 0;
            }
            "feature" => {
                let n = booster.trees[0].nodes.get_mut(&0).unwrap();
                n.is_leaf = false;
                n.split_feature = 10;
            }
            "child" => {
                let n = booster.trees[0].nodes.get_mut(&0).unwrap();
                n.is_leaf = false;
                n.left_child = 123456;
            }
            "nonfinite" => booster.base_score = f64::INFINITY,
            "metadata" => {
                booster.metadata.remove("trady.feature_names");
            }
            _ => unreachable!(),
        }
        assert!(
            validate_booster(&booster, &manifest("0".repeat(64))).is_err(),
            "{fault}"
        );
    }
}

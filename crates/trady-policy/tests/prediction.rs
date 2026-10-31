//! Synthetic fixtures test arithmetic and rejection, never predictive performance.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeSet;
use trady_domain::{InstrumentId, prediction::*};
use trady_policy::prediction::*;

fn d(value: &str) -> Decimal {
    Decimal::from_str_exact(value).unwrap()
}
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
        .unwrap()
        .to_utc()
}
fn context() -> FusionContext {
    FusionContext {
        instrument_id: InstrumentId::new("test_stock").unwrap(),
        market_snapshot_id: "market_1".into(),
        scenario_hash: "a".repeat(64),
        horizon: ForecastHorizon {
            decision_at: now(),
            deadline_at: now() + Duration::days(3),
            exchange_sessions: 3,
        },
        regime: "normal".into(),
    }
}
fn config() -> FusionConfig {
    FusionConfig {
        version: 1,
        revision_id: "baseline_1".into(),
        approved_models: [
            PredictionSource::LlmResearch,
            PredictionSource::FinBert,
            PredictionSource::Perpetual,
            PredictionSource::Quantitative,
        ]
        .into_iter()
        .map(|source| (source, format!("fixture_{source:?}")))
        .collect(),
        outcome_calibration_report_id: "calibration_1".into(),
        minimum_calibration_samples: 100,
        maximum_age_seconds: 300,
        maximum_uncertainty: d("0.002"),
    }
}
fn predictions() -> Vec<Prediction> {
    let context = context();
    config()
        .approved_models
        .into_iter()
        .map(|(source, model_version)| {
            let (target, payload) = match source {
                PredictionSource::LlmResearch => (
                    PredictionTarget::ResearchEvidence,
                    PredictionPayload::Research {
                        subjective_confidence: Some(100),
                        material_conflict: false,
                    },
                ),
                PredictionSource::FinBert => (
                    PredictionTarget::FinancialTextSentiment,
                    PredictionPayload::Sentiment {
                        positive: d("0.8"),
                        negative: d("0.1"),
                        neutral: d("0.1"),
                    },
                ),
                PredictionSource::Perpetual => (
                    PredictionTarget::ScenarioOutcomes,
                    PredictionPayload::ScenarioOutcomes {
                        probabilities: OutcomeProbabilities {
                            target_first: d("0.6"),
                            stop_first: d("0.2"),
                            expiry: d("0.1"),
                            non_fill: d("0.1"),
                        },
                    },
                ),
                PredictionSource::Quantitative => (
                    PredictionTarget::MarketIndicators,
                    PredictionPayload::Quantitative {
                        indicators: QuantitativeIndicators {
                            momentum_return: Some(d("0.01")),
                            atr: Some(d("0.5")),
                            realized_volatility: Some(d("0.02")),
                            relative_volume: Some(d("1.5")),
                            vwap_distance_fraction: Some(d("0.001")),
                            relative_strength: Some(d("0.01")),
                            spread_fraction: Some(d("0.0001")),
                        },
                    },
                ),
            };
            Prediction {
                metadata: PredictionMetadata {
                    schema_version: 1,
                    prediction_id: format!("prediction_{source:?}"),
                    source,
                    model_version,
                    instrument_id: context.instrument_id.clone(),
                    market_snapshot_id: context.market_snapshot_id.clone(),
                    scenario_hash: Some(context.scenario_hash.clone()),
                    observed_at: now() - Duration::seconds(2),
                    available_at: now() - Duration::seconds(1),
                    generated_at: now(),
                    valid_until: now() + Duration::minutes(5),
                    horizon: context.horizon.clone(),
                    target,
                    target_definition: "Synthetic four-outcome scenario fixture".into(),
                    label_version: "labels_1".into(),
                    uncertainty: Some(PredictionUncertainty {
                        unit: UncertaintyUnit::NetAccountReturn,
                        radius: d("0.001"),
                    }),
                    calibration: CalibrationRecord {
                        status: CalibrationStatus::Validated,
                        calibrator_version: Some("calibrator_1".into()),
                        report_id: Some("calibration_1".into()),
                        training_cutoff_at: Some(now() - Duration::days(60)),
                        validation_start_at: Some(now() - Duration::days(59)),
                        validation_end_at: Some(now() - Duration::days(1)),
                        sample_count: 100,
                        out_of_sample: true,
                        supported_regimes: vec!["normal".into()],
                    },
                    regime: "normal".into(),
                    dependence_group_id: format!("group_{source:?}"),
                    evidence_lineages: vec!["shared_market_observation".into()],
                },
                payload,
            }
        })
        .collect()
}
fn outcome_mut(values: &mut [Prediction]) -> &mut Prediction {
    values
        .iter_mut()
        .find(|p| p.metadata.source == PredictionSource::Perpetual)
        .unwrap()
}
fn utility_inputs() -> (OutcomeEstimates, UtilityConfig) {
    (
        OutcomeEstimates {
            scenario_hash: context().scenario_hash,
            forecast_prediction_id: "prediction_Perpetual".into(),
            estimation_report_id: "payoffs_1".into(),
            cost_model_hash: "c".repeat(64),
            available_at: now(),
            normalization: ReturnNormalization {
                account_id: trady_domain::AccountId::new("paper_prediction_test").unwrap(),
                account_snapshot_id: "account_1".into(),
                opening_equity: trady_domain::Money {
                    amount: d("1000"),
                    currency: trady_domain::Currency::new("EUR").unwrap(),
                },
            },
            net_account_returns: [d("0.02"), d("-0.01"), d("-0.002"), Decimal::ZERO],
        },
        UtilityConfig {
            version: 1,
            lambda_risk: d("2"),
            lambda_uncertainty: Decimal::ONE,
            lambda_complexity: d("0.0001"),
            complexity: 2,
            cash_hurdle: Decimal::ZERO,
            approved_estimation_report_id: "payoffs_1".into(),
            cost_model_hash: "c".repeat(64),
            comparison_deadline_at: context().horizon.deadline_at,
        },
    )
}

#[test]
fn baseline_preserves_probability_meaning_and_reports_dependence() {
    let engine = PredictionFusionEngine::new(config()).unwrap();
    let mut values = predictions();
    let fused = engine.fuse(&context(), &values, now()).unwrap();
    assert_eq!(fused.status(), FusionStatus::Ready);
    assert_eq!(fused.probabilities().unwrap().target_first, d("0.6"));
    assert_eq!(
        fused.duplicated_lineages()["shared_market_observation"].len(),
        4
    );
    values[0].payload = PredictionPayload::Research {
        subjective_confidence: Some(1),
        material_conflict: false,
    };
    let changed = engine.fuse(&context(), &values, now()).unwrap();
    assert_eq!(changed.probabilities(), fused.probabilities());
    values.push(values[0].clone());
    assert_eq!(
        engine
            .fuse(&context(), &values, now())
            .unwrap()
            .probabilities(),
        fused.probabilities()
    );
    values.last_mut().unwrap().metadata.target_definition = "changed immutable record".into();
    assert_eq!(
        engine.fuse(&context(), &values, now()).unwrap().status(),
        FusionStatus::Abstain
    );
}

#[test]
fn unsupported_forecasts_abstain_instead_of_inventing_outcomes() {
    let engine = PredictionFusionEngine::new(config()).unwrap();
    for case in 0..11 {
        let mut values = predictions();
        let p = outcome_mut(&mut values);
        match case {
            0 => {
                p.payload = PredictionPayload::BinaryTargetHit {
                    probability: d("0.9"),
                };
                p.metadata.target = PredictionTarget::TargetBeforeStop;
            }
            1 => p.metadata.uncertainty = None,
            2 => p.metadata.uncertainty.as_mut().unwrap().unit = UncertaintyUnit::ClassProbability,
            3 => p.metadata.calibration.status = CalibrationStatus::Uncalibrated,
            4 => p.metadata.calibration.status = CalibrationStatus::Drifted,
            5 => p.metadata.calibration.supported_regimes = vec!["different_regime".into()],
            6 => p.metadata.calibration.sample_count = 99,
            7 => p.metadata.available_at = now() + Duration::seconds(1),
            8 => p.metadata.valid_until = now(),
            9 => p.metadata.model_version = "unapproved_revision".into(),
            _ => p.metadata.calibration.validation_end_at = Some(now()),
        }
        let fused = engine.fuse(&context(), &values, now()).unwrap();
        assert_eq!(fused.status(), FusionStatus::Abstain, "case {case}");
        assert!(fused.probabilities().is_none());
    }
    assert_eq!(
        engine.fuse(&context(), &[], now()).unwrap().status(),
        FusionStatus::Abstain
    );
    assert_eq!(
        engine
            .fuse(&context(), &predictions(), now() + Duration::minutes(6))
            .unwrap()
            .status(),
        FusionStatus::Abstain
    );
}

#[test]
fn adverse_sentiment_conflicts_and_ablation_cannot_create_entry_evidence() {
    let engine = PredictionFusionEngine::new(config()).unwrap();
    let mut values = predictions();
    values[1].payload = PredictionPayload::Sentiment {
        positive: d("0.1"),
        negative: d("0.8"),
        neutral: d("0.1"),
    };
    assert!(
        engine
            .fuse(&context(), &values, now())
            .unwrap()
            .reason_codes()
            .iter()
            .any(|s| s == "ADVERSE_SENTIMENT_ENTRY_VETO")
    );
    values = predictions();
    values[0].payload = PredictionPayload::Research {
        subjective_confidence: Some(100),
        material_conflict: true,
    };
    assert_eq!(
        engine.fuse(&context(), &values, now()).unwrap().status(),
        FusionStatus::Abstain
    );
    let excluded = BTreeSet::from([PredictionSource::FinBert]);
    let ablated = engine
        .fuse_ablation(&context(), &predictions(), &excluded, now())
        .unwrap();
    let (payoffs, config) = utility_inputs();
    assert!(expected_utility(&ablated, &payoffs, &config, now()).is_err());
}

#[test]
fn full_net_outcomes_define_utility_and_cash_can_win() {
    let engine = PredictionFusionEngine::new(config()).unwrap();
    let fused = engine.fuse(&context(), &predictions(), now()).unwrap();
    let (mut payoffs, config) = utility_inputs();
    let score = expected_utility(&fused, &payoffs, &config, now()).unwrap();
    assert_eq!(score.expected_net_account_return(), d("0.0098"));
    assert_eq!(score.expected_downside(), d("0.0022"));
    // Returns already contain all costs; utility subtracts only declared penalties.
    assert_eq!(score.utility(), d("0.0042"));
    assert!(matches!(
        select_scenario(std::slice::from_ref(&score), &config, now()).unwrap(),
        UtilitySelection::Candidate { .. }
    ));
    let mut hurdle_config = config.clone();
    hurdle_config.cash_hurdle = d("0.0042");
    let hurdle_score = expected_utility(&fused, &payoffs, &hurdle_config, now()).unwrap();
    assert!(matches!(
        select_scenario(&[hurdle_score], &hurdle_config, now()).unwrap(),
        UtilitySelection::NoTrade { .. }
    ));
    assert!(matches!(
        select_scenario(&[], &config, now()).unwrap(),
        UtilitySelection::NoTrade { .. }
    ));
    assert!(select_scenario(&[score.clone(), score], &config, now()).is_err());
    payoffs.net_account_returns = [Decimal::ZERO, d("-0.01"), d("-0.002"), Decimal::ZERO];
    let score = expected_utility(&fused, &payoffs, &config, now()).unwrap();
    assert!(matches!(
        select_scenario(&[score], &config, now()).unwrap(),
        UtilitySelection::NoTrade { .. }
    ));
    payoffs.scenario_hash = "b".repeat(64);
    assert!(expected_utility(&fused, &payoffs, &config, now()).is_err());
}

#[test]
fn labels_mature_without_rewriting_the_original_forecast() {
    let values = predictions();
    let label = MaturedOutcome {
        prediction_id: "prediction_Perpetual".into(),
        scenario_hash: context().scenario_hash,
        label_version: "labels_1".into(),
        horizon_end_at: context().horizon.deadline_at,
        available_at: context().horizon.deadline_at + Duration::seconds(1),
        outcome: Some(ScenarioOutcome::TargetFirst),
    };
    let pending = score_matured_predictions(&values, std::slice::from_ref(&label), now()).unwrap();
    assert_eq!(pending.scored, 0);
    assert_eq!(pending.pending_or_censored, 1);
    let result =
        score_matured_predictions(&values, std::slice::from_ref(&label), label.available_at)
            .unwrap();
    assert_eq!(result.scored, 1);
    assert_eq!(result.multiclass_brier_score, Some(d("0.22")));
    assert_eq!(result.regime_samples["normal"], 1);
    let mut changed = label.clone();
    changed.outcome = None;
    assert_eq!(
        score_matured_predictions(&values, std::slice::from_ref(&changed), label.available_at)
            .unwrap()
            .pending_or_censored,
        1
    );
    changed.label_version = "labels_2".into();
    assert!(score_matured_predictions(&values, &[changed], label.available_at).is_err());
    let mut duplicate = values.clone();
    duplicate.push(values[0].clone());
    assert!(score_matured_predictions(&duplicate, &[label], now()).is_err());
}

#[test]
fn refreshed_generation_cannot_launder_old_information_or_expired_utility() {
    let engine = PredictionFusionEngine::new(config()).unwrap();
    let mut values = predictions();
    values[0].metadata.observed_at = now() - Duration::hours(1);
    assert_eq!(
        engine.fuse(&context(), &values, now()).unwrap().status(),
        FusionStatus::Abstain
    );
    let fused = engine.fuse(&context(), &predictions(), now()).unwrap();
    let (payoffs, config) = utility_inputs();
    let score = expected_utility(&fused, &payoffs, &config, now()).unwrap();
    assert!(expected_utility(&fused, &payoffs, &config, now() + Duration::minutes(6)).is_err());
    assert!(select_scenario(&[score], &config, now() + Duration::minutes(6)).is_err());
}

#[test]
fn ranking_requires_same_frozen_objective_and_account_normalization() {
    let engine = PredictionFusionEngine::new(config()).unwrap();
    let fused = engine.fuse(&context(), &predictions(), now()).unwrap();
    let (payoffs, config) = utility_inputs();
    let score = expected_utility(&fused, &payoffs, &config, now()).unwrap();
    let mut changed_config = config.clone();
    changed_config.lambda_risk = Decimal::ZERO;
    assert!(select_scenario(std::slice::from_ref(&score), &changed_config, now()).is_err());
    let mut other_context = context();
    other_context.scenario_hash = "b".repeat(64);
    let mut other_predictions = predictions();
    for prediction in &mut other_predictions {
        prediction.metadata.scenario_hash = Some(other_context.scenario_hash.clone());
    }
    let other = engine
        .fuse(&other_context, &other_predictions, now())
        .unwrap();
    let mut other_payoffs = payoffs;
    other_payoffs.scenario_hash = other_context.scenario_hash;
    other_payoffs.normalization.opening_equity.amount = d("2000");
    let other_score = expected_utility(&other, &other_payoffs, &config, now()).unwrap();
    assert!(select_scenario(&[score, other_score], &config, now()).is_err());
}

#[test]
fn training_purges_overlapping_outcomes_and_requires_embargo() {
    let split = ChronologicalSplit {
        training_end_at: now(),
        calibration_start_at: now() + Duration::days(2),
        calibration_end_at: now() + Duration::days(10),
        validation_start_at: now() + Duration::days(12),
        validation_end_at: now() + Duration::days(20),
        test_start_at: now() + Duration::days(22),
        test_end_at: now() + Duration::days(30),
        embargo_seconds: 86_400,
    };
    assert!(
        split
            .training_sample_eligible(now() - Duration::days(4), now() - Duration::seconds(1))
            .unwrap()
    );
    assert!(
        !split
            .training_sample_eligible(now() - Duration::days(4), now() + Duration::seconds(1))
            .unwrap()
    );
    let mut invalid = split;
    invalid.validation_start_at = invalid.calibration_end_at;
    assert!(invalid.validate().is_err());
}

#[test]
fn sentiment_payload_cannot_be_relabelled_as_target_hit_probability() {
    let mut value = predictions().remove(1);
    value.metadata.source = PredictionSource::Perpetual;
    value.metadata.target = PredictionTarget::TargetBeforeStop;
    assert!(value.validate(now()).is_err());
    let mut value = predictions().remove(2);
    if let PredictionPayload::ScenarioOutcomes { probabilities } = &mut value.payload {
        probabilities.non_fill = d("0.2");
    }
    assert!(value.validate(now()).is_err());
}

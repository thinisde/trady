//! TEST ONLY synthetic observations; none of these are financial history or release evidence.
use super::*;
use crate::optimization::*;
use trady_domain::{
    AccountId, AccountSnapshot, Currency, InstrumentId, Mode, Quote, prediction::*,
};
use trady_policy::{EntryCosts, EntryRange, InstrumentRules, Policy, limits::*, prediction::*};

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value).unwrap().to_utc()
}
fn d(v: &str) -> Decimal {
    Decimal::from_str_exact(v).unwrap()
}
fn money(v: Decimal) -> Money {
    Money {
        amount: v,
        currency: Currency::new("EUR").unwrap(),
    }
}
fn dataset(id: &str, starts: &[DateTime<Utc>]) -> ObservationDataset {
    let mut rows = Vec::new();
    for start in starts {
        for i in 0..80 {
            let class = i % 4;
            let decision = *start + Duration::days(i) + Duration::hours(15);
            let target = d("0.01") + Decimal::from(i % 3) * d("0.01");
            let stop = d("0.005") + Decimal::from(i % 4) * d("0.005");
            let payoff = match class {
                0 => target,
                1 => -stop,
                2 => d("0.002"),
                _ => Decimal::ZERO,
            };
            let execution = PayoffRecord {
                acquisition_cost: money(d("500")),
                opening_equity: money(d("1000")),
                gross_pnl: money(payoff * d("500") + d("0.15")),
                fees: money(d("0.1")),
                fx_cost: money(d("0.02")),
                slippage: money(d("0.03")),
                cash_pnl: money(d("0.01")),
            };
            rows.push(PointInTimeRow {
                id: format!("TEST_ONLY_{}_{}", start.timestamp(), i),
                instrument_id: "TEST_ONLY_STOCK".into(),
                market_snapshot_id: format!("TEST_ONLY_MARKET_{}_{}", start.timestamp(), i),
                scenario_hash: digest(&(i, decision, "TEST ONLY")).unwrap(),
                regime: "TEST_ONLY_REGIME".into(),
                feature_window_start_at: decision - Duration::minutes(10),
                observed_at: decision - Duration::minutes(1),
                available_at: decision - Duration::seconds(10),
                decision_at: decision,
                label_end_at: decision + Duration::hours(6),
                outcome_available_at: decision + Duration::hours(6) + Duration::minutes(1),
                source_record_sha256: digest(&(decision, "TEST ONLY synthetic source")).unwrap(),
                execution_record_sha256: digest(&execution).unwrap(),
                evidence_lineages: vec!["TEST_ONLY_ORIGIN".into()],
                features: (0..4)
                    .map(|k| {
                        if k == class {
                            Decimal::ONE
                        } else {
                            Decimal::ZERO
                        }
                    })
                    .collect(),
                scenario: ScenarioFeatures {
                    target_net_fraction: target,
                    stop_distance_fraction: stop,
                    spread_fraction: if i % 2 == 0 { d("0.0001") } else { d("0.005") },
                    exchange_sessions: 1,
                },
                outcome: Some(
                    [
                        ScenarioOutcome::TargetFirst,
                        ScenarioOutcome::StopFirst,
                        ScenarioOutcome::Expiry,
                        ScenarioOutcome::NonFill,
                    ][class as usize],
                ),
                execution: Some(execution),
            });
        }
    }
    ObservationDataset {
        schema_version: 1,
        dataset_id: id.into(),
        test_only: true,
        feature_schema_version: "TEST_ONLY_FEATURES_V1".into(),
        feature_names: (0..4).map(|n| format!("TEST_ONLY_FEATURE_{n}")).collect(),
        label_version: "TEST_ONLY_EXECUTABLE_FOUR_WAY_V1".into(),
        cost_model_hash: digest(&"TEST ONLY costs").unwrap(),
        rows,
    }
}
fn plan() -> TrainingPlan {
    TrainingPlan {
        model_id: "TEST_ONLY_MODEL_V1".into(),
        train: Window {
            start: at("2020-01-01T00:00:00Z"),
            end: at("2020-04-01T00:00:00Z"),
        },
        calibration: Window {
            start: at("2020-04-02T00:00:00Z"),
            end: at("2020-07-01T00:00:00Z"),
        },
        validation: Window {
            start: at("2020-07-02T00:00:00Z"),
            end: at("2020-10-01T00:00:00Z"),
        },
        sealed_holdout_start_at: at("2020-10-03T00:00:00Z"),
        embargo_seconds: 3600,
        fit_at: at("2020-10-02T00:00:00Z"),
        iterations: 400,
        learning_rate: 0.15,
        ridge: 0.001,
        minimum_class_samples: 4,
        minimum_regime_samples: 8,
        maximum_validation_brier: 0.4,
        maximum_validation_log_loss: 0.8,
        uncertainty_quantile: 0.9,
    }
}
fn fitted() -> (ModelCandidate, ObservationDataset) {
    let p = plan();
    let data = dataset(
        "TEST_ONLY_DEVELOPMENT",
        &[p.train.start, p.calibration.start, p.validation.start],
    );
    (
        fit(&data, &p).unwrap(),
        dataset("TEST_ONLY_SEALED_HOLDOUT", &[at("2020-10-04T00:00:00Z")]),
    )
}

#[test]
fn fits_nonconstant_probabilities_and_real_conditional_cost_records() {
    let (c, holdout) = fitted();
    assert!(c.development_eligible);
    let report = evaluate(&c, &holdout, at("2021-01-01T00:00:00Z")).unwrap();
    assert!(report.eligible);
    assert_eq!(report.metrics.samples, 80);
    assert!(report.metrics.multiclass_brier < 0.1);
    assert!(report.metrics.log_loss < 0.2);
    assert_eq!(report.metrics.class_counts, [20; 4]);
    let (a, payoffs) =
        fit::predict(&c, &holdout.rows[0].features, &holdout.rows[0].scenario).unwrap();
    let (b, _) = fit::predict(&c, &holdout.rows[1].features, &holdout.rows[1].scenario).unwrap();
    assert!(a[0] > a[1] && b[1] > b[0]);
    assert!(payoffs[0] > 0.0 && payoffs[1] < 0.0 && payoffs[3].abs() < 1e-9);
    assert!(report.metrics.cash_account_return > 0.0);
    assert_eq!(report.by_regime["TEST_ONLY_REGIME"].samples, 80);
    assert_eq!(report.by_horizon[&1].samples, 80);
}

#[test]
fn transforms_fit_only_training_and_validation_never_refits() {
    let p = plan();
    let mut data = dataset(
        "TEST_ONLY",
        &[p.train.start, p.calibration.start, p.validation.start],
    );
    let first = fit(&data, &p).unwrap();
    for row in &mut data.rows {
        if row.decision_at >= p.validation.start {
            row.features = [Decimal::ONE, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO].to_vec();
        }
    }
    let second = fit(&data, &p).unwrap();
    assert_eq!(first.center, second.center);
    assert_eq!(first.scale, second.scale);
    assert_eq!(first.classifier, second.classifier);
    assert_eq!(first.temperature, second.temperature);
    assert_eq!(first.conditional_payoffs, second.conditional_payoffs);
    assert!(!second.development_eligible);
}

#[test]
fn rejects_future_features_sealed_training_and_missing_payoffs() {
    let p = plan();
    let mut data = dataset(
        "TEST_ONLY",
        &[p.train.start, p.calibration.start, p.validation.start],
    );
    data.rows[0].available_at = data.rows[0].decision_at + Duration::seconds(1);
    assert!(fit(&data, &p).is_err());
    let mut data = dataset(
        "TEST_ONLY",
        &[p.train.start, p.calibration.start, p.validation.start],
    );
    data.rows[0].execution = None;
    assert!(fit(&data, &p).is_err());
    let data = dataset(
        "TEST_ONLY",
        &[
            p.train.start,
            p.calibration.start,
            p.validation.start,
            p.sealed_holdout_start_at,
        ],
    );
    assert!(fit(&data, &p).unwrap_err().0.contains("separate dataset"));
}

#[test]
fn pending_and_overlapping_horizons_are_purged_and_never_wins() {
    let p = plan();
    let mut data = dataset(
        "TEST_ONLY",
        &[p.train.start, p.calibration.start, p.validation.start],
    );
    data.rows[0].outcome = None;
    data.rows[0].execution = None;
    data.rows[1].label_end_at = p.calibration.start;
    data.rows[1].outcome_available_at = p.calibration.start;
    let c = fit(&data, &p).unwrap();
    assert_eq!(c.pending_rows, 1);
    assert_eq!(c.purged_rows, 1);
    assert_eq!(c.training_rows, 78);
}

#[test]
fn sealed_holdout_consumption_is_durable_and_revisions_cannot_reset_it() {
    let (c, data) = fitted();
    let directory = std::env::temp_dir().join(format!(
        "trady-inputs-worker-journal-{}",
        uuid::Uuid::new_v4()
    ));
    let journal = EvaluationJournal::open(&directory).unwrap();
    let time = at("2021-01-01T00:00:00Z");
    let report = journal.evaluate(&c, &data, time).unwrap();
    let again = EvaluationJournal::open(&directory)
        .unwrap()
        .evaluate(&c, &data, time + Duration::days(1))
        .unwrap();
    assert_eq!(digest(&report).unwrap(), digest(&again).unwrap());
    let mut changed = c.clone();
    changed.temperature += 0.01;
    assert!(
        journal
            .evaluate(&changed, &data, time)
            .unwrap_err()
            .0
            .contains("consumed")
    );
    let mut changed = data.clone();
    changed.rows[0].features[0] = Decimal::ZERO;
    assert!(journal.evaluate(&c, &changed, time).is_err());
    let file = directory.join("candidate.json");
    write_immutable(&file, &c).unwrap();
    let round_trip: ModelCandidate = read_document(&file).unwrap();
    assert_eq!(c.hash().unwrap(), round_trip.hash().unwrap());
    assert!(write_immutable(&file, &data).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

fn inference_fixture(
    c: &ModelCandidate,
    e: &EvaluationReport,
) -> (
    ApprovedModel,
    FormulaAuthority,
    LimitsInput,
    ForecastObservation,
    Money,
    DateTime<Utc>,
) {
    let now = at("2026-10-30T15:00:00Z");
    let until = now + Duration::seconds(10);
    let model = ApprovedModel {
        candidate: c.clone(),
        evaluation: e.clone(),
        approval: ModelApproval {
            schema_version: 1,
            candidate_sha256: c.hash().unwrap(),
            evaluation_sha256: digest(e).unwrap(),
            reviewer_id: "TEST_ONLY_REVIEWER".into(),
            approved_at: now - Duration::days(1),
            valid_until: now + Duration::days(1),
            dataset_verification_sha256: "a".repeat(64),
            prospective_shadow_sha256: "b".repeat(64),
            owner_approval_sha256: "c".repeat(64),
        },
    };
    let parameters = FormulaParameters {
        version: 1,
        family: FormulaFamily::Trend,
        entry_allowance_fraction: Decimal::ZERO,
        stop_atr_multiple: d("2"),
        target_net_fraction: d("0.03"),
        max_holding_sessions: 1,
        atr_lookback: 14,
        bar_seconds: 60,
    };
    let authority = FormulaAuthority {
        envelope: FormulaEnvelope {
            max_entry_allowance_fraction: d("0.005"),
            min_stop_atr_multiple: Decimal::ONE,
            max_stop_atr_multiple: d("4"),
            min_target_net_fraction: d("0.01"),
            max_target_net_fraction: d("0.03"),
        },
        max_account_risk_fraction: d("0.01"),
        active: ApprovedFormula {
            revision_id: "TEST_ONLY_FORMULA".into(),
            formula_hash: digest(&parameters).unwrap(),
            policy_hash: digest(&Policy::default()).unwrap(),
            parameters,
            approved_at: now - Duration::days(1),
            effective_from: now - Duration::hours(1),
            valid_until: now + Duration::days(1),
            proof_report_id: "TEST_ONLY_PROOF".into(),
            parity_report_id: "TEST_ONLY_PARITY".into(),
            evaluation_report_id: "TEST_ONLY_EVALUATION".into(),
            shadow_report_id: "TEST_ONLY_SHADOW".into(),
            owner_approval_id: "TEST_ONLY_OWNER".into(),
        },
        prediction: Some(PredictionAuthority {
            fusion: FusionConfig {
                version: 1,
                revision_id: "TEST_ONLY_FUSION".into(),
                approved_models: [
                    (PredictionSource::LlmResearch, "TEST_ONLY_RESEARCH".into()),
                    (PredictionSource::FinBert, "TEST_ONLY_FINBERT".into()),
                    (PredictionSource::Quantitative, "TEST_ONLY_QUANT".into()),
                    (PredictionSource::Perpetual, c.plan.model_id.clone()),
                ]
                .into(),
                outcome_calibration_report_id: model.report_id().unwrap(),
                minimum_calibration_samples: 4,
                maximum_age_seconds: 300,
                maximum_uncertainty: d("0.1"),
            },
            utility: UtilityConfig {
                version: 1,
                lambda_risk: Decimal::ZERO,
                lambda_uncertainty: Decimal::ZERO,
                lambda_complexity: Decimal::ZERO,
                complexity: 0,
                cash_hurdle: Decimal::ZERO,
                approved_estimation_report_id: model.estimation_id().unwrap(),
                cost_model_hash: c.cost_model_hash.clone(),
                comparison_deadline_at: now + Duration::hours(6),
            },
        }),
    };
    let instrument = InstrumentId::new("TEST_ONLY_STOCK").unwrap();
    let zero = EntryCosts {
        fixed: money(Decimal::ZERO),
        rate: Decimal::ZERO,
        uncertainty: money(Decimal::ZERO),
        fx: None,
    };
    let input = LimitsInput {
        account: AccountSnapshot {
            snapshot_id: "TEST_ONLY_ACCOUNT_SNAPSHOT".into(),
            account_id: AccountId::new("paper_TEST_ONLY").unwrap(),
            mode: Mode::Paper,
            spendable_cash: money(d("1000")),
            positions: vec![],
            orders: vec![],
            observed_at: now,
        },
        market_snapshot_id: "TEST_ONLY_MARKET".into(),
        quote: Quote {
            instrument_id: instrument.clone(),
            currency: Currency::new("EUR").unwrap(),
            bid: d("99.99"),
            ask: d("100"),
            observed_at: now,
            received_at: now,
            tradable: true,
        },
        entry_range: EntryRange {
            minimum: d("99"),
            maximum: d("101"),
            currency: Currency::new("EUR").unwrap(),
        },
        entry_costs: zero.clone(),
        exit_costs: trady_policy::economics::ExitCosts {
            fixed: zero.fixed.clone(),
            rate: zero.rate,
            uncertainty: zero.uncertainty.clone(),
            fx: None,
        },
        rules: InstrumentRules {
            price_tick: d("0.01"),
            quantity_step: d("0.01"),
            minimum_quantity: d("0.01"),
            currency_unit: d("0.01"),
        },
        evidence: FormulaEvidence {
            snapshot_id: "TEST_ONLY_BARS".into(),
            family: FormulaFamily::Trend,
            signal_eligible: true,
            atr: d("0.5"),
            atr_lookback: 14,
            bar_seconds: 60,
            completed_bars: 30,
            last_completed_at: now - Duration::seconds(45),
            available_at: now - Duration::seconds(30),
            valid_until: until,
            minimum_stop_distance: d("0.01"),
            noise_distance: d("0.1"),
            base_confidence_score: 100,
            confidence_score: 100,
            sentiment_entry_allowed: true,
            sentiment_target_ceiling: d("0.03"),
            exceptional_verified: false,
        },
        holding_deadline_at: now + Duration::hours(6),
        valid_until: until,
    };
    let signal = |version: &str| SignalObservation {
        model_version: version.into(),
        observed_at: now - Duration::seconds(30),
        available_at: now - Duration::seconds(20),
        generated_at: now - Duration::seconds(10),
        valid_until: until,
        evidence_lineages: vec!["TEST_ONLY_ORIGIN".into()],
    };
    let observation = ForecastObservation {
        instrument_id: instrument,
        market_snapshot_id: input.market_snapshot_id.clone(),
        feature_schema_version: c.feature_schema_version.clone(),
        feature_names: c.feature_names.clone(),
        features: vec![Decimal::ONE, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO],
        regime: "TEST_ONLY_REGIME".into(),
        observed_at: now - Duration::seconds(30),
        available_at: now - Duration::seconds(20),
        generated_at: now - Duration::seconds(10),
        valid_until: until,
        evidence_lineages: vec!["TEST_ONLY_ORIGIN".into()],
        signals: SupplementalSignals {
            research: signal("TEST_ONLY_RESEARCH"),
            subjective_confidence: Some(95),
            material_conflict: false,
            sentiment: signal("TEST_ONLY_FINBERT"),
            positive: d("0.8"),
            negative: d("0.1"),
            neutral: d("0.1"),
            quantitative: signal("TEST_ONLY_QUANT"),
            indicators: QuantitativeIndicators {
                momentum_return: Some(d("0.01")),
                atr: Some(d("0.5")),
                realized_volatility: Some(d("0.02")),
                relative_volume: Some(d("2")),
                vwap_distance_fraction: Some(d("0.01")),
                relative_strength: Some(d("0.02")),
                spread_fraction: Some(d("0.0001")),
            },
        },
    };
    (model, authority, input, observation, money(d("1000")), now)
}

#[test]
fn test_only_offline_inference_binds_exact_calculation_and_works_with_fusion() {
    let (c, data) = fitted();
    let e = evaluate(&c, &data, at("2021-01-01T00:00:00Z")).unwrap();
    let (model, authority, input, o, equity, now) = inference_fixture(&c, &e);
    assert!(model.validate(now).is_err());
    let calculation =
        calculate_authoritative_limits(&authority, &Policy::default(), &input, now).unwrap();
    let output = infer_offline(
        &model,
        &authority,
        &Policy::default(),
        &input,
        &calculation,
        &o,
        &equity,
        now,
    )
    .unwrap();
    assert!(output.test_only && !output.deployment_eligible);
    assert_eq!(
        output.prediction.context.scenario_hash,
        calculation.calculation_id.strip_prefix("limits_").unwrap()
    );
    assert_eq!(
        output.prediction.context.horizon.deadline_at,
        input.holding_deadline_at
    );
    let fusion = PredictionFusionEngine::new(authority.prediction.as_ref().unwrap().fusion.clone())
        .unwrap()
        .fuse(
            &output.prediction.context,
            &output.prediction.predictions,
            now,
        )
        .unwrap();
    assert_eq!(fusion.status(), FusionStatus::Ready);
    let utility = expected_utility(
        &fusion,
        &output.prediction.outcomes,
        &authority.prediction.as_ref().unwrap().utility,
        now,
    )
    .unwrap();
    assert!(utility.utility() > Decimal::ZERO);
    assert!(
        infer_scenario(
            &model,
            &authority,
            &Policy::default(),
            &input,
            &calculation,
            &o,
            &equity,
            now
        )
        .is_err()
    );
    let mut tampered = calculation.clone();
    tampered.limits.quantity += Decimal::ONE;
    assert!(
        infer_offline(
            &model,
            &authority,
            &Policy::default(),
            &input,
            &tampered,
            &o,
            &equity,
            now
        )
        .is_err()
    );
    let mut o = o.clone();
    o.features[0] = d("100");
    assert!(
        infer_offline(
            &model,
            &authority,
            &Policy::default(),
            &input,
            &calculation,
            &o,
            &equity,
            now
        )
        .is_err()
    );
}

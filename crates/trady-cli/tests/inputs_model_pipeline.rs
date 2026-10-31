//! TEST ONLY synthetic observations. Exercises the actual CLI without paid APIs or trading.
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::{path::Path, process::Command};
use trady_adapters::inputs_service::*;
use trady_application::{model_lab::*, optimization::*};
use trady_domain::{AccountId, Currency, Money, prediction::*};
use trady_policy::{Policy, limits::*, prediction::*};

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().to_utc()
}
fn d(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap()
}
fn money(s: &str) -> Money {
    Money {
        amount: d(s),
        currency: Currency::new("EUR").unwrap(),
    }
}
fn cli(args: &[&str]) -> Value {
    let result = Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn text(p: &Path) -> &str {
    p.to_str().unwrap()
}
fn plan() -> TrainingPlan {
    TrainingPlan {
        model_id: "TEST_ONLY_MODEL".into(),
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
fn dataset(id: &str, starts: &[DateTime<Utc>]) -> ObservationDataset {
    let mut rows = vec![];
    for start in starts {
        for i in 0..80 {
            let class = i % 4;
            let decision = *start + Duration::days(i) + Duration::hours(15);
            let target = d("0.01") + Decimal::from(i % 3) * d("0.01");
            let stop = d("0.005") + Decimal::from(i % 4) * d("0.005");
            let net = match class {
                0 => target,
                1 => -stop,
                2 => d("0.002"),
                _ => Decimal::ZERO,
            };
            let record = PayoffRecord {
                acquisition_cost: money("500"),
                opening_equity: money("1000"),
                gross_pnl: Money {
                    amount: net * d("500") + d("0.15"),
                    currency: Currency::new("EUR").unwrap(),
                },
                fees: money("0.1"),
                fx_cost: money("0.02"),
                slippage: money("0.03"),
                cash_pnl: money("0.01"),
            };
            rows.push(PointInTimeRow {
                id: format!("TEST_ONLY_{}_{}", start.timestamp(), i),
                instrument_id: "TEST_ONLY_STOCK".into(),
                market_snapshot_id: format!("TEST_ONLY_MARKET_{}_{}", start.timestamp(), i),
                scenario_hash: digest(&(decision, i, "TEST ONLY")).unwrap(),
                regime: "TEST_ONLY_REGIME".into(),
                feature_window_start_at: decision - Duration::minutes(10),
                observed_at: decision - Duration::minutes(1),
                available_at: decision - Duration::seconds(10),
                decision_at: decision,
                label_end_at: decision + Duration::hours(6),
                outcome_available_at: decision + Duration::hours(6) + Duration::minutes(1),
                source_record_sha256: digest(&(decision, "TEST ONLY source")).unwrap(),
                execution_record_sha256: digest(&record).unwrap(),
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
                execution: Some(record),
            });
        }
    }
    ObservationDataset {
        schema_version: 1,
        dataset_id: id.into(),
        test_only: true,
        feature_schema_version: "TEST_ONLY_SCHEMA".into(),
        feature_names: (0..4).map(|i| format!("TEST_ONLY_FEATURE_{i}")).collect(),
        label_version: "TEST_ONLY_LABEL".into(),
        cost_model_hash: digest(&"TEST ONLY costs").unwrap(),
        rows,
    }
}

#[test]
fn cli_ingestion_training_evaluation_serving_and_offline_inference_preserve_test_only_gate() {
    let root = std::env::temp_dir().join(format!(
        "trady-inputs-worker-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let p = plan();
    let data = dataset(
        "TEST_ONLY_DEVELOPMENT",
        &[p.train.start, p.calibration.start, p.validation.start],
    );
    let held = dataset("TEST_ONLY_HOLDOUT", &[at("2020-10-04T00:00:00Z")]);
    let data_file = root.join("data.json");
    let plan_file = root.join("plan.json");
    let held_file = root.join("held.json");
    write_immutable(&data_file, &data).unwrap();
    write_immutable(&plan_file, &p).unwrap();
    write_immutable(&held_file, &held).unwrap();
    let candidate_file = root.join("candidate.json");
    let report_file = root.join("evaluation.json");
    let journal = root.join("journal");
    let trained = cli(&[
        "model-lab",
        "fit",
        "--dataset",
        text(&data_file),
        "--plan",
        text(&plan_file),
        "--out",
        text(&candidate_file),
    ]);
    assert_eq!(trained["status"], "UNAPPROVED_CANDIDATE");
    assert_eq!(trained["test_only"], true);
    let evaluated = cli(&[
        "model-lab",
        "evaluate",
        "--candidate",
        text(&candidate_file),
        "--holdout",
        text(&held_file),
        "--journal",
        text(&journal),
        "--out",
        text(&report_file),
    ]);
    assert_eq!(evaluated["eligible"], true);
    assert_eq!(evaluated["samples"], 80);
    let c: ModelCandidate = read_document(&candidate_file).unwrap();
    let e: EvaluationReport = read_document(&report_file).unwrap();
    let now = Utc::now();
    let approval = ModelApproval {
        schema_version: 1,
        candidate_sha256: c.hash().unwrap(),
        evaluation_sha256: digest(&e).unwrap(),
        reviewer_id: "TEST_ONLY_AUDIT".into(),
        approved_at: now,
        valid_until: now + Duration::days(1),
        dataset_verification_sha256: "a".repeat(64),
        prospective_shadow_sha256: "b".repeat(64),
        owner_approval_sha256: "c".repeat(64),
    };
    let model = ApprovedModel {
        candidate: c.clone(),
        evaluation: e,
        approval: approval.clone(),
    };
    let bundle_file = root.join("TEST_ONLY_offline_bundle.json");
    write_immutable(&bundle_file, &model).unwrap();
    let approval_file = root.join("TEST_ONLY_approval.json");
    write_immutable(&approval_file, &approval).unwrap();
    let denied = Command::new(env!("CARGO_BIN_EXE_trady"))
        .args([
            "model-lab",
            "bundle",
            "--candidate",
            text(&candidate_file),
            "--evaluation",
            text(&report_file),
            "--approval",
            text(&approval_file),
            "--out",
            text(&root.join("must_not_exist.json")),
        ])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(!root.join("must_not_exist.json").exists());

    let close = now.date_naive().and_hms_opt(23, 59, 59).unwrap().and_utc();
    let open = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
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
            min_stop_atr_multiple: d("1"),
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
                comparison_deadline_at: close,
            },
        }),
    };
    let account = AccountId::new("paper_TEST_ONLY").unwrap();
    let currency = Currency::new("EUR").unwrap();
    let cfg = InputsServiceConfig {
        config_version: 1,
        store_directory: root.join("store"),
        listen: "127.0.0.1:0".parse().unwrap(),
        workers: 1,
        request_timeout_ms: 500,
        maximum_body_bytes: 262_144,
        maximum_response_bytes: 1024 * 1024,
        sources: [(
            "TEST_ONLY_PROVIDER".into(),
            SourcePolicy {
                provider: "TEST ONLY provider".into(),
                allowed_hosts: vec!["test-only.invalid".into()],
                independence_group: "TEST_ONLY_PROVIDER_GROUP".into(),
                may_be_primary: false,
                rights: trady_domain::research::SourceRights {
                    store_content: true,
                    llm_processing: true,
                    retention_hours: 24,
                    terms_reference: "TEST ONLY synthetic license".into(),
                },
                rights_valid_until: now + Duration::days(2),
                kinds: vec![
                    ImportKind::Evidence,
                    ImportKind::Calendar,
                    ImportKind::Session,
                    ImportKind::Quote,
                    ImportKind::Costs,
                    ImportKind::Research,
                    ImportKind::Forecast,
                ],
            },
        )]
        .into(),
        reviewers: [(
            "TEST_ONLY_AUDIT".into(),
            ReviewerPolicy {
                independence_group: "TEST_ONLY_AUDIT_GROUP".into(),
                may_verify_primary: false,
            },
        )]
        .into(),
        accounts: vec![ServiceAccount {
            owner_id: "TEST_ONLY_OWNER".into(),
            account_id: account.clone(),
            currency: currency.clone(),
            calendar_id: "TEST_ONLY_CALENDAR".into(),
            policy: Policy::default(),
            formula_authority: authority.clone(),
            approved_skills: Default::default(),
        }],
        model: None,
    };
    let config_file = root.join("service.json");
    write_immutable(&config_file, &cfg).unwrap();
    let service = InputsService::open(cfg.clone()).unwrap();
    let input:LimitsInput=serde_json::from_value(json!({
        "account":{"snapshot_id":"TEST_ONLY_ACCOUNT","account_id":account,"mode":"paper","spendable_cash":money("1000"),"positions":[],"orders":[],"observed_at":now},
        "market_snapshot_id":"TEST_ONLY_MARKET","quote":{"instrument_id":"TEST_ONLY_STOCK","currency":"EUR","bid":"99.99","ask":"100","observed_at":now,"received_at":now,"tradable":true},
        "entry_range":{"minimum":"99","maximum":"101","currency":"EUR"},
        "entry_costs":{"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null},"exit_costs":{"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null},
        "rules":{"price_tick":"0.01","quantity_step":"0.01","minimum_quantity":"0.01","currency_unit":"0.01"},
        "evidence":{"snapshot_id":"TEST_ONLY_BARS","family":"TREND","signal_eligible":true,"atr":"0.5","atr_lookback":14,"bar_seconds":60,"completed_bars":30,
            "last_completed_at":now-Duration::seconds(45),"available_at":now-Duration::seconds(30),"valid_until":now+Duration::minutes(5),"minimum_stop_distance":"0.01","noise_distance":"0.1",
            "base_confidence_score":100,"confidence_score":100,"sentiment_entry_allowed":true,"sentiment_target_ceiling":"0.03","exceptional_verified":false},
        "holding_deadline_at":close,"valid_until":now+Duration::minutes(5)})).unwrap();
    let signal = |version: &str| SignalObservation {
        model_version: version.into(),
        observed_at: now - Duration::seconds(30),
        available_at: now - Duration::seconds(20),
        generated_at: now - Duration::seconds(10),
        valid_until: now + Duration::minutes(5),
        evidence_lineages: vec!["TEST_ONLY_ORIGIN".into()],
    };
    let observation = ForecastObservation {
        instrument_id: input.quote.instrument_id.clone(),
        market_snapshot_id: input.market_snapshot_id.clone(),
        feature_schema_version: c.feature_schema_version.clone(),
        feature_names: c.feature_names.clone(),
        features: vec![Decimal::ONE, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO],
        regime: "TEST_ONLY_REGIME".into(),
        observed_at: now - Duration::seconds(30),
        available_at: now - Duration::seconds(20),
        generated_at: now - Duration::seconds(10),
        valid_until: now + Duration::minutes(5),
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
                relative_strength: Some(d("0.01")),
                spread_fraction: Some(d("0.0001")),
            },
        },
    };
    let publish = |id: &str, payload: ImportPayload, observed, available, deps: Vec<String>| {
        let raw = serde_json::to_string(&payload).unwrap();
        let import = SourceImport {
            schema_version: 1,
            id: id.into(),
            source_id: "TEST_ONLY_PROVIDER".into(),
            provider: "TEST ONLY provider".into(),
            canonical_url: format!("https://test-only.invalid/{id}"),
            origin_lineage_id: "TEST_ONLY_ORIGIN".into(),
            observed_at: observed,
            available_at: available,
            imported_at: now,
            valid_until: now + Duration::hours(1),
            retain_until: now + Duration::hours(2),
            raw_document_sha256: trady_application::screening::hex_digest(raw.as_bytes()),
            raw_document: raw,
            dependencies: deps,
            payload,
        };
        let review = SourceReview {
            schema_version: 1,
            import_id: id.into(),
            import_sha256: digest(&import).unwrap(),
            raw_document_sha256: import.raw_document_sha256.clone(),
            reviewer_id: "TEST_ONLY_AUDIT".into(),
            reviewed_at: now,
            valid_until: import.valid_until,
            source_identity_checked: true,
            rights_checked: true,
            original_availability_checked: true,
            mapping_checked: true,
            original_lineage_checked: true,
            model_self_attestation: false,
            review_note: "TEST ONLY independently reviewed synthetic fixture".into(),
        };
        let file = root.join(format!("{id}.json"));
        let reviewed = root.join(format!("{id}.review.json"));
        write_immutable(&file, &import).unwrap();
        write_immutable(&reviewed, &review).unwrap();
        assert_eq!(
            cli(&[
                "inputs-service",
                "import",
                "--config",
                text(&config_file),
                "--file",
                text(&file)
            ])["status"],
            "STAGED_UNVERIFIED"
        );
        cli(&[
            "inputs-service",
            "review",
            "--config",
            text(&config_file),
            "--file",
            text(&reviewed),
        ]);
    };
    publish(
        "TEST_ONLY_QUOTE",
        ImportPayload::Quote(input.quote.clone()),
        now,
        now,
        vec![],
    );
    publish(
        "TEST_ONLY_COSTS",
        ImportPayload::Costs(trady_adapters::autonomous::CostsPacket {
            instrument_id: input.quote.instrument_id.clone(),
            observed_at: now,
            valid_until: now + Duration::minutes(5),
            costs: input.entry_costs.clone(),
        }),
        now,
        now,
        vec![],
    );
    publish(
        "TEST_ONLY_CALENDAR_IMPORT",
        ImportPayload::Calendar(CalendarDocument {
            calendar_id: "TEST_ONLY_CALENDAR".into(),
            listing_mic: "XNAS".into(),
            observed_at: now,
            available_at: now,
            valid_until: now + Duration::minutes(30),
            coverage_start: now.date_naive(),
            coverage_end: now.date_naive(),
            sessions: vec![CalendarSession {
                session_id: "TEST_ONLY_SESSION".into(),
                opens_at: open,
                closes_at: close,
            }],
            closures: vec![],
        }),
        now,
        now,
        vec![],
    );
    publish(
        "TEST_ONLY_SESSION_IMPORT",
        ImportPayload::Session(trady_adapters::autonomous::SessionPacket {
            account_id: account.clone(),
            session: Some(trady_application::autonomous::MarketSession {
                session_id: "TEST_ONLY_SESSION".into(),
                opens_at: open,
                closes_at: close,
                observed_at: now,
                opening_equity: money("1000"),
                deposits: money("0"),
                withdrawals: money("0"),
            }),
        }),
        now,
        now,
        vec![],
    );
    let artifact = json!({"test_only":true,"content":"TEST ONLY original observations"});
    let original = now - Duration::seconds(40);
    publish(
        "TEST_ONLY_EVIDENCE",
        ImportPayload::Evidence(EvidenceArtifact {
            record: trady_application::autonomous::EvidenceRecord {
                id: "TEST_ONLY_EVIDENCE".into(),
                account_id: account.clone(),
                instrument_id: input.quote.instrument_id.clone(),
                policy_version: 1,
                kind: "source".into(),
                revision_hash: digest(&artifact).unwrap(),
                observed_at: original,
                valid_until: now + Duration::minutes(5),
                dependencies: vec![],
                source_lineage: Some("TEST_ONLY_ORIGIN".into()),
                primary_verified: false,
                material_conflict: false,
            },
            artifact,
        }),
        original,
        original,
        vec![],
    );
    publish(
        "TEST_ONLY_FORECAST",
        ImportPayload::Forecast(Box::new(ForecastImport {
            account_id: account.clone(),
            observation: observation.clone(),
        })),
        observation.observed_at,
        observation.available_at,
        vec!["TEST_ONLY_EVIDENCE".into()],
    );
    assert_eq!(
        service
            .respond("GET", "/quote/TEST_ONLY_STOCK", &[], now)
            .status,
        200
    );
    assert_eq!(
        service
            .respond("GET", "/costs/TEST_ONLY_STOCK", &[], now)
            .status,
        200
    );
    assert_eq!(
        service
            .respond("GET", "/session/paper_TEST_ONLY", &[], now)
            .status,
        200
    );
    let observed = service
        .forecast_observation(
            &account,
            &input.quote.instrument_id,
            &input.market_snapshot_id,
            now,
        )
        .unwrap();
    assert_eq!(observed, observation);
    let calculation =
        calculate_authoritative_limits(&authority, &Policy::default(), &input, now).unwrap();
    let request = root.join("offline-request.json");
    let output = root.join("offline-prediction.json");
    write_immutable(&request,&json!({"authority":authority,"policy":Policy::default(),"input":input,"calculation":calculation,"observation":observed,"opening_equity":money("1000"),"as_of":now})).unwrap();
    let result = cli(&[
        "model-lab",
        "offline-infer",
        "--bundle",
        text(&bundle_file),
        "--request",
        text(&request),
        "--out",
        text(&output),
    ]);
    assert_eq!(result["test_only"], true);
    assert_eq!(result["deployment_eligible"], false);
    let forecast: Value = read_document(&output).unwrap();
    assert_eq!(
        forecast["prediction"]["predictions"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        forecast["prediction"]["context"]["scenario_hash"],
        calculation.calculation_id.strip_prefix("limits_").unwrap()
    );
    let wire = serde_json::to_vec(&trady_adapters::autonomous::PredictionRequest {
        schema_version: 1,
        account_id: account,
        owner_id: "TEST_ONLY_OWNER".into(),
        input,
        calculation,
    })
    .unwrap();
    assert_eq!(
        service
            .respond("POST", "/predictions/paper_TEST_ONLY", &wire, now)
            .status,
        503
    );
    let mut configured = cfg;
    configured.model = Some(ModelDeployment {
        path: bundle_file,
        bundle_sha256: digest(&model).unwrap(),
        reviewer_id: "TEST_ONLY_AUDIT".into(),
        evidence_import_ids: vec![
            "TEST_ONLY_ONE".into(),
            "TEST_ONLY_TWO".into(),
            "TEST_ONLY_THREE".into(),
        ],
    });
    assert!(InputsService::open(configured).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

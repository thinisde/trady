//! Synthetic entry contexts exercise actual persisted reference joins, never financial approval.
use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use std::env;
use trady_application::{
    autonomous::{Dossier, EvidenceRecord, TradingResearchStore},
    optimization::*,
    perpetual::PerpetualRepository,
    screening::{ScreeningRepository, ScreeningSnapshot},
};
use trady_domain::{perpetual::*, *};
use trady_policy::{Policy, limits::*};
use trady_storage::{migrate, trading_research::PgTradingResearch};

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn entry_context_resolves_persisted_economics_screening_and_feature_scope() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&env::var("TRADY_MIGRATOR_DSN").expect("isolated migrator")).expect("migrations");
    let now = Utc::now();
    let money = |amount: &str| serde_json::json!({"amount":amount,"currency":"EUR"});
    let account = AccountId::new("paper_research_join").unwrap();
    let input: LimitsInput = serde_json::from_value(serde_json::json!({
        "account":{"snapshot_id":"account_join","account_id":account,"mode":"paper",
            "spendable_cash":money("1000"),"positions":[],"orders":[],"observed_at":now},
        "market_snapshot_id":"market_join",
        "quote":{"instrument_id":"stock_join","currency":"EUR","bid":"99.99","ask":"100",
            "observed_at":now,"received_at":now,"tradable":true},
        "entry_range":{"minimum":"99","maximum":"101","currency":"EUR"},
        "entry_costs":{"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null},
        "exit_costs":{"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null},
        "rules":{"price_tick":"0.01","quantity_step":"0.01","minimum_quantity":"0.01","currency_unit":"0.01"},
        "evidence":{"snapshot_id":"bars_join","family":"TREND","signal_eligible":true,"atr":"0.5",
            "atr_lookback":14,"bar_seconds":60,"completed_bars":30,"last_completed_at":now-Duration::seconds(45),
            "available_at":now-Duration::seconds(30),"valid_until":now+Duration::minutes(5),
            "minimum_stop_distance":"0.01","noise_distance":"0.1","base_confidence_score":100,"confidence_score":100,
            "sentiment_entry_allowed":true,"sentiment_target_ceiling":"0.03","exceptional_verified":false},
        "holding_deadline_at":now+Duration::hours(1),"valid_until":now+Duration::minutes(5)
    })).unwrap();
    let parameters = FormulaParameters {
        version: 1,
        family: FormulaFamily::Trend,
        entry_allowance_fraction: Decimal::new(1, 3),
        stop_atr_multiple: Decimal::from(2),
        target_net_fraction: Decimal::new(3, 2),
        max_holding_sessions: 3,
        atr_lookback: 14,
        bar_seconds: 60,
    };
    let authority = FormulaAuthority {
        envelope: FormulaEnvelope {
            max_entry_allowance_fraction: Decimal::new(5, 3),
            min_stop_atr_multiple: Decimal::ONE,
            max_stop_atr_multiple: Decimal::from(4),
            min_target_net_fraction: Decimal::new(1, 2),
            max_target_net_fraction: Decimal::new(3, 2),
        },
        max_account_risk_fraction: Decimal::new(1, 2),
        prediction: None,
        active: ApprovedFormula {
            revision_id: "join_fixture_formula".into(),
            formula_hash: digest(&parameters).unwrap(),
            policy_hash: digest(&Policy::default()).unwrap(),
            parameters,
            approved_at: now - Duration::days(1),
            effective_from: now - Duration::hours(1),
            valid_until: now + Duration::days(1),
            proof_report_id: "fixture_proof".into(),
            parity_report_id: "fixture_parity".into(),
            evaluation_report_id: "fixture_evaluation".into(),
            shadow_report_id: "fixture_shadow".into(),
            owner_approval_id: "fixture_owner".into(),
        },
    };
    let calculation =
        calculate_authoritative_limits(&authority, &Policy::default(), &input, now).unwrap();
    let args = AssessArguments {
        instrument_id: input.quote.instrument_id.clone(),
        scenario_calculation_id: calculation.calculation_id.clone(),
        market_snapshot_id: input.market_snapshot_id.clone(),
        profile_review_id: "profile_join".into(),
        screening_artifact_id: "screening_join".into(),
        position_id: None,
    };
    let context = AssessmentContext {
        scenario: ScenarioSnapshot {
            binding: ScenarioBinding {
                arguments: args.clone(),
                currency: input.quote.currency.clone(),
                entry_price: calculation.limits.buy_limit,
                stop_price: calculation.limits.stop_trigger,
                target_price: calculation.limits.target_sell_limit,
                estimated_cost_fraction: Decimal::ZERO,
                horizon_seconds: 3600,
                deadline_at: input.holding_deadline_at,
            },
            calculated_at: now,
            valid_until: calculation.limits.valid_until,
            market: MarketObservation {
                instrument_id: args.instrument_id.clone(),
                snapshot_id: args.market_snapshot_id.clone(),
                observed_at: now,
                valid_until: calculation.limits.valid_until,
                executable_price: input.quote.ask,
                fx_rate: Decimal::ONE,
            },
        },
        features: FeatureSnapshot {
            snapshot_id: "features_join".into(),
            schema_version: "fixture_v1".into(),
            as_of: now,
            valid_until: calculation.limits.valid_until,
            features: vec![FeatureValue {
                name: "atr".into(),
                value: 0.5,
                lineage: vec![FeatureLineage {
                    source_id: "fixture_source".into(),
                    source_revision: "fixture_r1".into(),
                    evidence_id: args.profile_review_id.clone(),
                    observed_at: now - Duration::seconds(30),
                    available_at: now - Duration::seconds(30),
                }],
            }],
        },
    };
    let mut repo = PgTradingResearch::connect(
        &env::var("TRADY_ANALYST_DSN").unwrap(),
        "owner_join".into(),
        account.clone(),
    )
    .unwrap();
    let retain = now + Duration::hours(1);
    assert!(
        repo.store_context(&context, retain)
            .unwrap_err()
            .0
            .contains("missing calculation")
    );
    repo.store_calculation(&input, &calculation, retain)
        .unwrap();
    let mut altered = context.clone();
    altered.scenario.binding.target_price += Decimal::ONE;
    assert!(
        repo.store_context(&altered, retain)
            .unwrap_err()
            .0
            .contains("persisted economics")
    );
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../config/autonomous.example.json")).unwrap();
    let mut screening:ScreeningSnapshot = serde_json::from_value(serde_json::json!({
        "snapshot_id":args.screening_artifact_id,"schema_version":1,"as_of":now,
        "request":fixture["inputs"]["screening"],"universe":null,"observations":null,"failure":null,
        "result":{"method":"synthetic_join_fixture","purpose":"RESEARCH_ONLY","as_of":now,"universe_snapshot_id":"fixture_universe",
            "classification_snapshot_id":"fixture_classification","candidates":[{
                "instrument_id":args.instrument_id,"issuer_id":"fixture_issuer","share_class_isin":"US0378331005",
                "sector_code":"45","industry_group_code":"4520","priority_score":90,"priority_components":[20,20,20,20,10],
                "spread_bps":"1","average_daily_notional":"1000000","broker_snapshot_id":"account_join",
                "market_snapshot_id":args.market_snapshot_id,"timing_snapshot_id":"fixture_timing","calculation_id":"fixture_priority","evidence_ids":[]}],
            "peer_controls":[],"finalist_instrument_id":args.instrument_id,"rejections":[],"blockers":[]}
    })).unwrap();
    screening.request.owner_id = "owner_join".into();
    repo.store_screening_snapshot(&screening).unwrap();
    let profile = EvidenceRecord {
        id: args.profile_review_id.clone(),
        account_id: account.clone(),
        instrument_id: args.instrument_id.clone(),
        policy_version: 1,
        kind: "company_profile".into(),
        revision_hash: "a".repeat(64),
        observed_at: now - Duration::seconds(30),
        valid_until: retain,
        dependencies: vec![],
        source_lineage: Some("fixture_source".into()),
        primary_verified: true,
        material_conflict: false,
    };
    let mut dossier = Dossier {
        id: "dossier_join".into(),
        finalized: true,
        sector_skill: "fixture_sector".into(),
        subsector_skill: "fixture_group".into(),
        records: vec![profile.clone()],
        skills: vec![],
        blockers: vec![],
    };
    repo.store_dossier(&dossier, retain).unwrap();
    repo.store_context(&context, retain).unwrap();
    assert_eq!(repo.load_context(&args).unwrap(), context);
    dossier.id = "dossier_wrong_join".into();
    dossier.records[0].id = "profile_wrong_join".into();
    dossier.records[0].instrument_id = InstrumentId::new("other_stock_join").unwrap();
    repo.store_dossier(&dossier, retain).unwrap();
    altered = context.clone();
    altered.scenario.binding.arguments.profile_review_id = "profile_wrong_join".into();
    assert!(
        repo.store_context(&altered, retain)
            .unwrap_err()
            .0
            .contains("differently bound")
    );
    altered = context.clone();
    altered.features.features[0].lineage[0].evidence_id = "profile_wrong_join".into();
    assert!(
        repo.store_context(&altered, retain)
            .unwrap_err()
            .0
            .contains("differently bound")
    );
    screening.snapshot_id = "screening_wrong_join".into();
    screening.result.as_mut().unwrap().candidates[0].market_snapshot_id = "other_market".into();
    repo.store_screening_snapshot(&screening).unwrap();
    altered = context;
    altered.scenario.binding.arguments.screening_artifact_id = screening.snapshot_id;
    assert!(
        repo.store_context(&altered, retain)
            .unwrap_err()
            .0
            .contains("another instrument or market")
    );
}

//! Synthetic financial inputs check revision binding and honest missing-model rejection.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use trady_application::optimization::*;
use trady_policy::{Policy, limits::*};

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
        .unwrap()
        .to_utc()
}
fn d(value: &str) -> Decimal {
    Decimal::from_str_exact(value).unwrap()
}
fn fixture() -> (FormulaAuthority, LimitsInput) {
    let parameters = FormulaParameters {
        version: 1,
        family: FormulaFamily::Trend,
        entry_allowance_fraction: d("0.001"),
        stop_atr_multiple: d("2"),
        target_net_fraction: d("0.03"),
        max_holding_sessions: 3,
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
        prediction: None,
        active: ApprovedFormula {
            revision_id: "fixture_revision".into(),
            formula_hash: digest(&parameters).unwrap(),
            policy_hash: digest(&Policy::default()).unwrap(),
            parameters,
            approved_at: now() - Duration::days(1),
            effective_from: now() - Duration::hours(1),
            valid_until: now() + Duration::days(7),
            proof_report_id: "fixture_proof".into(),
            parity_report_id: "fixture_parity".into(),
            evaluation_report_id: "fixture_evaluation".into(),
            shadow_report_id: "fixture_shadow".into(),
            owner_approval_id: "fixture_owner_approval".into(),
        },
    };
    let money = |amount: &str| serde_json::json!({"amount":amount, "currency":"EUR"});
    let input: LimitsInput = serde_json::from_value(serde_json::json!({
        "account":{"snapshot_id":"account_1","account_id":"paper_formula_fixture","mode":"paper",
            "spendable_cash":money("1000"),"positions":[],"orders":[],"observed_at":now()},
        "market_snapshot_id":"market_1",
        "quote":{"instrument_id":"test_stock","currency":"EUR","bid":"99.99","ask":"100",
            "observed_at":now(),"received_at":now(),"tradable":true},
        "entry_range":{"minimum":"99","maximum":"101","currency":"EUR"},
        "entry_costs":{"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null},
        "exit_costs":{"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null},
        "rules":{"price_tick":"0.01","quantity_step":"0.01","minimum_quantity":"0.01","currency_unit":"0.01"},
        "evidence":{"snapshot_id":"bars_1","family":"TREND","signal_eligible":true,"atr":"0.5",
            "atr_lookback":14,"bar_seconds":60,"completed_bars":30,"last_completed_at":now()-Duration::seconds(45),
            "available_at":now()-Duration::seconds(30),"valid_until":now()+Duration::minutes(5),
            "minimum_stop_distance":"0.01","noise_distance":"0.1","base_confidence_score":100,"confidence_score":100,
            "sentiment_entry_allowed":true,"sentiment_target_ceiling":"0.03","exceptional_verified":false},
        "holding_deadline_at":now()+Duration::days(3),"valid_until":now()+Duration::minutes(5)
    })).unwrap();
    (authority, input)
}

#[test]
fn revision_and_every_financial_input_bind_the_reproducible_calculation() {
    let (authority, input) = fixture();
    let calculate = |authority: &FormulaAuthority, input: &LimitsInput| {
        calculate_authoritative_limits(authority, &Policy::default(), input, now()).unwrap()
    };
    let first = calculate(&authority, &input);
    assert_eq!(first, calculate(&authority, &input));
    let mut changed = input.clone();
    changed.holding_deadline_at += Duration::hours(1);
    assert_ne!(
        first.calculation_id,
        calculate(&authority, &changed).calculation_id
    );
    let mut changed = authority.clone();
    changed.active.revision_id = "another_approved_revision".into();
    assert_ne!(
        first.calculation_id,
        calculate(&changed, &input).calculation_id
    );
    changed.active.parameters.target_net_fraction = d("0.02");
    assert!(calculate_authoritative_limits(&changed, &Policy::default(), &input, now()).is_err());
}

#[test]
fn missing_predictions_reject_after_unchanged_quote_refresh_without_extending_life() {
    let (authority, input) = fixture();
    let calculation =
        calculate_authoritative_limits(&authority, &Policy::default(), &input, now()).unwrap();
    let proposal = FormulaProposal {
        input,
        calculation,
        predictions: None,
    };
    let mut quote = proposal.input.quote.clone();
    quote.observed_at += Duration::seconds(1);
    quote.received_at += Duration::seconds(1);
    let error = proposal
        .validate(
            &authority,
            &Policy::default(),
            &quote,
            now() + Duration::seconds(1),
        )
        .unwrap_err();
    assert!(error.0.contains("prediction/fusion authority"));
    quote.ask += d("0.01");
    assert!(
        proposal
            .validate(
                &authority,
                &Policy::default(),
                &quote,
                now() + Duration::seconds(1)
            )
            .unwrap_err()
            .0
            .contains("market changed formula limits")
    );
    assert!(
        proposal
            .validate(
                &authority,
                &Policy::default(),
                &quote,
                now() + Duration::seconds(11)
            )
            .unwrap_err()
            .0
            .contains("expired")
    );
}

#[test]
fn altered_quantity_or_expired_authority_never_passes_replay() {
    let (mut authority, input) = fixture();
    let mut calculation =
        calculate_authoritative_limits(&authority, &Policy::default(), &input, now()).unwrap();
    calculation.limits.quantity += d("0.01");
    let proposal = FormulaProposal {
        input,
        calculation,
        predictions: None,
    };
    assert!(
        proposal
            .validate(&authority, &Policy::default(), &proposal.input.quote, now())
            .unwrap_err()
            .0
            .contains("differs from the approved Rust result")
    );
    authority.active.valid_until = now();
    assert!(
        calculate_authoritative_limits(&authority, &Policy::default(), &proposal.input, now())
            .is_err()
    );
}

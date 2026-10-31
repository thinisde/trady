//! TEST ONLY: executor-attested holding fixtures, with no production approvals.
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use trady_application::autonomous::{
    ActivePlan, EntryCandidate, ExecutionObservation, holding_advisory_cost_fraction,
};
use trady_domain::{perpetual::*, *};
use trady_policy::{
    EntryCosts, FxObservation, Policy, div,
    economics::{ExitCosts, net_executable_return, target_price},
    holding::{HoldingPlan, HoldingState, TrailingSettings},
};
use trady_storage::trading_research::VerifiedHolding;

fn now() -> DateTime<Utc> {
    "2026-10-31T12:00:00Z".parse().unwrap()
}
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn money(value: &str) -> Money {
    Money {
        amount: d(value),
        currency: Currency::new("EUR").unwrap(),
    }
}
fn holding() -> VerifiedHolding {
    let at = now();
    let account = AccountId::new("test_only_holding_account").unwrap();
    let instrument = InstrumentId::new("test_only_holding_stock").unwrap();
    let zero = json!({"fixed": money("0"), "rate":"0", "uncertainty":money("0"), "fx":null});
    let candidate: EntryCandidate = serde_json::from_value(json!({
        "proposal_id":"test_only_position", "account_id":account, "account_snapshot_id":"test_only_entry",
        "policy_version":1, "instrument_id":instrument, "currency":"EUR",
        "created_at":at-Duration::minutes(10), "valid_until":at-Duration::minutes(5),
        "entry_range":{"minimum":"99", "maximum":"100", "currency":"EUR"},
        "stop_price":"99", "target_net_fraction":"0.01", "max_holding_sessions":3,
        "deadline_at":at+Duration::hours(1), "exceptional_verified":false,
        "confidence_inputs":{"primary_verified":true, "catalyst":"DIRECT", "independent_lineages":2,
            "spread_fraction":"0.0025", "round_trip_cost_fraction":"0.005", "catalyst_age_hours":1,
            "sources_current":true, "continuing_relevance":true},
        "entry_costs":zero, "exit_costs":zero,
        "instrument_rules":{"price_tick":"0.01", "quantity_step":"0.01", "minimum_quantity":"0.01", "currency_unit":"0.01"},
        "dossier":{"id":"test_only_old_dossier", "finalized":false, "sector_skill":"sector-test",
            "subsector_skill":"subsector-test", "records":[], "skills":[], "blockers":[]},
        "screening":{"universe_snapshot_id":"test_only_universe", "screening_id":"test_only_screening",
            "finalist":instrument, "observed_at":at, "valid_until":at+Duration::minutes(5)},
        "advisor":{"assessment_id":"test_only_old_advice", "model_revision_id":"test_only_model",
            "scenario_hash":"a".repeat(64), "account_snapshot_id":"test_only_entry", "assessed_at":at,
            "valid_until":at+Duration::seconds(30), "assessed_bid":"100", "assessed_ask":"100",
            "quote_drift_tolerance":"0.001", "usable":false, "calibration_ok":false, "drift_ok":false,
            "review_reason":"TEST ONLY: no entry authority"}, "formula":null
    })).unwrap();
    let policy = Policy::default();
    let state = HoldingState::new(
        instrument.clone(),
        d("0.01"),
        HoldingPlan {
            version: 1,
            stop_price: d("99"),
            target_net_fraction: d("0.01"),
            deadline: at + Duration::hours(1),
            trailing: TrailingSettings::default(),
        },
        &policy,
    )
    .unwrap();
    let exit_costs = EntryCosts {
        fixed: money("0"),
        rate: Decimal::ZERO,
        uncertainty: money("0"),
        fx: None,
    };
    VerifiedHolding {
        owner_id: "test_only_owner".into(),
        account_id: account.clone(),
        position_id: candidate.proposal_id.clone(),
        revision: 1,
        policy,
        plan: ActivePlan {
            candidate,
            acquired_quantity: d("2"),
            entry_session: "test_only_session".into(),
            entry_session_close: at + Duration::hours(2),
            entered_at: at - Duration::minutes(10),
            first_order: 0,
            holding_state: state,
            holding_sessions: BTreeSet::from(["test_only_session".into()]),
            exit_latch: None,
            market_exit_required: false,
            data_unusable_since: None,
        },
        observation: ExecutionObservation {
            snapshot: AccountSnapshot {
                snapshot_id: "test_only_account_observation".into(),
                account_id: account,
                mode: Mode::Paper,
                spendable_cash: money("795"),
                positions: vec![Position {
                    instrument_id: instrument.clone(),
                    quantity: d("2"),
                }],
                orders: vec![],
                observed_at: at,
            },
            position_costs: BTreeMap::from([(instrument.clone(), money("205"))]),
            available_quantities: BTreeMap::from([(instrument, d("2"))]),
            total_equity: money("1000"),
        },
        exit_costs,
    }
}
fn costs(holding: &VerifiedHolding) -> ExitCosts {
    let c = &holding.exit_costs;
    ExitCosts {
        fixed: c.fixed.clone(),
        rate: c.rate,
        uncertainty: c.uncertainty.clone(),
        fx: c.fx.clone(),
    }
}
fn rebind(holding: &VerifiedHolding, context: &mut AssessmentContext) {
    context.scenario.binding.arguments.scenario_calculation_id = holding
        .calculation_id(&context.scenario.binding, now())
        .unwrap();
}
fn context(holding: &VerifiedHolding, fraction: Decimal) -> AssessmentContext {
    let at = now();
    let basis = holding.basis(at).unwrap();
    let costs = costs(holding);
    let fx = costs.fx.as_ref().map_or(Decimal::ONE, |f| f.rate);
    let entry = div(
        div(basis.acquisition_cost.amount, basis.quantity).unwrap(),
        fx,
    )
    .unwrap();
    let mut context = AssessmentContext {
        scenario: ScenarioSnapshot {
            binding: ScenarioBinding {
                arguments: AssessArguments {
                    instrument_id: basis.instrument_id.clone(),
                    scenario_calculation_id: "pending".into(),
                    market_snapshot_id: "test_only_market".into(),
                    profile_review_id: "test_only_profile".into(),
                    screening_artifact_id: "test_only_screening".into(),
                    position_id: Some(holding.position_id.clone()),
                },
                currency: basis.instrument_currency,
                entry_price: entry,
                stop_price: holding.plan.holding_state.plan().stop_price,
                target_price: target_price(
                    &holding.basis(at).unwrap(),
                    fraction,
                    &costs,
                    d("0.01"),
                    at,
                )
                .unwrap(),
                estimated_cost_fraction: holding_advisory_cost_fraction(
                    &holding.basis(at).unwrap(),
                    entry,
                    &costs,
                    at,
                )
                .unwrap(),
                horizon_seconds: 3600,
                deadline_at: at + Duration::hours(1),
            },
            calculated_at: at,
            valid_until: at + Duration::seconds(30),
            market: MarketObservation {
                instrument_id: basis.instrument_id,
                snapshot_id: "test_only_market".into(),
                observed_at: at,
                valid_until: at + Duration::seconds(30),
                executable_price: entry,
                fx_rate: fx,
            },
        },
        features: FeatureSnapshot {
            snapshot_id: "test_only_features".into(),
            schema_version: "test_only_schema".into(),
            as_of: at,
            valid_until: at + Duration::seconds(30),
            features: vec![FeatureValue {
                name: "signal".into(),
                value: 1.0,
                lineage: vec![FeatureLineage {
                    source_id: "test_only_source".into(),
                    source_revision: "test_only_revision".into(),
                    evidence_id: "test_only_profile".into(),
                    observed_at: at,
                    available_at: at,
                }],
            }],
        },
    };
    rebind(holding, &mut context);
    context
}

#[test]
fn holding_advice_admits_owner_domain_without_reusing_entry_confidence() {
    let holding = holding();
    assert_eq!(
        trady_policy::confidence(&holding.plan.candidate.confidence_inputs).total(),
        75
    );
    assert_eq!(
        holding.plan.holding_state.plan().target_net_fraction,
        d("0.01")
    );
    holding
        .validate_context(&context(&holding, d("0.02")), now())
        .unwrap();
}

#[test]
fn tick_rounding_allows_reachable_policy_boundary_and_rejects_overshoot() {
    let mut holding = holding();
    holding.policy.max_target_fraction = d("0.01");
    let mut context = context(&holding, d("0.01"));
    assert_eq!(context.scenario.binding.target_price, d("103.53"));
    let quote = Quote {
        instrument_id: holding.plan.candidate.instrument_id.clone(),
        currency: Currency::new("EUR").unwrap(),
        bid: d("103.53"),
        ask: d("103.53"),
        observed_at: now(),
        received_at: now(),
        tradable: true,
    };
    assert!(
        net_executable_return(
            &holding.basis(now()).unwrap(),
            &quote,
            &costs(&holding),
            now()
        )
        .unwrap()
        .fraction
            > d("0.01")
    );
    holding.validate_context(&context, now()).unwrap();
    for price in [d("103.531"), d("103.54"), d("104")] {
        context.scenario.binding.target_price = price;
        rebind(&holding, &mut context);
        assert!(
            holding.validate_context(&context, now()).is_err(),
            "unreachable policy price {price}"
        );
    }
}

#[test]
fn changed_verified_quantity_cost_fees_or_stale_observation_requires_reassessment() {
    let holding = holding();
    let context = context(&holding, d("0.02"));
    for fault in ["quantity", "cost", "fees", "stale"] {
        let mut changed = holding.clone();
        match fault {
            "quantity" => {
                changed.plan.acquired_quantity = d("3");
                changed.observation.snapshot.positions[0].quantity = d("3");
            }
            "cost" => {
                changed
                    .observation
                    .position_costs
                    .values_mut()
                    .next()
                    .unwrap()
                    .amount += Decimal::ONE
            }
            "fees" => changed.exit_costs.fixed.amount = Decimal::ONE,
            "stale" => changed.observation.snapshot.observed_at = now() - Duration::seconds(61),
            _ => unreachable!(),
        }
        assert!(
            changed.validate_context(&context, now()).is_err(),
            "{fault}"
        );
    }
}

#[test]
fn current_fx_can_make_an_existing_holding_stop_profitable() {
    let mut holding = holding();
    holding.plan.candidate.currency = Currency::new("USD").unwrap();
    holding.plan.acquired_quantity = Decimal::ONE;
    holding.observation.snapshot.positions[0].quantity = Decimal::ONE;
    holding
        .observation
        .position_costs
        .values_mut()
        .next()
        .unwrap()
        .amount = d("100");
    holding.exit_costs.fx = Some(FxObservation {
        from: Currency::new("USD").unwrap(),
        to: Currency::new("EUR").unwrap(),
        rate: d("1.02"),
        observed_at: now(),
    });
    let context = context(&holding, d("0.02"));
    assert!(context.scenario.binding.entry_price < context.scenario.binding.stop_price);
    holding.validate_context(&context, now()).unwrap();
    for fault in ["rate", "direction", "stale"] {
        let mut changed = holding.clone();
        let fx = changed.exit_costs.fx.as_mut().unwrap();
        match fault {
            "rate" => fx.rate = d("1.0201"),
            "direction" => std::mem::swap(&mut fx.from, &mut fx.to),
            "stale" => fx.observed_at = now() - Duration::seconds(61),
            _ => unreachable!(),
        }
        assert!(
            changed.validate_context(&context, now()).is_err(),
            "{fault}"
        );
    }
}

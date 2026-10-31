use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Serialize, de::DeserializeOwned};
use trady_domain::{Currency, InstrumentId, Money, Quote};
use trady_policy::{
    ConfidenceInputs, ConfidenceScore, EntryCosts, FxObservation, InstrumentRules, Policy,
    SessionLossGuard, SizedEntry,
    capital::{account_return_contribution, session_performance},
    ceil_to, confidence,
    economics::{
        AcquisitionFill, CostBasis, ExitCosts, StopConstraints, StopProposal, cost_basis,
        liquidation_proceeds, net_executable_return, target_price, validate_protective_stop,
    },
    floor_to, fresh,
    holding::{
        ExitReason, HoldingAction, HoldingContext, HoldingDegradation, HoldingPlan, HoldingState,
        PlanUpdate, SessionWindow, TrailingSettings, activate_plan, evaluate_holding,
        holding_target_ceiling,
    },
};

fn d(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-30T15:00:00Z")
        .unwrap()
        .to_utc()
}

fn money(value: &str, currency: &str) -> Money {
    Money {
        amount: d(value),
        currency: Currency::new(currency).unwrap(),
    }
}

fn basis() -> CostBasis {
    CostBasis {
        instrument_id: InstrumentId::new("stock").unwrap(),
        quantity: Decimal::ONE,
        instrument_currency: Currency::new("EUR").unwrap(),
        acquisition_cost: money("100", "EUR"),
    }
}

fn costs() -> ExitCosts {
    ExitCosts {
        fixed: money("0", "EUR"),
        rate: Decimal::ZERO,
        uncertainty: money("0", "EUR"),
        fx: None,
    }
}

fn quote(bid: &str) -> Quote {
    Quote {
        instrument_id: basis().instrument_id,
        currency: Currency::new("EUR").unwrap(),
        bid: d(bid),
        ask: d(bid) + d("0.02"),
        observed_at: now(),
        received_at: now(),
        tradable: true,
    }
}

fn context() -> HoldingContext {
    HoldingContext {
        now: now(),
        session: Some(SessionWindow {
            opens_at: now() - Duration::hours(1),
            closes_at: now() + Duration::hours(5),
        }),
        holding_session_number: Some(1),
        overnight_protection_verified: true,
        research_available: true,
        order_outcome_known: true,
        owner_close_authenticated: false,
    }
}

fn state(target: &str) -> HoldingState {
    HoldingState::new(
        basis().instrument_id,
        d("0.01"),
        HoldingPlan {
            version: 1,
            stop_price: d("99.4"),
            target_net_fraction: d(target),
            deadline: now() + Duration::days(4),
            trailing: TrailingSettings::default(),
        },
        &Policy::default(),
    )
    .unwrap()
}

fn update(state: &HoldingState, target: &str, score: u8) -> PlanUpdate {
    PlanUpdate {
        expected_version: state.plan().version,
        proposed: HoldingPlan {
            version: state.plan().version + 1,
            target_net_fraction: d(target),
            ..state.plan().clone()
        },
        confidence_score: score,
        exceptional_verified: false,
        target_increase_allowed: true,
    }
}

fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T) {
    assert_eq!(
        &serde_json::from_str::<T>(&serde_json::to_string(value).unwrap()).unwrap(),
        value
    );
}

#[test]
fn actual_partial_fills_and_both_sides_charges_define_account_currency_return() {
    let fx = |rate: &str| FxObservation {
        from: Currency::new("USD").unwrap(),
        to: Currency::new("EUR").unwrap(),
        rate: d(rate),
        observed_at: now(),
    };
    let first = AcquisitionFill {
        instrument_id: basis().instrument_id,
        quantity: d("2"),
        price: money("10", "USD"),
        charges: money("0.10", "EUR"),
        fx: Some(fx("0.9")),
        filled_at: now(),
    };
    let second = AcquisitionFill {
        quantity: d("3"),
        price: money("12", "USD"),
        charges: money("0.20", "EUR"),
        fx: Some(fx("0.8")),
        ..first.clone()
    };
    let filled = cost_basis(&[first.clone(), second.clone()]).unwrap();
    assert_eq!(filled.quantity, d("5"));
    assert_eq!(filled.acquisition_cost, money("47.10", "EUR"));
    let exit = ExitCosts {
        fixed: money("0.10", "EUR"),
        rate: d("0.001"),
        uncertainty: money("0.02", "EUR"),
        fx: Some(fx("0.85")),
    };
    let bid = Quote {
        currency: Currency::new("USD").unwrap(),
        ..quote("12")
    };
    let value = net_executable_return(&filled, &bid, &exit, now()).unwrap();
    assert_eq!(value.proceeds, money("50.829", "EUR"));
    assert_eq!(value.pnl, money("3.729", "EUR"));
    assert!(value.fraction > d("0.079") && value.fraction < d("0.080"));
    assert_eq!(
        target_price(&filled, d("0.01"), &exit, d("0.01"), now()).unwrap(),
        d("11.24")
    );
    assert!(
        liquidation_proceeds(&filled, d("11.23"), &exit, now())
            .unwrap()
            .amount
            < d("47.571")
    );
    round_trip(&filled);
    round_trip(&value);
    assert!(cost_basis(&[]).is_err());
    let wrong_class = AcquisitionFill {
        instrument_id: InstrumentId::new("other_class").unwrap(),
        ..second
    };
    assert!(cost_basis(&[first, wrong_class]).is_err());
}

#[test]
fn target_sell_tick_covers_exit_fees_and_never_uses_the_ask_as_proceeds() {
    let exit = ExitCosts {
        fixed: money("0.10", "EUR"),
        rate: d("0.001"),
        uncertainty: money("0.05", "EUR"),
        ..costs()
    };
    assert_eq!(
        target_price(&basis(), d("0.01"), &exit, d("0.01"), now()).unwrap(),
        d("101.26")
    );
    assert_eq!(
        liquidation_proceeds(&basis(), d("101.26"), &exit, now()).unwrap(),
        money("101.00874", "EUR")
    );
    assert!(
        liquidation_proceeds(&basis(), d("101.25"), &exit, now())
            .unwrap()
            .amount
            < d("101")
    );
    let wide = Quote {
        ask: d("110"),
        ..quote("100")
    };
    assert_eq!(
        net_executable_return(&basis(), &wide, &costs(), now())
            .unwrap()
            .fraction,
        Decimal::ZERO
    );
    assert!(target_price(&basis(), d("0.11"), &costs(), d("0.01"), now()).is_err());
}

#[test]
fn tiny_target_tick_still_recomputes_sufficient_net_proceeds() {
    let filled = CostBasis {
        quantity: d("3"),
        acquisition_cost: money("1", "EUR"),
        ..basis()
    };
    let tick = d("0.0000000000000000000000000001");
    let price = target_price(&filled, d("0.01"), &costs(), tick, now()).unwrap();
    assert_eq!(price, d("0.3366666666666666666666666667"));
    assert!(
        liquidation_proceeds(&filled, price, &costs(), now())
            .unwrap()
            .amount
            >= d("1.01")
    );
    assert!(
        liquidation_proceeds(&filled, price - tick, &costs(), now())
            .unwrap()
            .amount
            < d("1.01")
    );
}

#[test]
fn fx_changes_net_return_even_with_unchanged_stock_price_and_can_exceed_target_cap() {
    let filled = CostBasis {
        instrument_currency: Currency::new("USD").unwrap(),
        acquisition_cost: money("90", "EUR"),
        ..basis()
    };
    let bid = Quote {
        currency: Currency::new("USD").unwrap(),
        ..quote("100")
    };
    let exit = ExitCosts {
        fx: Some(FxObservation {
            from: Currency::new("USD").unwrap(),
            to: Currency::new("EUR").unwrap(),
            rate: Decimal::ONE,
            observed_at: now(),
        }),
        ..costs()
    };
    let value = net_executable_return(&filled, &bid, &exit, now()).unwrap();
    assert!(value.fraction > d("0.10"));
    let managed = evaluate_holding(
        &state("0.10"),
        &filled,
        Some(&bid),
        &exit,
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        managed.state.exit_latch().unwrap().reason,
        ExitReason::ProfitTarget
    );
}

#[test]
fn stop_rounding_preserves_loss_bound_and_rejects_the_unworkable_technical_stop() {
    let limits = StopConstraints {
        price_tick: d("0.01"),
        minimum_distance: d("0.66"),
        noise_distance: d("0.65"),
        protection_verified: true,
    };
    let proposed = StopProposal {
        price: d("99.331"),
        target_fraction: d("0.01"),
        confidence_score: 90,
        exceptional_verified: false,
    };
    let accepted = validate_protective_stop(
        &basis(),
        &quote("100"),
        &costs(),
        &limits,
        &proposed,
        now(),
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(accepted.price, d("99.34"));
    assert_eq!(accepted.planned_loss_fraction, d("0.0066"));
    assert!(accepted.reward_to_risk > d("1.5"));
    for stop in ["99.33", "98", "99.995"] {
        let proposal = StopProposal {
            price: d(stop),
            ..proposed.clone()
        };
        assert!(
            validate_protective_stop(
                &basis(),
                &quote("100"),
                &costs(),
                &limits,
                &proposal,
                now(),
                &Policy::default()
            )
            .is_err(),
            "{stop}"
        );
    }
    let inside_noise = StopConstraints {
        noise_distance: d("0.66"),
        ..limits.clone()
    };
    assert!(
        validate_protective_stop(
            &basis(),
            &quote("100"),
            &costs(),
            &inside_noise,
            &proposed,
            now(),
            &Policy::default()
        )
        .is_err()
    );
    let unprotected = StopConstraints {
        protection_verified: false,
        ..limits
    };
    assert!(
        validate_protective_stop(
            &basis(),
            &quote("100"),
            &costs(),
            &unprotected,
            &proposed,
            now(),
            &Policy::default()
        )
        .is_err()
    );
}

#[test]
fn stop_economics_include_exit_charges_and_recheck_wall_clock_freshness() {
    let limits = StopConstraints {
        price_tick: d("0.01"),
        minimum_distance: d("0.1"),
        noise_distance: d("0.1"),
        protection_verified: true,
    };
    let proposed = StopProposal {
        price: d("99.4"),
        target_fraction: d("0.01"),
        confidence_score: 95,
        exceptional_verified: false,
    };
    assert!(
        validate_protective_stop(
            &basis(),
            &quote("100"),
            &costs(),
            &limits,
            &proposed,
            now(),
            &Policy::default()
        )
        .is_ok()
    );
    let expensive = ExitCosts {
        fixed: money("0.1", "EUR"),
        ..costs()
    };
    assert!(
        validate_protective_stop(
            &basis(),
            &quote("100"),
            &expensive,
            &limits,
            &proposed,
            now(),
            &Policy::default()
        )
        .is_err()
    );
    assert!(
        validate_protective_stop(
            &basis(),
            &quote("100"),
            &costs(),
            &limits,
            &proposed,
            now() + Duration::seconds(11),
            &Policy::default()
        )
        .is_err()
    );
}

#[test]
fn exact_quote_and_fx_age_boundaries_and_directions_are_enforced() {
    let filled = CostBasis {
        instrument_currency: Currency::new("USD").unwrap(),
        ..basis()
    };
    let bid = Quote {
        currency: Currency::new("USD").unwrap(),
        observed_at: now() - Duration::seconds(10),
        ..quote("100")
    };
    let mut exit = ExitCosts {
        fx: Some(FxObservation {
            from: Currency::new("USD").unwrap(),
            to: Currency::new("EUR").unwrap(),
            rate: Decimal::ONE,
            observed_at: now() - Duration::seconds(60),
        }),
        ..costs()
    };
    assert!(net_executable_return(&filled, &bid, &exit, now()).is_ok());
    assert!(net_executable_return(&filled, &bid, &exit, now() + Duration::nanoseconds(1)).is_err());
    exit.fx.as_mut().unwrap().observed_at = now() - Duration::seconds(61);
    assert!(net_executable_return(&filled, &bid, &exit, now()).is_err());
    exit.fx.as_mut().unwrap().observed_at = now();
    exit.fx.as_mut().unwrap().from = Currency::new("EUR").unwrap();
    assert!(net_executable_return(&filled, &bid, &exit, now()).is_err());
    exit.fx = None;
    assert!(net_executable_return(&filled, &bid, &exit, now()).is_err());
    assert!(!fresh(now(), now(), -1));
    assert!(fresh(now(), now(), i64::MAX));
}

#[test]
fn unrepresentable_arithmetic_and_invalid_costs_fail_without_panics() {
    assert_eq!(floor_to(Decimal::MAX, d("0.0001")).unwrap(), Decimal::MAX);
    assert_eq!(
        floor_to(Decimal::MAX, d("2")).unwrap(),
        Decimal::MAX - Decimal::ONE
    );
    assert!(ceil_to(Decimal::MAX, d("2")).is_err());
    assert!(floor_to(d("7922816251426433759354395033.5"), d("0.11")).is_err());
    assert!(ceil_to(Decimal::ONE, Decimal::ZERO).is_err());
    assert!(
        target_price(
            &CostBasis {
                acquisition_cost: Money {
                    amount: Decimal::MAX,
                    ..money("0", "EUR")
                },
                ..basis()
            },
            d("0.01"),
            &costs(),
            d("0.01"),
            now()
        )
        .is_err()
    );
    assert!(
        liquidation_proceeds(
            &CostBasis {
                quantity: Decimal::MAX,
                ..basis()
            },
            d("2"),
            &costs(),
            now()
        )
        .is_err()
    );
    let fill = AcquisitionFill {
        instrument_id: basis().instrument_id,
        quantity: Decimal::MAX,
        price: money("2", "EUR"),
        charges: money("0", "EUR"),
        fx: None,
        filled_at: now(),
    };
    assert!(cost_basis(&[fill]).is_err());
    for rate in ["-0.01", "1", "1.1"] {
        assert!(
            liquidation_proceeds(
                &basis(),
                d("100"),
                &ExitCosts {
                    rate: d(rate),
                    ..costs()
                },
                now()
            )
            .is_err()
        );
    }
    let tiny_sale = ExitCosts {
        fixed: money("101", "EUR"),
        ..costs()
    };
    assert_eq!(
        net_executable_return(&basis(), &quote("100"), &tiny_sale, now())
            .unwrap()
            .fraction,
        d("-1.01")
    );
}

#[test]
fn position_gain_and_account_growth_use_different_denominators() {
    let filled = CostBasis {
        acquisition_cost: money("49", "EUR"),
        ..basis()
    };
    let value = net_executable_return(&filled, &quote("49.49"), &costs(), now()).unwrap();
    assert_eq!(value.fraction, d("0.01"));
    assert_eq!(
        account_return_contribution(&value.pnl, &money("50", "EUR")).unwrap(),
        d("0.0098")
    );
    assert!(account_return_contribution(&value.pnl, &money("50", "USD")).is_err());
}

#[test]
fn gross_deposits_define_loss_denominator_even_after_withdrawals() {
    let first = session_performance(d("50"), d("147.01"), d("100"), Decimal::ZERO).unwrap();
    assert_eq!(first.adjusted_pnl, d("-2.99"));
    assert_eq!(first.loss_guard_denominator, d("150"));
    assert!(first.loss_guard_fraction.unwrap() > d("-0.02"));
    let mut guard = SessionLossGuard {
        session_id: "actual_session".into(),
        paused: false,
    };
    guard
        .observe_cash_flows(d("50"), d("147.01"), d("100"), Decimal::ZERO)
        .unwrap();
    assert!(!guard.paused);
    let at_limit = guard
        .observe_cash_flows(d("50"), d("127"), d("100"), d("20"))
        .unwrap();
    assert_eq!(at_limit.adjusted_pnl, d("-3"));
    assert_eq!(at_limit.loss_guard_denominator, d("150"));
    assert_eq!(at_limit.loss_guard_fraction, Some(d("-0.02")));
    assert!(guard.paused);
    guard
        .observe_cash_flows(d("50"), d("400"), d("250"), d("20"))
        .unwrap();
    assert!(guard.paused);
    round_trip(&guard);
    round_trip(&at_limit);
    let withdrawal = session_performance(d("100"), d("48"), Decimal::ZERO, d("50")).unwrap();
    assert_eq!(withdrawal.loss_guard_denominator, d("100"));
    assert_eq!(withdrawal.loss_guard_fraction, Some(d("-0.02")));
}

#[test]
fn unavailable_or_overflowing_loss_metric_blocks_entries() {
    let mut guard = SessionLossGuard {
        session_id: "empty_account".into(),
        paused: false,
    };
    assert_eq!(
        guard
            .observe_cash_flows(Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
            .unwrap()
            .loss_guard_fraction,
        None
    );
    assert!(guard.paused);
    let mut guard = SessionLossGuard {
        session_id: "overflow".into(),
        paused: false,
    };
    assert!(
        guard
            .observe_cash_flows(Decimal::MAX, Decimal::MAX, Decimal::ONE, Decimal::ZERO)
            .is_err()
    );
    assert!(guard.paused);
    assert!(session_performance(d("100"), d("100"), d("-1"), Decimal::ZERO).is_err());
}

#[test]
fn supported_87_point_update_raises_holding_target_before_old_target() {
    let original = state("0.01");
    let mut proposed = update(&original, "0.02", 87);
    proposed.proposed.stop_price = d("99.5");
    proposed.proposed.deadline -= Duration::hours(1);
    let accepted = activate_plan(
        &original,
        &proposed,
        &basis(),
        Some(&quote("100.6")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert!(accepted.activated);
    assert_eq!(accepted.evaluation.state.plan().version, 2);
    assert_eq!(
        accepted.evaluation.state.plan().target_net_fraction,
        d("0.02")
    );
    assert_eq!(
        accepted.evaluation.state.trailing().floor_net_fraction,
        Some(d("0.001"))
    );
    assert_eq!(accepted.evaluation.action, HoldingAction::Monitor);
}

#[test]
fn old_target_wins_update_race_and_latch_cannot_be_revoked() {
    let original = state("0.01");
    let proposed = update(&original, "0.02", 95);
    let triggered = activate_plan(
        &original,
        &proposed,
        &basis(),
        Some(&quote("101")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert!(!triggered.activated);
    assert_eq!(triggered.evaluation.state.plan().version, 1);
    assert_eq!(
        triggered.evaluation.state.exit_latch().unwrap().reason,
        ExitReason::ProfitTarget
    );
    let latch = triggered.evaluation.state.exit_latch().unwrap().clone();
    let mut invalid = proposed;
    invalid.expected_version = 0;
    invalid.proposed.target_net_fraction = d("0.11");
    let restored: HoldingState =
        serde_json::from_str(&serde_json::to_string(&triggered.evaluation.state).unwrap()).unwrap();
    let result = activate_plan(
        &restored,
        &invalid,
        &basis(),
        Some(&quote("90")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert!(!result.activated);
    assert_eq!(result.evaluation.state.exit_latch(), Some(&latch));
    assert_eq!(result.evaluation.action, HoldingAction::ExitRequired);
}

#[test]
fn lower_target_or_tighter_stop_already_met_latches_new_version_immediately() {
    let original = state("0.02");
    let lower = update(&original, "0.005", 70);
    let result = activate_plan(
        &original,
        &lower,
        &basis(),
        Some(&quote("100.8")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert!(result.activated);
    let latch = result.evaluation.state.exit_latch().unwrap();
    assert_eq!(latch.plan_version, 2);
    assert_eq!(latch.reason, ExitReason::ProfitTarget);
    let mut tighter = update(&original, "0.02", 87);
    tighter.proposed.stop_price = d("100.5");
    let result = activate_plan(
        &original,
        &tighter,
        &basis(),
        Some(&quote("100.4")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert!(result.activated);
    assert_eq!(
        result.evaluation.state.exit_latch().unwrap().reason,
        ExitReason::HardStop
    );
}

#[test]
fn invalid_updates_do_not_lose_old_plan_peak_or_relax_any_protection() {
    let original = state("0.02");
    let mut cases = Vec::new();
    let mut loose = update(&original, "0.02", 87);
    loose.proposed.stop_price = d("99.3");
    cases.push(loose);
    let mut late = update(&original, "0.02", 87);
    late.proposed.deadline += Duration::seconds(1);
    cases.push(late);
    let mut trail = update(&original, "0.02", 87);
    trail.proposed.trailing.distance_fraction = d("0.006");
    cases.push(trail);
    let mut activation = update(&original, "0.02", 87);
    activation.proposed.trailing.activation_net_fraction = d("0.006");
    cases.push(activation);
    let mut stale = update(&original, "0.02", 87);
    stale.expected_version = 0;
    cases.push(stale);
    let mut unjustified = update(&state("0.01"), "0.02", 87);
    unjustified.target_increase_allowed = false;
    let rejected = activate_plan(
        &state("0.01"),
        &unjustified,
        &basis(),
        Some(&quote("100.6")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert!(!rejected.activated);
    for proposed in cases {
        let rejected = activate_plan(
            &original,
            &proposed,
            &basis(),
            Some(&quote("100.6")),
            &costs(),
            &context(),
            &Policy::default(),
        )
        .unwrap();
        assert!(!rejected.activated);
        assert!(rejected.rejection.is_some());
        assert_eq!(rejected.evaluation.state.plan(), original.plan());
        assert_eq!(
            rejected.evaluation.state.trailing().floor_net_fraction,
            Some(d("0.001"))
        );
    }
}

#[test]
fn holding_confidence_bands_and_exceptional_deterioration_do_not_force_an_unconfigured_exit() {
    let policy = Policy::default();
    for (score, ceiling) in [
        (69, "0.02"),
        (70, "0.01"),
        (79, "0.01"),
        (80, "0.02"),
        (89, "0.02"),
        (90, "0.03"),
        (94, "0.03"),
        (95, "0.03"),
        (100, "0.03"),
    ] {
        assert_eq!(
            holding_target_ceiling(score, d("0.02"), true, &policy).unwrap(),
            d(ceiling)
        );
    }
    let original = state("0.02");
    let unchanged = activate_plan(
        &original,
        &update(&original, "0.02", 69),
        &basis(),
        Some(&quote("100")),
        &costs(),
        &context(),
        &policy,
    )
    .unwrap();
    assert!(unchanged.activated);
    assert_eq!(unchanged.evaluation.action, HoldingAction::Monitor);
    let raised = activate_plan(
        &original,
        &update(&original, "0.03", 69),
        &basis(),
        Some(&quote("100")),
        &costs(),
        &context(),
        &policy,
    )
    .unwrap();
    assert!(!raised.activated);
    let exceptional = state("0.10");
    let refused = activate_plan(
        &exceptional,
        &update(&exceptional, "0.03", 87),
        &basis(),
        Some(&quote("100")),
        &costs(),
        &context(),
        &policy,
    )
    .unwrap();
    assert!(!refused.activated);
    let reduced = activate_plan(
        &exceptional,
        &update(&exceptional, "0.02", 87),
        &basis(),
        Some(&quote("100")),
        &costs(),
        &context(),
        &policy,
    )
    .unwrap();
    assert!(reduced.activated);
    assert_eq!(
        holding_target_ceiling(
            95,
            d("0.03"),
            true,
            &Policy {
                exceptional_targets_enabled: true,
                ..policy
            }
        )
        .unwrap(),
        d("0.10")
    );
}

#[test]
fn exact_trailing_activation_and_floor_survive_restart_and_rebound() {
    let original = state("0.02");
    let before = evaluate_holding(
        &original,
        &basis(),
        Some(&quote("100.499")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(before.state.trailing().floor_net_fraction, None);
    let mut clock = context();
    clock.now += Duration::seconds(1);
    let mut bid = quote("100.5");
    bid.observed_at = clock.now;
    bid.received_at = clock.now;
    let active = evaluate_holding(
        &before.state,
        &basis(),
        Some(&bid),
        &costs(),
        &clock,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        active.state.trailing().floor_net_fraction,
        Some(Decimal::ZERO)
    );
    clock.now += Duration::seconds(1);
    bid = Quote {
        observed_at: clock.now,
        received_at: clock.now,
        ..quote("100.9")
    };
    let peak = evaluate_holding(
        &active.state,
        &basis(),
        Some(&bid),
        &costs(),
        &clock,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(peak.state.trailing().floor_net_fraction, Some(d("0.004")));
    let restored: HoldingState =
        serde_json::from_str(&serde_json::to_string(&peak.state).unwrap()).unwrap();
    clock.now += Duration::seconds(1);
    bid = Quote {
        observed_at: clock.now,
        received_at: clock.now,
        ..quote("100.4")
    };
    let triggered = evaluate_holding(
        &restored,
        &basis(),
        Some(&bid),
        &costs(),
        &clock,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        triggered.state.exit_latch().unwrap().reason,
        ExitReason::TrailingFloor
    );
    assert_eq!(
        triggered.state.trailing().peak_net_fraction,
        Some(d("0.009"))
    );
    round_trip(&triggered.state);
    let rebound = evaluate_holding(
        &triggered.state,
        &basis(),
        Some(&quote("105")),
        &costs(),
        &clock,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(rebound.state.exit_latch(), triggered.state.exit_latch());
}

#[test]
fn tighter_trailing_update_keeps_the_peak_and_can_trigger_immediately() {
    let original = state("0.02");
    let peak = evaluate_holding(
        &original,
        &basis(),
        Some(&quote("100.9")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    let mut clock = context();
    clock.now += Duration::seconds(1);
    let bid = Quote {
        observed_at: clock.now,
        received_at: clock.now,
        ..quote("100.6")
    };
    let mut proposed = update(&peak.state, "0.02", 87);
    proposed.proposed.trailing.distance_fraction = d("0.002");
    let result = activate_plan(
        &peak.state,
        &proposed,
        &basis(),
        Some(&bid),
        &costs(),
        &clock,
        &Policy::default(),
    )
    .unwrap();
    assert!(result.activated);
    assert_eq!(
        result.evaluation.state.trailing().floor_net_fraction,
        Some(d("0.007"))
    );
    assert_eq!(
        result.evaluation.state.exit_latch().unwrap().reason,
        ExitReason::TrailingFloor
    );
}

#[test]
fn outages_preserve_thresholds_and_unknown_outcomes_require_reconciliation() {
    let original = state("0.01");
    let mut degraded = context();
    degraded.research_available = false;
    let exit = evaluate_holding(
        &original,
        &basis(),
        Some(&quote("101")),
        &costs(),
        &degraded,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        exit.state.exit_latch().unwrap().reason,
        ExitReason::ProfitTarget
    );
    assert!(
        exit.degradation
            .contains(&HoldingDegradation::ResearchUnavailable)
    );
    let blocked = activate_plan(
        &original,
        &update(&original, "0.02", 95),
        &basis(),
        Some(&quote("100.6")),
        &costs(),
        &degraded,
        &Policy::default(),
    )
    .unwrap();
    assert!(!blocked.activated);
    assert_eq!(blocked.evaluation.state.plan(), original.plan());
    let stale = Quote {
        observed_at: now() - Duration::seconds(11),
        ..quote("101")
    };
    let unchanged = evaluate_holding(
        &original,
        &basis(),
        Some(&stale),
        &costs(),
        &degraded,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(unchanged.action, HoldingAction::Monitor);
    assert!(unchanged.state.exit_latch().is_none());
    degraded.order_outcome_known = false;
    let unknown = evaluate_holding(
        &original,
        &basis(),
        Some(&quote("101")),
        &costs(),
        &degraded,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(unknown.action, HoldingAction::Reconcile);
    assert_eq!(
        unknown.state.exit_latch().unwrap().reason,
        ExitReason::ProfitTarget
    );
    let again = evaluate_holding(
        &unknown.state,
        &basis(),
        None,
        &costs(),
        &degraded,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(again.state.exit_latch(), unknown.state.exit_latch());
    assert_eq!(again.action, HoldingAction::Reconcile);
}

#[test]
fn hard_stop_and_time_exits_do_not_depend_on_available_fx_or_research() {
    let filled = CostBasis {
        instrument_currency: Currency::new("USD").unwrap(),
        ..basis()
    };
    let bid = Quote {
        currency: Currency::new("USD").unwrap(),
        ..quote("99.4")
    };
    let exit = evaluate_holding(
        &state("0.01"),
        &filled,
        Some(&bid),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        exit.state.exit_latch().unwrap().reason,
        ExitReason::HardStop
    );
    assert!(
        exit.degradation
            .contains(&HoldingDegradation::ValuationUnavailable)
    );
    let mut expired = context();
    expired.research_available = false;
    expired.now = state("0.01").plan().deadline;
    let result = evaluate_holding(
        &state("0.01"),
        &basis(),
        None,
        &costs(),
        &expired,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        result.state.exit_latch().unwrap().reason,
        ExitReason::PlanDeadline
    );
    let mut owner = context();
    owner.owner_close_authenticated = true;
    let result = evaluate_holding(
        &state("0.01"),
        &basis(),
        None,
        &costs(),
        &owner,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        result.state.exit_latch().unwrap().reason,
        ExitReason::OwnerClose
    );
}

#[test]
fn actual_early_close_and_fifth_holding_session_use_their_time_boundaries() {
    let mut early = context();
    early.session.as_mut().unwrap().closes_at = now() + Duration::minutes(15);
    early.overnight_protection_verified = false;
    let result = evaluate_holding(
        &state("0.01"),
        &basis(),
        None,
        &costs(),
        &early,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        result.state.exit_latch().unwrap().reason,
        ExitReason::MarketClose
    );
    early.overnight_protection_verified = true;
    early.holding_session_number = Some(5);
    let fifth = evaluate_holding(
        &state("0.01"),
        &basis(),
        None,
        &costs(),
        &early,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        fifth.state.exit_latch().unwrap().reason,
        ExitReason::MaximumHoldingSessions
    );
    early.now -= Duration::nanoseconds(1);
    let before = evaluate_holding(
        &state("0.01"),
        &basis(),
        None,
        &costs(),
        &early,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(before.action, HoldingAction::Monitor);
    early.holding_session_number = Some(6);
    let overdue = evaluate_holding(
        &state("0.01"),
        &basis(),
        None,
        &costs(),
        &early,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        overdue.state.exit_latch().unwrap().reason,
        ExitReason::MaximumHoldingSessions
    );
    // A close timestamp passing never marks a still-unresolved holding flat.
    early.now = early.session.as_ref().unwrap().closes_at;
    let at_close = evaluate_holding(
        &fifth.state,
        &basis(),
        None,
        &costs(),
        &early,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(at_close.action, HoldingAction::ExitRequired);
    assert_eq!(at_close.state.exit_latch(), fifth.state.exit_latch());
}

#[test]
fn entry_windows_expire_at_five_minutes_or_actual_close_cutoff() {
    let session = context().session.unwrap();
    assert_eq!(
        session
            .validate_entry_time(now(), now() + Duration::seconds(299))
            .unwrap(),
        now() + Duration::minutes(5)
    );
    assert!(
        session
            .validate_entry_time(now(), now() + Duration::minutes(5))
            .is_err()
    );
    assert!(
        session
            .validate_entry_time(now() + Duration::seconds(1), now())
            .is_err()
    );
    let early = SessionWindow {
        closes_at: now() + Duration::minutes(32),
        ..session
    };
    assert_eq!(
        early.entry_expires_at(now()).unwrap(),
        now() + Duration::minutes(2)
    );
    assert!(
        early
            .validate_entry_time(now(), now() + Duration::minutes(2))
            .is_err()
    );
    assert!(!early.permits_entry(now() + Duration::minutes(2)));
    assert!(!early.permits_entry(early.opens_at - Duration::seconds(1)));
}

#[test]
fn missing_calendar_blocks_updates_but_does_not_disable_fresh_stop_checks() {
    let mut missing = context();
    missing.session = None;
    missing.holding_session_number = None;
    let result = activate_plan(
        &state("0.01"),
        &update(&state("0.01"), "0.02", 95),
        &basis(),
        Some(&quote("100.6")),
        &costs(),
        &missing,
        &Policy::default(),
    )
    .unwrap();
    assert!(!result.activated);
    assert!(
        result
            .evaluation
            .degradation
            .contains(&HoldingDegradation::CalendarUnavailable)
    );
    let stop = evaluate_holding(
        &state("0.01"),
        &basis(),
        Some(&quote("99.4")),
        &costs(),
        &missing,
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(
        stop.state.exit_latch().unwrap().reason,
        ExitReason::HardStop
    );
}

#[test]
fn earlier_or_conflicting_same_timestamp_quote_cannot_replace_latest_accepted_observation() {
    let original = evaluate_holding(
        &state("0.02"),
        &basis(),
        Some(&quote("100.6")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    for changed in [
        quote("103"),
        Quote {
            observed_at: now() - Duration::seconds(1),
            ..quote("103")
        },
    ] {
        let result = evaluate_holding(
            &original.state,
            &basis(),
            Some(&changed),
            &costs(),
            &context(),
            &Policy::default(),
        )
        .unwrap();
        assert!(
            result
                .degradation
                .contains(&HoldingDegradation::QuoteRejected)
        );
        assert_eq!(result.action, HoldingAction::Monitor);
        assert_eq!(result.state.latest_quote().unwrap().bid, d("100.6"));
    }
    let lower = update(&original.state, "0.005", 70);
    let result = activate_plan(
        &original.state,
        &lower,
        &basis(),
        None,
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert!(result.activated);
    assert_eq!(
        result.evaluation.state.exit_latch().unwrap().reason,
        ExitReason::ProfitTarget
    );
}

#[test]
fn version_overflow_is_a_rejection_with_preserved_old_state() {
    let plan = HoldingPlan {
        version: u64::MAX,
        ..state("0.02").plan().clone()
    };
    let original =
        HoldingState::new(basis().instrument_id, d("0.01"), plan, &Policy::default()).unwrap();
    let proposed = PlanUpdate {
        expected_version: u64::MAX,
        proposed: HoldingPlan {
            version: 1,
            ..original.plan().clone()
        },
        confidence_score: 95,
        exceptional_verified: false,
        target_increase_allowed: true,
    };
    let result = activate_plan(
        &original,
        &proposed,
        &basis(),
        Some(&quote("100.6")),
        &costs(),
        &context(),
        &Policy::default(),
    )
    .unwrap();
    assert!(!result.activated);
    assert_eq!(result.evaluation.state.plan().version, u64::MAX);
    assert!(result.rejection.unwrap().contains("overflow"));
    assert_eq!(
        result.evaluation.state.trailing().floor_net_fraction,
        Some(d("0.001"))
    );
}

#[test]
fn policy_snapshots_round_trip_but_confidence_components_cannot_bypass_the_constructor() {
    let verified = confidence(&ConfidenceInputs::default());
    round_trip(&verified);
    let valid = serde_json::to_value(&verified).unwrap();
    for (key, value) in [
        ("primary", 26),
        ("catalyst", 11),
        ("independent", 14),
        ("price", 255),
        ("economics", 6),
        ("freshness", 255),
    ] {
        let mut forged = valid.clone();
        forged[key] = serde_json::json!(value);
        assert!(serde_json::from_value::<ConfidenceScore>(forged).is_err());
    }
    assert!(serde_json::from_value::<ConfidenceScore>(serde_json::json!({"total":100})).is_err());
    let fx = FxObservation {
        from: Currency::new("USD").unwrap(),
        to: Currency::new("EUR").unwrap(),
        rate: d("0.9"),
        observed_at: now(),
    };
    round_trip(&fx);
    round_trip(&EntryCosts {
        fixed: money("0.1", "EUR"),
        rate: d("0.001"),
        uncertainty: money("0.01", "EUR"),
        fx: Some(fx),
    });
    round_trip(&InstrumentRules {
        price_tick: d("0.01"),
        quantity_step: d("0.001"),
        minimum_quantity: d("0.001"),
        currency_unit: d("0.01"),
    });
    round_trip(&SizedEntry {
        limit_price: d("10"),
        quantity: d("4.9"),
        budget: money("49", "EUR"),
        estimated_cost: money("49", "EUR"),
        remaining_cash: money("1", "EUR"),
    });
}

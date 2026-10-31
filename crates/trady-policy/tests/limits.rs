//! Deterministic financial fixtures, not market forecasts or backtest results.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use trady_domain::{AccountId, AccountSnapshot, Currency, InstrumentId, Mode, Money, Quote};
use trady_policy::{
    EntryCosts, EntryRange, FxObservation, InstrumentRules, Policy, ceil_to, economics::ExitCosts,
    floor_to, limits::*,
};

fn d(value: &str) -> Decimal {
    Decimal::from_str_exact(value).unwrap()
}
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
        .unwrap()
        .to_utc()
}
fn currency() -> Currency {
    Currency::new("EUR").unwrap()
}
fn money(amount: &str) -> Money {
    Money {
        amount: d(amount),
        currency: currency(),
    }
}
fn fixture() -> (FormulaEnvelope, FormulaParameters, LimitsInput) {
    let envelope = FormulaEnvelope {
        max_entry_allowance_fraction: d("0.005"),
        min_stop_atr_multiple: Decimal::ONE,
        max_stop_atr_multiple: d("4"),
        min_target_net_fraction: d("0.01"),
        max_target_net_fraction: d("0.03"),
    };
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
    let input = LimitsInput {
        account: AccountSnapshot {
            snapshot_id: "account_1".into(),
            account_id: AccountId::new("paper_limits_test").unwrap(),
            mode: Mode::Paper,
            spendable_cash: money("1000"),
            positions: vec![],
            orders: vec![],
            observed_at: now(),
        },
        market_snapshot_id: "market_1".into(),
        quote: Quote {
            instrument_id: InstrumentId::new("test_stock").unwrap(),
            currency: currency(),
            bid: d("99.99"),
            ask: d("100"),
            observed_at: now(),
            received_at: now(),
            tradable: true,
        },
        entry_range: EntryRange {
            minimum: d("99"),
            maximum: d("101"),
            currency: currency(),
        },
        entry_costs: EntryCosts {
            fixed: money("0.1"),
            rate: d("0.001"),
            uncertainty: money("0.1"),
            fx: None,
        },
        exit_costs: ExitCosts {
            fixed: money("0.1"),
            rate: d("0.001"),
            uncertainty: money("0.1"),
            fx: None,
        },
        rules: InstrumentRules {
            price_tick: d("0.01"),
            quantity_step: d("0.01"),
            minimum_quantity: d("0.01"),
            currency_unit: d("0.01"),
        },
        evidence: FormulaEvidence {
            snapshot_id: "bars_1".into(),
            family: FormulaFamily::Trend,
            signal_eligible: true,
            atr: d("0.5"),
            atr_lookback: 14,
            bar_seconds: 60,
            completed_bars: 30,
            last_completed_at: now() - Duration::seconds(45),
            available_at: now() - Duration::seconds(30),
            valid_until: now() + Duration::minutes(5),
            minimum_stop_distance: d("0.01"),
            noise_distance: d("0.1"),
            base_confidence_score: 100,
            confidence_score: 100,
            sentiment_entry_allowed: true,
            sentiment_target_ceiling: d("0.03"),
            exceptional_verified: false,
        },
        holding_deadline_at: now() + Duration::days(3),
        valid_until: now() + Duration::minutes(5),
    };
    (envelope, parameters, input)
}

#[test]
fn formula_solves_prices_and_largest_affordable_risk_bounded_lot() {
    let (envelope, parameters, input) = fixture();
    let limits = calculate_limits(
        &Policy::default(),
        &envelope,
        &parameters,
        d("0.01"),
        &input,
        now(),
    )
    .unwrap();
    assert_eq!(limits.buy_limit, d("100.10"));
    assert_eq!(limits.stop_trigger, d("99.10"));
    assert_eq!(limits.quantity, d("8"));
    assert_eq!(limits.acquisition_cost.amount, d("801.8008"));
    assert_eq!(limits.account_risk.amount, d("9.9936"));
    assert!(limits.acquisition_cost.amount <= limits.budget.amount);
    assert!(limits.account_risk.amount <= d("10"));
    assert!(limits.reward_to_risk >= d("1.5"));
    assert!(limits.target_rounding_overshoot >= Decimal::ZERO);
    assert_eq!(limits.holding_deadline_at, input.holding_deadline_at);
    assert_eq!(limits.valid_until, now() + Duration::seconds(10));
    for price in [
        limits.buy_limit,
        limits.stop_trigger,
        limits.target_sell_limit,
    ] {
        assert_eq!(price % input.rules.price_tick, Decimal::ZERO);
    }
}

#[test]
fn fx_and_both_fee_legs_are_included_once() {
    let (envelope, parameters, mut input) = fixture();
    let usd = Currency::new("USD").unwrap();
    input.quote.currency = usd.clone();
    input.entry_range.currency = usd.clone();
    let fx = FxObservation {
        from: usd,
        to: currency(),
        rate: d("0.9"),
        observed_at: now(),
    };
    input.entry_costs.fx = Some(fx.clone());
    input.exit_costs.fx = Some(fx);
    let limits = calculate_limits(
        &Policy::default(),
        &envelope,
        &parameters,
        d("0.01"),
        &input,
        now(),
    )
    .unwrap();
    let expected_cost = limits.quantity * d("100.10") * d("0.9") * d("1.001") + d("0.2");
    let expected_stop = limits.quantity * d("99.10") * d("0.9") * d("0.999") - d("0.2");
    assert_eq!(limits.acquisition_cost.amount, expected_cost);
    assert_eq!(limits.proceeds_at_stop.amount, expected_stop);
    assert_eq!(limits.account_risk.amount, expected_cost - expected_stop);
    input.exit_costs.fx.as_mut().unwrap().observed_at = now() - Duration::seconds(61);
    assert!(
        calculate_limits(
            &Policy::default(),
            &envelope,
            &parameters,
            d("0.01"),
            &input,
            now()
        )
        .is_err()
    );
}

#[test]
fn infeasible_or_unverified_inputs_reject_without_relaxing_policy() {
    for case in 0..11 {
        let (envelope, mut parameters, mut input) = fixture();
        match case {
            0 => input.evidence.signal_eligible = false,
            1 => input.evidence.confidence_score = 89,
            2 => input.evidence.sentiment_entry_allowed = false,
            3 => input.evidence.atr = d("0.001"),
            4 => input.account.spendable_cash = money("0.1"),
            5 => input.evidence.available_at = now() + Duration::seconds(1),
            6 => input.evidence.completed_bars = 14,
            7 => input.quote.observed_at = now() - Duration::seconds(11),
            8 => parameters.target_net_fraction = d("0.1"),
            9 => input.evidence.last_completed_at = now() - Duration::hours(1),
            _ => input.rules.quantity_step = Decimal::ZERO,
        }
        assert!(
            calculate_limits(
                &Policy::default(),
                &envelope,
                &parameters,
                d("0.01"),
                &input,
                now()
            )
            .is_err(),
            "case {case}"
        );
    }
    let (envelope, parameters, input) = fixture();
    assert!(
        calculate_limits(
            &Policy::default(),
            &envelope,
            &parameters,
            d("0.03"),
            &input,
            now()
        )
        .is_err()
    );
}

#[test]
fn precision_exhaustion_never_silently_returns_an_off_grid_price() {
    let large = d("10000000000000000000000000000");
    assert!(floor_to(large, d("0.6")).is_err());
    assert!(ceil_to(large, d("0.6")).is_err());
    let (envelope, parameters, mut input) = fixture();
    input.quote.ask = Decimal::MAX;
    assert!(
        calculate_limits(
            &Policy::default(),
            &envelope,
            &parameters,
            d("0.01"),
            &input,
            now()
        )
        .is_err()
    );
}

#[test]
fn allocation_rounding_at_decimal_boundaries_never_spends_past_cash() {
    for cash in [
        "79228162514264337593543950335",
        "1000.0000000000000000000000001",
    ] {
        let (envelope, parameters, mut input) = fixture();
        input.account.spendable_cash = money(cash);
        input.rules.quantity_step = Decimal::ONE;
        input.rules.currency_unit = Decimal::ONE;
        let limits = calculate_limits(
            &Policy::default(),
            &envelope,
            &parameters,
            d("0.01"),
            &input,
            now(),
        )
        .unwrap();
        assert!(limits.acquisition_cost.amount <= limits.budget.amount);
        assert!(limits.budget.amount <= input.account.spendable_cash.amount);
        assert!(limits.acquisition_cost.amount <= input.account.spendable_cash.amount);
    }

    // At the smallest supported unit 0.98 * cash rounds upward to cash. This
    // remains an affordable unit, with no manufactured remaining cash.
    let (_, _, mut input) = fixture();
    let unit = d("0.0000000000000000000000000001");
    input.account.spendable_cash = money("0.0000000000000000000000000001");
    input.quote.bid = unit;
    input.quote.ask = unit;
    input.entry_range.minimum = unit;
    input.entry_range.maximum = unit;
    input.entry_costs.fixed = money("0");
    input.entry_costs.uncertainty = money("0");
    input.entry_costs.rate = Decimal::ZERO;
    input.rules.price_tick = unit;
    input.rules.quantity_step = Decimal::ONE;
    input.rules.minimum_quantity = Decimal::ONE;
    input.rules.currency_unit = unit;
    let sized = trady_policy::size_entry(
        &input.account.spendable_cash,
        &input.quote,
        &input.entry_range,
        &input.entry_costs,
        &input.rules,
        now(),
        &Policy::default(),
    )
    .unwrap();
    assert_eq!(sized.budget.amount, unit);
    assert_eq!(sized.quantity, Decimal::ONE);
    assert_eq!(sized.estimated_cost.amount, unit);
    assert_eq!(sized.remaining_cash.amount, Decimal::ZERO);
}

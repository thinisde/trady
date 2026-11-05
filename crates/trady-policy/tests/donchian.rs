use chrono::{Duration, TimeZone, Utc};
use rust_decimal::Decimal;
use trady_policy::Policy;
use trady_policy::limits::{
    DonchianAtrSpec, DonchianBar, DonchianEntryDecision, DonchianEntryRejection,
    DonchianExitReason, DonchianModelFusion, DonchianSignal, evaluate_signal, resolve_entry,
    resolve_exit, trade_plan, wilder_atr,
};

fn d(value: &str) -> Decimal {
    Decimal::from_str_exact(value).unwrap()
}

fn bars() -> Vec<DonchianBar> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 21, 0, 0).unwrap();
    (0..35)
        .map(|index| {
            let value = d("100") + Decimal::from(index as u64);
            DonchianBar {
                observed_at: start + Duration::days(index),
                open: value,
                high: value + Decimal::ONE,
                low: value - Decimal::ONE,
                close: value,
            }
        })
        .collect()
}

#[test]
fn recommended_spec_is_policy_compatible_and_model_free() {
    let spec = DonchianAtrSpec::recommended();
    let parameters = trady_policy::limits::FormulaParameters {
        version: 1,
        family: trady_policy::limits::FormulaFamily::Trend,
        entry_allowance_fraction: Decimal::ZERO,
        stop_atr_multiple: d("1.50"),
        target_net_fraction: d("0.01"),
        max_holding_sessions: 5,
        atr_lookback: 14,
        bar_seconds: 86_400,
    };
    spec.validate(&parameters, &Policy::default()).unwrap();
    assert_eq!(spec.model_fusion, DonchianModelFusion::Disabled);
}

#[test]
fn wilder_seed_and_signal_exclude_the_current_breakout_bar() {
    let spec = DonchianAtrSpec::recommended();
    let mut input = bars();
    input[34].close = d("200");
    input[34].high = d("201");
    input[34].open = d("199");
    input[34].low = d("198");
    let atr = wilder_atr(&input, 14).unwrap();
    assert!(atr > Decimal::ZERO);
    let signal = evaluate_signal(&input, &spec).unwrap().unwrap();
    assert!(signal.signal_close > signal.breakout_level);
    assert_eq!(signal.target_distance, signal.stop_distance * d("1.50"));
    assert!(signal.entry_ceiling > signal.signal_close);

    input[34].open = signal.breakout_level;
    input[34].high = signal.breakout_level + Decimal::ONE;
    input[34].low = signal.breakout_level - Decimal::ONE;
    input[34].close = signal.breakout_level;
    assert!(evaluate_signal(&input, &spec).unwrap().is_none());
}

#[test]
fn trade_plan_binds_the_declared_one_point_five_r_target() {
    let spec = DonchianAtrSpec::recommended();
    let plan = trade_plan(d("100"), d("2"), &spec).unwrap();
    assert_eq!(plan.risk_unit, d("3"));
    assert_eq!(plan.stop, d("97"));
    assert_eq!(plan.target, d("104.50"));
    assert_eq!(plan.reward_to_risk, d("1.50"));
}

#[test]
fn invalid_warmup_or_unsorted_bars_fail_closed() {
    let spec = DonchianAtrSpec::recommended();
    assert!(evaluate_signal(&bars()[..34], &spec).is_err());
    let mut bad = bars();
    bad[10].observed_at = bad[9].observed_at;
    assert!(evaluate_signal(&bad, &spec).is_err());
}

#[test]
fn opening_gap_boundary_is_inclusive_and_missing_open_is_not_a_fill() {
    let spec = DonchianAtrSpec::recommended();
    let mut input = bars();
    input[34].close = d("200");
    input[34].high = d("201");
    input[34].open = d("199");
    input[34].low = d("198");
    let signal = evaluate_signal(&input, &spec).unwrap().unwrap();

    let accepted = resolve_entry(&signal, Some(signal.entry_ceiling)).unwrap();
    assert!(matches!(accepted, DonchianEntryDecision::Accepted { .. }));

    let rejected = resolve_entry(&signal, Some(signal.entry_ceiling + Decimal::new(1, 2))).unwrap();
    assert!(matches!(
        rejected,
        DonchianEntryDecision::Rejected {
            reason: DonchianEntryRejection::OpeningGapAboveAllowance,
            ..
        }
    ));

    let missing = resolve_entry(&signal, None).unwrap();
    assert!(matches!(
        missing,
        DonchianEntryDecision::Rejected {
            reason: DonchianEntryRejection::MissingNextRegularSessionOpen,
            ..
        }
    ));
}

#[test]
fn exit_resolution_is_gap_safe_stop_first_and_expires_on_session_five() {
    let spec = DonchianAtrSpec::recommended();
    let plan = trade_plan(d("100"), d("2"), &spec).unwrap();

    let gap = DonchianBar {
        observed_at: Utc.with_ymd_and_hms(2026, 2, 1, 21, 0, 0).unwrap(),
        open: d("95"),
        high: d("99"),
        low: d("94"),
        close: d("96"),
    };
    let gap_exit = resolve_exit(&plan, &gap, 1, &spec).unwrap().unwrap();
    assert_eq!(gap_exit.reason, DonchianExitReason::GapThroughStop);
    assert_eq!(gap_exit.executable_price, d("95"));

    let same_bar = DonchianBar {
        observed_at: Utc.with_ymd_and_hms(2026, 2, 2, 21, 0, 0).unwrap(),
        open: d("100"),
        high: d("105"),
        low: d("96"),
        close: d("102"),
    };
    let same_bar_exit = resolve_exit(&plan, &same_bar, 2, &spec).unwrap().unwrap();
    assert_eq!(same_bar_exit.reason, DonchianExitReason::StopSameBarFirst);
    assert_eq!(same_bar_exit.executable_price, plan.stop);

    for opening in [plan.target, plan.target + d("1")] {
        let target_gap = DonchianBar {
            open: opening,
            high: opening + d("1"),
            low: plan.stop - d("1"),
            close: plan.entry,
            ..same_bar.clone()
        };
        let exit = resolve_exit(&plan, &target_gap, 2, &spec).unwrap().unwrap();
        assert_eq!(exit.reason, DonchianExitReason::Target);
        assert_eq!(exit.executable_price, opening);
        assert_eq!(
            exit.price_source,
            trady_policy::limits::DonchianExecutionPriceSource::HoldingBarOpen
        );
    }

    let open = DonchianBar {
        observed_at: Utc.with_ymd_and_hms(2026, 2, 3, 21, 0, 0).unwrap(),
        open: d("100"),
        high: d("101"),
        low: d("99"),
        close: d("100.5"),
    };
    assert!(resolve_exit(&plan, &open, 4, &spec).unwrap().is_none());
    let expiry = resolve_exit(&plan, &open, 5, &spec).unwrap().unwrap();
    assert_eq!(expiry.reason, DonchianExitReason::FifthHoldingSessionClose);
    assert_eq!(expiry.executable_price, open.close);
}

#[allow(dead_code)]
fn _assert_signal_shape(signal: &DonchianSignal) {
    assert!(signal.atr > Decimal::ZERO);
}

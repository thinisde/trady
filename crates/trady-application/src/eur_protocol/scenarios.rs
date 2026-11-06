use super::*;
use chrono::Duration;

thread_local! { static TRACE:std::cell::RefCell<Vec<ProtocolRecord>>=const {std::cell::RefCell::new(Vec::new())}; }

#[derive(Default)]
struct MemoryJournal {
    records: Vec<ProtocolRecord>,
    fail: bool,
}
impl ProtocolJournal for MemoryJournal {
    fn load(&mut self, _: &str) -> Result<Vec<ProtocolRecord>> {
        Ok(self.records.clone())
    }
    fn append(&mut self, revision: u64, record: &ProtocolRecord) -> Result<()> {
        require(
            !self.fail && revision == self.records.len() as u64,
            "journal commit failed",
        )?;
        self.records.push(record.clone());
        TRACE.with(|trace| trace.borrow_mut().push(record.clone()));
        Ok(())
    }
}
pub(super) fn context() -> ProtocolContext {
    let at = "2026-11-04T09:00:00Z".parse().unwrap();
    ProtocolContext {
        runtime_mode: trady_domain::ExecutionStage::Replay,
        policy_revision: "eur_intraday_v1".into(),
        intent_id: "fixture_entry".into(),
        account_id: "replay_eur_protocol".into(),
        broker_account_id: 52211463,
        instrument_id: "AIR_PA_EQ".into(),
        broker_ticker: "AIRp_EQ".into(),
        isin: "NL0000235190".into(),
        policy_hash: "a".repeat(64),
        configuration_hash: "b".repeat(64),
        evidence_ids: vec!["synthetic_protocol_fixture_not_provider_evidence".into()],
        quantity: Decimal::new(2, 0),
        entry_limit: Decimal::new(100, 0),
        initial_stop: Decimal::new(98, 0),
        target: Decimal::new(103, 0),
        created_at: at,
        flatten_at: at + Duration::hours(7),
        max_unprotected_seconds: 5,
        research_cycle_id: None,
        entry_valid_until: None,
    }
}
fn step(j: &mut MemoryJournal, input: ProtocolInput) -> ProtocolEffect {
    let c = context();
    let at = c.created_at + Duration::milliseconds(j.records.len() as i64 * 10);
    let result = advance(j, &c, format!("event_{}", j.records.len()), input, at).unwrap();
    let encoded = serde_json::to_vec(&j.records).unwrap();
    let recovered: Vec<ProtocolRecord> = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        recover(&recovered).unwrap(),
        j.records.last().unwrap().state
    );
    result
}
fn observation(
    leg: OrderLeg,
    id: &str,
    quantity: Decimal,
    filled: Decimal,
    status: OrderStatus,
    holding: Decimal,
) -> ProtocolInput {
    ProtocolInput::Observed {
        leg,
        broker_id: id.into(),
        broker_ticker: "AIRp_EQ".into(),
        quantity,
        filled,
        status,
        holding_quantity: holding,
        pending_present: !status.terminal(),
        history_complete: status.terminal(),
        stop_price: if leg == OrderLeg::Protection {
            Some(Decimal::new(98, 0))
        } else {
            None
        },
    }
}
fn entered() -> MemoryJournal {
    let mut j = MemoryJournal::default();
    step(&mut j, ProtocolInput::Reserve);
    step(&mut j, ProtocolInput::DispatchEntry);
    step(
        &mut j,
        ProtocolInput::Acknowledged {
            leg: OrderLeg::Entry,
            broker_id: "1001".into(),
        },
    );
    step(
        &mut j,
        observation(
            OrderLeg::Entry,
            "1001",
            Decimal::new(2, 0),
            Decimal::new(2, 0),
            OrderStatus::Filled,
            Decimal::new(2, 0),
        ),
    );
    j
}
fn protected() -> MemoryJournal {
    let mut j = entered();
    step(&mut j, ProtocolInput::DispatchProtection);
    step(
        &mut j,
        ProtocolInput::Acknowledged {
            leg: OrderLeg::Protection,
            broker_id: "1002".into(),
        },
    );
    step(
        &mut j,
        observation(
            OrderLeg::Protection,
            "1002",
            Decimal::new(2, 0),
            Decimal::ZERO,
            OrderStatus::Open,
            Decimal::new(2, 0),
        ),
    );
    j
}
fn exit_ready() -> MemoryJournal {
    let mut j = protected();
    step(
        &mut j,
        ProtocolInput::RequestExit {
            reason: "SAME_DAY_FLATTEN".into(),
        },
    );
    step(
        &mut j,
        ProtocolInput::Cancel {
            leg: OrderLeg::Protection,
        },
    );
    step(
        &mut j,
        ProtocolInput::CancellationAcknowledged {
            leg: OrderLeg::Protection,
        },
    );
    assert_eq!(
        j.records.last().unwrap().state.phase,
        ProtocolPhase::StopCancelPending
    );
    step(
        &mut j,
        observation(
            OrderLeg::Protection,
            "1002",
            Decimal::new(2, 0),
            Decimal::ZERO,
            OrderStatus::Cancelled,
            Decimal::new(2, 0),
        ),
    );
    j
}

#[cfg_attr(test, test)]
fn dispatch_commits_unknown_before_returning_effect_and_cannot_repeat_after_restart() {
    let c = context();
    let mut j = MemoryJournal::default();
    step(&mut j, ProtocolInput::Reserve);
    assert!(matches!(
        step(&mut j, ProtocolInput::DispatchEntry),
        ProtocolEffect::SubmitEntry { .. }
    ));
    assert_eq!(
        j.records.last().unwrap().state.phase,
        ProtocolPhase::EntryUnknown
    );
    assert!(matches!(
        advance(
            &mut j,
            &c,
            "event_1".into(),
            ProtocolInput::DispatchEntry,
            c.created_at
        ),
        Ok(ProtocolEffect::RequireOperator { .. })
    ));
    assert!(
        advance(
            &mut j,
            &c,
            "new_retry".into(),
            ProtocolInput::DispatchEntry,
            c.created_at
        )
        .is_err()
    );
    j.fail = true;
    assert!(
        advance(
            &mut j,
            &c,
            "ack".into(),
            ProtocolInput::Acknowledged {
                leg: OrderLeg::Entry,
                broker_id: "1001".into()
            },
            c.created_at + Duration::seconds(1)
        )
        .is_err()
    );
    assert!(
        j.records
            .last()
            .unwrap()
            .state
            .entry
            .as_ref()
            .unwrap()
            .broker_id
            .is_none()
    );
}
#[cfg_attr(test, test)]
fn partial_entry_cancellation_ack_is_not_terminal_and_late_fill_sizes_protection() {
    let mut j = MemoryJournal::default();
    step(&mut j, ProtocolInput::Reserve);
    step(&mut j, ProtocolInput::DispatchEntry);
    step(
        &mut j,
        ProtocolInput::Acknowledged {
            leg: OrderLeg::Entry,
            broker_id: "1001".into(),
        },
    );
    step(
        &mut j,
        observation(
            OrderLeg::Entry,
            "1001",
            Decimal::new(2, 0),
            Decimal::ONE,
            OrderStatus::PartiallyFilled,
            Decimal::ONE,
        ),
    );
    step(
        &mut j,
        ProtocolInput::Cancel {
            leg: OrderLeg::Entry,
        },
    );
    step(
        &mut j,
        ProtocolInput::CancellationAcknowledged {
            leg: OrderLeg::Entry,
        },
    );
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "premature_stop".into(),
            ProtocolInput::DispatchProtection,
            c.created_at + Duration::seconds(1)
        )
        .is_err()
    );
    step(
        &mut j,
        observation(
            OrderLeg::Entry,
            "1001",
            Decimal::new(2, 0),
            Decimal::new(15, 1),
            OrderStatus::Cancelled,
            Decimal::new(15, 1),
        ),
    );
    assert_eq!(
        step(&mut j, ProtocolInput::DispatchProtection),
        ProtocolEffect::SubmitProtection {
            quantity: Decimal::new(15, 1),
            stop: Decimal::new(98, 0)
        }
    );
}
#[cfg_attr(test, test)]
fn stop_rejection_flattens_filled_quantity_and_expired_entry_authority_does_not_block_exit() {
    let mut j = entered();
    step(&mut j, ProtocolInput::DispatchProtection);
    step(
        &mut j,
        ProtocolInput::Rejected {
            leg: OrderLeg::Protection,
        },
    );
    assert_eq!(
        j.records.last().unwrap().state.phase,
        ProtocolPhase::ExitPrepared
    );
    let c = context();
    let at = c.flatten_at + Duration::seconds(1);
    advance(
        &mut j,
        &c,
        "fresh_holding".into(),
        ProtocolInput::HoldingReconciled {
            quantity: Decimal::new(2, 0),
            pending_order_ids: vec![],
            history_complete: true,
        },
        at,
    )
    .unwrap();
    assert_eq!(
        advance(
            &mut j,
            &c,
            "emergency".into(),
            ProtocolInput::DispatchExit,
            at
        )
        .unwrap(),
        ProtocolEffect::SubmitFlatten {
            quantity: Decimal::new(2, 0)
        }
    );
}
#[cfg_attr(test, test)]
fn partial_flatten_cancels_and_reconciles_before_selling_only_the_remaining_quantity() {
    let mut j = exit_ready();
    step(&mut j, ProtocolInput::DispatchExit);
    step(
        &mut j,
        ProtocolInput::Acknowledged {
            leg: OrderLeg::Exit,
            broker_id: "1003".into(),
        },
    );
    step(
        &mut j,
        observation(
            OrderLeg::Exit,
            "1003",
            Decimal::new(2, 0),
            Decimal::new(75, 2),
            OrderStatus::PartiallyFilled,
            Decimal::new(125, 2),
        ),
    );
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "unsafe_retry".into(),
            ProtocolInput::DispatchExit,
            c.created_at + Duration::seconds(1)
        )
        .is_err()
    );
    step(
        &mut j,
        ProtocolInput::Cancel {
            leg: OrderLeg::Exit,
        },
    );
    step(
        &mut j,
        ProtocolInput::CancellationAcknowledged {
            leg: OrderLeg::Exit,
        },
    );
    step(
        &mut j,
        observation(
            OrderLeg::Exit,
            "1003",
            Decimal::new(2, 0),
            Decimal::new(9, 1),
            OrderStatus::Cancelled,
            Decimal::new(11, 1),
        ),
    );
    assert_eq!(
        step(&mut j, ProtocolInput::DispatchExit),
        ProtocolEffect::SubmitFlatten {
            quantity: Decimal::new(11, 1)
        }
    );
    step(
        &mut j,
        ProtocolInput::Acknowledged {
            leg: OrderLeg::Exit,
            broker_id: "1004".into(),
        },
    );
    step(
        &mut j,
        observation(
            OrderLeg::Exit,
            "1004",
            Decimal::new(11, 1),
            Decimal::new(11, 1),
            OrderStatus::Filled,
            Decimal::ZERO,
        ),
    );
    assert_eq!(j.records.last().unwrap().state.phase, ProtocolPhase::Flat);
}
#[cfg_attr(test, test)]
fn disconnect_preserves_protection_and_requires_reconciliation_before_any_mutation() {
    let mut j = protected();
    step(&mut j, ProtocolInput::Disconnected);
    assert!(j.records.last().unwrap().state.protection.is_some());
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "blind_cancel".into(),
            ProtocolInput::Cancel {
                leg: OrderLeg::Protection
            },
            c.created_at + Duration::seconds(1)
        )
        .is_err()
    );
    step(
        &mut j,
        ProtocolInput::HoldingReconciled {
            quantity: Decimal::new(2, 0),
            pending_order_ids: vec!["1002".into()],
            history_complete: true,
        },
    );
    assert!(!j.records.last().unwrap().state.reconciliation_required);
}
#[cfg_attr(test, test)]
fn stop_widening_and_forged_restart_records_are_rejected() {
    let mut j = protected();
    step(
        &mut j,
        ProtocolInput::TightenStop {
            price: Decimal::new(99, 0),
        },
    );
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "widen".into(),
            ProtocolInput::TightenStop {
                price: Decimal::new(98, 0)
            },
            c.created_at + Duration::seconds(1)
        )
        .is_err()
    );
    let mut forged = j.records.clone();
    forged.last_mut().unwrap().state.holding_quantity = Decimal::ZERO;
    assert!(recover(&forged).is_err());
    assert_eq!(
        j.records
            .last()
            .unwrap()
            .state
            .protection
            .as_ref()
            .unwrap()
            .stop_price,
        Some(Decimal::new(98, 0))
    );
}
#[cfg_attr(test, test)]
fn unprotected_exposure_timer_does_not_reset_on_repeated_broker_observations() {
    let mut j = entered();
    let c = context();
    let since = j.records.last().unwrap().state.unprotected_since.unwrap();
    advance(
        &mut j,
        &c,
        "reconciled_again".into(),
        observation(
            OrderLeg::Entry,
            "1001",
            Decimal::new(2, 0),
            Decimal::new(2, 0),
            OrderStatus::Filled,
            Decimal::new(2, 0),
        ),
        since + Duration::seconds(4),
    )
    .unwrap();
    advance(
        &mut j,
        &c,
        "timeout".into(),
        ProtocolInput::Tick,
        since + Duration::seconds(5),
    )
    .unwrap();
    assert_eq!(
        j.records.last().unwrap().state.phase,
        ProtocolPhase::ExitPrepared
    );
}

#[cfg_attr(test, test)]
fn unknown_protection_cannot_be_retried_or_flattened_without_exact_id_recovery() {
    let mut j = entered();
    step(&mut j, ProtocolInput::DispatchProtection);
    let c = context();
    let at = c.created_at + Duration::seconds(1);
    assert!(matches!(
        recover(&j.records).unwrap().recovery_effect(),
        ProtocolEffect::RequireOperator { .. }
    ));
    assert!(
        advance(
            &mut j,
            &c,
            "duplicate_stop".into(),
            ProtocolInput::DispatchProtection,
            at
        )
        .is_err()
    );
    assert!(
        advance(
            &mut j,
            &c,
            "unsafe_flatten".into(),
            ProtocolInput::DispatchExit,
            at
        )
        .is_err()
    );
}
#[cfg_attr(test, test)]
fn unknown_flatten_is_never_retried_after_process_restart() {
    let mut j = exit_ready();
    step(&mut j, ProtocolInput::DispatchExit);
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "repeat_sell".into(),
            ProtocolInput::DispatchExit,
            c.created_at + Duration::seconds(1)
        )
        .is_err()
    );
    assert!(matches!(
        recover(&j.records).unwrap().recovery_effect(),
        ProtocolEffect::RequireOperator { .. }
    ));
}
#[cfg_attr(test, test)]
fn conflicting_history_and_holding_halt_without_a_sell_effect() {
    let mut j = entered();
    step(
        &mut j,
        observation(
            OrderLeg::Entry,
            "1001",
            Decimal::new(2, 0),
            Decimal::new(2, 0),
            OrderStatus::Filled,
            Decimal::ONE,
        ),
    );
    assert_eq!(
        j.records.last().unwrap().state.phase,
        ProtocolPhase::Recovery
    );
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "conflicting_sell".into(),
            ProtocolInput::DispatchExit,
            c.created_at + Duration::seconds(1)
        )
        .is_err()
    );
}
#[cfg_attr(test, test)]
fn rotation_cannot_dispatch_an_overlapping_entry() {
    let mut j = protected();
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "rotation".into(),
            ProtocolInput::DispatchEntry,
            c.created_at + Duration::seconds(1)
        )
        .is_err()
    );
    assert_eq!(
        j.records.last().unwrap().state.phase,
        ProtocolPhase::Protected
    );
}
#[cfg_attr(test, test)]
fn flatten_deadline_prevents_entry_after_rehydration() {
    let mut j = MemoryJournal::default();
    step(&mut j, ProtocolInput::Reserve);
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "late_entry".into(),
            ProtocolInput::DispatchEntry,
            c.flatten_at
        )
        .is_err()
    );
    advance(
        &mut j,
        &c,
        "deadline".into(),
        ProtocolInput::Tick,
        c.flatten_at,
    )
    .unwrap();
    assert_eq!(recover(&j.records).unwrap().phase, ProtocolPhase::Flat);
}
#[cfg_attr(test, test)]
fn failed_entry_commit_returns_no_effect_and_preserves_the_reserved_intent() {
    let mut j = MemoryJournal::default();
    step(&mut j, ProtocolInput::Reserve);
    j.fail = true;
    let c = context();
    assert!(
        advance(
            &mut j,
            &c,
            "lost_commit".into(),
            ProtocolInput::DispatchEntry,
            c.created_at
        )
        .is_err()
    );
    assert_eq!(j.records.len(), 1);
    assert!(j.records[0].state.entry.is_none());
}
#[cfg_attr(test, test)]
fn partial_stop_fill_keeps_remaining_quantity_protected() {
    let mut j = protected();
    step(
        &mut j,
        observation(
            OrderLeg::Protection,
            "1002",
            Decimal::new(2, 0),
            Decimal::new(5, 1),
            OrderStatus::PartiallyFilled,
            Decimal::new(15, 1),
        ),
    );
    assert_eq!(
        j.records.last().unwrap().state.phase,
        ProtocolPhase::Protected
    );
    assert!(j.records.last().unwrap().state.unprotected_since.is_none());
}
#[cfg_attr(test, test)]
fn adverse_news_uses_deterministic_exit_without_model_availability() {
    let mut j = protected();
    step(
        &mut j,
        ProtocolInput::RequestExit {
            reason: "ADVERSE_NEWS".into(),
        },
    );
    assert_eq!(
        j.records.last().unwrap().state.phase,
        ProtocolPhase::StopCancelPending
    );
    assert!(
        !j.records
            .last()
            .unwrap()
            .state
            .protection
            .as_ref()
            .unwrap()
            .reconciled_terminal
    );
}

/// Fixed synthetic fixtures exercise the reducer, not a broker or real quote
/// entitlement. Their ledger is always labelled separately from captured inputs.
pub fn run_all() -> serde_json::Value {
    let cases: [(&str, fn()); 15] = [
        (
            "unknown_entry_restart",
            dispatch_commits_unknown_before_returning_effect_and_cannot_repeat_after_restart,
        ),
        (
            "partial_entry_and_cancellation_race",
            partial_entry_cancellation_ack_is_not_terminal_and_late_fill_sizes_protection,
        ),
        (
            "protection_rejected_emergency_exit",
            stop_rejection_flattens_filled_quantity_and_expired_entry_authority_does_not_block_exit,
        ),
        (
            "partial_flatten_no_oversell",
            partial_flatten_cancels_and_reconciles_before_selling_only_the_remaining_quantity,
        ),
        (
            "broker_disconnect",
            disconnect_preserves_protection_and_requires_reconciliation_before_any_mutation,
        ),
        (
            "dynamic_stop_never_widens",
            stop_widening_and_forged_restart_records_are_rejected,
        ),
        (
            "unprotected_exposure_timeout",
            unprotected_exposure_timer_does_not_reset_on_repeated_broker_observations,
        ),
        (
            "unknown_protection",
            unknown_protection_cannot_be_retried_or_flattened_without_exact_id_recovery,
        ),
        (
            "unknown_flatten",
            unknown_flatten_is_never_retried_after_process_restart,
        ),
        (
            "conflicting_reconciliation",
            conflicting_history_and_holding_halt_without_a_sell_effect,
        ),
        (
            "rotation_no_overlap",
            rotation_cannot_dispatch_an_overlapping_entry,
        ),
        (
            "same_day_entry_deadline",
            flatten_deadline_prevents_entry_after_rehydration,
        ),
        (
            "postgres_commit_failure",
            failed_entry_commit_returns_no_effect_and_preserves_the_reserved_intent,
        ),
        (
            "partial_stop_fill",
            partial_stop_fill_keeps_remaining_quantity_protected,
        ),
        (
            "adverse_news_without_llm",
            adverse_news_uses_deterministic_exit_without_model_availability,
        ),
    ];
    let mut scenarios = Vec::new();
    for (name, case) in cases {
        TRACE.with(|trace| trace.borrow_mut().clear());
        let passed = std::panic::catch_unwind(case).is_ok();
        let records = TRACE.with(|trace| trace.borrow().clone());
        scenarios.push(serde_json::json!({"scenario":name,"passed":passed,"classification":"SYNTHETIC_PROTOCOL_FIXTURE",
            "restart_after_every_event":true,"records":records}));
    }
    serde_json::json!({"schema_version":1,"classification":"SYNTHETIC_PROTOCOL_REGRESSION",
        "passed":scenarios.iter().all(|s|s["passed"]==true),"scenarios":scenarios,
        "provider_calls":0,"broker_mutations":0,"broker_capability_proven":false})
}

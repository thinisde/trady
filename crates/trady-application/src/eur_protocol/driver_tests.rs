use super::{driver::*, *};
use crate::broker::{
    BrokerFailure, BrokerFailureKind, BrokerOrderRequest, BrokerResult, CancellationReceipt,
    OrderKind,
};
use std::{cell::RefCell, rc::Rc};
use trady_domain::{BrokerOrder, Side};

#[derive(Default)]
struct Journal {
    records: Vec<ProtocolRecord>,
    fail_dispatch: bool,
    observations: usize,
    trace: Rc<RefCell<Vec<String>>>,
}
impl ProtocolJournal for Journal {
    fn record_observation(&mut self, _: &ProtocolContext, _: &BrokerObservation) -> Result<()> {
        self.observations += 1;
        Ok(())
    }
    fn record_protection_confirmation(
        &mut self,
        _: &ProtocolContext,
        _: &ProtectionConfirmation,
    ) -> Result<()> {
        self.trace
            .borrow_mut()
            .push("persist_exact_stop_confirmation".into());
        Ok(())
    }
    fn load(&mut self, _: &str) -> Result<Vec<ProtocolRecord>> {
        Ok(self.records.clone())
    }
    fn append(&mut self, expected: u64, r: &ProtocolRecord) -> Result<()> {
        require(
            !(self.fail_dispatch && matches!(r.input, ProtocolInput::DispatchEntry)),
            "injected commit failure",
        )?;
        assert_eq!(expected, self.records.len() as u64);
        self.trace
            .borrow_mut()
            .push(format!("commit:{:?}", r.input));
        self.records.push(r.clone());
        Ok(())
    }
}
struct Broker {
    at: DateTime<Utc>,
    entry_filled: Decimal,
    entry_terminal: bool,
    stop_filled: Decimal,
    stop_terminal: bool,
    exit_filled: Decimal,
    unknown_entry: bool,
    reject_stop: bool,
    fail_cancel_prerequisites: bool,
    fail_cancel: bool,
    history_coverage_exhausted: bool,
    confirmation_delay_ms: Option<i64>,
    actual_entry_fill: Option<DateTime<Utc>>,
    fast_entry_prior: Option<DateTime<Utc>>,
    fast_entry_unrelated: bool,
    submitted: Vec<BrokerOrderRequest>,
    submission_deadlines: Vec<DateTime<Utc>>,
    cancelled: Vec<String>,
    trace: Rc<RefCell<Vec<String>>>,
}
impl ProtocolBroker for Broker {
    fn prepare_cancel(&mut self, _: &ProtocolState, _: OrderLeg) -> BrokerResult<()> {
        if self.fail_cancel_prerequisites {
            Err(BrokerFailure::new(BrokerFailureKind::RateLimited))
        } else {
            Ok(())
        }
    }
    fn now(&self) -> DateTime<Utc> {
        self.at
    }
    fn confirm_entry(&mut self, s: &ProtocolState) -> BrokerResult<Option<BrokerObservation>> {
        let Some(prior) = self.fast_entry_prior else {
            return Ok(None);
        };
        if s.entry.is_none() || s.protection.is_some() {
            return Ok(None);
        }
        let mut view = self.observe(s)?;
        view.entry_confirmation_from_full_view_at = Some(prior);
        if self.fast_entry_unrelated {
            view.pending_order_ids.push("foreign_order".into());
        }
        Ok(Some(view))
    }
    fn observe(&mut self, s: &ProtocolState) -> BrokerResult<BrokerObservation> {
        let holding = self.entry_filled - self.stop_filled - self.exit_filled;
        let mut exact = vec![];
        for (leg, p) in [
            (OrderLeg::Entry, s.entry.as_ref()),
            (OrderLeg::Protection, s.protection.as_ref()),
            (OrderLeg::Exit, s.exits.last()),
        ] {
            if let Some(p) = p
                && let Some(id) = &p.broker_id
            {
                let (filled, terminal) = match leg {
                    OrderLeg::Entry => (self.entry_filled, self.entry_terminal),
                    OrderLeg::Protection => (self.stop_filled, self.stop_terminal),
                    OrderLeg::Exit => (self.exit_filled, self.exit_filled == p.quantity),
                };
                let status = if filled == p.quantity {
                    OrderStatus::Filled
                } else if terminal {
                    OrderStatus::Cancelled
                } else if filled > Decimal::ZERO {
                    OrderStatus::PartiallyFilled
                } else {
                    OrderStatus::Open
                };
                exact.push(ExactOrderObservation {
                    leg,
                    order: BrokerOrder {
                        broker_order_id: id.clone(),
                        client_intent_id: None,
                        instrument_id: trady_domain::InstrumentId::new(
                            s.context.broker_ticker.clone(),
                        )
                        .unwrap(),
                        side: if leg == OrderLeg::Entry {
                            Side::Buy
                        } else {
                            Side::Sell
                        },
                        quantity: p.quantity,
                        filled_quantity: filled,
                        status,
                    },
                    stop_price: p.stop_price,
                    pending_present: !status.terminal(),
                });
            }
        }
        let pending = exact
            .iter()
            .filter(|e| e.pending_present)
            .map(|e| e.order.broker_order_id.clone())
            .collect();
        let reserved = exact
            .iter()
            .filter(|e| e.leg == OrderLeg::Protection && e.pending_present)
            .map(|e| e.order.quantity - e.order.filled_quantity)
            .sum::<Decimal>();
        Ok(BrokerObservation {
            holding_quantity: holding,
            available_quantity: holding - reserved,
            pending_order_ids: pending,
            exact_orders: exact,
            history_complete: !self.history_coverage_exhausted,
            history_coverage_exhausted: self.history_coverage_exhausted,
            observed_at: self.at,
            entry_first_filled_at: if self.entry_filled > Decimal::ZERO {
                self.actual_entry_fill
            } else {
                None
            },
            entry_confirmation_from_full_view_at: None,
            evidence: serde_json::json!({"classification":"SYNTHETIC_DRIVER_FIXTURE"}),
        })
    }
    fn confirm_protection(
        &mut self,
        s: &ProtocolState,
    ) -> BrokerResult<Option<ProtectionConfirmation>> {
        let Some(delay) = self.confirmation_delay_ms else {
            return Ok(None);
        };
        let view = self.observe(s)?;
        self.at += chrono::Duration::milliseconds(delay);
        let exact = view
            .exact_orders
            .into_iter()
            .find(|e| e.leg == OrderLeg::Protection)
            .unwrap();
        Ok(Some(ProtectionConfirmation {
            order: exact.order,
            stop_price: exact.stop_price.unwrap(),
            holding_quantity: view.holding_quantity,
            observed_at: self.at,
            evidence: serde_json::json!({"classification":"SYNTHETIC_DRIVER_FIXTURE"}),
        }))
    }
    fn submit(
        &mut self,
        r: &BrokerOrderRequest,
        deadline: DateTime<Utc>,
    ) -> BrokerResult<BrokerOrder> {
        self.submission_deadlines.push(deadline);
        self.trace.borrow_mut().push("send".into());
        self.submitted.push(r.clone());
        if self.unknown_entry && r.side == Side::Buy {
            let mut e = BrokerFailure::new(BrokerFailureKind::Transport);
            e.outcome_unknown = true;
            return Err(e);
        }
        if self.reject_stop && matches!(r.kind, OrderKind::Stop { .. }) {
            return Err(BrokerFailure::new(BrokerFailureKind::Rejected));
        }
        Ok(BrokerOrder {
            broker_order_id: (1000 + self.submitted.len()).to_string(),
            client_intent_id: None,
            instrument_id: r.instrument_id.clone(),
            side: r.side,
            quantity: r.quantity,
            filled_quantity: Decimal::ZERO,
            status: OrderStatus::Open,
        })
    }
    fn cancel(
        &mut self,
        _: &trady_domain::AccountId,
        id: &str,
    ) -> BrokerResult<CancellationReceipt> {
        self.cancelled.push(id.into());
        if self.fail_cancel {
            let mut error = BrokerFailure::new(BrokerFailureKind::Transport);
            error.outcome_unknown = true;
            return Err(error);
        }
        Ok(CancellationReceipt {
            broker_order_id: id.into(),
        })
    }
}
fn fixture() -> (ProtocolContext, ProtocolDriver<Journal, Broker>) {
    let c = super::scenarios::context();
    let j = Journal::default();
    let b = Broker {
        at: c.created_at,
        entry_filled: Decimal::ZERO,
        entry_terminal: false,
        stop_filled: Decimal::ZERO,
        stop_terminal: false,
        exit_filled: Decimal::ZERO,
        unknown_entry: false,
        reject_stop: false,
        fail_cancel_prerequisites: false,
        fail_cancel: false,
        history_coverage_exhausted: false,
        confirmation_delay_ms: None,
        actual_entry_fill: None,
        fast_entry_prior: None,
        fast_entry_unrelated: false,
        submitted: vec![],
        submission_deadlines: vec![],
        cancelled: vec![],
        trace: j.trace.clone(),
    };
    (
        c,
        ProtocolDriver {
            journal: j,
            broker: b,
        },
    )
}
fn drive(
    d: &mut ProtocolDriver<Journal, Broker>,
    c: &ProtocolContext,
    exit: Option<&str>,
) -> ProtocolState {
    d.broker.at += chrono::Duration::milliseconds(20);
    d.drive_once(c, d.broker.at, exit).unwrap()
}

#[test]
fn interrupted_cancellation_requires_review_and_can_resume_once_after_restart() {
    for dispatched in [false, true] {
        let (c, mut d) = fixture();
        drive(&mut d, &c, None);
        d.broker.entry_filled = Decimal::ONE;
        if dispatched {
            d.broker.fail_cancel = true;
            drive(&mut d, &c, None);
        } else {
            // Simulate death between the durable marker and the DELETE.
            advance(
                &mut d.journal,
                &c,
                "interrupted_cancel".into(),
                ProtocolInput::Cancel {
                    leg: OrderLeg::Entry,
                },
                d.broker.at,
            )
            .unwrap();
        }
        let attempts = d.broker.cancelled.len();
        d.journal.records =
            serde_json::from_slice(&serde_json::to_vec(&d.journal.records).unwrap()).unwrap();
        let state = drive(&mut d, &c, None);
        assert_eq!(d.broker.cancelled.len(), attempts);
        assert!(
            matches!(d.recovery_action(&state).unwrap(), ProtocolEffect::RequireOperator { reason } if reason.contains("CANCELLATION_OUTCOME_REQUIRES_REVIEW"))
        );
        assert!(
            state
                .transition(
                    "wrong_order".into(),
                    ProtocolInput::ReviewedCancellationRetry {
                        leg: OrderLeg::Entry,
                        broker_id: "foreign_order".into(),
                        review_reference: "operator_review".into(),
                    },
                    d.broker.at
                )
                .is_err()
        );
        advance(
            &mut d.journal,
            &c,
            "reviewed_cancel_retry".into(),
            ProtocolInput::ReviewedCancellationRetry {
                leg: OrderLeg::Entry,
                broker_id: "1001".into(),
                review_reference: "operator_review".into(),
            },
            d.broker.at,
        )
        .unwrap();
        d.broker.fail_cancel = false;
        let state = drive(&mut d, &c, None);
        assert_eq!(d.broker.cancelled.len(), attempts + 1);
        assert!(!matches!(
            d.recovery_action(&state).unwrap(),
            ProtocolEffect::RequireOperator { .. }
        ));
        drive(&mut d, &c, None);
        assert_eq!(d.broker.cancelled.len(), attempts + 1);
        d.broker.entry_terminal = true;
        assert_eq!(
            drive(&mut d, &c, None).phase,
            ProtocolPhase::ProtectionUnknown
        );
    }
}

#[test]
fn overdue_acknowledged_cancellation_stays_reconcilable_and_requires_review() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = Decimal::ONE;
    drive(&mut d, &c, None);
    d.broker.at += chrono::Duration::seconds(11);
    let state = drive(&mut d, &c, None);
    assert!(matches!(
        d.recovery_action(&state).unwrap(),
        ProtocolEffect::RequireOperator { .. }
    ));
    assert_eq!(d.broker.cancelled.len(), 1);
    d.broker.entry_terminal = true;
    let state = drive(&mut d, &c, None);
    assert!(!matches!(
        d.recovery_action(&state).unwrap(),
        ProtocolEffect::RequireOperator { .. }
    ));
}

#[test]
fn incomplete_exact_history_requires_review_and_keeps_the_working_stop() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Protected);
    d.broker.history_coverage_exhausted = true;
    let state = drive(&mut d, &c, Some("OWNER_EXIT"));
    assert!(state.reconciliation_required);
    assert!(
        matches!(d.recovery_action(&state).unwrap(), ProtocolEffect::RequireOperator { reason } if reason == "EXACT_ORDER_HISTORY_COVERAGE_EXHAUSTED")
    );
    assert!(d.broker.cancelled.is_empty());
    d.broker.history_coverage_exhausted = false;
    drive(&mut d, &c, None);
    assert_eq!(d.broker.cancelled, vec!["1002"]);
}

#[test]
fn reviewed_retry_waits_for_complete_history_and_accepts_a_late_terminal_order() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = Decimal::ONE;
    d.broker.fail_cancel = true;
    drive(&mut d, &c, None);
    advance(
        &mut d.journal,
        &c,
        "reviewed_retry".into(),
        ProtocolInput::ReviewedCancellationRetry {
            leg: OrderLeg::Entry,
            broker_id: "1001".into(),
            review_reference: "review".into(),
        },
        d.broker.at,
    )
    .unwrap();
    d.broker.fail_cancel = false;
    d.broker.history_coverage_exhausted = true;
    drive(&mut d, &c, None);
    assert_eq!(d.broker.cancelled.len(), 1);
    d.broker.history_coverage_exhausted = false;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    assert_eq!(
        d.broker.cancelled.len(),
        1,
        "terminal observation wins over the reviewed retry"
    );
}

#[test]
fn unchanged_protected_observations_do_not_grow_the_state_journal() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Protected);
    let count = d.journal.records.len();
    let observations = d.journal.observations;
    for _ in 0..200 {
        assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Protected);
    }
    assert_eq!(d.journal.records.len(), count);
    assert_eq!(d.journal.observations, observations + 200);
    d.journal.records =
        serde_json::from_slice(&serde_json::to_vec(&d.journal.records).unwrap()).unwrap();
    drive(&mut d, &c, Some("OWNER_EXIT"));
    assert_eq!(d.broker.cancelled, vec!["1002"]);
    d.broker.stop_terminal = true;
    drive(&mut d, &c, None);
    d.broker.exit_filled = c.quantity;
    let final_state = drive(&mut d, &c, None);
    assert_eq!(final_state.phase, ProtocolPhase::Flat);
    assert_eq!(final_state.reason.as_deref(), Some("OWNER_EXIT"));
    assert!(!final_state.reconciliation_required);
    assert_eq!(recover(&d.journal.records).unwrap(), final_state);
}

#[test]
fn existing_history_beyond_the_old_limit_remains_recoverable_and_can_flatten() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    let mut state = drive(&mut d, &c, None);
    // Legacy loops persisted even identical observations. Build a valid old
    // chain directly without repeatedly replaying every preceding prefix.
    while d.journal.records.len() <= 10_000 {
        let record = state
            .transition(
                format!("legacy_observation_{}", state.revision),
                ProtocolInput::HoldingReconciled {
                    quantity: c.quantity,
                    pending_order_ids: vec!["1002".into()],
                    history_complete: true,
                },
                d.broker.at,
            )
            .unwrap();
        state = record.state.clone();
        d.journal.records.push(record);
    }
    assert_eq!(recover(&d.journal.records).unwrap(), state);
    drive(&mut d, &c, Some("OWNER_EXIT"));
    d.broker.stop_terminal = true;
    drive(&mut d, &c, None);
    d.broker.exit_filled = c.quantity;
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Flat);
}

#[test]
fn entry_quote_expiry_caps_only_entry_transport() {
    let (mut c, mut d) = fixture();
    c.entry_valid_until = Some(c.created_at + chrono::Duration::seconds(3));
    drive(&mut d, &c, None);
    assert_eq!(
        d.broker.submission_deadlines[0],
        c.entry_valid_until.unwrap()
    );
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    d.broker.at += chrono::Duration::seconds(4);
    drive(&mut d, &c, None);
    assert_eq!(d.broker.submission_deadlines.len(), 2);
    assert!(d.broker.submission_deadlines[1] > c.entry_valid_until.unwrap());
}

#[test]
fn exact_stop_confirmation_persists_before_protection_and_uses_broker_fill_clock() {
    for (delay, protected) in [(500, true), (6000, false)] {
        let (c, mut d) = fixture();
        drive(&mut d, &c, None);
        d.broker.entry_filled = c.quantity;
        d.broker.entry_terminal = true;
        d.broker.actual_entry_fill = Some(c.created_at + chrono::Duration::milliseconds(25));
        d.broker.confirmation_delay_ms = Some(delay);
        let s = drive(&mut d, &c, None);
        assert_eq!(s.entry_first_filled_at, d.broker.actual_entry_fill);
        assert_eq!(s.phase == ProtocolPhase::Protected, protected);
        if !protected {
            assert!(s.exit_requested);
            assert_eq!(
                s.reason.as_deref(),
                Some("BROKER_FILL_PROTECTION_DEADLINE_EXCEEDED")
            );
        }
        let trace = d.journal.trace.borrow();
        let durable = trace
            .iter()
            .position(|e| e == "persist_exact_stop_confirmation")
            .unwrap();
        assert!(trace[durable + 1].contains("Observed { leg: Protection"));
        assert_eq!(recover(&d.journal.records).unwrap(), s);
    }
}

#[test]
fn failed_liquidation_prerequisites_preserve_the_working_stop_and_can_retry_reads() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Protected);
    d.broker.fail_cancel_prerequisites = true;
    let state = drive(&mut d, &c, Some("OWNER_EXIT"));
    assert!(state.exit_requested);
    assert!(!state.protection.as_ref().unwrap().cancel_requested);
    assert!(d.broker.cancelled.is_empty());
    d.broker.fail_cancel_prerequisites = false;
    drive(&mut d, &c, None);
    assert_eq!(d.broker.cancelled.len(), 1);
}

#[test]
fn old_journal_replays_without_a_broker_clock_field() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    let encoded = serde_json::to_string(&d.journal.records).unwrap();
    assert!(!encoded.contains("entry_first_filled_at"));
    assert_eq!(
        recover(&serde_json::from_str::<Vec<ProtocolRecord>>(&encoded).unwrap()).unwrap(),
        d.state(&c).unwrap()
    );
}

#[test]
fn scoped_entry_update_never_claims_a_new_complete_account_view() {
    for case in ["valid", "stale", "unrelated", "missing_fill_clock"] {
        let (c, mut d) = fixture();
        drive(&mut d, &c, None);
        d.broker.entry_filled = c.quantity;
        d.broker.entry_terminal = true;
        d.broker.actual_entry_fill = if case == "missing_fill_clock" {
            None
        } else {
            Some(c.created_at + chrono::Duration::milliseconds(25))
        };
        d.broker.fast_entry_prior = Some(if case == "stale" {
            c.created_at - chrono::Duration::seconds(11)
        } else {
            c.created_at
        });
        d.broker.fast_entry_unrelated = case == "unrelated";
        d.broker.at += chrono::Duration::milliseconds(20);
        let result = d.drive_once(&c, d.broker.at, None);
        assert_eq!(result.is_ok(), case == "valid");
        assert_eq!(
            d.broker.submitted.len(),
            if case == "valid" { 2 } else { 1 }
        );
        assert_eq!(
            d.journal
                .records
                .iter()
                .filter(|r| matches!(r.input, ProtocolInput::HoldingReconciled { .. }))
                .count(),
            1
        );
    }
}
#[test]
fn complete_round_trip_restarts_with_exact_stop_and_cancels_before_flatten() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Protected);
    // Recreate the durable journal from bytes, as a process restart would.
    d.journal.records =
        serde_json::from_slice(&serde_json::to_vec(&d.journal.records).unwrap()).unwrap();
    assert_eq!(
        recover(&d.journal.records).unwrap().phase,
        ProtocolPhase::Protected
    );
    drive(&mut d, &c, Some("TARGET_REACHED"));
    assert_eq!(d.broker.cancelled, vec!["1002"]);
    assert_eq!(d.broker.submitted.len(), 2);
    d.broker.stop_terminal = true;
    drive(&mut d, &c, None);
    assert!(matches!(
        d.broker.submitted.last().unwrap().kind,
        OrderKind::Market {
            extended_hours: false
        }
    ));
    d.broker.exit_filled = c.quantity;
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Flat);
    let trace = d.journal.trace.borrow();
    for (i, item) in trace.iter().enumerate() {
        if item == "send" {
            assert!(trace[..i].last().unwrap().contains("Dispatch"));
        }
    }
}
#[test]
fn partial_entry_waits_for_terminal_cancellation_and_protects_late_fill() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = Decimal::ONE;
    drive(&mut d, &c, None);
    assert_eq!(d.broker.cancelled, vec!["1001"]);
    assert_eq!(d.broker.submitted.len(), 1);
    drive(&mut d, &c, None);
    assert_eq!(d.broker.cancelled.len(), 1);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    assert_eq!(d.broker.submitted.last().unwrap().quantity, c.quantity);
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Protected);
}
#[test]
fn complete_view_applies_partial_stop_before_checking_the_shared_holding() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    drive(&mut d, &c, None);
    let before = d.journal.records.len();
    d.broker.stop_filled = Decimal::ONE;
    let state = drive(&mut d, &c, None);
    assert_eq!(state.phase, ProtocolPhase::Protected);
    assert_eq!(state.holding_quantity, Decimal::ONE);
    assert!(!state.exit_requested && !state.reconciliation_required);
    assert!(state.reason.is_none());
    assert_eq!(d.broker.submitted.len(), 2);
    assert!(d.broker.cancelled.is_empty());
    assert!(
        d.journal.records[before..]
            .iter()
            .any(|r| matches!(r.input, ProtocolInput::ObservedBatch { .. }))
    );
    assert!(
        d.journal.records[before..]
            .iter()
            .all(|r| r.state.reason.as_deref() != Some("HISTORY_AND_HOLDING_CONFLICT"))
    );
    assert_eq!(recover(&d.journal.records).unwrap(), state);
    let mut batch = d.journal.records[before..]
        .iter()
        .find_map(|r| match &r.input {
            ProtocolInput::ObservedBatch { orders } => Some(orders.clone()),
            _ => None,
        })
        .unwrap();
    // A genuine complete-view mismatch must still latch recovery. These are
    // synthetic faults, never claimed as new broker observations.
    for order in &mut batch {
        if let ProtocolInput::Observed {
            holding_quantity, ..
        } = order
        {
            *holding_quantity = c.quantity;
        }
    }
    let conflict = state
        .transition(
            "actual_ledger_conflict_control".into(),
            ProtocolInput::ObservedBatch {
                orders: batch.clone(),
            },
            d.broker.at,
        )
        .unwrap()
        .state;
    assert!(conflict.reconciliation_required && conflict.exit_requested);
    assert_eq!(
        conflict.reason.as_deref(),
        Some("HISTORY_AND_HOLDING_CONFLICT")
    );
    batch[1] = batch[0].clone();
    assert!(
        state
            .transition(
                "duplicate_leg_control".into(),
                ProtocolInput::ObservedBatch { orders: batch },
                d.broker.at
            )
            .is_err()
    );
}
#[test]
fn stop_cancel_race_subtracts_its_partial_fill_before_market_exit() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    drive(&mut d, &c, None);
    drive(&mut d, &c, None);
    drive(&mut d, &c, Some("TARGET_REACHED"));
    d.broker.stop_filled = Decimal::ONE;
    d.broker.stop_terminal = true;
    drive(&mut d, &c, None);
    assert_eq!(d.broker.submitted.last().unwrap().quantity, Decimal::ONE);
    d.broker.exit_filled = Decimal::ONE;
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::Flat);
}
#[test]
fn rejected_stop_flattens_exact_reconciled_holding() {
    let (c, mut d) = fixture();
    drive(&mut d, &c, None);
    d.broker.entry_filled = c.quantity;
    d.broker.entry_terminal = true;
    d.broker.reject_stop = true;
    assert_eq!(drive(&mut d, &c, None).phase, ProtocolPhase::ExitPrepared);
    drive(&mut d, &c, None);
    assert!(matches!(
        d.broker.submitted.last().unwrap().kind,
        OrderKind::Market { .. }
    ));
    d.broker.exit_filled = c.quantity;
    let state = drive(&mut d, &c, None);
    assert_eq!(state.phase, ProtocolPhase::Flat);
    assert_eq!(state.reason.as_deref(), Some("PROTECTION_OR_EXIT_REJECTED"));
    assert!(!state.reconciliation_required);
    assert_eq!(d.broker.submitted.len(), 3);
    assert!(d.broker.cancelled.is_empty());
    assert_eq!(recover(&d.journal.records).unwrap(), state);
}
#[test]
fn unknown_submission_survives_restart_and_never_resends() {
    let (c, mut d) = fixture();
    d.broker.unknown_entry = true;
    drive(&mut d, &c, None);
    for _ in 0..3 {
        let s = drive(&mut d, &c, None);
        assert!(matches!(
            s.recovery_effect(),
            ProtocolEffect::RequireOperator { .. }
        ));
    }
    assert_eq!(d.broker.submitted.len(), 1);
}
#[test]
fn failed_durable_dispatch_cannot_reach_transport() {
    let (c, mut d) = fixture();
    d.journal.fail_dispatch = true;
    assert!(d.drive_once(&c, c.created_at, None).is_err());
    assert!(d.broker.submitted.is_empty());
}

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
    trace: Rc<RefCell<Vec<String>>>,
}
impl ProtocolJournal for Journal {
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
            history_complete: true,
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

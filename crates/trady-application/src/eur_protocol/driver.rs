//! Write-before-send bridge. Broker acknowledgements never establish protection;
//! only a complete exact-order/holding observation can do that.
use super::*;
use crate::broker::{
    BrokerOrderRequest, BrokerResult, CancellationReceipt, OrderKind, TimeInForce,
};
use trady_domain::{AccountId, BrokerOrder, Currency, IntentId, Side};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactOrderObservation {
    pub leg: OrderLeg,
    pub order: BrokerOrder,
    pub stop_price: Option<Decimal>,
    pub pending_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerObservation {
    pub holding_quantity: Decimal,
    pub available_quantity: Decimal,
    /// Includes all account orders, so unrelated orders fail closed.
    pub pending_order_ids: Vec<String>,
    pub exact_orders: Vec<ExactOrderObservation>,
    pub history_complete: bool,
    /// The bounded exact-order scan ended without a complete execution proof.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub history_coverage_exhausted: bool,
    pub observed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_first_filled_at: Option<DateTime<Utc>>,
    /// Some means an exact entry update inheriting a recent complete account
    /// view; pending IDs and history completeness then describe that leg only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_confirmation_from_full_view_at: Option<DateTime<Utc>>,
    /// Provider responses, including fill prices and wallet fees, retained for
    /// economic reconciliation. No credentials or HTTP headers are included.
    pub evidence: serde_json::Value,
}

/// Exact working STOP and a fresh holding, following a complete account view.
/// This does not imply that the full pending-order list was read again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectionConfirmation {
    pub order: BrokerOrder,
    pub stop_price: Decimal,
    pub holding_quantity: Decimal,
    pub observed_at: DateTime<Utc>,
    pub evidence: serde_json::Value,
}

pub trait ProtocolBroker {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
    /// Must validate account, ticker, side, order type, prices and TIF against
    /// `request_for`. Missing history or a truncated page is never terminal.
    fn observe(&mut self, state: &ProtocolState) -> BrokerResult<BrokerObservation>;
    fn confirm_entry(&mut self, _state: &ProtocolState) -> BrokerResult<Option<BrokerObservation>> {
        Ok(None)
    }
    fn confirm_protection(
        &mut self,
        _state: &ProtocolState,
    ) -> BrokerResult<Option<ProtectionConfirmation>> {
        Ok(None)
    }
    /// Complete read-only liquidation prerequisites while the STOP still works.
    fn prepare_cancel(&mut self, _state: &ProtocolState, _leg: OrderLeg) -> BrokerResult<()> {
        Ok(())
    }
    fn submit(
        &mut self,
        request: &BrokerOrderRequest,
        expires_at: DateTime<Utc>,
    ) -> BrokerResult<BrokerOrder>;
    fn cancel(&mut self, account: &AccountId, broker_id: &str)
    -> BrokerResult<CancellationReceipt>;
}

pub fn request_for(state: &ProtocolState, leg: OrderLeg) -> Result<BrokerOrderRequest> {
    let (quantity, kind) = match leg {
        OrderLeg::Entry => (
            state.context.quantity,
            OrderKind::Limit {
                price: state.context.entry_limit,
                time_in_force: TimeInForce::Day,
            },
        ),
        OrderLeg::Protection => {
            let p = state
                .protection
                .as_ref()
                .ok_or_else(|| Error("protection not reserved".into()))?;
            (
                p.quantity,
                OrderKind::Stop {
                    price: p
                        .stop_price
                        .ok_or_else(|| Error("reserved stop missing".into()))?,
                    time_in_force: TimeInForce::Day,
                },
            )
        }
        OrderLeg::Exit => (
            state
                .exits
                .last()
                .ok_or_else(|| Error("exit not reserved".into()))?
                .quantity,
            OrderKind::Market {
                extended_hours: false,
            },
        ),
    };
    Ok(BrokerOrderRequest {
        intent_id: IntentId::new(format!(
            "{}_{}",
            state.context.intent_id,
            match leg {
                OrderLeg::Entry => "entry".into(),
                OrderLeg::Protection => "stop".into(),
                OrderLeg::Exit => format!("exit{}", state.exits.len()),
            }
        ))?,
        account_id: AccountId::new(state.context.account_id.clone())?,
        instrument_id: trady_domain::InstrumentId::new(state.context.broker_ticker.clone())?,
        instrument_currency: Currency::new("EUR")?,
        side: if leg == OrderLeg::Entry {
            Side::Buy
        } else {
            Side::Sell
        },
        quantity,
        kind,
    })
}

pub struct ProtocolDriver<J, B> {
    pub journal: J,
    pub broker: B,
}

impl<J: ProtocolJournal, B: ProtocolBroker> ProtocolDriver<J, B> {
    /// Report ambiguous or overdue DELETE outcomes without changing historical
    /// reducer effects. Reads remain allowed so a late terminal result can win.
    pub fn recovery_action(&mut self, state: &ProtocolState) -> Result<ProtocolEffect> {
        let effect = state.recovery_effect();
        if matches!(effect, ProtocolEffect::RequireOperator { .. }) {
            return Ok(effect);
        }
        if state.history_coverage_exhausted {
            return Ok(ProtocolEffect::RequireOperator {
                reason: "EXACT_ORDER_HISTORY_COVERAGE_EXHAUSTED".into(),
            });
        }
        let records = self.journal.load(&state.context.intent_id)?;
        for leg in [OrderLeg::Entry, OrderLeg::Protection, OrderLeg::Exit] {
            let Ok(progress) = state.progress(leg) else {
                continue;
            };
            if !progress.cancel_requested || progress.reconciled_terminal {
                continue;
            }
            let mut acknowledged = false;
            for record in records.iter().rev() {
                match record.input {
                    ProtocolInput::CancellationAcknowledged { leg: l } if l == leg => {
                        acknowledged = true;
                    }
                    ProtocolInput::Cancel { leg: l } if l == leg => {
                        if !acknowledged
                            || (state.last_reconciled_at - record.at).num_seconds() >= 10
                        {
                            return Ok(ProtocolEffect::RequireOperator {
                                reason: format!(
                                    "CANCELLATION_OUTCOME_REQUIRES_REVIEW:{leg:?}:{}",
                                    progress.broker_id.as_deref().unwrap_or_default()
                                ),
                            });
                        }
                        break;
                    }
                    _ => (),
                }
            }
        }
        Ok(effect)
    }

    pub fn state(&mut self, context: &ProtocolContext) -> Result<ProtocolState> {
        let records = self.journal.load(&context.intent_id)?;
        if records.is_empty() {
            return ProtocolState::new(context.clone());
        }
        let state = recover(&records)?;
        require(
            state.context == *context,
            "protocol context changed on restart",
        )?;
        Ok(state)
    }

    fn record(
        &mut self,
        context: &ProtocolContext,
        input: ProtocolInput,
        at: DateTime<Utc>,
    ) -> Result<ProtocolEffect> {
        advance(
            &mut self.journal,
            context,
            crate::harness::new_id("protocol"),
            input,
            at,
        )
    }

    /// One call performs at most one new broker mutation. Every call starts
    /// with reads, including after restart. Unknown submissions are never retried.
    pub fn drive_once(
        &mut self,
        context: &ProtocolContext,
        now: DateTime<Utc>,
        exit_reason: Option<&str>,
    ) -> Result<ProtocolState> {
        let initial = self.state(context)?;
        if initial.revision == 0 {
            self.record(context, ProtocolInput::Reserve, now)?;
        }
        let state = self.state(context)?;
        if matches!(
            state.recovery_effect(),
            ProtocolEffect::RequireOperator { .. }
        ) {
            return Ok(state);
        }
        let fast_entry = self.broker.confirm_entry(&state).ok().flatten();
        let view = match fast_entry
            .map(Ok)
            .unwrap_or_else(|| self.broker.observe(&state))
        {
            Ok(view) => view,
            Err(_) => {
                let failed_at = self.broker.now().max(now);
                self.record(context, ProtocolInput::Disconnected, failed_at)?;
                if failed_at >= context.flatten_at
                    || state.unprotected_since.is_some_and(|since| {
                        (failed_at - since).num_seconds() >= context.max_unprotected_seconds
                    })
                {
                    self.record(context, ProtocolInput::Tick, failed_at)?;
                }
                return self.state(context);
            }
        };
        let at = view.observed_at;
        let entry_update = view.entry_confirmation_from_full_view_at;
        if let Some(prior) = entry_update {
            require(
                !state.reconciliation_required
                    && !state.exit_requested
                    && state.protection.is_none()
                    && state.exits.is_empty()
                    && at >= prior
                    && prior <= state.last_reconciled_at
                    && (at - prior).num_milliseconds() <= 10000
                    && view.exact_orders.len() == 1
                    && view.exact_orders[0].leg == OrderLeg::Entry
                    && (view.exact_orders[0].order.filled_quantity == Decimal::ZERO
                        || view.entry_first_filled_at.is_some())
                    && view.pending_order_ids.iter().all(|id| {
                        state.entry.as_ref().and_then(|p| p.broker_id.as_ref()) == Some(id)
                    }),
                "invalid exact entry confirmation scope",
            )?;
        }
        require(
            at >= now
                && at >= state.last_event_at
                && view.available_quantity >= Decimal::ZERO
                && view.available_quantity <= view.holding_quantity,
            "invalid broker observation time or available quantity",
        )?;
        self.journal.record_observation(context, &view)?;
        let mut ids = std::collections::BTreeSet::new();
        let mut updates = Vec::new();
        for exact in &view.exact_orders {
            let expected = state.progress(exact.leg)?;
            require(
                ids.insert(exact.order.broker_order_id.clone())
                    && expected.broker_id.as_deref() == Some(&exact.order.broker_order_id)
                    && exact.order.side
                        == if exact.leg == OrderLeg::Entry {
                            Side::Buy
                        } else {
                            Side::Sell
                        }
                    && exact.order.instrument_id.as_str() == context.broker_ticker,
                "broker observation contains an unexpected exact order",
            )?;
            let terminal =
                exact.order.status.terminal() && view.history_complete && !exact.pending_present;
            let unchanged = !state.reconciliation_required
                && expected.filled == exact.order.filled_quantity
                && expected.status == exact.order.status
                && expected.reconciled_terminal == terminal
                && expected.stop_price == exact.stop_price
                && state.holding_quantity == view.holding_quantity;
            updates.push((
                unchanged,
                ProtocolInput::Observed {
                    leg: exact.leg,
                    broker_id: exact.order.broker_order_id.clone(),
                    broker_ticker: exact.order.instrument_id.to_string(),
                    quantity: exact.order.quantity,
                    filled: exact.order.filled_quantity,
                    status: exact.order.status,
                    holding_quantity: view.holding_quantity,
                    pending_present: exact.pending_present,
                    history_complete: view.history_complete,
                    stop_price: exact.stop_price,
                },
            ));
        }
        let complete_batch = view.history_complete
            && updates.len() > 1
            && state
                .entry
                .iter()
                .chain(state.protection.iter())
                .chain(state.exits.last())
                .filter(|p| p.requires_native_observation())
                .all(|p| p.broker_id.as_ref().is_some_and(|id| ids.contains(id)));
        if complete_batch {
            if updates.iter().any(|(unchanged, _)| !unchanged) {
                self.record(
                    context,
                    ProtocolInput::ObservedBatch {
                        orders: updates.into_iter().map(|(_, order)| order).collect(),
                    },
                    at,
                )?;
            }
        } else {
            for (unchanged, order) in updates {
                if !unchanged {
                    self.record(context, order, at)?;
                }
            }
        }
        if let Some(filled_at) = view.entry_first_filled_at {
            let current = self.state(context)?;
            if current
                .entry_first_filled_at
                .is_none_or(|old| filled_at < old)
            {
                let broker_id = current
                    .entry
                    .as_ref()
                    .and_then(|p| p.broker_id.clone())
                    .ok_or_else(|| Error("broker fill time has no exact entry ID".into()))?;
                self.record(
                    context,
                    ProtocolInput::EntryFillTimeObserved {
                        broker_id,
                        filled_at,
                    },
                    at,
                )?;
            }
        }
        let missing = state
            .entry
            .iter()
            .chain(state.protection.iter())
            .chain(state.exits.last())
            .any(|p| p.broker_id.as_ref().is_some_and(|id| !ids.contains(id)));
        // Raw observations above remain durable on every pass. A stable
        // protected position needs no duplicate reducer event; any change or
        // recovery still records a fresh complete account reconciliation.
        let current = self.state(context)?;
        let unchanged_protected = current.phase == ProtocolPhase::Protected
            && current == state
            && !current.reconciliation_required
            && view.history_complete
            && !missing
            && current.holding_quantity == view.holding_quantity
            && view.pending_order_ids.iter().all(|id| {
                current
                    .all_orders()
                    .any(|p| p.broker_id.as_ref() == Some(id) && !p.reconciled_terminal)
            });
        if entry_update.is_none() && !unchanged_protected {
            self.record(
                context,
                ProtocolInput::HoldingReconciled {
                    quantity: view.holding_quantity,
                    pending_order_ids: view.pending_order_ids.clone(),
                    history_complete: view.history_complete && !missing,
                },
                at,
            )?;
        }
        if view.history_coverage_exhausted {
            require(!view.history_complete, "contradictory history completeness")?;
            self.record(context, ProtocolInput::HistoryCoverageExhausted, at)?;
        }
        let refreshed = self.state(context)?;
        if !refreshed.exit_requested
            && (at >= context.flatten_at
                || refreshed.unprotected_since.is_some_and(|since| {
                    (at - since).num_seconds() >= context.max_unprotected_seconds
                }))
        {
            self.record(context, ProtocolInput::Tick, at)?;
        }
        if let Some(reason) = exit_reason
            && !self.state(context)?.exit_requested
        {
            self.record(
                context,
                ProtocolInput::RequestExit {
                    reason: reason.into(),
                },
                at,
            )?;
        }
        let state = self.state(context)?;
        if state.reconciliation_required || missing {
            return Ok(state);
        }
        let input = match state.phase {
            ProtocolPhase::Prepared
                if view.holding_quantity == Decimal::ZERO && view.pending_order_ids.is_empty() =>
            {
                Some(ProtocolInput::DispatchEntry)
            }
            ProtocolPhase::EntryReconciling
            | ProtocolPhase::EntryCancelPending
            | ProtocolPhase::Recovery
                if state
                    .entry
                    .as_ref()
                    .is_some_and(|p| !p.reconciled_terminal && !p.cancel_requested)
                    && (state.exit_requested
                        || state
                            .entry
                            .as_ref()
                            .is_some_and(|p| p.filled > Decimal::ZERO)) =>
            {
                Some(ProtocolInput::Cancel {
                    leg: OrderLeg::Entry,
                })
            }
            ProtocolPhase::ProtectionPrepared if view.available_quantity == state.remaining() => {
                Some(ProtocolInput::DispatchProtection)
            }
            ProtocolPhase::StopCancelPending
                if state
                    .protection
                    .as_ref()
                    .is_some_and(|p| !p.cancel_requested) =>
            {
                Some(ProtocolInput::Cancel {
                    leg: OrderLeg::Protection,
                })
            }
            ProtocolPhase::ExitPrepared if view.available_quantity == state.remaining() => {
                Some(ProtocolInput::DispatchExit)
            }
            _ => None,
        };
        if let Some(input) = input {
            if let ProtocolInput::Cancel { leg } = &input
                && self.broker.prepare_cancel(&state, *leg).is_err()
            {
                // Nothing was dispatched or marked cancel-requested. Preserve
                // native protection and permit a later read-only retry.
                return Ok(state);
            }
            let effect = self.record(context, input.clone(), at)?;
            let reserved = self.state(context)?;
            match effect {
                ProtocolEffect::SubmitEntry { .. }
                | ProtocolEffect::SubmitProtection { .. }
                | ProtocolEffect::SubmitFlatten { .. } => {
                    let leg = match input {
                        ProtocolInput::DispatchEntry => OrderLeg::Entry,
                        ProtocolInput::DispatchProtection => OrderLeg::Protection,
                        _ => OrderLeg::Exit,
                    };
                    let request = request_for(&reserved, leg)?;
                    // Expired entry/protection must never reach transport. A
                    // latched risk-reducing flatten remains legal after cutoff.
                    let mut deadline = if leg == OrderLeg::Exit {
                        at + chrono::Duration::seconds(10)
                    } else {
                        context.flatten_at.min(at + chrono::Duration::seconds(10))
                    };
                    if leg == OrderLeg::Entry
                        && let Some(until) = context.entry_valid_until
                    {
                        deadline = deadline.min(until);
                    }
                    match self.broker.submit(&request, deadline) {
                        Ok(order) => {
                            require(
                                order.instrument_id == request.instrument_id
                                    && order.side == request.side
                                    && order.quantity == request.quantity,
                                "submission acknowledgement identity conflict",
                            )?;
                            self.record(
                                context,
                                ProtocolInput::Acknowledged {
                                    leg,
                                    broker_id: order.broker_order_id,
                                },
                                self.broker.now().max(at),
                            )?;
                            if leg == OrderLeg::Protection {
                                let current = self.state(context)?;
                                if let Ok(Some(confirmation)) =
                                    self.broker.confirm_protection(&current)
                                {
                                    let expected = current.progress(OrderLeg::Protection)?;
                                    require(
                                        confirmation.observed_at >= current.last_event_at
                                            && (confirmation.observed_at
                                                - current.last_reconciled_at)
                                                .num_milliseconds()
                                                <= 10000
                                            && !current.reconciliation_required
                                            && confirmation.order.broker_order_id
                                                == *expected.broker_id.as_ref().ok_or_else(
                                                    || Error("stop ID missing".into()),
                                                )?
                                            && confirmation.order.instrument_id
                                                == request.instrument_id
                                            && confirmation.order.side == Side::Sell
                                            && confirmation.order.quantity == expected.quantity
                                            && confirmation.stop_price
                                                == expected.stop_price.ok_or_else(|| {
                                                    Error("stop price missing".into())
                                                })?
                                            && matches!(
                                                confirmation.order.status,
                                                OrderStatus::Open | OrderStatus::PartiallyFilled
                                            ),
                                        "exact protection confirmation conflicts or is stale",
                                    )?;
                                    self.journal
                                        .record_protection_confirmation(context, &confirmation)?;
                                    self.record(
                                        context,
                                        ProtocolInput::Observed {
                                            leg: OrderLeg::Protection,
                                            broker_id: confirmation.order.broker_order_id,
                                            broker_ticker: confirmation
                                                .order
                                                .instrument_id
                                                .to_string(),
                                            quantity: confirmation.order.quantity,
                                            filled: confirmation.order.filled_quantity,
                                            status: confirmation.order.status,
                                            holding_quantity: confirmation.holding_quantity,
                                            pending_present: true,
                                            history_complete: true,
                                            stop_price: Some(confirmation.stop_price),
                                        },
                                        confirmation.observed_at,
                                    )?;
                                }
                            }
                        }
                        Err(e) if e.outcome_unknown => {
                            // A returned ID on an invalid response is not a
                            // verified acknowledgement. Retain UNKNOWN for review.
                            self.record(
                                context,
                                ProtocolInput::Disconnected,
                                self.broker.now().max(at),
                            )?;
                        }
                        Err(_) => {
                            self.record(
                                context,
                                ProtocolInput::Rejected { leg },
                                self.broker.now().max(at),
                            )?;
                        }
                    }
                }
                ProtocolEffect::CancelExactOrder { broker_id } => {
                    let account = AccountId::new(context.account_id.clone())?;
                    if let Ok(receipt) = self.broker.cancel(&account, &broker_id) {
                        require(
                            receipt.broker_order_id == broker_id,
                            "cancellation ID conflict",
                        )?;
                        if let ProtocolInput::Cancel { leg } = input {
                            self.record(
                                context,
                                ProtocolInput::CancellationAcknowledged { leg },
                                self.broker.now().max(at),
                            )?;
                        }
                    }
                }
                _ => (),
            }
        }
        self.state(context)
    }
}

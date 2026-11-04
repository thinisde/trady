//! Durable sequential-order protocol. A returned effect is an instruction to a
//! separately gated executor, never a broker capability or release approval.
//! Every dispatch is journalled as UNKNOWN before transport is invoked.
use crate::harness::content_hash;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trady_domain::{Error, OrderStatus, Result, require};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolContext {
    pub runtime_mode: trady_domain::ExecutionStage,
    pub policy_revision: String,
    pub intent_id: String,
    pub account_id: String,
    pub broker_account_id: u64,
    pub instrument_id: String,
    pub broker_ticker: String,
    pub isin: String,
    pub policy_hash: String,
    pub configuration_hash: String,
    pub evidence_ids: Vec<String>,
    pub quantity: Decimal,
    pub entry_limit: Decimal,
    pub initial_stop: Decimal,
    pub target: Decimal,
    pub created_at: DateTime<Utc>,
    pub flatten_at: DateTime<Utc>,
    pub max_unprotected_seconds: i64,
}
impl ProtocolContext {
    pub fn validate(&self) -> Result<()> {
        require(
            [
                &self.intent_id,
                &self.account_id,
                &self.instrument_id,
                &self.broker_ticker,
                &self.isin,
            ]
            .iter()
            .all(|v| valid_id(v))
                && self.broker_account_id == 52211463
                && ((self.runtime_mode == trady_domain::ExecutionStage::Demo
                    && self.account_id.starts_with("paper_"))
                    || (self.runtime_mode == trady_domain::ExecutionStage::Replay
                        && self.account_id.starts_with("replay_")))
                && valid_id(&self.policy_revision)
                && self.isin.len() == 12
                && sha(&self.policy_hash)
                && sha(&self.configuration_hash)
                && !self.evidence_ids.is_empty()
                && self.evidence_ids.len() <= 64
                && self.evidence_ids.iter().all(|v| valid_id(v))
                && self.quantity > Decimal::ZERO
                && self.entry_limit > self.initial_stop
                && self.initial_stop > Decimal::ZERO
                && self.target > self.entry_limit
                && (self.entry_limit - self.initial_stop) / self.entry_limit <= Decimal::new(2, 2)
                && (self.target - self.entry_limit) / (self.entry_limit - self.initial_stop)
                    >= Decimal::new(15, 1)
                && self.created_at < self.flatten_at
                && self.created_at.date_naive() == self.flatten_at.date_naive()
                && (1..=10).contains(&self.max_unprotected_seconds),
            "invalid EUR protocol authority or risk bounds",
        )
    }
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 120
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}
fn sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtocolPhase {
    Prepared,
    EntryUnknown,
    EntryReconciling,
    EntryCancelPending,
    ProtectionPrepared,
    ProtectionUnknown,
    Protected,
    StopCancelPending,
    ExitPrepared,
    ExitUnknown,
    ExitReconciling,
    Recovery,
    Flat,
    Rejected,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderLeg {
    Entry,
    Protection,
    Exit,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderProgress {
    pub broker_id: Option<String>,
    pub quantity: Decimal,
    pub filled: Decimal,
    pub status: OrderStatus,
    pub reconciled_terminal: bool,
    pub cancel_requested: bool,
    pub dispatched_at: DateTime<Utc>,
    pub stop_price: Option<Decimal>,
}
impl OrderProgress {
    fn new(quantity: Decimal, at: DateTime<Utc>) -> Self {
        Self {
            broker_id: None,
            quantity,
            filled: Decimal::ZERO,
            status: OrderStatus::Unknown,
            reconciled_terminal: false,
            cancel_requested: false,
            dispatched_at: at,
            stop_price: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ProtocolInput {
    Reserve,
    DispatchEntry,
    Acknowledged {
        leg: OrderLeg,
        broker_id: String,
    },
    ReviewedBrokerId {
        leg: OrderLeg,
        broker_id: String,
        review_reference: String,
    },
    Observed {
        leg: OrderLeg,
        broker_id: String,
        broker_ticker: String,
        quantity: Decimal,
        filled: Decimal,
        status: OrderStatus,
        holding_quantity: Decimal,
        pending_present: bool,
        history_complete: bool,
        stop_price: Option<Decimal>,
    },
    Rejected {
        leg: OrderLeg,
    },
    Cancel {
        leg: OrderLeg,
    },
    CancellationAcknowledged {
        leg: OrderLeg,
    },
    DispatchProtection,
    RequestExit {
        reason: String,
    },
    DispatchExit,
    HoldingReconciled {
        quantity: Decimal,
        pending_order_ids: Vec<String>,
        history_complete: bool,
    },
    Disconnected,
    Tick,
    TightenStop {
        price: Decimal,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ProtocolEffect {
    None,
    SubmitEntry { quantity: Decimal, limit: Decimal },
    SubmitProtection { quantity: Decimal, stop: Decimal },
    SubmitFlatten { quantity: Decimal },
    CancelExactOrder { broker_id: String },
    ReconcileExactOrders { broker_ids: Vec<String> },
    RequireOperator { reason: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolState {
    pub context: ProtocolContext,
    pub revision: u64,
    pub phase: ProtocolPhase,
    pub entry: Option<OrderProgress>,
    pub protection: Option<OrderProgress>,
    pub exits: Vec<OrderProgress>,
    pub holding_quantity: Decimal,
    pub last_reconciled_at: DateTime<Utc>,
    pub desired_stop: Decimal,
    pub exit_requested: bool,
    pub reconciliation_required: bool,
    pub unprotected_since: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub last_event_at: DateTime<Utc>,
}
impl ProtocolState {
    pub fn new(context: ProtocolContext) -> Result<Self> {
        context.validate()?;
        Ok(Self {
            desired_stop: context.initial_stop,
            last_reconciled_at: context.created_at,
            last_event_at: context.created_at,
            context,
            revision: 0,
            phase: ProtocolPhase::Prepared,
            entry: None,
            protection: None,
            exits: vec![],
            holding_quantity: Decimal::ZERO,
            exit_requested: false,
            reconciliation_required: false,
            unprotected_since: None,
            reason: None,
        })
    }
    pub fn remaining(&self) -> Decimal {
        self.entry.as_ref().map_or(Decimal::ZERO, |p| p.filled)
            - self.protection.as_ref().map_or(Decimal::ZERO, |p| p.filled)
            - self.exits.iter().map(|p| p.filled).sum::<Decimal>()
    }
    fn progress(&self, leg: OrderLeg) -> Result<&OrderProgress> {
        match leg {
            OrderLeg::Entry => self.entry.as_ref(),
            OrderLeg::Protection => self.protection.as_ref(),
            OrderLeg::Exit => self.exits.last(),
        }
        .ok_or_else(|| Error("order leg was never dispatched".into()))
    }
    fn progress_mut(&mut self, leg: OrderLeg) -> Result<&mut OrderProgress> {
        match leg {
            OrderLeg::Entry => self.entry.as_mut(),
            OrderLeg::Protection => self.protection.as_mut(),
            OrderLeg::Exit => self.exits.last_mut(),
        }
        .ok_or_else(|| Error("order leg was never dispatched".into()))
    }
    fn all_orders(&self) -> impl Iterator<Item = &OrderProgress> {
        self.entry
            .iter()
            .chain(self.protection.iter())
            .chain(self.exits.iter())
    }
    pub fn recovery_effect(&self) -> ProtocolEffect {
        if self
            .all_orders()
            .any(|p| p.broker_id.is_none() && !p.reconciled_terminal)
        {
            ProtocolEffect::RequireOperator {
                reason: "AMBIGUOUS_SUBMISSION_REQUIRES_REVIEWED_EXACT_ID".into(),
            }
        } else {
            ProtocolEffect::ReconcileExactOrders {
                broker_ids: self
                    .all_orders()
                    .filter_map(|p| p.broker_id.clone())
                    .collect(),
            }
        }
    }
    fn exit_phase(&mut self) {
        if !self.exit_requested {
            return;
        }
        if self.reconciliation_required
            || self.entry.as_ref().is_some_and(|p| !p.reconciled_terminal)
        {
            self.phase = ProtocolPhase::Recovery;
        } else if self
            .protection
            .as_ref()
            .is_some_and(|p| !p.reconciled_terminal)
        {
            self.phase = ProtocolPhase::StopCancelPending;
        } else if self.exits.iter().any(|p| !p.reconciled_terminal) {
            self.phase = ProtocolPhase::ExitReconciling;
        } else if self.remaining() == Decimal::ZERO && self.holding_quantity == Decimal::ZERO {
            self.phase = ProtocolPhase::Flat;
        } else {
            self.phase = ProtocolPhase::ExitPrepared;
        }
    }
    fn reduce(&mut self, input: &ProtocolInput, at: DateTime<Utc>) -> Result<ProtocolEffect> {
        require(at >= self.last_event_at, "protocol clock moved backwards")?;
        let mut effect = ProtocolEffect::None;
        match input {
            ProtocolInput::Reserve => require(self.revision == 0, "intent already reserved")?,
            ProtocolInput::DispatchEntry => {
                require(
                    self.revision > 0
                        && self.phase == ProtocolPhase::Prepared
                        && self.entry.is_none()
                        && at < self.context.flatten_at
                        && (at - self.last_reconciled_at).num_seconds() <= 10
                        && !self.reconciliation_required
                        && !self.exit_requested,
                    "entry dispatch is expired, duplicated or unreconciled",
                )?;
                self.entry = Some(OrderProgress::new(self.context.quantity, at));
                self.phase = ProtocolPhase::EntryUnknown;
                effect = ProtocolEffect::SubmitEntry {
                    quantity: self.context.quantity,
                    limit: self.context.entry_limit,
                };
            }
            ProtocolInput::Acknowledged { leg, broker_id }
            | ProtocolInput::ReviewedBrokerId { leg, broker_id, .. } => {
                require(valid_id(broker_id), "invalid exact broker order ID")?;
                if let ProtocolInput::ReviewedBrokerId {
                    review_reference, ..
                } = input
                {
                    require(
                        valid_id(review_reference),
                        "exact-ID recovery review missing",
                    )?;
                }
                let p = self.progress_mut(*leg)?;
                require(
                    p.broker_id.as_ref().is_none_or(|old| old == broker_id),
                    "broker order ID rebound",
                )?;
                p.broker_id = Some(broker_id.clone());
                self.phase = match leg {
                    OrderLeg::Entry => ProtocolPhase::EntryReconciling,
                    OrderLeg::Protection => ProtocolPhase::ProtectionUnknown,
                    OrderLeg::Exit => ProtocolPhase::ExitReconciling,
                };
                effect = self.recovery_effect();
            }
            ProtocolInput::Observed {
                leg,
                broker_id,
                broker_ticker,
                quantity,
                filled,
                status,
                holding_quantity,
                pending_present,
                history_complete,
                stop_price,
            } => {
                require(
                    broker_ticker == &self.context.broker_ticker
                        && *holding_quantity >= Decimal::ZERO,
                    "broker identity or quantity conflict",
                )?;
                let late_entry_fill = *leg == OrderLeg::Entry
                    && self.protection.is_some()
                    && self.entry.as_ref().is_some_and(|p| *filled > p.filled);
                let p = self.progress_mut(*leg)?;
                require(
                    p.broker_id.as_ref() == Some(broker_id)
                        && *quantity == p.quantity
                        && *filled >= p.filled
                        && *filled <= p.quantity
                        && (*status != OrderStatus::Filled || *filled == p.quantity),
                    "exact order observation conflicts",
                )?;
                if *leg == OrderLeg::Protection {
                    require(
                        stop_price == &p.stop_price,
                        "observed stop differs from reserved protection",
                    )?;
                }
                p.filled = *filled;
                p.status = *status;
                p.reconciled_terminal = status.terminal() && *history_complete && !*pending_present;
                self.holding_quantity = *holding_quantity;
                self.last_reconciled_at = at;
                if self.remaining() < Decimal::ZERO || self.remaining() != *holding_quantity {
                    self.reconciliation_required = true;
                    self.exit_requested = true;
                    self.phase = ProtocolPhase::Recovery;
                    self.reason = Some("HISTORY_AND_HOLDING_CONFLICT".into());
                    effect = self.recovery_effect();
                } else {
                    self.reconciliation_required = false;
                    match leg {
                        OrderLeg::Entry
                            if self.entry.as_ref().is_some_and(|p| p.reconciled_terminal) =>
                        {
                            self.phase = if self.remaining() == Decimal::ZERO {
                                ProtocolPhase::Rejected
                            } else {
                                ProtocolPhase::ProtectionPrepared
                            };
                        }
                        OrderLeg::Protection => {
                            if self
                                .protection
                                .as_ref()
                                .is_some_and(|p| p.reconciled_terminal)
                            {
                                self.exit_requested = true;
                            } else if matches!(
                                status,
                                OrderStatus::Open | OrderStatus::PartiallyFilled
                            ) && self
                                .protection
                                .as_ref()
                                .is_some_and(|p| p.quantity - p.filled == self.remaining())
                                && self.entry.as_ref().is_some_and(|p| p.reconciled_terminal)
                            {
                                self.phase = ProtocolPhase::Protected;
                            }
                        }
                        OrderLeg::Exit => self.exit_requested = true,
                        _ => {}
                    }
                    if late_entry_fill {
                        self.exit_requested = true;
                        self.reason = Some("LATE_ENTRY_FILL_AFTER_PROTECTION".into());
                    }
                    self.exit_phase();
                }
            }
            ProtocolInput::Rejected { leg } => {
                let p = self.progress_mut(*leg)?;
                require(
                    p.broker_id.is_none() && p.filled == Decimal::ZERO,
                    "rejection conflicts with acknowledged order",
                )?;
                p.status = OrderStatus::Rejected;
                p.reconciled_terminal = true;
                if *leg == OrderLeg::Entry {
                    self.phase = ProtocolPhase::Rejected;
                } else {
                    self.exit_requested = true;
                    self.reason = Some("PROTECTION_OR_EXIT_REJECTED".into());
                    self.exit_phase();
                }
            }
            ProtocolInput::Cancel { leg } => {
                require(
                    !self.reconciliation_required,
                    "broker disconnect requires reconciliation",
                )?;
                let p = self.progress_mut(*leg)?;
                require(!p.reconciled_terminal, "terminal order cannot be cancelled")?;
                let id = p
                    .broker_id
                    .clone()
                    .ok_or_else(|| Error("cannot cancel an unknown order ID".into()))?;
                if p.cancel_requested {
                    effect = self.recovery_effect();
                } else {
                    p.cancel_requested = true;
                    effect = ProtocolEffect::CancelExactOrder { broker_id: id };
                }
                self.phase = match leg {
                    OrderLeg::Entry => ProtocolPhase::EntryCancelPending,
                    OrderLeg::Protection => ProtocolPhase::StopCancelPending,
                    OrderLeg::Exit => ProtocolPhase::ExitReconciling,
                };
            }
            ProtocolInput::CancellationAcknowledged { leg } => {
                require(
                    self.progress(*leg)?.cancel_requested,
                    "unsolicited cancellation acknowledgement",
                )?;
                // The order is still unresolved until pending state, history
                // and actual holdings agree. A late fill is still admissible.
                effect = self.recovery_effect();
            }
            ProtocolInput::DispatchProtection => {
                require(
                    self.phase == ProtocolPhase::ProtectionPrepared
                        && self.protection.is_none()
                        && !self.exit_requested
                        && !self.reconciliation_required
                        && self.remaining() > Decimal::ZERO
                        && at < self.context.flatten_at
                        && (at - self.last_reconciled_at).num_seconds() <= 10,
                    "protection requires final fills and a current reconciled holding",
                )?;
                let mut p = OrderProgress::new(self.remaining(), at);
                p.stop_price = Some(self.desired_stop);
                effect = ProtocolEffect::SubmitProtection {
                    quantity: p.quantity,
                    stop: self.desired_stop,
                };
                self.protection = Some(p);
                self.phase = ProtocolPhase::ProtectionUnknown;
            }
            ProtocolInput::RequestExit { reason } => {
                require(valid_id(reason), "exit reason missing")?;
                self.exit_requested = true;
                self.reason = Some(reason.clone());
                self.exit_phase();
                effect = self.recovery_effect();
            }
            ProtocolInput::DispatchExit => {
                require(
                    self.phase == ProtocolPhase::ExitPrepared
                        && self.exit_requested
                        && !self.reconciliation_required
                        && self.all_orders().all(|p| p.reconciled_terminal)
                        && self.remaining() > Decimal::ZERO
                        && self.remaining() == self.holding_quantity
                        && (at - self.last_reconciled_at).num_seconds() <= 10,
                    "flatten requires reconciled sellable quantity and terminal prior orders",
                )?;
                let p = OrderProgress::new(self.remaining(), at);
                effect = ProtocolEffect::SubmitFlatten {
                    quantity: p.quantity,
                };
                self.exits.push(p);
                self.phase = ProtocolPhase::ExitUnknown;
            }
            ProtocolInput::HoldingReconciled {
                quantity,
                pending_order_ids,
                history_complete,
            } => {
                require(
                    *quantity >= Decimal::ZERO && pending_order_ids.len() <= 64,
                    "invalid holding reconciliation",
                )?;
                let known: Vec<_> = self
                    .all_orders()
                    .filter_map(|p| p.broker_id.clone())
                    .collect();
                self.holding_quantity = *quantity;
                self.last_reconciled_at = at;
                self.reconciliation_required = !*history_complete
                    || *quantity != self.remaining()
                    || pending_order_ids.iter().any(|id| !known.contains(id))
                    || self.all_orders().any(|p| {
                        p.reconciled_terminal
                            && p.broker_id
                                .as_ref()
                                .is_some_and(|id| pending_order_ids.contains(id))
                    });
                if self.reconciliation_required {
                    self.phase = ProtocolPhase::Recovery;
                    effect = self.recovery_effect();
                }
                self.exit_phase();
            }
            ProtocolInput::Disconnected => {
                self.reconciliation_required = true;
                self.reason = Some("BROKER_DISCONNECTED".into());
                effect = self.recovery_effect();
            }
            ProtocolInput::Tick => {
                let exposure_expired = self.unprotected_since.is_some_and(|since| {
                    (at - since).num_seconds() >= self.context.max_unprotected_seconds
                });
                if at >= self.context.flatten_at || exposure_expired {
                    self.exit_requested = true;
                    self.reason = Some("DEADLINE_OR_UNPROTECTED_EXPOSURE".into());
                    self.exit_phase();
                    effect = self.recovery_effect();
                }
            }
            ProtocolInput::TightenStop { price } => {
                require(
                    *price >= self.desired_stop
                        && *price > Decimal::ZERO
                        && self.remaining() > Decimal::ZERO,
                    "a stop may never widen after entry",
                )?;
                // This records a bounded proposal. The verified broker stop
                // remains in force; it is not falsely marked amended.
                self.desired_stop = *price;
            }
        }
        if (self.remaining() > Decimal::ZERO || self.holding_quantity > Decimal::ZERO)
            && self.phase != ProtocolPhase::Protected
        {
            self.unprotected_since.get_or_insert(at);
        } else {
            self.unprotected_since = None;
        }
        self.revision += 1;
        self.last_event_at = at;
        Ok(effect)
    }
    pub fn transition(
        &self,
        event_id: String,
        input: ProtocolInput,
        at: DateTime<Utc>,
    ) -> Result<ProtocolRecord> {
        require(valid_id(&event_id), "invalid protocol event ID")?;
        let previous_hash = state_hash(self)?;
        let mut state = self.clone();
        let effect = state.reduce(&input, at)?;
        Ok(ProtocolRecord {
            event_id,
            actor: if matches!(input, ProtocolInput::ReviewedBrokerId { .. }) {
                "OPERATOR_REVIEWED_RECOVERY".into()
            } else {
                "EUR_COORDINATOR".into()
            },
            previous_phase: self.phase,
            previous_hash,
            input,
            at,
            state,
            effect,
        })
    }
}
pub fn state_hash(state: &ProtocolState) -> Result<String> {
    Ok(content_hash(&serde_json::to_string(state).map_err(
        |_| Error("protocol hash encoding failed".into()),
    )?))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRecord {
    pub event_id: String,
    pub actor: String,
    pub previous_phase: ProtocolPhase,
    pub previous_hash: String,
    pub input: ProtocolInput,
    pub at: DateTime<Utc>,
    pub state: ProtocolState,
    pub effect: ProtocolEffect,
}
pub fn recover(records: &[ProtocolRecord]) -> Result<ProtocolState> {
    require(
        !records.is_empty() && records.len() <= 10000,
        "protocol history missing or oversized",
    )?;
    let mut state = ProtocolState::new(records[0].state.context.clone())?;
    let mut ids = std::collections::BTreeSet::new();
    for record in records {
        require(ids.insert(&record.event_id), "duplicate protocol event ID")?;
        let expected =
            state.transition(record.event_id.clone(), record.input.clone(), record.at)?;
        require(
            expected == *record,
            "protocol state, effect or hash chain differs",
        )?;
        state = record.state.clone();
    }
    Ok(state)
}
pub trait ProtocolJournal {
    fn load(&mut self, intent_id: &str) -> Result<Vec<ProtocolRecord>>;
    fn append(&mut self, expected_revision: u64, record: &ProtocolRecord) -> Result<()>;
}
/// Only return an effect after its UNKNOWN dispatch marker has committed.
/// Replaying the same event after a crash always reconciles, never resubmits.
pub fn advance<J: ProtocolJournal>(
    journal: &mut J,
    context: &ProtocolContext,
    event_id: String,
    input: ProtocolInput,
    at: DateTime<Utc>,
) -> Result<ProtocolEffect> {
    let records = journal.load(&context.intent_id)?;
    let state = if records.is_empty() {
        ProtocolState::new(context.clone())?
    } else {
        recover(&records)?
    };
    require(
        state.context == *context,
        "protocol intent authority rebound",
    )?;
    if let Some(old) = records.iter().find(|r| r.event_id == event_id) {
        require(old.input == input, "protocol event ID rebound")?;
        return Ok(state.recovery_effect());
    }
    let record = state.transition(event_id, input, at)?;
    journal.append(state.revision, &record)?;
    Ok(record.effect)
}

pub mod scenarios;

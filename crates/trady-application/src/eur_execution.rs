//! Target EUR execution cycle. This is the concrete bridge between the
//! deterministic order gate/coordinator and the broker mutation port.

use crate::telegram::{LifecycleEvent, LifecycleEventKind};
use crate::{
    broker::{BrokerOrderRequest, BrokerResult, OrderKind, TimeInForce},
    eur_runtime::{
        EurCoordinator, EurRuntimeConfig, HoldingAction, HoldingDecision, HoldingObservation,
        OrderReadiness, build_risk_plan, evaluate_holding, evaluate_order_readiness,
    },
};
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use trady_domain::{
    AccountId, AccountSnapshot, BrokerOrder, Capabilities, CoordinatorEvent, CoordinatorEventKind,
    CoordinatorState, EurInstrumentBinding, EurUniverseSnapshot, InstrumentId, Quote, Result,
    SessionWindow, Side, SignalSnapshot, VenueQuote, require,
};
use trady_policy::economics::ExitCosts;
use trady_policy::intraday::{
    DynamicExitDecision, IntradayTradePlan, RotationAssessment, approve_rotation,
};
use trady_policy::{EntryCosts, InstrumentRules};

pub trait EurExecutionPort {
    fn observe_account(&mut self, account_id: &trady_domain::AccountId) -> Result<AccountSnapshot>;
    fn submit_eur_order_before(
        &mut self,
        request: &BrokerOrderRequest,
        binding: &EurInstrumentBinding,
        expires_at: DateTime<Utc>,
    ) -> BrokerResult<BrokerOrder>;
    fn cancel_order(
        &mut self,
        account: &trady_domain::AccountId,
        broker_order_id: &str,
    ) -> BrokerResult<crate::broker::CancellationReceipt>;
}

/// Append-only journal boundary for the target executor. Storage adapters can
/// implement it without granting persistence or mutation capabilities to the
/// analyst or broker transport.
pub trait EurExecutionJournal: Send {
    fn coordinator_event(&mut self, event: &CoordinatorEvent) -> Result<()>;
    fn trade_plan(&mut self, plan: &IntradayTradePlan) -> Result<()>;
    fn order_intent(&mut self, request: &BrokerOrderRequest, at: DateTime<Utc>) -> Result<()>;
    fn broker_order(&mut self, order: &BrokerOrder, at: DateTime<Utc>) -> Result<()>;
    fn dynamic_exit(&mut self, decision: &DynamicExitDecision) -> Result<()>;
}

pub trait EurLifecycleSink: Send {
    fn emit(&mut self, event: &LifecycleEvent) -> Result<()>;
}

struct NoopLifecycleSink;
impl EurLifecycleSink for NoopLifecycleSink {
    fn emit(&mut self, _event: &LifecycleEvent) -> Result<()> {
        Ok(())
    }
}

struct NoopExecutionJournal;
impl EurExecutionJournal for NoopExecutionJournal {
    fn coordinator_event(&mut self, _event: &CoordinatorEvent) -> Result<()> {
        Ok(())
    }
    fn trade_plan(&mut self, _plan: &IntradayTradePlan) -> Result<()> {
        Ok(())
    }
    fn order_intent(&mut self, _request: &BrokerOrderRequest, _at: DateTime<Utc>) -> Result<()> {
        Ok(())
    }
    fn broker_order(&mut self, _order: &BrokerOrder, _at: DateTime<Utc>) -> Result<()> {
        Ok(())
    }
    fn dynamic_exit(&mut self, _decision: &DynamicExitDecision) -> Result<()> {
        Ok(())
    }
}

struct SharedSink<S>(Arc<Mutex<S>>);
impl<S: EurExecutionJournal> EurExecutionJournal for SharedSink<S> {
    fn coordinator_event(&mut self, event: &CoordinatorEvent) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| trady_domain::Error("EUR event sink lock poisoned".into()))?
            .coordinator_event(event)
    }
    fn trade_plan(&mut self, plan: &IntradayTradePlan) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| trady_domain::Error("EUR event sink lock poisoned".into()))?
            .trade_plan(plan)
    }
    fn order_intent(&mut self, request: &BrokerOrderRequest, at: DateTime<Utc>) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| trady_domain::Error("EUR event sink lock poisoned".into()))?
            .order_intent(request, at)
    }
    fn broker_order(&mut self, order: &BrokerOrder, at: DateTime<Utc>) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| trady_domain::Error("EUR event sink lock poisoned".into()))?
            .broker_order(order, at)
    }
    fn dynamic_exit(&mut self, decision: &DynamicExitDecision) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| trady_domain::Error("EUR event sink lock poisoned".into()))?
            .dynamic_exit(decision)
    }
}
impl<S: EurLifecycleSink> EurLifecycleSink for SharedSink<S> {
    fn emit(&mut self, event: &LifecycleEvent) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| trady_domain::Error("EUR event sink lock poisoned".into()))?
            .emit(event)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EurEntryOutcome {
    pub plan: IntradayTradePlan,
    pub order: BrokerOrder,
    pub readiness: OrderReadiness,
    pub coordinator_state: CoordinatorState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EurExitOutcome {
    pub decision: HoldingDecision,
    pub order: Option<BrokerOrder>,
    pub coordinator_state: CoordinatorState,
}

pub struct EurExecutionCycle<B: EurExecutionPort> {
    pub config: EurRuntimeConfig,
    pub coordinator: EurCoordinator,
    pub broker: B,
    entries_today: u8,
    exits_today: u8,
    journal: Box<dyn EurExecutionJournal>,
    lifecycle: Box<dyn EurLifecycleSink>,
    journal_bound: bool,
    lifecycle_bound: bool,
}

impl<B: EurExecutionPort> EurExecutionCycle<B> {
    pub fn new(config: EurRuntimeConfig, coordinator: EurCoordinator, broker: B) -> Result<Self> {
        config.validate(Utc::now())?;
        Ok(Self {
            config,
            coordinator,
            broker,
            entries_today: 0,
            exits_today: 0,
            journal: Box::new(NoopExecutionJournal),
            lifecycle: Box::new(NoopLifecycleSink),
            journal_bound: false,
            lifecycle_bound: false,
        })
    }

    pub fn with_journal<J: EurExecutionJournal + 'static>(mut self, journal: J) -> Self {
        self.journal = Box::new(journal);
        self.journal_bound = true;
        self
    }

    pub fn with_lifecycle_sink<S: EurLifecycleSink + 'static>(mut self, sink: S) -> Self {
        self.lifecycle = Box::new(sink);
        self.lifecycle_bound = true;
        self
    }

    /// Bind one durable object that records both execution evidence and
    /// lifecycle events. The shared lock keeps the two append paths on the
    /// same underlying journal without duplicating ownership.
    pub fn with_combined_sink<S>(mut self, sink: S) -> Self
    where
        S: EurExecutionJournal + EurLifecycleSink + 'static,
    {
        let shared = Arc::new(Mutex::new(sink));
        self.journal = Box::new(SharedSink(shared.clone()));
        self.lifecycle = Box::new(SharedSink(shared));
        self.journal_bound = true;
        self.lifecycle_bound = true;
        self
    }

    pub fn entries_today(&self) -> u8 {
        self.entries_today
    }

    pub fn exits_today(&self) -> u8 {
        self.exits_today
    }

    #[allow(clippy::too_many_arguments)]
    fn transition(
        &mut self,
        event_id: String,
        kind: CoordinatorEventKind,
        to: CoordinatorState,
        instrument_id: Option<InstrumentId>,
        evidence_snapshot_ids: Vec<String>,
        reason: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.coordinator.transition(
            event_id,
            kind,
            to,
            instrument_id,
            evidence_snapshot_ids,
            reason,
            now,
        )?;
        let event = self
            .coordinator
            .events()
            .last()
            .cloned()
            .ok_or_else(|| trady_domain::Error("coordinator event was not recorded".into()))?;
        self.journal.coordinator_event(&event)?;
        let lifecycle = LifecycleEvent {
            event_id: format!("telegram_{}", event.event_id),
            kind: lifecycle_kind(event.kind, event.to, event.reason.as_deref()),
            account_id: trady_domain::AccountId::new(event.account_id.clone())?,
            session_id: self.coordinator.snapshot().session_id.clone(),
            instrument_id: event.instrument_id.clone(),
            evidence_ids: event.evidence_snapshot_ids.clone(),
            text: event
                .reason
                .clone()
                .unwrap_or_else(|| format!("coordinator {:?} -> {:?}", event.from, event.to)),
            occurred_at: event.occurred_at,
        };
        self.lifecycle.emit(&lifecycle)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn enter(
        &mut self,
        account: &AccountSnapshot,
        universe: &EurUniverseSnapshot,
        binding: &EurInstrumentBinding,
        venue_quote: &VenueQuote,
        session: &SessionWindow,
        capabilities: &Capabilities,
        selection: &trady_domain::SelectionRun,
        signal: &SignalSnapshot,
        entry_costs: &EntryCosts,
        exit_costs: &ExitCosts,
        rules: &InstrumentRules,
        now: DateTime<Utc>,
    ) -> Result<EurEntryOutcome> {
        self.config.validate(now)?;
        if self.config.enabled {
            require(
                self.journal_bound,
                "EUR mutation requires a durable target execution journal",
            )?;
            if self.config.telegram_enabled {
                require(
                    self.lifecycle_bound,
                    "EUR mutation requires a Telegram lifecycle sink when enabled",
                )?;
            }
        }
        require(
            self.coordinator.snapshot().state == CoordinatorState::Idle,
            "EUR entry cycle requires an idle coordinator",
        )?;
        require(
            self.entries_today < self.config.policy.max_daily_entries,
            "EUR daily entry limit reached",
        )?;
        require(
            account.account_id == self.config.account_id,
            "EUR entry account differs from runtime authority",
        )?;
        require(
            matches!(
                signal.formula_id.as_str(),
                "donchian_atr_intraday_v1" | "perpetual_intraday_v1"
            ) && selection.candidates.iter().any(|candidate| {
                candidate.instrument_id == binding.instrument_id
                    && candidate.eligible
                    && !candidate.strategy_snapshot_ids.is_empty()
            }),
            "EUR entry requires a registered deterministic strategy snapshot",
        )?;
        // The caller's snapshot is context only. A fresh broker observation is
        // mandatory immediately before planning and is the account authority
        // used by the gate and sizing calculation.
        let observed = self
            .broker
            .observe_account(&self.config.account_id)
            .map_err(|error| trady_domain::Error(error.to_string()))?;
        observed.validate()?;
        require(
            observed.account_id == self.config.account_id
                && observed.positions.is_empty()
                && observed.orders.iter().all(|order| order.status.terminal()),
            "EUR entry requires a reconciled flat account with no pending order",
        )?;
        let check_now = now.max(observed.observed_at);
        let legacy_quote = legacy_quote(venue_quote)?;
        let plan = build_risk_plan(
            &observed,
            &legacy_quote,
            signal,
            entry_costs,
            exit_costs,
            rules,
            session.flatten_at,
            &self.config.policy,
            format!("plan_{}", now.timestamp_millis()),
            selection.evidence_snapshot_ids.clone(),
            check_now,
        )?;
        let readiness = evaluate_order_readiness(
            &self.config,
            &observed,
            universe,
            binding,
            venue_quote,
            session,
            capabilities,
            selection,
            &plan,
            self.entries_today,
            check_now,
        )?;
        let OrderReadiness::Approved { .. } = readiness else {
            return Err(trady_domain::Error(
                "EUR order-readiness gate blocked entry".into(),
            ));
        };
        self.journal.trade_plan(&plan)?;
        let instrument_id = binding.instrument_id.clone();
        self.transition(
            format!("event_research_{}", check_now.timestamp_millis()),
            CoordinatorEventKind::StartResearch,
            CoordinatorState::Researching,
            None,
            selection.evidence_snapshot_ids.clone(),
            None,
            check_now,
        )?;
        self.transition(
            format!("event_ranked_{}", check_now.timestamp_millis()),
            CoordinatorEventKind::CandidatesRanked,
            CoordinatorState::CandidatesRanked,
            Some(instrument_id.clone()),
            selection.evidence_snapshot_ids.clone(),
            None,
            check_now,
        )?;
        self.transition(
            format!("event_validated_{}", check_now.timestamp_millis()),
            CoordinatorEventKind::CandidateValidated,
            CoordinatorState::CandidateValidated,
            Some(instrument_id.clone()),
            selection.evidence_snapshot_ids.clone(),
            None,
            check_now,
        )?;
        self.transition(
            format!("event_signal_{}", check_now.timestamp_millis()),
            CoordinatorEventKind::SignalProduced,
            CoordinatorState::SignalReady,
            Some(instrument_id.clone()),
            vec![signal.bar_snapshot_id.clone()],
            None,
            check_now,
        )?;
        self.transition(
            format!("event_risk_{}", check_now.timestamp_millis()),
            CoordinatorEventKind::RiskApproved,
            CoordinatorState::RiskApproved,
            Some(instrument_id.clone()),
            selection.evidence_snapshot_ids.clone(),
            None,
            check_now,
        )?;
        let request = BrokerOrderRequest {
            intent_id: trady_domain::IntentId::new(format!(
                "intent_{}",
                check_now.timestamp_millis()
            ))?,
            account_id: observed.account_id.clone(),
            instrument_id: InstrumentId::new(binding.broker_instrument_id.clone())?,
            instrument_currency: binding.currency.clone(),
            side: Side::Buy,
            quantity: plan.quantity,
            kind: OrderKind::Limit {
                price: plan.entry_price,
                time_in_force: TimeInForce::Day,
            },
        };
        self.transition(
            format!("event_pending_{}", check_now.timestamp_millis()),
            CoordinatorEventKind::EntrySubmitted,
            CoordinatorState::EntryPending,
            Some(instrument_id.clone()),
            selection.evidence_snapshot_ids.clone(),
            None,
            check_now,
        )?;
        self.journal.order_intent(&request, check_now)?;
        self.entries_today = self.entries_today.saturating_add(1);
        let mut order = self
            .broker
            .submit_eur_order_before(&request, binding, plan.flatten_deadline)
            .map_err(|error| trady_domain::Error(error.to_string()))?;
        bind_order_intent(&mut order, &request)?;
        self.journal.broker_order(&order, check_now)?;
        self.emit_order_lifecycle(
            LifecycleEventKind::OrderAcknowledged,
            &instrument_id,
            &plan.evidence_snapshot_ids,
            &format!("entry broker order {} acknowledged", order.broker_order_id),
            check_now,
        )?;
        if order.status == trady_domain::OrderStatus::Filled {
            self.transition(
                format!("event_open_{}", check_now.timestamp_millis()),
                CoordinatorEventKind::EntryFilled,
                CoordinatorState::Open,
                Some(instrument_id),
                selection.evidence_snapshot_ids.clone(),
                None,
                check_now,
            )?;
        }
        Ok(EurEntryOutcome {
            plan,
            order,
            readiness,
            coordinator_state: self.coordinator.snapshot().state,
        })
    }

    pub fn manage(
        &mut self,
        observation: &HoldingObservation,
        binding: &EurInstrumentBinding,
        now: DateTime<Utc>,
    ) -> Result<EurExitOutcome> {
        self.config.validate(now)?;
        require(
            matches!(
                self.coordinator.snapshot().state,
                CoordinatorState::Open | CoordinatorState::ExitManaging
            ),
            "holding management requires an open coordinator",
        )?;
        require(
            binding.instrument_id == observation.plan.instrument_id,
            "holding binding does not match the reviewed plan",
        )?;
        let decision = evaluate_holding(observation)?;
        if let Some(exit) = &decision.exit {
            self.journal.dynamic_exit(exit)?;
        }
        if matches!(
            decision.action,
            HoldingAction::Hold | HoldingAction::TightenStop
        ) {
            return Ok(EurExitOutcome {
                decision,
                order: None,
                coordinator_state: self.coordinator.snapshot().state,
            });
        }
        require(
            self.exits_today < self.config.policy.max_daily_exits,
            "EUR daily exit limit reached",
        )?;
        let account_id = trady_domain::AccountId::new(observation.plan.account_id.clone())?;
        let observed = self
            .broker
            .observe_account(&account_id)
            .map_err(|error| trady_domain::Error(error.to_string()))?;
        observed.validate()?;
        require(
            observed.account_id == self.config.account_id
                && observed.orders.iter().all(|order| order.status.terminal())
                && observed.positions.iter().any(|position| {
                    position.instrument_id.as_str() == binding.broker_instrument_id
                        && position.quantity >= observation.plan.quantity
                }),
            "EUR exit requires a reconciled broker position with no pending order",
        )?;
        self.transition(
            format!("event_exit_{}", now.timestamp_millis()),
            CoordinatorEventKind::ExitStarted,
            CoordinatorState::ExitManaging,
            Some(observation.plan.instrument_id.clone()),
            observation.plan.evidence_snapshot_ids.clone(),
            Some(format!("{:?}", decision.reason)),
            now,
        )?;
        let broker_instrument = InstrumentId::new(binding.broker_instrument_id.clone())?;
        let request = BrokerOrderRequest {
            intent_id: trady_domain::IntentId::new(format!("exit_{}", now.timestamp_millis()))?,
            account_id: trady_domain::AccountId::new(observation.plan.account_id.clone())?,
            instrument_id: broker_instrument,
            instrument_currency: binding.currency.clone(),
            side: Side::Sell,
            quantity: observation.plan.quantity,
            kind: OrderKind::Market {
                extended_hours: false,
            },
        };
        self.journal.order_intent(&request, now)?;
        self.exits_today = self.exits_today.saturating_add(1);
        // A late recovery must still be able to submit the risk-reducing
        // flatten. Entry expiry is hard; an exit gets a short broker grace
        // window when the deadline has already latched.
        let exit_expiry = if now >= observation.plan.flatten_deadline {
            now.checked_add_signed(chrono::Duration::seconds(30))
                .ok_or_else(|| trady_domain::Error("EUR exit expiry overflow".into()))?
        } else {
            observation.plan.flatten_deadline
        };
        let mut order = self
            .broker
            .submit_eur_order_before(&request, binding, exit_expiry)
            .map_err(|error| trady_domain::Error(error.to_string()))?;
        bind_order_intent(&mut order, &request)?;
        self.journal.broker_order(&order, now)?;
        self.emit_order_lifecycle(
            LifecycleEventKind::OrderAcknowledged,
            &observation.plan.instrument_id,
            &observation.plan.evidence_snapshot_ids,
            &format!("exit broker order {} acknowledged", order.broker_order_id),
            now,
        )?;
        if order.status == trady_domain::OrderStatus::Filled {
            self.transition(
                format!("event_closed_{}", now.timestamp_millis()),
                CoordinatorEventKind::ExitClosed,
                CoordinatorState::Closed,
                Some(observation.plan.instrument_id.clone()),
                observation.plan.evidence_snapshot_ids.clone(),
                None,
                now,
            )?;
        }
        Ok(EurExitOutcome {
            decision,
            order: Some(order),
            coordinator_state: self.coordinator.snapshot().state,
        })
    }

    /// Rotate only after the existing position has been verified closed. The
    /// same order-readiness gate is then run for the replacement entry.
    #[allow(clippy::too_many_arguments)]
    pub fn rotate(
        &mut self,
        assessment: &RotationAssessment,
        account: &AccountSnapshot,
        universe: &EurUniverseSnapshot,
        replacement: &EurInstrumentBinding,
        venue_quote: &VenueQuote,
        session: &SessionWindow,
        capabilities: &Capabilities,
        selection: &trady_domain::SelectionRun,
        signal: &SignalSnapshot,
        entry_costs: &EntryCosts,
        exit_costs: &ExitCosts,
        rules: &InstrumentRules,
        now: DateTime<Utc>,
    ) -> Result<EurEntryOutcome> {
        require(
            self.coordinator.snapshot().state == CoordinatorState::Closed,
            "EUR rotation requires the current position to be closed",
        )?;
        require(
            assessment.replacement_instrument_id == replacement.instrument_id,
            "rotation replacement differs from the reviewed binding",
        )?;
        approve_rotation(assessment, &self.config.policy, now)?;
        let observed = self
            .broker
            .observe_account(&self.config.account_id)
            .map_err(|error| trady_domain::Error(error.to_string()))?;
        observed.validate()?;
        require(
            observed.positions.is_empty()
                && observed.orders.iter().all(|order| order.status.terminal()),
            "EUR rotation requires broker reconciliation to be flat",
        )?;
        self.transition(
            format!("event_reset_{}", now.timestamp_millis()),
            CoordinatorEventKind::Reset,
            CoordinatorState::Idle,
            None,
            vec![],
            Some("verified_same_day_rotation".into()),
            now,
        )?;
        self.enter(
            account,
            universe,
            replacement,
            venue_quote,
            session,
            capabilities,
            selection,
            signal,
            entry_costs,
            exit_costs,
            rules,
            now,
        )
    }

    fn emit_order_lifecycle(
        &mut self,
        kind: LifecycleEventKind,
        instrument_id: &InstrumentId,
        evidence_ids: &[String],
        text: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let snapshot = self.coordinator.snapshot();
        self.lifecycle.emit(&LifecycleEvent {
            event_id: format!(
                "telegram_order_{}_{}",
                kind as u8,
                crate::harness::content_hash(text)
                    .get(..16)
                    .unwrap_or("event")
            ),
            kind,
            account_id: AccountId::new(snapshot.account_id.clone())?,
            session_id: snapshot.session_id.clone(),
            instrument_id: Some(instrument_id.clone()),
            evidence_ids: evidence_ids.to_vec(),
            text: text.into(),
            occurred_at: now,
        })
    }
}

fn lifecycle_kind(
    event: CoordinatorEventKind,
    to: CoordinatorState,
    reason: Option<&str>,
) -> LifecycleEventKind {
    match event {
        CoordinatorEventKind::StartResearch => LifecycleEventKind::ResearchStarted,
        CoordinatorEventKind::CandidatesRanked => LifecycleEventKind::CandidatesRanked,
        CoordinatorEventKind::CandidateValidated => LifecycleEventKind::CandidateSelected,
        CoordinatorEventKind::SignalProduced | CoordinatorEventKind::RiskApproved => {
            LifecycleEventKind::EvidenceUpdated
        }
        CoordinatorEventKind::EntrySubmitted => LifecycleEventKind::OrderIntent,
        CoordinatorEventKind::EntryFilled => LifecycleEventKind::FillObserved,
        CoordinatorEventKind::ExitStarted => {
            if to == CoordinatorState::EmergencyExit
                || reason.is_some_and(|reason| {
                    reason.contains("Deadline")
                        || reason.contains("StaleData")
                        || reason.contains("BrokerDisconnect")
                        || reason.contains("Halt")
                })
            {
                LifecycleEventKind::ForcedFlatten
            } else {
                LifecycleEventKind::ExitUpdated
            }
        }
        CoordinatorEventKind::ExitClosed => LifecycleEventKind::PositionClosed,
        CoordinatorEventKind::Reject
        | CoordinatorEventKind::Expire
        | CoordinatorEventKind::Emergency
        | CoordinatorEventKind::Recover => LifecycleEventKind::RiskHalt,
        CoordinatorEventKind::Reset => LifecycleEventKind::ResearchStarted,
    }
}

fn legacy_quote(quote: &VenueQuote) -> Result<Quote> {
    let bid = quote
        .bid
        .ok_or_else(|| trady_domain::Error("EUR venue quote bid unavailable".into()))?;
    let ask = quote
        .ask
        .ok_or_else(|| trady_domain::Error("EUR venue quote ask unavailable".into()))?;
    Ok(Quote {
        instrument_id: quote.instrument_id.clone(),
        currency: quote.currency.clone(),
        bid,
        ask,
        observed_at: quote.observed_at,
        received_at: quote.received_at,
        tradable: quote.quality == trady_domain::QuoteQuality::Executable,
    })
}

fn bind_order_intent(order: &mut BrokerOrder, request: &BrokerOrderRequest) -> Result<()> {
    require(
        order.instrument_id == request.instrument_id
            && order.side == request.side
            && order.quantity == request.quantity
            && order
                .client_intent_id
                .as_ref()
                .is_none_or(|intent| intent == &request.intent_id),
        "broker acknowledgement does not match EUR order intent",
    )?;
    order.client_intent_id = Some(request.intent_id.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{BrokerFailure, BrokerFailureKind};
    use rust_decimal::Decimal;
    use trady_domain::{AccountId, BrokerOrder, Currency, Mode, Money, OrderStatus, SelectionRun};

    #[derive(Default)]
    struct FakeBroker {
        orders: Vec<BrokerOrder>,
    }
    impl EurExecutionPort for FakeBroker {
        fn observe_account(&mut self, account: &AccountId) -> Result<AccountSnapshot> {
            Ok(AccountSnapshot {
                snapshot_id: "snapshot".into(),
                account_id: account.clone(),
                mode: Mode::Paper,
                spendable_cash: Money {
                    amount: Decimal::new(10_000, 0),
                    currency: Currency::new("EUR")?,
                },
                positions: vec![],
                orders: self.orders.clone(),
                observed_at: Utc::now(),
            })
        }
        fn submit_eur_order_before(
            &mut self,
            request: &BrokerOrderRequest,
            _binding: &EurInstrumentBinding,
            _expires_at: DateTime<Utc>,
        ) -> BrokerResult<BrokerOrder> {
            let order = BrokerOrder {
                broker_order_id: format!("broker_{}", self.orders.len() + 1),
                client_intent_id: Some(request.intent_id.clone()),
                instrument_id: request.instrument_id.clone(),
                side: request.side,
                quantity: request.quantity,
                filled_quantity: request.quantity,
                status: OrderStatus::Filled,
            };
            self.orders.push(order.clone());
            Ok(order)
        }
        fn cancel_order(
            &mut self,
            _account: &AccountId,
            _broker_order_id: &str,
        ) -> BrokerResult<crate::broker::CancellationReceipt> {
            Err(BrokerFailure::new(BrokerFailureKind::Unsupported))
        }
    }

    // The full entry-cycle fixture is intentionally kept in the application
    // contract tests; this smoke test ensures the fake port is still isolated.
    #[test]
    fn fake_target_port_is_mutation_scoped() {
        let mut broker = FakeBroker::default();
        assert!(
            broker
                .observe_account(&AccountId::new("paper_eur").unwrap())
                .is_ok()
        );
        let _ = SelectionRun {
            schema_version: 1,
            run_id: "r".into(),
            account_id: "paper_eur".into(),
            session_id: "s".into(),
            universe_snapshot_id: "u".into(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
            model: trady_domain::selection::ModelFingerprint {
                run_id: "r".into(),
                session_id: "s".into(),
                provider: "opencode-go".into(),
                model: "deepseek-v4.1-flash".into(),
                prompt_hash: "a".repeat(64),
                tool_manifest_hash: "b".repeat(64),
                model_revision: "c".repeat(64),
            },
            fallback_attempts: vec![],
            candidates: vec![],
            selected_instrument_id: None,
            proposal: None,
            evidence_snapshot_ids: vec![],
            failure: None,
        };
    }
}

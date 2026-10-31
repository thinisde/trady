//! Use cases and ports. Analyst and gateway deliberately receive no broker mutation port.

pub mod broker;
pub mod contracts;
pub mod harness;
pub mod optimization;
pub mod research;
pub mod sentiment;
pub mod telegram;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use trady_domain::*;
use trady_policy::{Policy, fresh};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoundationConfig {
    pub config_version: u32,
    pub mode: Mode,
    pub account_id: AccountId,
    pub base_currency: Currency,
    pub policy: Policy,
    pub capabilities: Capabilities,
}
impl FoundationConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1,
            "unsupported configuration version",
        )?;
        self.policy.validate()?;
        require(
            self.mode == Mode::Paper && self.account_id.as_str().starts_with("paper_"),
            "foundation supports explicitly named paper accounts only",
        )
    }
    pub fn entry_blockers(&self, now: DateTime<Utc>) -> Vec<&'static str> {
        let mut reasons = vec![
            "research/readiness verifier is not implemented",
            "approved skill bundle is not loaded",
            "usable Perpetual assessment is unavailable",
            "validated execution and protection workflow is not implemented",
        ];
        if ![
            &self.capabilities.cash_account,
            &self.capabilities.instrument,
            &self.capabilities.protective_orders,
            &self.capabilities.quotes,
        ]
        .iter()
        .all(|c| c.usable(now))
        {
            reasons.push("required provider capabilities are unknown, unsupported or expired");
        }
        reasons
    }
}

pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeSession {
    pub session_id: String,
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
}
impl ExchangeSession {
    pub fn permits_entry(&self, now: DateTime<Utc>) -> bool {
        self.opens_at < self.closes_at
            && now >= self.opens_at
            && self.closes_at.signed_duration_since(now) > chrono::Duration::minutes(30)
    }
}
pub trait ExchangeCalendar {
    fn session(&self, now: DateTime<Utc>) -> Option<ExchangeSession>;
}
pub trait QuotePort {
    fn quote(&mut self, instrument: &InstrumentId) -> Result<Quote>;
}

/// Read-only broker information boundary; no submission or cancellation operations.
pub trait BrokerInformationPort {
    fn snapshot(&mut self, account: &AccountId) -> Result<AccountSnapshot>;
}

/// Observation/recovery boundary. Brokers without client intent IDs must report that limitation.
pub trait BrokerPort: BrokerInformationPort {
    fn capabilities(&self) -> Capabilities;
    fn find_order(&mut self, intent: &IntentId) -> Result<Option<BrokerOrder>>;
}

pub trait ExecutionRepository {
    fn acquire(
        &mut self,
        account: &AccountId,
        worker: &WorkerId,
        ttl_seconds: i32,
    ) -> Result<Fence>;
    fn renew(&mut self, fence: &Fence, ttl_seconds: i32) -> Result<()>;
    fn unresolved_intents(&mut self, account: &AccountId) -> Result<Vec<OrderIntent>>;
    fn save_reconciliation(&mut self, fence: &Fence, reconciliation: &Reconciliation)
    -> Result<()>;
    fn latest_reconciliation(&mut self, account: &AccountId) -> Result<Option<Reconciliation>>;
    /// Offline fixture reservation only. SQL independently requires a paper account.
    fn reserve_paper_intent(&mut self, fence: &Fence, intent: &OrderIntent) -> Result<bool>;
}

/// Reconciliation never submits an order and never assumes an unobserved order failed.
pub fn reconcile(
    snapshot: AccountSnapshot,
    intents: &[OrderIntent],
    now: DateTime<Utc>,
) -> Result<Reconciliation> {
    reconcile_observed(snapshot, intents, &[], now)
}

fn reconcile_observed(
    snapshot: AccountSnapshot,
    intents: &[OrderIntent],
    previous_positions: &[Position],
    now: DateTime<Utc>,
) -> Result<Reconciliation> {
    snapshot.validate()?;
    require(
        fresh(snapshot.observed_at, now, 60),
        "cannot reconcile stale or future broker state",
    )?;
    let mut issues = Vec::new();
    let mut resolutions = Vec::new();
    if snapshot.positions.len() > 1 {
        issues.push("multiple distinct holdings require recovery".into());
    }
    for position in &snapshot.positions {
        if !previous_positions.contains(position)
            && !intents
                .iter()
                .any(|i| i.instrument_id == position.instrument_id && i.side == Side::Buy)
        {
            issues.push(format!(
                "unexpected holding or quantity change for {}",
                position.instrument_id
            ));
        }
    }
    for previous in previous_positions {
        if !snapshot.positions.contains(previous)
            && !intents
                .iter()
                .any(|i| i.instrument_id == previous.instrument_id)
        {
            issues.push(format!(
                "unexplained holding change for {}",
                previous.instrument_id
            ));
        }
    }
    for intent in intents {
        intent.validate()?;
        require(
            intent.account_id == snapshot.account_id,
            "intent belongs to another account",
        )?;
        if intent.side == Side::Sell {
            issues.push("sell-intent recovery is not implemented in the foundation".into());
        }
        let matches: Vec<_> = snapshot
            .orders
            .iter()
            .filter(|order| {
                order.client_intent_id.as_ref() == Some(&intent.intent_id)
                    || intent.broker_order_id.as_ref() == Some(&order.broker_order_id)
            })
            .collect();
        if matches.len() != 1 {
            issues.push(format!(
                "intent {} has {} matching orders; reconcile without resubmission",
                intent.intent_id,
                matches.len()
            ));
            resolutions.push(IntentResolution {
                intent_id: intent.intent_id.clone(),
                state: IntentState::Unknown,
                broker_order_id: intent.broker_order_id.clone(),
            });
            continue;
        }
        let order = matches[0];
        let compatible = order.instrument_id == intent.instrument_id
            && order.side == intent.side
            && order.quantity == intent.quantity;
        if compatible && order.side == Side::Buy {
            let actual = snapshot
                .positions
                .iter()
                .find(|p| p.instrument_id == intent.instrument_id)
                .map(|p| p.quantity)
                .unwrap_or(rust_decimal::Decimal::ZERO);
            if actual != order.filled_quantity {
                issues.push(format!(
                    "position quantity disagrees with filled entry {}",
                    intent.intent_id
                ));
            }
        }
        if !compatible || order.status == OrderStatus::Unknown {
            issues.push(format!(
                "intent {} has inconsistent or unknown broker state",
                intent.intent_id
            ));
        }
        resolutions.push(IntentResolution {
            intent_id: intent.intent_id.clone(),
            state: if !compatible || order.status == OrderStatus::Unknown {
                IntentState::Unknown
            } else if order.status.terminal() {
                IntentState::Resolved
            } else {
                IntentState::Acknowledged
            },
            broker_order_id: Some(order.broker_order_id.clone()),
        });
    }
    // Any unexpected open order is an account-level ambiguity, even with no local intent.
    for order in snapshot.orders.iter().filter(|o| !o.status.terminal()) {
        if !intents.iter().any(|i| {
            order.client_intent_id.as_ref() == Some(&i.intent_id)
                || i.broker_order_id.as_ref() == Some(&order.broker_order_id)
        }) {
            issues.push(format!(
                "untracked live broker order {}",
                order.broker_order_id
            ));
        }
    }
    let pending = snapshot.orders.iter().any(|o| !o.status.terminal());
    let state = if !issues.is_empty() {
        PositionState::RecoveryRequired
    } else if !snapshot.positions.is_empty() {
        PositionState::OpenUnprotected
    } else if pending {
        PositionState::EntryPending
    } else {
        PositionState::Flat
    };
    Ok(Reconciliation {
        snapshot,
        state,
        resolutions,
        issues,
    })
}

pub fn reconcile_account(
    broker: &mut impl BrokerInformationPort,
    repo: &mut impl ExecutionRepository,
    fence: &Fence,
    clock: &impl Clock,
) -> Result<Reconciliation> {
    let snapshot = broker.snapshot(&fence.account_id)?;
    require(
        snapshot.account_id == fence.account_id && snapshot.mode == Mode::Paper,
        "wrong account or mode",
    )?;
    let intents = repo.unresolved_intents(&fence.account_id)?;
    let previous = repo.latest_reconciliation(&fence.account_id)?;
    let previous_positions = previous
        .as_ref()
        .filter(|r| r.state != PositionState::RecoveryRequired)
        .map(|r| r.snapshot.positions.as_slice())
        .unwrap_or(&[]);
    let result = reconcile_observed(snapshot, &intents, previous_positions, clock.now())?;
    repo.save_reconciliation(fence, &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
            .unwrap()
            .to_utc()
    }
    fn snapshot() -> AccountSnapshot {
        AccountSnapshot {
            snapshot_id: "snapshot_1".into(),
            account_id: AccountId::new("paper_test").unwrap(),
            mode: Mode::Paper,
            spendable_cash: Money {
                amount: Decimal::new(50, 0),
                currency: Currency::new("EUR").unwrap(),
            },
            positions: vec![],
            orders: vec![],
            observed_at: now(),
        }
    }
    fn intent() -> OrderIntent {
        OrderIntent {
            intent_id: IntentId::new("intent_1").unwrap(),
            account_id: AccountId::new("paper_test").unwrap(),
            instrument_id: InstrumentId::new("stock_1").unwrap(),
            side: Side::Buy,
            quantity: Decimal::ONE,
            limit_price: Decimal::TEN,
            session_id: "xnys_2026_10_30".into(),
            created_at: now(),
            state: IntentState::Prepared,
            broker_order_id: None,
        }
    }
    #[test]
    fn ord_03_04_missing_order_remains_unknown_on_replay() {
        for _ in 0..2 {
            let result = reconcile(snapshot(), &[intent()], now()).unwrap();
            assert_eq!(result.state, PositionState::RecoveryRequired);
            assert_eq!(result.resolutions[0].state, IntentState::Unknown);
        }
    }
    #[test]
    fn partial_fills_are_never_claimed_protected_or_flat() {
        let mut s = snapshot();
        let i = intent();
        s.positions.push(Position {
            instrument_id: i.instrument_id.clone(),
            quantity: Decimal::new(5, 1),
        });
        s.orders.push(BrokerOrder {
            broker_order_id: "broker_1".into(),
            client_intent_id: Some(i.intent_id.clone()),
            instrument_id: i.instrument_id.clone(),
            side: Side::Buy,
            quantity: Decimal::ONE,
            filled_quantity: Decimal::new(5, 1),
            status: OrderStatus::PartiallyFilled,
        });
        let r = reconcile(s, &[i], now()).unwrap();
        assert_eq!(r.state, PositionState::OpenUnprotected);
        assert_eq!(r.resolutions[0].state, IntentState::Acknowledged);
    }
    #[test]
    fn dat_01_stale_accounts_and_untracked_orders_block() {
        let mut s = snapshot();
        s.observed_at = now() - chrono::Duration::seconds(61);
        assert!(reconcile(s, &[], now()).is_err());
        let mut s = snapshot();
        let i = intent();
        s.orders.push(BrokerOrder {
            broker_order_id: "manual".into(),
            client_intent_id: None,
            instrument_id: i.instrument_id,
            side: Side::Buy,
            quantity: Decimal::ONE,
            filled_quantity: Decimal::ZERO,
            status: OrderStatus::Open,
        });
        assert_eq!(
            reconcile(s, &[], now()).unwrap().state,
            PositionState::RecoveryRequired
        );
    }
    #[test]
    fn explicit_calendar_handles_early_close_and_missing_sessions() {
        let session = ExchangeSession {
            session_id: "early_close".into(),
            opens_at: now() - chrono::Duration::hours(1),
            closes_at: now() + chrono::Duration::minutes(30),
        };
        assert!(!session.permits_entry(now()));
        assert!(session.permits_entry(now() - chrono::Duration::seconds(1)));
    }
    #[test]
    fn ord_09_manual_holdings_and_inconsistent_fills_require_recovery() {
        let mut s = snapshot();
        let i = intent();
        s.positions.push(Position {
            instrument_id: i.instrument_id.clone(),
            quantity: Decimal::ONE,
        });
        assert_eq!(
            reconcile(s.clone(), &[], now()).unwrap().state,
            PositionState::RecoveryRequired
        );
        s.orders.push(BrokerOrder {
            broker_order_id: "broker_1".into(),
            client_intent_id: Some(i.intent_id.clone()),
            instrument_id: i.instrument_id.clone(),
            side: Side::Buy,
            quantity: Decimal::ONE,
            filled_quantity: Decimal::new(5, 1),
            status: OrderStatus::PartiallyFilled,
        });
        assert_eq!(
            reconcile(s, &[i], now()).unwrap().state,
            PositionState::RecoveryRequired
        );
    }
}

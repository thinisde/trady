//! Executor-only transport contracts. These types confer no execution authority.
//!
//! The caller must durably reserve an intent, validate policy/readiness, hold the
//! current account fence and coordinate protection/available sell quantities before
//! calling the port. A transport acknowledgment does not establish a fill or protection.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use trady_domain::{AccountId, BrokerOrder, Currency, InstrumentId, IntentId, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    Day,
    GoodTillCancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderKind {
    Market {
        extended_hours: bool,
    },
    Limit {
        price: Decimal,
        time_in_force: TimeInForce,
    },
    Stop {
        price: Decimal,
        time_in_force: TimeInForce,
    },
    StopLimit {
        stop: Decimal,
        limit: Decimal,
        time_in_force: TimeInForce,
    },
}

/// Prices use the instrument's currency; quantity is positive for either side.
/// `intent_id` is local correlation, not a claim of broker-side idempotency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerOrderRequest {
    pub intent_id: IntentId,
    pub account_id: AccountId,
    pub instrument_id: InstrumentId,
    pub instrument_currency: Currency,
    pub side: Side,
    pub quantity: Decimal,
    pub kind: OrderKind,
}

impl BrokerOrderRequest {
    pub fn validate(&self) -> BrokerResult<()> {
        let prices_positive = match self.kind {
            OrderKind::Market { .. } => true,
            OrderKind::Limit { price, .. } | OrderKind::Stop { price, .. } => price > Decimal::ZERO,
            OrderKind::StopLimit { stop, limit, .. } => {
                stop > Decimal::ZERO && limit > Decimal::ZERO
            }
        };
        if self.quantity <= Decimal::ZERO || !prices_positive {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerFailureKind {
    InvalidRequest,
    InvalidResponse,
    AccountMismatch,
    Unsupported,
    Transport,
    Http,
    Rejected,
    RateLimited,
}

/// Contains only sanitized diagnostics. Never retain response bodies or credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerFailure {
    pub kind: BrokerFailureKind,
    pub http_status: Option<u16>,
    pub retry_after_seconds: Option<u64>,
    /// True means reconcile before any decision to send another mutation.
    pub outcome_unknown: bool,
    pub broker_order_id: Option<String>,
}

impl BrokerFailure {
    pub fn new(kind: BrokerFailureKind) -> Self {
        Self {
            kind,
            http_status: None,
            retry_after_seconds: None,
            outcome_unknown: false,
            broker_order_id: None,
        }
    }
}

impl fmt::Display for BrokerFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "broker {:?}", self.kind)?;
        if let Some(status) = self.http_status {
            write!(f, " (HTTP {status})")?;
        }
        if self.outcome_unknown {
            write!(f, "; outcome unknown, reconcile before further mutation")?;
        }
        if let Some(seconds) = self.retry_after_seconds {
            write!(f, "; retry after at least {seconds}s")?;
        }
        Ok(())
    }
}

impl std::error::Error for BrokerFailure {}
pub type BrokerResult<T> = std::result::Result<T, BrokerFailure>;

/// Only the cancellation request was accepted. The order may still fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancellationReceipt {
    pub broker_order_id: String,
}

/// Give this port only to an executor, never a model tool registry or analyst.
pub trait BrokerExecutionPort {
    fn submit_order(&mut self, request: &BrokerOrderRequest) -> BrokerResult<BrokerOrder>;
    fn cancel_order(
        &mut self,
        account: &AccountId,
        broker_order_id: &str,
    ) -> BrokerResult<CancellationReceipt>;
}

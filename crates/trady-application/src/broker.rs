//! Executor-only transport contracts. These types confer no execution authority.
//!
//! The caller must durably reserve an intent, validate policy/readiness, hold the
//! current account fence and coordinate protection/available sell quantities before
//! calling the port. A transport acknowledgment does not establish a fill or protection.

use chrono::{DateTime, Utc};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionSpec {
    pub stop_price: Decimal,
    pub quantity: Decimal,
    pub time_in_force: TimeInForce,
}

impl ProtectionSpec {
    pub fn validate(&self) -> BrokerResult<()> {
        if self.stop_price <= Decimal::ZERO || self.quantity <= Decimal::ZERO {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedOrderRequest {
    pub entry: BrokerOrderRequest,
    pub protection: ProtectionSpec,
    pub capability_evidence_id: String,
    pub capability_evidence_sha256: String,
    pub expires_at: DateTime<Utc>,
}

impl ProtectedOrderRequest {
    pub fn validate(&self, now: DateTime<Utc>) -> BrokerResult<()> {
        self.entry.validate()?;
        self.protection.validate()?;
        let matching_time_in_force = match self.entry.kind {
            OrderKind::Market { .. } => true,
            OrderKind::Limit { time_in_force, .. }
            | OrderKind::Stop { time_in_force, .. }
            | OrderKind::StopLimit { time_in_force, .. } => {
                time_in_force == self.protection.time_in_force
            }
        };
        if self.protection.quantity != self.entry.quantity
            || !matching_time_in_force
            || self.capability_evidence_id.is_empty()
            || self.capability_evidence_id.len() > 160
            || !self
                .capability_evidence_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
            || self.capability_evidence_sha256.len() != 64
            || !self
                .capability_evidence_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || self.expires_at <= now
        {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedOrderReceipt {
    pub broker_order_id: String,
    pub protected_order_ids: Vec<String>,
    pub account_id: AccountId,
    pub instrument_id: InstrumentId,
    pub quantity: Decimal,
    pub atomic: bool,
    pub request_sha256: String,
    pub acknowledged_at: DateTime<Utc>,
}

impl ProtectedOrderReceipt {
    /// Validate the broker acknowledgement against the exact protected request.
    /// This proves transport identity and attachment shape; it does not prove a fill.
    pub fn validate(&self, request: &ProtectedOrderRequest) -> BrokerResult<()> {
        request.entry.validate()?;
        request.protection.validate()?;
        if self.broker_order_id.is_empty()
            || self.protected_order_ids.is_empty()
            || self.protected_order_ids.iter().any(String::is_empty)
            || !self.atomic
            || self.account_id != request.entry.account_id
            || self.instrument_id != request.entry.instrument_id
            || self.quantity != request.entry.quantity
            || self.request_sha256.len() != 64
            || !self
                .request_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidResponse));
        }
        Ok(())
    }
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

    fn submit_protected_order(
        &mut self,
        _request: &ProtectedOrderRequest,
        _now: DateTime<Utc>,
    ) -> BrokerResult<ProtectedOrderReceipt> {
        Err(BrokerFailure::new(BrokerFailureKind::Unsupported))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trady_domain::AccountId;

    fn request() -> BrokerOrderRequest {
        BrokerOrderRequest {
            intent_id: IntentId::new("intent").unwrap(),
            account_id: AccountId::new("paper_demo").unwrap(),
            instrument_id: InstrumentId::new("AAPL_US_EQ").unwrap(),
            instrument_currency: Currency::new("USD").unwrap(),
            side: Side::Buy,
            quantity: Decimal::ONE,
            kind: OrderKind::Limit {
                price: Decimal::new(100, 0),
                time_in_force: TimeInForce::Day,
            },
        }
    }

    #[test]
    fn protected_request_rejects_non_hex_evidence_and_expired_deadline() {
        let mut protected = ProtectedOrderRequest {
            entry: request(),
            protection: ProtectionSpec {
                stop_price: Decimal::new(99, 0),
                quantity: Decimal::ONE,
                time_in_force: TimeInForce::Day,
            },
            capability_evidence_id: "capability".into(),
            capability_evidence_sha256: "z".repeat(64),
            expires_at: Utc::now() + chrono::Duration::minutes(1),
        };
        assert!(protected.validate(Utc::now()).is_err());
        protected.capability_evidence_sha256 = "a".repeat(64);
        protected.expires_at = Utc::now() - chrono::Duration::seconds(1);
        assert!(protected.validate(Utc::now()).is_err());
    }

    #[test]
    fn protected_receipt_must_match_account_instrument_quantity_and_attachment() {
        let request = ProtectedOrderRequest {
            entry: request(),
            protection: ProtectionSpec {
                stop_price: Decimal::new(99, 0),
                quantity: Decimal::ONE,
                time_in_force: TimeInForce::Day,
            },
            capability_evidence_id: "capability".into(),
            capability_evidence_sha256: "a".repeat(64),
            expires_at: Utc::now() + chrono::Duration::minutes(1),
        };
        let receipt = ProtectedOrderReceipt {
            broker_order_id: "order-1".into(),
            protected_order_ids: vec!["stop-1".into()],
            account_id: AccountId::new("paper_other").unwrap(),
            instrument_id: request.entry.instrument_id.clone(),
            quantity: Decimal::ONE,
            atomic: true,
            request_sha256: "b".repeat(64),
            acknowledged_at: Utc::now(),
        };
        assert!(matches!(
            receipt.validate(&request),
            Err(BrokerFailure {
                kind: BrokerFailureKind::InvalidResponse,
                ..
            })
        ));
    }
}

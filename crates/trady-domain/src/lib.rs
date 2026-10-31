//! Provider-neutral, validated identifiers and execution observations.

pub mod research;
pub mod screening;
pub mod sentiment;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                require(
                    !value.is_empty()
                        && value.len() <= 96
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
                    "invalid identifier",
                )?;
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl TryFrom<String> for $name {
            type Error = Error;
            fn try_from(value: String) -> Result<Self> {
                Self::new(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
identifier!(AccountId);
identifier!(InstrumentId);
identifier!(IntentId);
identifier!(WorkerId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency(String);
impl Currency {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        require(
            value.len() == 3 && value.bytes().all(|b| b.is_ascii_uppercase()),
            "currency must be three uppercase letters",
        )?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for Currency {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}
impl From<Currency> for String {
    fn from(value: Currency) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Money {
    pub amount: Decimal,
    pub currency: Currency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Paper,
    Shadow,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PositionState {
    Flat,
    EntryPending,
    OpenUnprotected,
    OpenProtected,
    ExitLatched,
    ExitPending,
    Closed,
    RecoveryRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
    Unknown,
}
impl OrderStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Filled | Self::Cancelled | Self::Rejected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub instrument_id: InstrumentId,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerOrder {
    pub broker_order_id: String,
    pub client_intent_id: Option<IntentId>,
    pub instrument_id: InstrumentId,
    pub side: Side,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub status: OrderStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSnapshot {
    pub snapshot_id: String,
    pub account_id: AccountId,
    pub mode: Mode,
    pub spendable_cash: Money,
    pub positions: Vec<Position>,
    /// Includes terminal orders needed to resolve outstanding intents.
    pub orders: Vec<BrokerOrder>,
    pub observed_at: DateTime<Utc>,
}
impl AccountSnapshot {
    pub fn validate(&self) -> Result<()> {
        require(!self.snapshot_id.is_empty(), "snapshot ID is required")?;
        require(
            self.spendable_cash.amount >= Decimal::ZERO,
            "negative spendable cash",
        )?;
        let mut instruments = std::collections::BTreeSet::new();
        for position in &self.positions {
            require(
                position.quantity > Decimal::ZERO,
                "position must be positive; shorting is unsupported",
            )?;
            require(
                instruments.insert(&position.instrument_id),
                "duplicate position",
            )?;
        }
        let mut order_ids = std::collections::BTreeSet::new();
        for order in &self.orders {
            require(
                !order.broker_order_id.is_empty() && order_ids.insert(&order.broker_order_id),
                "invalid or duplicate broker order",
            )?;
            require(
                order.quantity > Decimal::ZERO
                    && order.filled_quantity >= Decimal::ZERO
                    && order.filled_quantity <= order.quantity,
                "invalid broker quantity",
            )?;
            require(
                order.status != OrderStatus::Filled || order.quantity == order.filled_quantity,
                "filled order has remaining quantity",
            )?;
            require(
                order.status != OrderStatus::PartiallyFilled
                    || (order.filled_quantity > Decimal::ZERO
                        && order.filled_quantity < order.quantity),
                "partial order must have positive filled and remaining quantities",
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quote {
    pub instrument_id: InstrumentId,
    pub currency: Currency,
    pub bid: Decimal,
    pub ask: Decimal,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub tradable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Support {
    Unknown,
    Unsupported,
    Verified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub support: Support,
    pub evidence_id: Option<String>,
    pub valid_until: Option<DateTime<Utc>>,
}
impl Capability {
    pub fn usable(&self, now: DateTime<Utc>) -> bool {
        self.support == Support::Verified
            && self.evidence_id.as_ref().is_some_and(|id| !id.is_empty())
            && self.valid_until.is_some_and(|expiry| expiry > now)
    }
}
impl Default for Capability {
    fn default() -> Self {
        Self {
            support: Support::Unknown,
            evidence_id: None,
            valid_until: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub cash_account: Capability,
    pub instrument: Capability,
    pub fractional_orders: Capability,
    pub protective_orders: Capability,
    pub quotes: Capability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntentState {
    Prepared,
    Unknown,
    Acknowledged,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderIntent {
    pub intent_id: IntentId,
    pub account_id: AccountId,
    pub instrument_id: InstrumentId,
    pub side: Side,
    pub quantity: Decimal,
    pub limit_price: Decimal,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub state: IntentState,
    pub broker_order_id: Option<String>,
}
impl OrderIntent {
    pub fn validate(&self) -> Result<()> {
        require(
            self.quantity > Decimal::ZERO && self.limit_price > Decimal::ZERO,
            "intent price and quantity must be positive",
        )?;
        require(
            !self.session_id.is_empty(),
            "exchange session ID is required",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fence {
    pub account_id: AccountId,
    pub worker_id: WorkerId,
    pub generation: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentResolution {
    pub intent_id: IntentId,
    pub state: IntentState,
    pub broker_order_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reconciliation {
    pub snapshot: AccountSnapshot,
    pub state: PositionState,
    pub resolutions: Vec<IntentResolution>,
    pub issues: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_and_currencies_validate_on_deserialization() {
        assert!(serde_json::from_str::<AccountId>(r#""../account""#).is_err());
        assert!(serde_json::from_str::<Currency>(r#""eur""#).is_err());
        assert!(serde_json::from_str::<AccountId>(r#""paper_account""#).is_ok());
    }
    #[test]
    fn financial_decimals_are_strings() {
        let currency = Currency::new("EUR").unwrap();
        let money = Money {
            amount: Decimal::new(50, 0),
            currency,
        };
        assert_eq!(serde_json::to_value(&money).unwrap()["amount"], "50");
        assert!(serde_json::from_str::<Money>(r#"{"amount":50,"currency":"EUR"}"#).is_err());
    }
    #[test]
    fn unknown_or_expired_capabilities_never_become_support() {
        let now = DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
            .unwrap()
            .to_utc();
        assert!(!Capability::default().usable(now));
        assert!(
            !Capability {
                support: Support::Verified,
                evidence_id: Some("checked".into()),
                valid_until: Some(now)
            }
            .usable(now)
        );
    }
}

//! Private wire types. Decimal JSON tokens never pass through binary floating point.

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::value::RawValue;
use trady_application::broker::{
    BrokerFailure, BrokerFailureKind, BrokerOrderRequest, BrokerResult, OrderKind, TimeInForce,
};
use trady_domain::{BrokerOrder, Currency, InstrumentId, OrderStatus, Position, Side};

pub(super) fn invalid() -> BrokerFailure {
    BrokerFailure::new(BrokerFailureKind::InvalidResponse)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Number(pub Decimal);
impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(d)?;
        parse_number(raw.get())
            .map(Self)
            .ok_or_else(|| D::Error::custom("unsupported exact decimal number"))
    }
}
impl Serialize for Number {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Decimal's canonical text is a valid JSON number, not a quoted string.
        RawValue::from_string(self.0.normalize().to_string())
            .map_err(serde::ser::Error::custom)?
            .serialize(s)
    }
}

fn parse_number(raw: &str) -> Option<Decimal> {
    if raw.len() > 200 || !matches!(raw.as_bytes().first(), Some(b'-' | b'0'..=b'9')) {
        return None;
    }
    let Some((base, exp)) = raw.split_once(['e', 'E']) else {
        return Decimal::from_str_exact(raw).ok();
    };
    let exp: i32 = exp.parse().ok()?;
    if !(-100..=100).contains(&exp) {
        return None;
    }
    let negative = base.starts_with('-');
    let base = base.trim_start_matches('-');
    let point = base.find('.').unwrap_or(base.len()) as i32 + exp;
    let digits = base.replace('.', "");
    let expanded = if point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}", "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    };
    Decimal::from_str_exact(&format!("{}{expanded}", if negative { "-" } else { "" })).ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AccountSummary {
    pub id: i64,
    pub currency: Currency,
    pub cash: Cash,
    pub total_value: Option<Number>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Cash {
    pub available_to_trade: Number,
}

#[derive(Deserialize)]
pub(super) struct Instrument {
    pub ticker: InstrumentId,
    pub currency: Currency,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstrumentMetadata {
    pub ticker: InstrumentId,
    pub currency_code: Currency,
    pub isin: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Deserialize)]
pub(super) struct PositionDto {
    pub instrument: Instrument,
    pub quantity: Number,
    #[serde(rename = "quantityAvailableForTrading")]
    pub available: Option<Number>,
    #[serde(rename = "walletImpact")]
    pub wallet_impact: Option<PositionWalletImpact>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PositionWalletImpact {
    pub currency: Currency,
    pub total_cost: Number,
}
impl PositionDto {
    pub fn normalize(self) -> BrokerResult<Position> {
        if self.quantity.0 <= Decimal::ZERO {
            return Err(invalid());
        }
        Ok(Position {
            instrument_id: self.instrument.ticker,
            quantity: self.quantity.0,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Order {
    pub id: i64,
    ticker: Option<InstrumentId>,
    instrument: Option<Instrument>,
    quantity: Number,
    filled_quantity: Number,
    side: Side,
    status: String,
    strategy: String,
    #[serde(rename = "type")]
    kind: String,
    limit_price: Option<Number>,
    stop_price: Option<Number>,
    time_in_force: Option<String>,
    extended_hours: Option<bool>,
}
impl Order {
    pub fn normalize(&self) -> BrokerResult<BrokerOrder> {
        if self.id <= 0
            || self.strategy != "QUANTITY"
            || !matches!(
                self.kind.as_str(),
                "MARKET" | "LIMIT" | "STOP" | "STOP_LIMIT"
            )
        {
            return Err(invalid());
        }
        let instrument_id = self
            .ticker
            .as_ref()
            .or_else(|| self.instrument.as_ref().map(|i| &i.ticker))
            .ok_or_else(invalid)?;
        if self
            .instrument
            .as_ref()
            .is_some_and(|i| &i.ticker != instrument_id)
            || (self.side == Side::Buy
                && (self.quantity.0 < Decimal::ZERO || self.filled_quantity.0 < Decimal::ZERO))
        {
            return Err(invalid());
        }
        let quantity = self.quantity.0.abs();
        let filled_quantity = self.filled_quantity.0.abs();
        let status = match self.status.as_str() {
            "NEW" | "CONFIRMED" => OrderStatus::Open,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED" => OrderStatus::Filled,
            "CANCELLED" => OrderStatus::Cancelled,
            "REJECTED" => OrderStatus::Rejected,
            _ => OrderStatus::Unknown,
        };
        if quantity <= Decimal::ZERO
            || filled_quantity > quantity
            || (status == OrderStatus::Filled && filled_quantity != quantity)
            || (status == OrderStatus::PartiallyFilled
                && (filled_quantity <= Decimal::ZERO || filled_quantity == quantity))
        {
            return Err(invalid());
        }
        Ok(BrokerOrder {
            broker_order_id: self.id.to_string(),
            client_intent_id: None,
            instrument_id: instrument_id.clone(),
            side: self.side,
            quantity,
            filled_quantity,
            status,
        })
    }

    pub fn matches(&self, request: &BrokerOrderRequest, normalized: &BrokerOrder) -> bool {
        if normalized.instrument_id != request.instrument_id
            || normalized.side != request.side
            || normalized.quantity != request.quantity
            || self
                .instrument
                .as_ref()
                .is_some_and(|i| i.currency != request.instrument_currency)
        {
            return false;
        }
        let price = self.limit_price.map(|n| n.0);
        let stop = self.stop_price.map(|n| n.0);
        match request.kind {
            OrderKind::Market { extended_hours } => {
                self.kind == "MARKET" && self.extended_hours == Some(extended_hours)
            }
            OrderKind::Limit {
                price: limit,
                time_in_force,
            } => {
                self.kind == "LIMIT"
                    && price == Some(limit)
                    && self.time_in_force.as_deref() == Some(validity(time_in_force))
            }
            OrderKind::Stop {
                price: trigger,
                time_in_force,
            } => {
                self.kind == "STOP"
                    && stop == Some(trigger)
                    && self.time_in_force.as_deref() == Some(validity(time_in_force))
            }
            OrderKind::StopLimit {
                stop: trigger,
                limit,
                time_in_force,
            } => {
                self.kind == "STOP_LIMIT"
                    && stop == Some(trigger)
                    && price == Some(limit)
                    && self.time_in_force.as_deref() == Some(validity(time_in_force))
            }
        }
    }
}

pub(super) fn validity(value: TimeInForce) -> &'static str {
    match value {
        TimeInForce::Day => "DAY",
        TimeInForce::GoodTillCancel => "GOOD_TILL_CANCEL",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderBody<'a> {
    ticker: &'a str,
    quantity: Number,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit_price: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_price: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_validity: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extended_hours: Option<bool>,
}

pub(super) fn order_body(request: &BrokerOrderRequest) -> BrokerResult<String> {
    let mut body = OrderBody {
        ticker: request.instrument_id.as_str(),
        quantity: Number(if request.side == Side::Sell {
            -request.quantity
        } else {
            request.quantity
        }),
        limit_price: None,
        stop_price: None,
        time_validity: None,
        extended_hours: None,
    };
    match request.kind {
        OrderKind::Market { extended_hours } => body.extended_hours = Some(extended_hours),
        OrderKind::Limit {
            price,
            time_in_force,
        } => {
            body.limit_price = Some(Number(price));
            body.time_validity = Some(validity(time_in_force));
        }
        OrderKind::Stop {
            price,
            time_in_force,
        } => {
            body.stop_price = Some(Number(price));
            body.time_validity = Some(validity(time_in_force));
        }
        OrderKind::StopLimit {
            stop,
            limit,
            time_in_force,
        } => {
            body.limit_price = Some(Number(limit));
            body.stop_price = Some(Number(stop));
            body.time_validity = Some(validity(time_in_force));
        }
    }
    serde_json::to_string(&body).map_err(|_| BrokerFailure::new(BrokerFailureKind::InvalidRequest))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct History {
    pub items: Vec<HistoryItem>,
    #[serde(deserialize_with = "continuation")]
    pub next_page_path: Option<String>,
}

fn continuation<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    // The provider documents explicit null as end-of-history. Missing is not null.
    Option::<String>::deserialize(d)
}
#[derive(Deserialize)]
pub(super) struct HistoryItem {
    pub order: Order,
}

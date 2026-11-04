//! Application-owned read tools exposed to the analyst. Implementations are
//! backed by reviewed adapters; no arbitrary URL or shell tool is accepted.

use crate::harness::ToolSpec;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use trady_domain::{
    BarSnapshot, Currency, EvidenceItem, InstrumentId, Result, SessionWindow, VenueQuote, require,
};

pub const EUR_MARKET_TOOL_NAMES: [&str; 6] = [
    "market.quote",
    "market.bars",
    "market.sessions",
    "market.fx",
    "evidence.search",
    "evidence.read",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketToolSpec {
    pub name: String,
    pub description: String,
    pub arbitrary_network: bool,
    pub broker_mutation: bool,
}

pub fn eur_market_tool_specs() -> Vec<MarketToolSpec> {
    EUR_MARKET_TOOL_NAMES
        .into_iter()
        .map(|name| MarketToolSpec {
            name: name.into(),
            description: format!("Read the reviewed {name} application port."),
            arbitrary_network: false,
            broker_mutation: false,
        })
        .collect()
}

/// Canonical native-tool contracts advertised to the pinned EUR analyst. The
/// names are application-owned and every implementation remains read-only.
pub fn eur_analyst_tool_specs() -> Vec<ToolSpec> {
    let id = json!({"type":"string","minLength":1,"maxLength":160});
    let read_result = || {
        object(
            json!({
                "result":{"type":"object"},
                "entry_authorized":{"const":false}
            }),
            &["result", "entry_authorized"],
        )
    };
    vec![
        ToolSpec {
            name: "market.quote".into(),
            version: "1.0.0".into(),
            description: "Read the latest reviewed two-sided quote for an approved EUR instrument.".into(),
            arguments_schema: object(json!({"instrument_id":id.clone()}), &["instrument_id"]),
            result_schema: read_result(),
        },
        ToolSpec {
            name: "market.bars".into(),
            version: "1.0.0".into(),
            description: "Read completed OHLCV bars only; incomplete bars are rejected.".into(),
            arguments_schema: object(json!({"instrument_id":id.clone(),"interval_seconds":{"type":"integer","enum":[60,300,900,1800,3600,86400]}}), &["instrument_id","interval_seconds"]),
            result_schema: read_result(),
        },
        ToolSpec {
            name: "market.sessions".into(),
            version: "1.0.0".into(),
            description: "Read the reviewed exchange session and local same-day flatten deadline.".into(),
            arguments_schema: object(json!({"exchange_mic":{"type":"string","minLength":4,"maxLength":4},"as_of":{"type":"string","format":"date-time"}}), &["exchange_mic","as_of"]),
            result_schema: read_result(),
        },
        ToolSpec {
            name: "market.fx".into(),
            version: "1.0.0".into(),
            description: "Read a reviewed FX observation for cost calculations; never changes account currency.".into(),
            arguments_schema: object(json!({"from":{"type":"string","minLength":3,"maxLength":3},"to":{"type":"string","minLength":3,"maxLength":3},"as_of":{"type":"string","format":"date-time"}}), &["from","to","as_of"]),
            result_schema: read_result(),
        },
        ToolSpec {
            name: "evidence.search".into(),
            version: "1.0.0".into(),
            description: "Search the normalized rights-aware evidence ledger for one approved instrument.".into(),
            arguments_schema: object(json!({"instrument_id":id.clone(),"as_of":{"type":"string","format":"date-time"}}), &["instrument_id","as_of"]),
            result_schema: read_result(),
        },
        ToolSpec {
            name: "evidence.read".into(),
            version: "1.0.0".into(),
            description: "Read one evidence item by immutable ID with current rights and freshness.".into(),
            arguments_schema: object(json!({"evidence_id":id,"as_of":{"type":"string","format":"date-time"}}), &["evidence_id","as_of"]),
            result_schema: read_result(),
        },
    ]
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
}

/// Read-only port supplied by the target runtime. The analyst adapter can
/// advertise the contracts without receiving a broker mutation capability.
pub trait EurAnalystReadPort: Send {
    fn execute(&mut self, name: &str, arguments: &Value) -> Result<Value>;
}

pub trait EurMarketReadPort {
    fn quote(&mut self, instrument_id: &InstrumentId) -> Result<VenueQuote>;
    fn bars(&mut self, instrument_id: &InstrumentId, interval_seconds: u32) -> Result<BarSnapshot>;
    fn session(&mut self, exchange_mic: &str, now: DateTime<Utc>) -> Result<SessionWindow>;
    fn fx(
        &mut self,
        from: &Currency,
        to: &Currency,
        now: DateTime<Utc>,
    ) -> Result<Option<DecimalFx>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecimalFx {
    pub from: Currency,
    pub to: Currency,
    pub rate: rust_decimal::Decimal,
    pub observed_at: DateTime<Utc>,
}

impl DecimalFx {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.rate > rust_decimal::Decimal::ZERO
                && self.observed_at <= now
                && self.from != self.to,
            "invalid FX read-tool observation",
        )
    }
}

pub trait EurEvidenceReadPort {
    fn search(
        &mut self,
        instrument_id: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<Vec<EvidenceItem>>;
    fn read(&mut self, evidence_id: &str, now: DateTime<Utc>) -> Result<EvidenceItem>;
}

/// Generic adapter used by the OpenCode bridge. It composes reviewed market
/// and evidence ports without exposing a transport, filesystem or broker
/// mutation method to the model.
pub struct RegisteredEurReadTools<M, E> {
    pub market: M,
    pub evidence: E,
}

impl<M: EurMarketReadPort + Send, E: EurEvidenceReadPort + Send> EurAnalystReadPort
    for RegisteredEurReadTools<M, E>
{
    fn execute(&mut self, name: &str, arguments: &Value) -> Result<Value> {
        let instrument = || {
            arguments
                .get("instrument_id")
                .and_then(Value::as_str)
                .ok_or_else(|| trady_domain::Error("instrument_id is required".into()))
                .and_then(InstrumentId::new)
        };
        let as_of = || {
            arguments
                .get("as_of")
                .and_then(Value::as_str)
                .ok_or_else(|| trady_domain::Error("as_of is required".into()))
                .and_then(|value| {
                    value
                        .parse::<DateTime<Utc>>()
                        .map_err(|_| trady_domain::Error("as_of is not RFC3339".into()))
                })
        };
        match name {
            "market.quote" => Ok(json!({"quote":self.market.quote(&instrument()?)?})),
            "market.bars" => {
                let interval = arguments
                    .get("interval_seconds")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| trady_domain::Error("interval_seconds is required".into()))?;
                Ok(json!({"bars":self.market.bars(&instrument()?, interval)?}))
            }
            "market.sessions" => {
                let exchange = arguments
                    .get("exchange_mic")
                    .and_then(Value::as_str)
                    .ok_or_else(|| trady_domain::Error("exchange_mic is required".into()))?;
                Ok(json!({"session":self.market.session(exchange, as_of()?)?}))
            }
            "market.fx" => {
                let from = Currency::new(
                    arguments
                        .get("from")
                        .and_then(Value::as_str)
                        .ok_or_else(|| trady_domain::Error("from is required".into()))?,
                )?;
                let to = Currency::new(
                    arguments
                        .get("to")
                        .and_then(Value::as_str)
                        .ok_or_else(|| trady_domain::Error("to is required".into()))?,
                )?;
                Ok(json!({"fx":self.market.fx(&from, &to, as_of()?)?}))
            }
            "evidence.search" => {
                let now = as_of()?;
                let items = self
                    .evidence
                    .search(&instrument()?, now)?
                    .into_iter()
                    .collect::<Vec<_>>();
                Ok(json!({"evidence":items}))
            }
            "evidence.read" => {
                let id = arguments
                    .get("evidence_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| trady_domain::Error("evidence_id is required".into()))?;
                Ok(json!({"evidence":self.evidence.read(id, as_of()?)?}))
            }
            _ => Err(trady_domain::Error(
                "analyst requested an unregistered EUR read tool".into(),
            )),
        }
    }
}

pub fn validate_tool_name(name: &str) -> Result<()> {
    require(
        EUR_MARKET_TOOL_NAMES.contains(&name),
        "analyst requested an unregistered EUR read tool",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_registered_application_read_tools_are_exposed() {
        let specs = eur_market_tool_specs();
        assert_eq!(specs.len(), 6);
        assert!(
            specs
                .iter()
                .all(|spec| !spec.arbitrary_network && !spec.broker_mutation)
        );
        assert!(validate_tool_name("market.quote").is_ok());
        assert!(validate_tool_name("http.get").is_err());
        let analyst_specs = eur_analyst_tool_specs();
        assert_eq!(analyst_specs.len(), 6);
        assert_eq!(
            analyst_specs
                .iter()
                .map(|spec| spec.name.as_str())
                .collect::<Vec<_>>(),
            EUR_MARKET_TOOL_NAMES
        );
    }
}

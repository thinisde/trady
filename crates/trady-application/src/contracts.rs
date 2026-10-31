//! Versioned syntax boundaries. Schema validity is not research or trading authority.

use chrono::{DateTime, Utc};
use jsonschema::{Draft, Validator};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use trady_domain::{Error, Result, require};

pub const LLM_SCHEMA: &str = include_str!("../../../schemas/llm-contracts.v2.json");
pub const RESEARCH_SCHEMA: &str = include_str!("../../../schemas/research-contracts.v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contract {
    ModelTurn,
    ToolResults,
    ProposalEnvelope,
    StoreArtifactArguments,
    StoreArtifactResult,
    PriorityArguments,
    PriorityResult,
    RotationArguments,
    RotationResult,
}

#[derive(Debug, Clone)]
pub struct ValidatedDocument {
    contract: Contract,
    value: Value,
}
impl ValidatedDocument {
    pub fn contract(&self) -> Contract {
        self.contract
    }
    pub fn value(&self) -> &Value {
        &self.value
    }
}

pub struct Contracts {
    validators: Vec<(Contract, Validator)>,
}
impl Contracts {
    pub fn new() -> Result<Self> {
        let mut validators = Vec::new();
        for (contract, text, definition) in [
            (Contract::ModelTurn, LLM_SCHEMA, "ModelTurn"),
            (Contract::ToolResults, LLM_SCHEMA, "ToolResults"),
            (
                Contract::ProposalEnvelope,
                LLM_SCHEMA,
                "ApplicationProposalEnvelope",
            ),
            (
                Contract::StoreArtifactArguments,
                RESEARCH_SCHEMA,
                "StoreArtifactArguments",
            ),
            (
                Contract::StoreArtifactResult,
                RESEARCH_SCHEMA,
                "StoreArtifactResult",
            ),
            (
                Contract::PriorityArguments,
                RESEARCH_SCHEMA,
                "PriorityArguments",
            ),
            (Contract::PriorityResult, RESEARCH_SCHEMA, "PriorityResult"),
            (
                Contract::RotationArguments,
                RESEARCH_SCHEMA,
                "RotationArguments",
            ),
            (Contract::RotationResult, RESEARCH_SCHEMA, "RotationResult"),
        ] {
            let mut schema: Value = serde_json::from_str(text)
                .map_err(|e| Error(format!("invalid embedded schema: {e}")))?;
            schema["$ref"] = Value::String(format!("#/$defs/{definition}"));
            schema["$defs"]["Decimal"]["format"] = Value::String("trady-decimal".into());
            let validator = jsonschema::options()
                .with_draft(Draft::Draft202012)
                .should_validate_formats(true)
                .with_format("date-time", valid_utc_time)
                .with_format("trady-decimal", |text: &str| {
                    Decimal::from_str_exact(text).is_ok()
                })
                .build(&schema)
                .map_err(|e| Error(format!("schema compilation failed: {e}")))?;
            validators.push((contract, validator));
        }
        // Demonstrate that format checks are assertions, including impossible calendar dates.
        require(
            !valid_utc_time("2026-02-30T14:00:00Z"),
            "calendar checker is ineffective",
        )?;
        Ok(Self { validators })
    }
    pub fn validate(&self, contract: Contract, input: &str) -> Result<ValidatedDocument> {
        let value = parse_json(input)?;
        let validator = &self
            .validators
            .iter()
            .find(|(kind, _)| *kind == contract)
            .ok_or_else(|| Error("unsupported contract".into()))?
            .1;
        if let Err(error) = validator.validate(&value) {
            return Err(Error(format!("schema validation failed: {error}")));
        }
        semantic_shape(&value)?;
        if contract == Contract::ProposalEnvelope {
            let created = parse_time(
                value["created_at"]
                    .as_str()
                    .ok_or_else(|| Error("missing creation time".into()))?,
            )?;
            let expires = parse_time(
                value["valid_until"]
                    .as_str()
                    .ok_or_else(|| Error("missing expiry".into()))?,
            )?;
            require(
                expires > created
                    && expires.signed_duration_since(created) <= chrono::Duration::seconds(300),
                "proposal lifetime must be positive and at most five minutes",
            )?;
            if value["body"]["kind"] == "ENTRY_PROPOSAL" {
                require(
                    !value["account_snapshot_id"].is_null()
                        && !value["research_dossier_id"].is_null(),
                    "entry envelope requires account and dossier bindings",
                )?;
            }
        }
        Ok(ValidatedDocument { contract, value })
    }
}

/// Parse a single bounded JSON document, rejecting duplicate keys at every depth.
pub fn parse_json(input: &str) -> Result<Value> {
    require(input.len() <= 262_144, "JSON document exceeds input limit")?;
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = StrictValue::deserialize(&mut deserializer)
        .map_err(|_| Error("invalid JSON or duplicate object key".into()))?
        .0;
    deserializer
        .end()
        .map_err(|_| Error("trailing JSON data".into()))?;
    Ok(value)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    require(value.ends_with('Z'), "timestamp must use UTC Z")?;
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.to_utc())
        .map_err(|_| Error("invalid calendar timestamp".into()))
}
fn valid_utc_time(value: &str) -> bool {
    parse_time(value).is_ok()
}

fn decimal(value: &Value) -> Result<Decimal> {
    Decimal::from_str_exact(
        value
            .as_str()
            .ok_or_else(|| Error("decimal must be a string".into()))?,
    )
    .map_err(|_| Error("decimal outside supported precision or range".into()))
}

fn semantic_shape(value: &Value) -> Result<()> {
    match value {
        Value::Object(map) => {
            if let Some(kind) = map.get("kind").and_then(Value::as_str)
                && matches!(kind, "ENTRY_PROPOSAL" | "PLAN_UPDATE")
            {
                let target = decimal(&value["target_net_fraction"])?;
                require(
                    target > Decimal::ZERO && target <= Decimal::new(10, 2),
                    "target must be positive and at most 10%",
                )?;
                require(
                    decimal(&value["stop_price"])? > Decimal::ZERO,
                    "stop must be positive",
                )?;
                if !value["target_price_hint"].is_null() {
                    require(
                        decimal(&value["target_price_hint"])? > Decimal::ZERO,
                        "target hint must be positive",
                    )?;
                }
                if kind == "ENTRY_PROPOSAL" {
                    let range = &value["entry_range"];
                    require(
                        decimal(&range["minimum"])? > Decimal::ZERO
                            && decimal(&range["minimum"])? <= decimal(&range["maximum"])?,
                        "invalid entry price range",
                    )?;
                    require(
                        range["currency"] == value["currency"],
                        "entry currency mismatch",
                    )?;
                }
            }
            for key in ["tool_calls", "claims", "skill_findings"] {
                if let Some(rows) = map.get(key).and_then(Value::as_array) {
                    let field = match key {
                        "tool_calls" => "call_id",
                        "claims" => "claim_id",
                        _ => "skill_name",
                    };
                    let mut seen = BTreeSet::new();
                    for row in rows {
                        if let Some(id) = row.get(field).and_then(Value::as_str) {
                            require(seen.insert(id), "duplicate local identity")?;
                        }
                    }
                }
            }
            for child in map.values() {
                semantic_shape(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                semantic_shape(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// serde_json's default object handling accepts repeated keys; protocol boundaries do not.
struct StrictValue(Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StrictValue;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "a JSON value without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| StrictValue(n.into()))
                    .ok_or_else(|| E::custom("invalid number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element::<StrictValue>()? {
                    out.push(v.0);
                }
                Ok(StrictValue(Value::Array(out)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if out.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate JSON object key"));
                    }
                    out.insert(key, a.next_value::<StrictValue>()?.0);
                }
                Ok(StrictValue(Value::Object(out)))
            }
        }
        d.deserialize_any(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const BLOCKED: &str = include_str!("../../../schemas/fixtures/llm-contracts-1.json");
    const ENTRY: &str = include_str!("../../../schemas/fixtures/llm-contracts-2.json");
    #[test]
    fn documented_examples_validate_against_exact_schemas() {
        let contracts = Contracts::new().unwrap();
        for text in [
            BLOCKED,
            ENTRY,
            include_str!("../../../schemas/fixtures/conversation-1.json"),
            include_str!("../../../schemas/fixtures/conversation-3.json"),
            include_str!("../../../schemas/fixtures/conversation-5.json"),
        ] {
            contracts.validate(Contract::ModelTurn, text).unwrap();
        }
        for text in [
            include_str!("../../../schemas/fixtures/conversation-2.json"),
            include_str!("../../../schemas/fixtures/conversation-4.json"),
        ] {
            contracts.validate(Contract::ToolResults, text).unwrap();
        }
        contracts
            .validate(
                Contract::StoreArtifactArguments,
                include_str!("../../../schemas/fixtures/research-contracts-1.json"),
            )
            .unwrap();
        contracts
            .validate(
                Contract::StoreArtifactResult,
                include_str!("../../../schemas/fixtures/research-contracts-2.json"),
            )
            .unwrap();
    }
    #[test]
    fn model_cannot_supply_authority_versions_or_extra_fields() {
        let c = Contracts::new().unwrap();
        for (key, value) in [
            ("proposal_id", Value::String("forged".into())),
            ("policy_version", Value::from(1)),
            ("protocol_version", Value::from(2)),
        ] {
            let mut v: Value = serde_json::from_str(ENTRY).unwrap();
            v[key] = value;
            assert!(c.validate(Contract::ModelTurn, &v.to_string()).is_err());
        }
        let mut v: Value = serde_json::from_str(ENTRY).unwrap();
        v["proposal"]["target_net_fraction"] = "0.11".into();
        assert!(c.validate(Contract::ModelTurn, &v.to_string()).is_err());
        v["proposal"]["target_net_fraction"] = "0.01".into();
        v["proposal"]["entry_range"]["minimum"] = "9999999999999999999999999999999999999999".into();
        assert!(c.validate(Contract::ModelTurn, &v.to_string()).is_err());
    }
    #[test]
    fn impossible_dates_offsets_and_duplicate_keys_are_rejected() {
        let c = Contracts::new().unwrap();
        let original: Value = serde_json::from_str(BLOCKED).unwrap();
        for time in [
            "2026-02-30T14:00:00Z",
            "2026-10-30T14:00:00+00:00",
            "2026-13-01T14:00:00Z",
        ] {
            let mut v = original.clone();
            v["research"]["scope"]["as_of"] = time.into();
            assert!(c.validate(Contract::ModelTurn, &v.to_string()).is_err());
        }
        assert!(
            c.validate(Contract::ModelTurn, &format!("{BLOCKED} {{}}"))
                .is_err()
        );
        assert!(
            c.validate(
                Contract::ModelTurn,
                r#"{"protocol_version":1,"protocol_version":1}"#
            )
            .is_err()
        );
    }
    #[test]
    fn incomplete_answers_cannot_carry_entry_proposals() {
        let c = Contracts::new().unwrap();
        let mut v: Value = serde_json::from_str(ENTRY).unwrap();
        v["reply"]["status"] = "INCOMPLETE".into();
        assert!(c.validate(Contract::ModelTurn, &v.to_string()).is_err());
        v["reply"]["status"] = "ANSWER".into();
        v["proposal"].as_object_mut().unwrap().remove("stop_price");
        assert!(c.validate(Contract::ModelTurn, &v.to_string()).is_err());
    }
    #[test]
    fn application_envelope_lifetime_is_exact_and_binding_is_required() {
        let c = Contracts::new().unwrap();
        let entry: Value = serde_json::from_str(ENTRY).unwrap();
        let mut envelope = serde_json::json!({"envelope_version":1,"proposal_id":"proposal_1","run_id":"run_1","session_id":"session_1","policy_version":1,"context_manifest_id":"context_1","account_snapshot_id":"account_1","research_dossier_id":"dossier_1","created_at":"2026-10-30T14:00:00Z","valid_until":"2026-10-30T14:05:00Z","body":entry["proposal"]});
        c.validate(Contract::ProposalEnvelope, &envelope.to_string())
            .unwrap();
        envelope["valid_until"] = "2026-10-30T14:05:00.001Z".into();
        assert!(
            c.validate(Contract::ProposalEnvelope, &envelope.to_string())
                .is_err()
        );
        envelope["valid_until"] = "2026-10-30T14:05:00Z".into();
        envelope["account_snapshot_id"] = Value::Null;
        assert!(
            c.validate(Contract::ProposalEnvelope, &envelope.to_string())
                .is_err()
        );
    }
}

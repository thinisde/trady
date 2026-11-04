//! Non-mutating, deterministic assessment of captured EUR inputs. Shadow identities
//! deliberately do not implement an orderable instrument or a broker mutation port.
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use trady_domain::{Error, InstrumentId, MarketState, Result, require};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowSettings {
    pub deployment_id: String,
    pub crosswalk_config: String,
    pub state_directory: String,
    pub interval_seconds: u32,
}

impl ShadowSettings {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_id(&self.deployment_id)
                && !self.crosswalk_config.is_empty()
                && self.state_directory.starts_with("/var/lib/")
                && !std::path::Path::new(&self.state_directory)
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
                && (10..=3600).contains(&self.interval_seconds),
            "invalid durable EUR shadow settings",
        )
    }
}

/// Additional provenance fields in the review bundle carry no executable authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowIdentity {
    pub internal_instrument_id: InstrumentId,
    pub broker_ticker: String,
    pub broker_instrument_id: String,
    pub isin: String,
    pub legal_name: String,
    pub exchange: String,
    pub exchange_mic: String,
    pub broker_exchange_id: i64,
    pub working_schedule_id: i64,
    pub currency: String,
    pub session_timezone: String,
    pub yahoo_symbol: String,
    pub alpha_vantage_symbol: String,
    pub minimum_quantity: Option<Decimal>,
    pub quantity_step: Option<Decimal>,
    pub price_tick: Option<Decimal>,
    pub fractional_support: String,
    pub orderable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowCrosswalk {
    pub schema_version: u32,
    pub review_reference: String,
    pub review_scope: String,
    pub broker_environment: String,
    pub broker_account_id: i64,
    pub base_currency: String,
    pub effective_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub broker_mutations_authorized: bool,
    pub live_enabled: bool,
    pub members: Vec<ShadowIdentity>,
}

impl ShadowCrosswalk {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.schema_version == 1
                && self.review_scope == "SHADOW_IDENTITY_ONLY"
                && self.broker_environment == "demo"
                && self.broker_account_id == 52211463
                && self.base_currency == "EUR"
                && !self.broker_mutations_authorized
                && !self.live_enabled
                && valid_id(&self.review_reference)
                && self.effective_at <= now
                && now < self.expires_at
                && !self.members.is_empty()
                && self.members.len() <= 8,
            "invalid or expired EUR shadow identity review",
        )?;
        let mut ids = BTreeSet::new();
        let mut tickers = BTreeSet::new();
        let mut isins = BTreeSet::new();
        for member in &self.members {
            require(
                ids.insert(member.internal_instrument_id.clone())
                    && tickers.insert(member.broker_ticker.clone())
                    && isins.insert(member.isin.clone())
                    && member.internal_instrument_id.as_str().ends_with("_EQ")
                    && member.broker_ticker == member.broker_instrument_id
                    && member.broker_ticker.ends_with("_EQ")
                    && valid_id(&member.broker_ticker)
                    && member.isin.len() == 12
                    && member.isin.bytes().all(|c| c.is_ascii_alphanumeric())
                    && member.currency == "EUR"
                    && !member.legal_name.is_empty()
                    && member.exchange_mic.len() == 4
                    && member.broker_exchange_id > 0
                    && member.working_schedule_id > 0
                    && !member.orderable
                    && member.minimum_quantity.is_none()
                    && member.quantity_step.is_none()
                    && member.price_tick.is_none()
                    && member.fractional_support == "UNVERIFIED",
                "shadow identities cannot supply guessed order rules or ambiguous broker mappings",
            )?;
            let suffix = match member.exchange_mic.as_str() {
                "XPAR" => ".PA",
                "XAMS" => ".AS",
                "XBRU" => ".BR",
                "XLIS" => ".LS",
                _ => return Err(Error("unreviewed exchange in EUR shadow crosswalk".into())),
            };
            require(
                member.yahoo_symbol.ends_with(suffix),
                "Yahoo symbol differs from reviewed listing",
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObservationStatus {
    Ok,
    Stale,
    Error,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderObservation {
    pub observation_id: String,
    pub provider: String,
    pub endpoint: String,
    pub source_id: String,
    pub requested_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub response_hash: String,
    pub rights_reference: String,
    pub status: ObservationStatus,
    pub error_code: Option<String>,
    pub capture_level: String,
    pub response: Value,
}

impl ProviderObservation {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_id(&self.observation_id)
                && valid_id(&self.source_id)
                && self.requested_at <= self.received_at
                && !self.provider.is_empty()
                && !self.rights_reference.is_empty()
                && !self.endpoint.contains(['?', '\n', '\r'])
                && self.response_hash
                    == crate::harness::content_hash(
                        &serde_json::to_string(&self.response)
                            .map_err(|_| Error("cannot encode EUR provider response".into()))?,
                    )
                && (self.status == ObservationStatus::Ok || self.error_code.is_some()),
            "invalid EUR provider observation or response hash",
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowQuote {
    pub instrument_id: InstrumentId,
    pub symbol: String,
    pub currency: String,
    pub bid: Option<Decimal>,
    pub ask: Option<Decimal>,
    pub last: Option<Decimal>,
    pub market_state: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowCandidate {
    pub instrument_id: InstrumentId,
    pub broker_ticker: String,
    pub isin: String,
    pub quote_observation_id: String,
    pub research_eligible: bool,
    pub entry_eligible: bool,
    pub blockers: Vec<String>,
}

pub fn assess_quote(
    identity: &ShadowIdentity,
    observation: &ProviderObservation,
    now: DateTime<Utc>,
    max_age_seconds: i64,
) -> Result<ShadowCandidate> {
    require(
        (1..=60).contains(&max_age_seconds),
        "EUR quote age exceeds the absolute service bound",
    )?;
    observation.validate()?;
    let mut blockers = Vec::new();
    let quote: Option<ShadowQuote> = serde_json::from_value(observation.response.clone()).ok();
    match quote {
        Some(quote) => {
            if quote.instrument_id != identity.internal_instrument_id
                || quote.symbol != identity.yahoo_symbol
                || quote.currency != "EUR"
            {
                blockers.push("QUOTE_IDENTITY_MISMATCH".into());
            }
            if !matches!((quote.bid, quote.ask), (Some(b), Some(a)) if b > Decimal::ZERO && a >= b)
            {
                blockers.push("MISSING_INVALID_OR_CROSSED_BID_ASK".into());
            }
            let age = now
                .signed_duration_since(quote.observed_at)
                .num_milliseconds();
            if age < 0 || age > max_age_seconds * 1000 || quote.received_at > now {
                blockers.push("QUOTE_STALE_OR_FUTURE".into());
            }
            if quote.market_state.as_deref().map(MarketState::parse) != Some(MarketState::Regular) {
                blockers.push("MARKET_NOT_OPEN_OR_UNKNOWN".into());
            }
        }
        None => blockers.push("QUOTE_UNAVAILABLE".into()),
    }
    if !matches!(
        observation.status,
        ObservationStatus::Ok | ObservationStatus::Stale
    ) {
        blockers.push("PROVIDER_NOT_AVAILABLE".into());
    }
    let research_eligible = blockers.is_empty();
    // A research identity carries no executable precision or venue acceptance.
    blockers.push("BROKER_EXECUTION_RULES_UNVERIFIED".into());
    blockers.push("QUOTE_TO_BROKER_COMPATIBILITY_UNACCEPTED".into());
    Ok(ShadowCandidate {
        instrument_id: identity.internal_instrument_id.clone(),
        broker_ticker: identity.broker_ticker.clone(),
        isin: identity.isin.clone(),
        quote_observation_id: observation.observation_id.clone(),
        research_eligible,
        entry_eligible: false,
        blockers,
    })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    fn crosswalk() -> ShadowCrosswalk {
        serde_json::from_str(include_str!(
            "../../../docs/evidence/eur-shadow-review-2026-11-04/crosswalk.json"
        ))
        .unwrap()
    }
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-04T13:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn quote(age: i64, state: &str) -> ProviderObservation {
        let response = json!({"instrument_id":"AIR_PA_EQ","symbol":"AIR.PA","currency":"EUR",
            "bid":"100","ask":"100.01","last":"100","market_state":state,
            "observed_at":now()-Duration::seconds(age),"received_at":now()});
        ProviderObservation {
            observation_id: "quote_test".into(),
            provider: "recorded".into(),
            endpoint: "https://example.invalid/quote".into(),
            source_id: "test".into(),
            requested_at: now(),
            received_at: now(),
            published_at: None,
            response_hash: crate::harness::content_hash(&response.to_string()),
            rights_reference: "fixture".into(),
            status: ObservationStatus::Ok,
            error_code: None,
            capture_level: "RECORDED_FIXTURE".into(),
            response,
        }
    }
    #[test]
    fn reviewed_identity_cannot_turn_guessed_quantity_rules_into_execution_authority() {
        let mut value = crosswalk();
        value.validate(now()).unwrap();
        value.members[0].minimum_quantity = Some(Decimal::ONE);
        assert!(value.validate(now()).is_err());
        let mut value = crosswalk();
        value.members[0].orderable = true;
        assert!(value.validate(now()).is_err());
        let mut value = crosswalk();
        value.broker_environment = "live".into();
        assert!(value.validate(now()).is_err());
    }
    #[test]
    fn fresh_research_quote_does_not_authorize_a_broker_mutation() {
        let candidate =
            assess_quote(&crosswalk().members[0], &quote(1, "REGULAR"), now(), 10).unwrap();
        assert!(candidate.research_eligible);
        assert!(!candidate.entry_eligible);
        assert!(
            candidate
                .blockers
                .contains(&"BROKER_EXECUTION_RULES_UNVERIFIED".into())
        );
    }
    #[test]
    fn stale_unknown_rebound_and_tampered_inputs_are_rejected() {
        let identity = &crosswalk().members[0];
        for observation in [
            quote(11, "REGULAR"),
            quote(0, "UNKNOWN"),
            quote(-1, "REGULAR"),
        ] {
            assert!(
                !assess_quote(identity, &observation, now(), 10)
                    .unwrap()
                    .research_eligible
            );
        }
        assert!(assess_quote(identity, &quote(0, "REGULAR"), now(), 61).is_err());
        let mut rebound = quote(0, "REGULAR");
        rebound.response["symbol"] = json!("ASML");
        assert!(assess_quote(identity, &rebound, now(), 10).is_err());
        rebound.response_hash = crate::harness::content_hash(&rebound.response.to_string());
        assert!(
            !assess_quote(identity, &rebound, now(), 10)
                .unwrap()
                .research_eligible
        );
    }
}

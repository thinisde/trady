//! Reviewed EUR instrument identity, universe snapshots, and deterministic
//! eligibility checks.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{
    Currency, InstrumentId, Result,
    market::{SessionWindow, VenueQuote},
    require,
    research::SourceRights,
    screening::european_listing_mic,
};

pub const EUR_UNIVERSE_SCHEMA_VERSION: u32 = 1;
pub const MAX_EUR_UNIVERSE_MEMBERS: usize = 2_048;

/// A reviewed, exact crosswalk between the share class, Yahoo listing, and
/// Trading 212 instrument. Ticker strings never become executable identities
/// without this record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EurInstrumentBinding {
    pub instrument_id: InstrumentId,
    pub broker_instrument_id: String,
    pub isin: String,
    pub legal_name: String,
    pub ticker: String,
    pub exchange_mic: String,
    pub country: String,
    pub currency: Currency,
    pub session_timezone: String,
    pub yahoo_symbol: String,
    pub minimum_quantity: Decimal,
    pub price_tick: Decimal,
    pub quantity_step: Decimal,
    pub fractional_orders: bool,
    pub effective_from: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub liquidity_threshold: LiquidityThreshold,
    pub volatility_threshold: VolatilityThreshold,
    pub provenance_ids: Vec<String>,
    pub review_owner: String,
}

impl EurInstrumentBinding {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.instrument_id.as_str().ends_with("_EQ")
                && valid_reference(&self.broker_instrument_id)
                && valid_isin(&self.isin)
                && text(&self.legal_name, 256)
                && valid_ticker(&self.ticker)
                && self.exchange_mic.len() == 4
                && self
                    .exchange_mic
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && european_listing_mic(&self.exchange_mic)
                && self.country.len() == 2
                && self.country.bytes().all(|b| b.is_ascii_uppercase())
                && self.currency.as_str() == "EUR"
                && !self.session_timezone.is_empty()
                && self.session_timezone.len() <= 64
                && reviewed_yahoo_symbol(&self.yahoo_symbol, &self.exchange_mic)
                && self.minimum_quantity > Decimal::ZERO
                && self.price_tick > Decimal::ZERO
                && self.quantity_step > Decimal::ZERO
                && self.effective_from <= now
                && self
                    .expires_at
                    .is_none_or(|until| self.effective_from < until && now < until)
                && !self.provenance_ids.is_empty()
                && self.provenance_ids.len() <= 16
                && self.provenance_ids.iter().all(|id| valid_reference(id))
                && valid_reference(&self.review_owner),
            "invalid EUR instrument binding",
        )?;
        self.liquidity_threshold.validate()?;
        self.volatility_threshold.validate()?;
        Ok(())
    }

    pub fn maps_quote(&self, quote: &VenueQuote) -> bool {
        quote.instrument_id == self.instrument_id
            && quote.broker_instrument_id == self.broker_instrument_id
            && quote.isin == self.isin
            && quote.exchange_mic == self.exchange_mic
            && quote.currency == self.currency
            && quote.yahoo_symbol == self.yahoo_symbol
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiquidityThreshold {
    pub minimum_average_daily_notional: Decimal,
    pub maximum_order_participation_bps: Decimal,
    pub maximum_spread_bps: Decimal,
}

impl LiquidityThreshold {
    fn validate(&self) -> Result<()> {
        require(
            self.minimum_average_daily_notional > Decimal::ZERO
                && self.maximum_order_participation_bps > Decimal::ZERO
                && self.maximum_order_participation_bps <= Decimal::from(1000u32)
                && self.maximum_spread_bps > Decimal::ZERO
                && self.maximum_spread_bps <= Decimal::from(500u32),
            "invalid EUR liquidity thresholds",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolatilityThreshold {
    pub minimum_fraction: Decimal,
    pub maximum_fraction: Decimal,
}

impl VolatilityThreshold {
    fn validate(&self) -> Result<()> {
        require(
            self.minimum_fraction >= Decimal::ZERO
                && self.maximum_fraction > self.minimum_fraction
                && self.maximum_fraction <= Decimal::ONE,
            "invalid EUR volatility thresholds",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EurUniverseProvenance {
    pub source_id: String,
    pub source_url: String,
    pub provider: String,
    pub content_sha256: String,
    pub fetched_at: DateTime<Utc>,
    pub rights: SourceRights,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EurUniverseSnapshot {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub revision: u32,
    pub effective_from: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub generated_at: DateTime<Utc>,
    pub provenance: EurUniverseProvenance,
    pub members: Vec<EurInstrumentBinding>,
}

impl EurUniverseSnapshot {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.schema_version == EUR_UNIVERSE_SCHEMA_VERSION
                && self.revision > 0
                && valid_reference(&self.snapshot_id)
                && self.effective_from <= self.generated_at
                && self.generated_at <= now
                && self.effective_from <= now
                && self.expires_at > self.effective_from
                && now < self.expires_at
                && self.members.len() <= MAX_EUR_UNIVERSE_MEMBERS,
            "invalid, future or expired EUR universe snapshot",
        )?;
        require(
            valid_reference(&self.provenance.source_id)
                && self.provenance.source_url.starts_with("https://")
                && self.provenance.source_url.len() <= 2048
                && text(&self.provenance.provider, 256)
                && is_sha256(&self.provenance.content_sha256)
                && self.provenance.fetched_at <= now
                && self.provenance.rights.store_content
                && self.provenance.rights.retention_hours > 0,
            "invalid EUR universe provenance",
        )?;
        let mut instrument_ids = BTreeSet::new();
        let mut broker_ids = BTreeSet::new();
        let mut isins = BTreeSet::new();
        let mut yahoo_symbols = BTreeSet::new();
        for member in &self.members {
            member.validate(now)?;
            require(
                instrument_ids.insert(&member.instrument_id)
                    && broker_ids.insert(&member.broker_instrument_id)
                    && isins.insert(&member.isin)
                    && yahoo_symbols.insert(&member.yahoo_symbol),
                "EUR universe contains an ambiguous instrument crosswalk",
            )?;
        }
        require(!self.members.is_empty(), "EUR universe snapshot is empty")
    }

    pub fn member(&self, id: &InstrumentId) -> Option<&EurInstrumentBinding> {
        self.members
            .iter()
            .find(|member| &member.instrument_id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EurEligibilityPolicy {
    pub quote_max_age_seconds: i64,
    pub minimum_seconds_to_flatten: i64,
    pub minimum_price: Decimal,
    pub maximum_price: Decimal,
    pub maximum_volatility_fraction: Decimal,
}

impl Default for EurEligibilityPolicy {
    fn default() -> Self {
        Self {
            quote_max_age_seconds: 10,
            minimum_seconds_to_flatten: 1_800,
            minimum_price: Decimal::new(1, 0),
            maximum_price: Decimal::new(10_000, 0),
            maximum_volatility_fraction: Decimal::new(12, 2),
        }
    }
}

impl EurEligibilityPolicy {
    pub fn validate(&self) -> Result<()> {
        require(
            (1..=300).contains(&self.quote_max_age_seconds)
                && (60..=86_400).contains(&self.minimum_seconds_to_flatten)
                && self.minimum_price > Decimal::ZERO
                && self.maximum_price > self.minimum_price
                && self.maximum_volatility_fraction > Decimal::ZERO
                && self.maximum_volatility_fraction <= Decimal::ONE,
            "invalid EUR eligibility policy",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EurLiquidityObservation {
    pub average_daily_notional: Decimal,
    pub order_notional: Decimal,
    pub volatility_fraction: Decimal,
    pub broker_available: bool,
    pub event_window_clear: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EligibilityRejection {
    MissingBrokerMapping,
    QuoteUnavailable,
    QuoteStale,
    QuoteNotTwoSided,
    QuoteCrossed,
    WrongVenue,
    WrongCurrency,
    MarketClosed,
    Halted,
    SpreadTooWide,
    InsufficientLiquidity,
    VolatilityOutOfBounds,
    PriceOutOfBounds,
    EventWindowBlocked,
    TooLittleTimeToFlatten,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EurEligibilityDecision {
    pub instrument_id: InstrumentId,
    pub eligible: bool,
    pub rejection_reasons: Vec<EligibilityRejection>,
    pub quote_id: Option<String>,
    pub session_id: Option<String>,
}

/// Every universe member receives a decision. This is intentionally separate
/// from LLM ranking: a model only sees decisions with no rejection reasons.
pub fn evaluate_eligibility(
    binding: &EurInstrumentBinding,
    quote: Option<&VenueQuote>,
    session: Option<&SessionWindow>,
    liquidity: Option<&EurLiquidityObservation>,
    policy: &EurEligibilityPolicy,
    now: DateTime<Utc>,
) -> Result<EurEligibilityDecision> {
    policy.validate()?;
    binding.validate(now)?;
    let mut reasons = Vec::new();
    let mut quote_id = None;
    let mut session_id = None;
    let Some(quote) = quote else {
        reasons.push(EligibilityRejection::QuoteUnavailable);
        return Ok(decision(binding, reasons, quote_id, session_id));
    };
    quote_id = Some(quote.quote_id.clone());
    if !binding.maps_quote(quote) {
        reasons.push(EligibilityRejection::WrongVenue);
    }
    if quote.currency != binding.currency || quote.currency.as_str() != "EUR" {
        reasons.push(EligibilityRejection::WrongCurrency);
    }
    if quote.received_at > now
        || quote.observed_at > quote.received_at
        || now.signed_duration_since(quote.observed_at)
            > Duration::seconds(policy.quote_max_age_seconds)
    {
        reasons.push(EligibilityRejection::QuoteStale);
    }
    match (quote.bid, quote.ask) {
        (Some(bid), Some(ask)) if bid > Decimal::ZERO && ask >= bid => {
            let spread = quote.spread_bps()?;
            if spread > binding.liquidity_threshold.maximum_spread_bps {
                reasons.push(EligibilityRejection::SpreadTooWide);
            }
            let price = ask;
            if price < policy.minimum_price || price > policy.maximum_price {
                reasons.push(EligibilityRejection::PriceOutOfBounds);
            }
            if quote.quality != crate::market::QuoteQuality::Executable {
                reasons.push(EligibilityRejection::QuoteNotTwoSided);
            }
        }
        (Some(_), Some(_)) => reasons.push(EligibilityRejection::QuoteCrossed),
        _ => reasons.push(EligibilityRejection::QuoteNotTwoSided),
    }
    if !quote.market_state.entry_allowed() {
        if matches!(quote.market_state, crate::market::MarketState::Halted) {
            reasons.push(EligibilityRejection::Halted);
        } else {
            reasons.push(EligibilityRejection::MarketClosed);
        }
    }
    let Some(session) = session else {
        reasons.push(EligibilityRejection::MarketClosed);
        return Ok(decision(binding, reasons, quote_id, session_id));
    };
    session_id = Some(session.session_id.clone());
    if session.exchange_mic != binding.exchange_mic {
        reasons.push(EligibilityRejection::WrongVenue);
    }
    if !session.permits_entry(now, policy.minimum_seconds_to_flatten) {
        reasons.push(EligibilityRejection::TooLittleTimeToFlatten);
    }
    if let Some(liquidity) = liquidity {
        if !liquidity.broker_available {
            reasons.push(EligibilityRejection::MissingBrokerMapping);
        }
        if liquidity.average_daily_notional
            < binding.liquidity_threshold.minimum_average_daily_notional
            || liquidity.order_notional <= Decimal::ZERO
            || liquidity
                .order_notional
                .checked_div(liquidity.average_daily_notional)
                .is_none_or(|fraction| {
                    fraction * Decimal::from(10_000u32)
                        > binding.liquidity_threshold.maximum_order_participation_bps
                })
        {
            reasons.push(EligibilityRejection::InsufficientLiquidity);
        }
        if liquidity.volatility_fraction < binding.volatility_threshold.minimum_fraction
            || liquidity.volatility_fraction > binding.volatility_threshold.maximum_fraction
            || liquidity.volatility_fraction > policy.maximum_volatility_fraction
        {
            reasons.push(EligibilityRejection::VolatilityOutOfBounds);
        }
        if !liquidity.event_window_clear {
            reasons.push(EligibilityRejection::EventWindowBlocked);
        }
    } else {
        reasons.push(EligibilityRejection::InsufficientLiquidity);
    }
    Ok(decision(binding, reasons, quote_id, session_id))
}

fn decision(
    binding: &EurInstrumentBinding,
    rejection_reasons: Vec<EligibilityRejection>,
    quote_id: Option<String>,
    session_id: Option<String>,
) -> EurEligibilityDecision {
    EurEligibilityDecision {
        instrument_id: binding.instrument_id.clone(),
        eligible: rejection_reasons.is_empty(),
        rejection_reasons,
        quote_id,
        session_id,
    }
}

fn reviewed_yahoo_symbol(symbol: &str, mic: &str) -> bool {
    let symbol = symbol.trim().to_ascii_uppercase();
    let suffix = match mic {
        "XAMS" => ".AS",
        "XBRU" => ".BR",
        "XLIS" => ".LS",
        "XMIL" => ".MI",
        "XPAR" => ".PA",
        "XOSL" => ".OL",
        "XETR" | "XFRA" => ".DE",
        "XSTO" => ".ST",
        "XWAR" => ".WA",
        "XHEL" => ".HE",
        "XMAD" | "XVAL" => ".MC",
        "XICE" => ".IC",
        "XCSE" => ".CO",
        "XATH" => ".AT",
        _ => return false,
    };
    symbol.ends_with(suffix)
        && symbol.len() > suffix.len()
        && symbol[..symbol.len() - suffix.len()]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_isin(value: &str) -> bool {
    value.len() == 12 && value.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn valid_ticker(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b".-".contains(&b))
}

fn valid_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

fn text(value: &str, max: usize) -> bool {
    let length = value.chars().count();
    !value.trim().is_empty() && length <= max && !value.contains(['\0', '\r', '\n'])
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::{MarketState, QuoteQuality};

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }

    fn binding() -> EurInstrumentBinding {
        EurInstrumentBinding {
            instrument_id: InstrumentId::new("AIR_PA_EQ").expect("id"),
            broker_instrument_id: "t212_123".into(),
            isin: "FR0000120404".into(),
            legal_name: "Airbus SE".into(),
            ticker: "AIR".into(),
            exchange_mic: "XPAR".into(),
            country: "FR".into(),
            currency: Currency::new("EUR").expect("currency"),
            session_timezone: "Europe/Paris".into(),
            yahoo_symbol: "AIR.PA".into(),
            minimum_quantity: Decimal::ONE,
            price_tick: Decimal::new(1, 2),
            quantity_step: Decimal::ONE,
            fractional_orders: false,
            effective_from: now() - Duration::days(1),
            expires_at: Some(now() + Duration::days(1)),
            liquidity_threshold: LiquidityThreshold {
                minimum_average_daily_notional: Decimal::from(1_000_000u32),
                maximum_order_participation_bps: Decimal::from(10u32),
                maximum_spread_bps: Decimal::from(20u32),
            },
            volatility_threshold: VolatilityThreshold {
                minimum_fraction: Decimal::new(1, 3),
                maximum_fraction: Decimal::new(12, 2),
            },
            provenance_ids: vec!["review_1".into()],
            review_owner: "owner".into(),
        }
    }

    fn quote() -> VenueQuote {
        VenueQuote {
            quote_id: "quote_1".into(),
            instrument_id: InstrumentId::new("AIR_PA_EQ").expect("id"),
            broker_instrument_id: "t212_123".into(),
            isin: "FR0000120404".into(),
            exchange_mic: "XPAR".into(),
            currency: Currency::new("EUR").expect("currency"),
            yahoo_symbol: "AIR.PA".into(),
            bid: Some(Decimal::new(100, 0)),
            ask: Some(Decimal::new(1001, 1)),
            last: Some(Decimal::new(100, 0)),
            bid_size: Some(Decimal::ONE),
            ask_size: Some(Decimal::ONE),
            volume: Some(Decimal::from(1000)),
            observed_at: now(),
            received_at: now(),
            market_state: MarketState::Regular,
            source: "yfinance_rs".into(),
            quality: QuoteQuality::Executable,
        }
    }

    fn session() -> SessionWindow {
        SessionWindow {
            session_id: "xpar_2026_11_03".into(),
            exchange_mic: "XPAR".into(),
            timezone: "Europe/Paris".into(),
            opens_at: now() - Duration::hours(1),
            closes_at: now() + Duration::hours(8),
            flatten_at: now() + Duration::hours(7),
            observed_at: now() - Duration::hours(2),
            source: "reviewed_calendar".into(),
        }
    }

    #[test]
    fn exact_crosswalk_and_eur_constraints_are_required() {
        assert!(binding().validate(now()).is_ok());
        let mut wrong = binding();
        wrong.yahoo_symbol = "AIR.DE".into();
        assert!(wrong.validate(now()).is_err());
    }

    #[test]
    fn screener_returns_every_exclusion_and_only_eligible_rows_to_ranker() {
        let b = binding();
        let good = evaluate_eligibility(
            &b,
            Some(&quote()),
            Some(&session()),
            Some(&EurLiquidityObservation {
                average_daily_notional: Decimal::from(10_000_000u32),
                order_notional: Decimal::from(1000u32),
                volatility_fraction: Decimal::new(2, 2),
                broker_available: true,
                event_window_clear: true,
            }),
            &EurEligibilityPolicy::default(),
            now(),
        )
        .expect("decision");
        assert!(good.eligible);

        let mut bad_quote = quote();
        bad_quote.market_state = MarketState::Closed;
        let bad = evaluate_eligibility(
            &b,
            Some(&bad_quote),
            Some(&session()),
            None,
            &EurEligibilityPolicy::default(),
            now(),
        )
        .expect("decision");
        assert!(!bad.eligible);
        assert!(
            bad.rejection_reasons
                .contains(&EligibilityRejection::MarketClosed)
        );
        assert!(
            bad.rejection_reasons
                .contains(&EligibilityRejection::InsufficientLiquidity)
        );
    }
}

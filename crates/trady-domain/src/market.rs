//! Venue-bound market observations used by the EUR runtime.
//!
//! The existing [`Quote`](crate::Quote) type is kept for legacy replay
//! contracts.  These types carry the identity and quality facts that are
//! required before a Yahoo observation can be compared with a Trading 212
//! instrument.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{Currency, Error, InstrumentId, Result, require};

/// Provider-reported market state. Unknown values remain unknown and cannot
/// authorize a new entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketState {
    Regular,
    PreMarket,
    PostMarket,
    Auction,
    Closed,
    Halted,
    #[default]
    Unknown,
}

impl MarketState {
    pub fn parse(value: &str) -> Self {
        match value
            .trim()
            .to_ascii_uppercase()
            .replace(['-', '_', ' '], "")
            .as_str()
        {
            "REGULAR" | "REGULARMARKET" | "REGULARREGULAR" => Self::Regular,
            "PRE" | "PREMARKET" | "PREPRE" => Self::PreMarket,
            "POST" | "POSTMARKET" | "POSTPOST" => Self::PostMarket,
            "AUCTION" | "AUCTIONOPEN" | "AUCTIONCLOSE" => Self::Auction,
            "CLOSED" | "CLOSE" => Self::Closed,
            "HALTED" | "SUSPENDED" => Self::Halted,
            _ => Self::Unknown,
        }
    }

    pub fn entry_allowed(self) -> bool {
        matches!(self, Self::Regular | Self::Auction)
    }

    pub fn session_open(self) -> bool {
        matches!(
            self,
            Self::Regular | Self::PreMarket | Self::PostMarket | Self::Auction
        )
    }
}

/// Quality of a normalized provider observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuoteQuality {
    Executable,
    LastTradeOnly,
    Delayed,
    Stale,
    Crossed,
    VenueMismatch,
    Missing,
    Invalid,
    #[default]
    Unknown,
}

impl QuoteQuality {
    pub fn executable(self) -> bool {
        matches!(self, Self::Executable)
    }
}

/// A two-sided observation bound to the exact share class and broker listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VenueQuote {
    pub quote_id: String,
    pub instrument_id: InstrumentId,
    /// Trading 212's account-scoped instrument identifier, not a Yahoo symbol.
    pub broker_instrument_id: String,
    pub isin: String,
    /// ISO 10383 market identifier.
    pub exchange_mic: String,
    pub currency: Currency,
    pub yahoo_symbol: String,
    pub bid: Option<Decimal>,
    pub ask: Option<Decimal>,
    pub last: Option<Decimal>,
    pub bid_size: Option<Decimal>,
    pub ask_size: Option<Decimal>,
    pub volume: Option<Decimal>,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub market_state: MarketState,
    pub source: String,
    pub quality: QuoteQuality,
}

impl VenueQuote {
    pub fn validate(&self, now: DateTime<Utc>, max_age_seconds: i64) -> Result<()> {
        require(
            valid_reference(&self.quote_id)
                && valid_reference(&self.broker_instrument_id)
                && valid_reference(&self.source)
                && self.isin.len() == 12
                && self.isin.bytes().all(|b| b.is_ascii_alphanumeric())
                && self.exchange_mic.len() == 4
                && self
                    .exchange_mic
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && !self.yahoo_symbol.is_empty()
                && self.yahoo_symbol.len() <= 64,
            "invalid venue quote identity",
        )?;
        require(
            self.currency.as_str() == "EUR"
                && self.observed_at <= self.received_at
                && self.received_at <= now
                && max_age_seconds >= 0
                && now.signed_duration_since(self.observed_at)
                    <= Duration::seconds(max_age_seconds),
            "venue quote is future-dated or stale",
        )?;
        for value in [
            self.bid,
            self.ask,
            self.last,
            self.bid_size,
            self.ask_size,
            self.volume,
        ]
        .into_iter()
        .flatten()
        {
            require(
                value >= Decimal::ZERO,
                "venue quote contains a negative value",
            )?;
        }
        if let (Some(bid), Some(ask)) = (self.bid, self.ask) {
            require(
                bid > Decimal::ZERO && ask > Decimal::ZERO && ask >= bid,
                "venue quote is crossed or empty",
            )?;
        }
        if self.quality == QuoteQuality::Executable {
            require(
                self.bid.is_some() && self.ask.is_some() && self.market_state.entry_allowed(),
                "executable quote lacks an entry-eligible market state",
            )?;
        }
        Ok(())
    }

    pub fn spread_bps(&self) -> Result<Decimal> {
        let bid = self
            .bid
            .ok_or_else(|| Error("quote bid unavailable".into()))?;
        let ask = self
            .ask
            .ok_or_else(|| Error("quote ask unavailable".into()))?;
        require(bid > Decimal::ZERO && ask >= bid, "invalid quote spread")?;
        let midpoint = bid
            .checked_add(ask)
            .and_then(|sum| sum.checked_div(Decimal::from(2)))
            .ok_or_else(|| Error("quote midpoint overflow".into()))?;
        ask.checked_sub(bid)
            .and_then(|gap| gap.checked_div(midpoint))
            .and_then(|fraction| fraction.checked_mul(Decimal::from(10_000u32)))
            .ok_or_else(|| Error("quote spread overflow".into()))
    }

    pub fn executable(&self, now: DateTime<Utc>, max_age_seconds: i64) -> Result<()> {
        self.validate(now, max_age_seconds)?;
        require(
            self.quality.executable()
                && self.bid.is_some()
                && self.ask.is_some()
                && self.market_state.entry_allowed(),
            "quote is not executable",
        )
    }
}

/// A verified exchange session with a hard same-day flatten deadline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionWindow {
    pub session_id: String,
    pub exchange_mic: String,
    pub timezone: String,
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
    pub flatten_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub source: String,
}

impl SessionWindow {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_reference(&self.session_id)
                && self.exchange_mic.len() == 4
                && self.timezone.len() <= 64
                && !self.timezone.is_empty()
                && valid_reference(&self.source)
                && self.opens_at < self.flatten_at
                && self.flatten_at <= self.closes_at
                && self.observed_at <= now
                && self.observed_at <= self.opens_at,
            "invalid or unverified exchange session",
        )
    }

    pub fn permits_entry(&self, now: DateTime<Utc>, minimum_seconds_to_close: i64) -> bool {
        self.opens_at <= now
            && now < self.flatten_at
            && self.flatten_at.signed_duration_since(now).num_seconds() > minimum_seconds_to_close
    }

    pub fn must_flatten(&self, now: DateTime<Utc>) -> bool {
        now >= self.flatten_at
    }
}

/// Completed OHLCV observation. The current, incomplete bar is never accepted
/// by the strategy registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedBar {
    pub bar_id: String,
    pub instrument_id: InstrumentId,
    pub interval_seconds: u32,
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Option<Decimal>,
    pub provider: String,
    pub received_at: DateTime<Utc>,
    pub completed: bool,
}

impl CompletedBar {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_reference(&self.bar_id)
                && valid_reference(&self.provider)
                && self.interval_seconds >= 60
                && self.opens_at < self.closes_at
                && self.closes_at <= now
                && self.received_at <= now
                && self.open > Decimal::ZERO
                && self.high >= self.open
                && self.high >= self.close
                && self.low > Decimal::ZERO
                && self.low <= self.open
                && self.low <= self.close
                && self.completed,
            "invalid or incomplete OHLCV bar",
        )?;
        if let Some(volume) = self.volume {
            require(volume >= Decimal::ZERO, "negative bar volume")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarSnapshot {
    pub snapshot_id: String,
    pub instrument_id: InstrumentId,
    pub provider: String,
    pub interval_seconds: u32,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub bars: Vec<CompletedBar>,
}

impl BarSnapshot {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_reference(&self.snapshot_id)
                && valid_reference(&self.provider)
                && self.bars.len() <= 2_048
                && self.observed_at <= self.received_at
                && self.received_at <= now,
            "invalid bar snapshot metadata",
        )?;
        let mut ids = BTreeSet::new();
        let mut last_close = None;
        for bar in &self.bars {
            require(
                bar.instrument_id == self.instrument_id
                    && bar.provider == self.provider
                    && bar.interval_seconds == self.interval_seconds
                    && ids.insert(&bar.bar_id),
                "bar snapshot contains an incompatible or duplicate bar",
            )?;
            bar.validate(now)?;
            if let Some(previous) = last_close {
                require(previous < bar.closes_at, "bars are unordered or duplicated")?;
            }
            last_close = Some(bar.closes_at);
        }
        Ok(())
    }
}

fn valid_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Currency;

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }

    fn quote(state: MarketState, quality: QuoteQuality) -> VenueQuote {
        VenueQuote {
            quote_id: "q_1".into(),
            instrument_id: InstrumentId::new("AIR_PA_EQ").expect("id"),
            broker_instrument_id: "t212_123".into(),
            isin: "FR0000120404".into(),
            exchange_mic: "XPAR".into(),
            currency: Currency::new("EUR").expect("currency"),
            yahoo_symbol: "AIR.PA".into(),
            bid: Some(Decimal::new(100, 0)),
            ask: Some(Decimal::new(101, 0)),
            last: Some(Decimal::new(100, 0)),
            bid_size: Some(Decimal::ONE),
            ask_size: Some(Decimal::ONE),
            volume: Some(Decimal::from(1000)),
            observed_at: at(),
            received_at: at(),
            market_state: state,
            source: "yfinance_rs".into(),
            quality,
        }
    }

    #[test]
    fn market_state_parser_does_not_conflate_unknown_values() {
        assert_eq!(MarketState::parse("REGULAR"), MarketState::Regular);
        assert_eq!(MarketState::parse("PRE_MARKET"), MarketState::PreMarket);
        assert_eq!(MarketState::parse("post-market"), MarketState::PostMarket);
        assert_eq!(MarketState::parse("POSTPOST"), MarketState::PostMarket);
        assert_eq!(MarketState::parse("closed"), MarketState::Closed);
        assert_eq!(MarketState::parse("mystery"), MarketState::Unknown);
    }

    #[test]
    fn executable_quotes_require_regular_or_auction_state() {
        assert!(
            quote(MarketState::Regular, QuoteQuality::Executable)
                .executable(at(), 10)
                .is_ok()
        );
        assert!(
            quote(MarketState::Closed, QuoteQuality::Executable)
                .executable(at(), 10)
                .is_err()
        );
        assert!(
            quote(MarketState::Regular, QuoteQuality::LastTradeOnly)
                .executable(at(), 10)
                .is_err()
        );
    }

    #[test]
    fn session_and_flatten_deadline_are_monotonic() {
        let session = SessionWindow {
            session_id: "xpar_2026_11_03".into(),
            exchange_mic: "XPAR".into(),
            timezone: "Europe/Paris".into(),
            opens_at: at() - Duration::hours(1),
            closes_at: at() + Duration::hours(8),
            flatten_at: at() + Duration::hours(7),
            observed_at: at() - Duration::hours(2),
            source: "reviewed_calendar".into(),
        };
        assert!(session.validate(at()).is_ok());
        assert!(session.permits_entry(at(), 1800));
        assert!(session.must_flatten(session.flatten_at));
    }
}

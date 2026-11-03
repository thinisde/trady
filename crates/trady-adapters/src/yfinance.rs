//! Read-only Yahoo Finance quotes through `yfinance-rs`.
//!
//! Yahoo symbols are deliberately supplied by an approved, bounded mapping.
//! The provider is useful for Euronext listings, but a ticker string alone is
//! not an instrument identity and is never inferred from arbitrary user input.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    thread,
    time::Duration,
};
use trady_domain::{Currency, Error, InstrumentId, Quote, Result, require};
use yfinance_rs::{Ticker, YfClient};

const MAX_SYMBOLS: usize = 512;
const MAX_YAHOO_SYMBOL_BYTES: usize = 64;
const EURONEXT_SUFFIXES: [&str; 7] = [".AS", ".BR", ".IR", ".LS", ".MI", ".OL", ".PA"];

fn default_timeout_seconds() -> u64 {
    15
}

fn default_quote_max_age_seconds() -> u64 {
    10
}

/// Explicit Yahoo symbol bindings for an Euronext-oriented deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YfinanceDataConfig {
    pub config_version: u32,
    pub enabled: bool,
    pub base_currency: Currency,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_quote_max_age_seconds")]
    pub quote_max_age_seconds: u64,
    /// Internal Trady instrument ID to Yahoo Finance ticker, for example
    /// `AIR_PA_EQ` to `AIR.PA`.
    pub symbols: BTreeMap<InstrumentId, String>,
}

impl YfinanceDataConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1,
            "unsupported yfinance configuration version",
        )?;
        require(
            self.base_currency.as_str() == "EUR",
            "yfinance Euronext quotes must use EUR",
        )?;
        require(
            (1..=60).contains(&self.timeout_seconds),
            "invalid yfinance timeout",
        )?;
        require(
            (1..=300).contains(&self.quote_max_age_seconds),
            "invalid yfinance quote age",
        )?;
        require(
            self.symbols.len() <= MAX_SYMBOLS && (!self.enabled || !self.symbols.is_empty()),
            "invalid yfinance symbol mapping size",
        )?;
        let mut yahoo_symbols = BTreeSet::new();
        for (instrument, symbol) in &self.symbols {
            require(
                instrument.as_str().ends_with("_EQ")
                    && !symbol.is_empty()
                    && symbol.len() <= MAX_YAHOO_SYMBOL_BYTES
                    && symbol
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-=^_".contains(&b))
                    && is_euronext_symbol(symbol)
                    && yahoo_symbols.insert(symbol),
                "invalid or duplicate yfinance Euronext symbol mapping",
            )?;
        }
        Ok(())
    }
}

/// Yahoo exchange suffixes used by the Euronext group.
pub fn is_euronext_symbol(symbol: &str) -> bool {
    let symbol = symbol.trim().to_ascii_uppercase();
    EURONEXT_SUFFIXES
        .iter()
        .any(|suffix| symbol.ends_with(suffix))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderQuote {
    currency: String,
    price: Option<rust_decimal::Decimal>,
    bid: Option<rust_decimal::Decimal>,
    ask: Option<rust_decimal::Decimal>,
    observed_at: Option<DateTime<Utc>>,
    market_state: Option<String>,
}

/// A read-only live quote client backed by Yahoo Finance.
pub struct YfinanceMarketData {
    config: YfinanceDataConfig,
    client: YfClient,
}

/// Provider snapshot returned by the read-only CLI and diagnostics path.
///
/// Yahoo frequently omits a two-sided book for delayed or closed European
/// listings. Those observations remain useful as last-trade context, but they
/// do not become an executable Quote unless both bid and ask are present and
/// fresh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct YfinanceQuoteSnapshot {
    pub instrument_id: InstrumentId,
    pub symbol: String,
    pub currency: Currency,
    pub last: Option<rust_decimal::Decimal>,
    pub bid: Option<rust_decimal::Decimal>,
    pub ask: Option<rust_decimal::Decimal>,
    pub observed_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub market_state: Option<String>,
    pub fresh: bool,
    pub tradable: bool,
}

impl YfinanceMarketData {
    pub fn new(config: YfinanceDataConfig) -> Result<Self> {
        config.validate()?;
        let client = YfClient::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(config.timeout_seconds.min(10)))
            .retry_enabled(false)
            .build()
            .map_err(|_| Error("cannot initialize yfinance client".into()))?;
        Ok(Self { config, client })
    }

    pub fn config(&self) -> &YfinanceDataConfig {
        &self.config
    }

    pub fn supports(&self, instrument: &InstrumentId) -> bool {
        self.config.enabled && self.config.symbols.contains_key(instrument)
    }

    pub fn symbol_for(&self, instrument: &InstrumentId) -> Result<&str> {
        require(self.config.enabled, "yfinance market data is disabled")?;
        self.config
            .symbols
            .get(instrument)
            .map(String::as_str)
            .ok_or_else(|| Error("instrument is not mapped to yfinance".into()))
    }

    pub fn latest_quote(&self, instrument: &InstrumentId, now: DateTime<Utc>) -> Result<Quote> {
        let symbol = self.symbol_for(instrument)?.to_owned();
        let provider = self.fetch_quote(symbol)?;
        normalize_quote(
            instrument,
            &self.config.base_currency,
            provider,
            Utc::now().max(now),
            self.config.quote_max_age_seconds,
        )
    }

    pub fn latest_snapshot(
        &self,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<YfinanceQuoteSnapshot> {
        let symbol = self.symbol_for(instrument)?.to_owned();
        let provider = self.fetch_quote(symbol.clone())?;
        normalize_snapshot(
            instrument,
            &symbol,
            &self.config.base_currency,
            provider,
            Utc::now().max(now),
            self.config.quote_max_age_seconds,
        )
    }

    fn fetch_quote(&self, symbol: String) -> Result<ProviderQuote> {
        let client = self.client.clone();
        let thread = thread::Builder::new()
            .name("trady-yfinance-quote".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| Error("cannot start yfinance runtime".into()))?;
                let quote = runtime.block_on(async move {
                    Ticker::new(&client, symbol)
                        .quote()
                        .await
                        .map_err(|_| Error("yfinance quote request failed".into()))
                })?;
                Ok(ProviderQuote {
                    currency: quote.currency.code().to_owned(),
                    price: quote.price.as_ref().map(|price| *price.as_decimal()),
                    bid: quote.bid.as_ref().map(|level| *level.price.as_decimal()),
                    ask: quote.ask.as_ref().map(|level| *level.price.as_decimal()),
                    observed_at: quote.as_of,
                    market_state: quote
                        .market_state
                        .map(|state| market_state_label(state.as_ref())),
                })
            })
            .map_err(|_| Error("cannot start yfinance quote worker".into()))?;
        thread
            .join()
            .map_err(|_| Error("yfinance quote worker stopped unexpectedly".into()))?
    }
}

fn normalize_quote(
    instrument: &InstrumentId,
    expected_currency: &Currency,
    provider: ProviderQuote,
    received_at: DateTime<Utc>,
    max_age_seconds: u64,
) -> Result<Quote> {
    let snapshot = normalize_snapshot(
        instrument,
        "unmapped",
        expected_currency,
        provider,
        received_at,
        max_age_seconds,
    )?;
    let bid = snapshot
        .bid
        .ok_or_else(|| Error("yfinance quote bid unavailable".into()))?;
    let ask = snapshot
        .ask
        .ok_or_else(|| Error("yfinance quote ask unavailable".into()))?;
    require(
        snapshot.fresh && bid > rust_decimal::Decimal::ZERO && ask >= bid,
        "yfinance quote is stale, crossed or empty",
    )?;
    require(snapshot.tradable, "yfinance market is not tradable")?;
    Ok(Quote {
        instrument_id: snapshot.instrument_id,
        currency: snapshot.currency,
        bid,
        ask,
        observed_at: snapshot.observed_at,
        received_at: snapshot.received_at,
        tradable: true,
    })
}

fn normalize_snapshot(
    instrument: &InstrumentId,
    symbol: &str,
    expected_currency: &Currency,
    provider: ProviderQuote,
    received_at: DateTime<Utc>,
    max_age_seconds: u64,
) -> Result<YfinanceQuoteSnapshot> {
    let observed_at = provider
        .observed_at
        .ok_or_else(|| Error("yfinance quote timestamp unavailable".into()))?;
    require(
        provider.currency == expected_currency.as_str(),
        "yfinance quote currency differs from configured currency",
    )?;
    if let Some(price) = &provider.price {
        require(
            *price > rust_decimal::Decimal::ZERO,
            "yfinance quote last price is empty",
        )?;
    }
    let age = received_at.signed_duration_since(observed_at);
    require(
        age >= ChronoDuration::zero(),
        "yfinance quote is future-dated",
    )?;
    let fresh = age <= ChronoDuration::seconds(i64::try_from(max_age_seconds).unwrap_or(i64::MAX));
    let book_valid = provider
        .bid
        .as_ref()
        .zip(provider.ask.as_ref())
        .is_some_and(|(bid, ask)| *bid > rust_decimal::Decimal::ZERO && ask >= bid);
    let market_tradable = provider
        .market_state
        .as_deref()
        .is_none_or(market_state_is_tradable);
    Ok(YfinanceQuoteSnapshot {
        instrument_id: instrument.clone(),
        symbol: symbol.into(),
        currency: expected_currency.clone(),
        last: provider.price,
        bid: provider.bid,
        ask: provider.ask,
        observed_at,
        received_at,
        market_state: provider.market_state,
        fresh,
        tradable: fresh && book_valid && market_tradable,
    })
}

fn market_state_is_tradable(state: &str) -> bool {
    let state = market_state_label(state);
    state.contains("PRE")
        || state.contains("REGULAR")
        || state.contains("POST")
        || state.contains("AUCTION")
}

fn market_state_label(state: &str) -> String {
    let state = state.to_ascii_uppercase();
    for label in [
        "PRE",
        "PREMARKET",
        "REGULAR",
        "POST",
        "POSTMARKET",
        "CLOSED",
        "HALTED",
        "SUSPENDED",
        "AUCTION",
    ] {
        if state == format!("{label}{label}") {
            return label.into();
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::collections::BTreeMap;

    fn config(enabled: bool) -> YfinanceDataConfig {
        YfinanceDataConfig {
            config_version: 1,
            enabled,
            base_currency: Currency::new("EUR").expect("currency"),
            timeout_seconds: 10,
            quote_max_age_seconds: 10,
            symbols: BTreeMap::from([(
                InstrumentId::new("AIR_PA_EQ").expect("instrument"),
                "AIR.PA".into(),
            )]),
        }
    }

    #[test]
    fn validates_euronext_symbol_bindings() {
        assert!(config(true).validate().is_ok());
        assert!(is_euronext_symbol("AIR.PA"));
        assert!(is_euronext_symbol("ASML.AS"));
        assert!(!is_euronext_symbol("AAPL"));

        let mut invalid = config(true);
        invalid.symbols.insert(
            InstrumentId::new("AAPL_US_EQ").expect("instrument"),
            "AAPL".into(),
        );
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn normalizes_fresh_executable_quote_and_market_state() {
        let now = DateTime::parse_from_rfc3339("2026-11-03T10:00:10Z")
            .expect("time")
            .to_utc();
        let instrument = InstrumentId::new("AIR_PA_EQ").expect("instrument");
        let quote = normalize_quote(
            &instrument,
            &Currency::new("EUR").expect("currency"),
            ProviderQuote {
                currency: "EUR".into(),
                price: Some(Decimal::new(18423, 2)),
                bid: Some(Decimal::new(18420, 2)),
                ask: Some(Decimal::new(18425, 2)),
                observed_at: Some(now - ChronoDuration::seconds(2)),
                market_state: Some("REGULAR".into()),
            },
            now,
            10,
        )
        .expect("quote");
        assert_eq!(quote.instrument_id, instrument);
        assert_eq!(quote.bid, Decimal::new(18420, 2));
        assert!(quote.tradable);
    }

    #[test]
    fn rejects_stale_or_closed_quotes_without_losing_the_timestamp_guard() {
        let now = DateTime::parse_from_rfc3339("2026-11-03T10:00:10Z")
            .expect("time")
            .to_utc();
        let instrument = InstrumentId::new("AIR_PA_EQ").expect("instrument");
        let stale = normalize_quote(
            &instrument,
            &Currency::new("EUR").expect("currency"),
            ProviderQuote {
                currency: "EUR".into(),
                price: Some(Decimal::ONE),
                bid: Some(Decimal::ONE),
                ask: Some(Decimal::new(101, 2)),
                observed_at: Some(now - ChronoDuration::seconds(11)),
                market_state: Some("REGULAR".into()),
            },
            now,
            10,
        );
        assert!(stale.is_err());

        let closed = normalize_snapshot(
            &instrument,
            "AIR.PA",
            &Currency::new("EUR").expect("currency"),
            ProviderQuote {
                currency: "EUR".into(),
                price: Some(Decimal::ONE),
                bid: Some(Decimal::ONE),
                ask: Some(Decimal::new(101, 2)),
                observed_at: Some(now),
                market_state: Some("CLOSED".into()),
            },
            now,
            10,
        )
        .expect("closed quote remains an observation");
        assert!(!closed.tradable);
    }

    #[test]
    fn preserves_last_trade_context_when_yahoo_has_no_two_sided_book() {
        let now = DateTime::parse_from_rfc3339("2026-11-03T10:00:10Z")
            .expect("time")
            .to_utc();
        let instrument = InstrumentId::new("AIR_PA_EQ").expect("instrument");
        let snapshot = normalize_snapshot(
            &instrument,
            "AIR.PA",
            &Currency::new("EUR").expect("currency"),
            ProviderQuote {
                currency: "EUR".into(),
                price: Some(Decimal::new(19332, 2)),
                bid: None,
                ask: None,
                observed_at: Some(now - ChronoDuration::seconds(2)),
                market_state: Some("CLOSED".into()),
            },
            now,
            10,
        )
        .expect("snapshot");
        assert_eq!(snapshot.last, Some(Decimal::new(19332, 2)));
        assert!(snapshot.fresh);
        assert!(!snapshot.tradable);
    }
}

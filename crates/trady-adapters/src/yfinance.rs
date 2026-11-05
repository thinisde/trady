//! Read-only Yahoo Finance quotes through `yfinance-rs`.
//!
//! Yahoo symbols are deliberately supplied by an approved, bounded mapping.
//! The provider is useful for Euronext listings, but a ticker string alone is
//! not an instrument identity and is never inferred from arbitrary user input.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    thread,
    time::Duration,
};
use trady_application::market_tools::{DecimalFx, EurMarketReadPort};
use trady_domain::{
    BarSnapshot, CompletedBar, Currency, Error, EvidenceItem, EvidenceLineage, EvidenceSourceKind,
    InstrumentId, MarketState, Quote, QuoteQuality, Result, RightsStatus, VenueQuote, require,
};
use yfinance_rs::{Interval, Range, StreamBuilder, StreamMethod, Ticker, YfClient};

const MAX_SYMBOLS: usize = 512;
const MAX_YAHOO_SYMBOL_BYTES: usize = 64;
const EURONEXT_SUFFIXES: [&str; 8] = [".AS", ".BR", ".DE", ".IR", ".LS", ".MI", ".OL", ".PA"];

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
    /// Optional exact broker/venue crosswalk. A symbol without this record is
    /// useful for research only and cannot authorize a Trading 212 order.
    #[serde(default)]
    pub bindings: BTreeMap<InstrumentId, YfinanceInstrumentBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YfinanceInstrumentBinding {
    pub broker_instrument_id: String,
    pub isin: String,
    pub exchange_mic: String,
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
            (1..=60).contains(&self.quote_max_age_seconds),
            "invalid yfinance quote age",
        )?;
        require(
            self.symbols.len() <= MAX_SYMBOLS
                && (!self.enabled || !self.symbols.is_empty())
                && (!self.enabled || self.bindings.len() == self.symbols.len()),
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
        let mut broker_ids = BTreeSet::new();
        let mut isins = BTreeSet::new();
        for (instrument, binding) in &self.bindings {
            require(
                self.symbols.contains_key(instrument)
                    && !binding.broker_instrument_id.is_empty()
                    && binding.broker_instrument_id.len() <= 128
                    && binding
                        .broker_instrument_id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
                    && binding.isin.len() == 12
                    && binding.isin.bytes().all(|b| b.is_ascii_alphanumeric())
                    && binding.exchange_mic.len() == 4
                    && binding
                        .exchange_mic
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                    && broker_ids.insert(&binding.broker_instrument_id)
                    && isins.insert(&binding.isin),
                "invalid yfinance instrument crosswalk",
            )?;
            require(
                yahoo_symbol_matches_mic(
                    self.symbols
                        .get(instrument)
                        .ok_or_else(|| Error("yfinance symbol binding disappeared".into()))?,
                    &binding.exchange_mic,
                ),
                "yfinance symbol suffix does not match the reviewed exchange MIC",
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
    bid_size: Option<rust_decimal::Decimal>,
    ask_size: Option<rust_decimal::Decimal>,
    observed_at: Option<DateTime<Utc>>,
    market_state: Option<String>,
}

/// A read-only live quote client backed by Yahoo Finance.
pub struct YfinanceMarketData {
    config: YfinanceDataConfig,
    client: YfClient,
}

/// Application-owned read port for the target analyst. It keeps session and
/// FX observations explicit; Yahoo is only responsible for the exact EUR
/// quote and completed bars.
pub struct YfinanceEurMarketPort {
    pub data: YfinanceMarketData,
    pub sessions: BTreeMap<String, trady_domain::SessionWindow>,
    pub fx: BTreeMap<(String, String), DecimalFx>,
}

impl YfinanceEurMarketPort {
    pub fn new(
        data: YfinanceMarketData,
        sessions: BTreeMap<String, trady_domain::SessionWindow>,
        fx: BTreeMap<(String, String), DecimalFx>,
    ) -> Self {
        Self { data, sessions, fx }
    }
}

impl EurMarketReadPort for YfinanceEurMarketPort {
    fn quote(&mut self, instrument_id: &InstrumentId) -> Result<VenueQuote> {
        self.data.latest_venue_quote(instrument_id, Utc::now())
    }

    fn bars(&mut self, instrument_id: &InstrumentId, interval_seconds: u32) -> Result<BarSnapshot> {
        self.data
            .completed_bars(instrument_id, interval_seconds, Utc::now())
    }

    fn session(
        &mut self,
        exchange_mic: &str,
        now: DateTime<Utc>,
    ) -> Result<trady_domain::SessionWindow> {
        let session = self
            .sessions
            .get(exchange_mic)
            .ok_or_else(|| Error("reviewed EUR exchange session is unavailable".into()))?
            .clone();
        session.validate(now)?;
        Ok(session)
    }

    fn fx(
        &mut self,
        from: &Currency,
        to: &Currency,
        now: DateTime<Utc>,
    ) -> Result<Option<DecimalFx>> {
        let value = self
            .fx
            .get(&(from.as_str().to_owned(), to.as_str().to_owned()))
            .cloned();
        if let Some(value) = &value {
            value.validate(now)?;
        }
        Ok(value)
    }
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
    pub bid_size: Option<rust_decimal::Decimal>,
    pub ask_size: Option<rust_decimal::Decimal>,
    pub source: String,
    pub quote_quality: QuoteQuality,
    /// Hash of the exact instrument-to-broker crosswalk used for this quote.
    pub crosswalk_hash: Option<String>,
    pub fresh: bool,
    pub tradable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YfinancePollConfig {
    pub interval_seconds: u64,
    pub jitter_seconds: u64,
    pub max_concurrency: usize,
    pub max_attempts: u8,
    pub retry_backoff_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YfinancePollScheduler {
    config: YfinancePollConfig,
    due_at: BTreeMap<InstrumentId, DateTime<Utc>>,
    failures: BTreeMap<InstrumentId, u8>,
}

impl YfinancePollScheduler {
    pub fn new(config: YfinancePollConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            due_at: BTreeMap::new(),
            failures: BTreeMap::new(),
        })
    }

    /// Return at most the configured concurrency worth of due instruments. The
    /// stable map order makes a replay deterministic; callers add network I/O
    /// around this bounded batch.
    pub fn due(&mut self, instruments: &[InstrumentId], now: DateTime<Utc>) -> Vec<InstrumentId> {
        let mut due = Vec::new();
        for instrument in instruments {
            let entry = self.due_at.entry(instrument.clone()).or_insert(now);
            if *entry <= now && due.len() < self.config.max_concurrency {
                due.push(instrument.clone());
            }
        }
        due
    }

    pub fn record_success(
        &mut self,
        instrument: &InstrumentId,
        observed_at: DateTime<Utc>,
    ) -> Result<()> {
        require(
            observed_at <= Utc::now() + ChronoDuration::seconds(300),
            "poll observation is too far in the future",
        )?;
        self.failures.remove(instrument);
        let jitter = deterministic_jitter(instrument.as_str(), self.config.jitter_seconds);
        self.due_at.insert(
            instrument.clone(),
            observed_at
                + ChronoDuration::seconds(
                    i64::try_from(self.config.interval_seconds + jitter).unwrap_or(i64::MAX),
                ),
        );
        Ok(())
    }

    pub fn record_failure(&mut self, instrument: &InstrumentId, now: DateTime<Utc>) -> Result<()> {
        let failure = self.failures.entry(instrument.clone()).or_insert(0);
        *failure = failure.saturating_add(1).min(self.config.max_attempts);
        let exponent = u32::from((*failure).saturating_sub(1));
        let backoff = self
            .config
            .retry_backoff_seconds
            .saturating_mul(2_u64.saturating_pow(exponent));
        self.due_at.insert(
            instrument.clone(),
            now + ChronoDuration::seconds(i64::try_from(backoff).unwrap_or(i64::MAX)),
        );
        Ok(())
    }

    pub fn attempts(&self, instrument: &InstrumentId) -> u8 {
        self.failures.get(instrument).copied().unwrap_or(0)
    }

    pub fn config(&self) -> &YfinancePollConfig {
        &self.config
    }
}

fn deterministic_jitter(instrument: &str, maximum: u64) -> u64 {
    if maximum == 0 {
        return 0;
    }
    instrument.bytes().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(33).wrapping_add(u64::from(byte))
    }) % (maximum + 1)
}

impl Default for YfinancePollConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 5,
            jitter_seconds: 2,
            max_concurrency: 8,
            max_attempts: 3,
            retry_backoff_seconds: 2,
        }
    }
}

impl YfinancePollConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            (1..=300).contains(&self.interval_seconds)
                && self.jitter_seconds <= self.interval_seconds
                && (1..=64).contains(&self.max_concurrency)
                && (1..=3).contains(&self.max_attempts)
                && (1..=60).contains(&self.retry_backoff_seconds),
            "invalid yfinance polling configuration",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YfinanceNewsItem {
    pub id: String,
    pub instrument_id: InstrumentId,
    pub title: String,
    pub publisher: Option<String>,
    pub canonical_url: Option<String>,
    pub published_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
}

/// Normalize Yahoo article metadata into the shared evidence contract. A
/// missing provider link is counted by the caller rather than replaced with a
/// fabricated URL.
pub fn normalize_news_evidence(
    items: &[YfinanceNewsItem],
    now: DateTime<Utc>,
) -> Result<(Vec<EvidenceItem>, usize)> {
    let mut evidence = Vec::new();
    let mut without_link = 0usize;
    for item in items {
        let Some(url) = &item.canonical_url else {
            without_link += 1;
            continue;
        };
        if !url.starts_with("https://")
            || item.published_at > item.fetched_at
            || item.fetched_at > now
        {
            return Err(Error("invalid yfinance news provenance".into()));
        }
        let mut hash = Sha256::new();
        hash.update(item.id.as_bytes());
        hash.update(b"\n");
        hash.update(item.title.as_bytes());
        hash.update(b"\n");
        hash.update(url.as_bytes());
        let content_hash = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let normalized = EvidenceItem {
            schema_version: 1,
            id: item.id.clone(),
            canonical_url: url.clone(),
            source_kind: EvidenceSourceKind::YfinanceNews,
            publisher: item.publisher.clone(),
            author: None,
            title: Some(item.title.clone()),
            body_or_excerpt: None,
            symbol_mentions: vec![],
            entity_ids: vec![item.instrument_id.to_string()],
            published_at: Some(item.published_at),
            fetched_at: item.fetched_at,
            content_hash,
            rights_status: RightsStatus::MetadataOnly,
            primary_or_secondary: EvidenceLineage::Secondary,
            contradictions: vec![],
            raw_ref: None,
            untrusted_input: false,
        };
        normalized.validate(now)?;
        evidence.push(normalized);
    }
    Ok((evidence, without_link))
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

    pub fn crosswalk_hash(&self) -> Result<String> {
        let canonical = serde_json::to_string(&self.config.bindings)
            .map_err(|_| Error("yfinance crosswalk encoding failed".into()))?;
        Ok(Sha256::digest(canonical.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
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
        let mut snapshot = normalize_snapshot(
            instrument,
            &symbol,
            &self.config.base_currency,
            provider,
            Utc::now().max(now),
            self.config.quote_max_age_seconds,
        )?;
        snapshot.crosswalk_hash = Some(self.crosswalk_hash()?);
        Ok(snapshot)
    }

    /// Convert a provider observation into the target quote contract only
    /// when an exact broker/ISIN/venue crosswalk is configured.
    pub fn latest_venue_quote(
        &self,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<VenueQuote> {
        let binding = self
            .config
            .bindings
            .get(instrument)
            .ok_or_else(|| Error("yfinance quote lacks an exact broker crosswalk".into()))?;
        let symbol = self.symbol_for(instrument)?.to_owned();
        let provider = self.fetch_quote(symbol.clone())?;
        let received_at = Utc::now().max(now);
        let mut snapshot = normalize_snapshot(
            instrument,
            &symbol,
            &self.config.base_currency,
            provider,
            received_at,
            self.config.quote_max_age_seconds,
        )?;
        snapshot.crosswalk_hash = Some(self.crosswalk_hash()?);
        let market_state = snapshot
            .market_state
            .as_deref()
            .map(MarketState::parse)
            .unwrap_or(MarketState::Unknown);
        let quality = snapshot.quote_quality;
        let quote = VenueQuote {
            quote_id: format!(
                "yf_{}_{}",
                instrument,
                snapshot.observed_at.timestamp_millis()
            ),
            instrument_id: instrument.clone(),
            broker_instrument_id: binding.broker_instrument_id.clone(),
            isin: binding.isin.clone(),
            exchange_mic: binding.exchange_mic.clone(),
            currency: snapshot.currency,
            yahoo_symbol: symbol,
            bid: snapshot.bid,
            ask: snapshot.ask,
            last: snapshot.last,
            bid_size: snapshot.bid_size,
            ask_size: snapshot.ask_size,
            volume: None,
            observed_at: snapshot.observed_at,
            received_at: snapshot.received_at,
            market_state,
            source: "yfinance_rs".into(),
            quality,
        };
        quote.validate(
            received_at,
            i64::try_from(self.config.quote_max_age_seconds).unwrap_or(i64::MAX),
        )?;
        Ok(quote)
    }

    /// Retrieve completed daily or intraday OHLCV bars. The requested interval
    /// is deliberately bounded; incomplete/future bars are never returned.
    pub fn completed_bars(
        &self,
        instrument: &InstrumentId,
        interval_seconds: u32,
        now: DateTime<Utc>,
    ) -> Result<BarSnapshot> {
        require(
            matches!(interval_seconds, 60 | 300 | 900 | 1800 | 3600 | 86_400),
            "unsupported yfinance bar interval",
        )?;
        let symbol = self.symbol_for(instrument)?.to_owned();
        let client = self.client.clone();
        let thread = thread::Builder::new()
            .name("trady-yfinance-history".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| Error("cannot start yfinance history runtime".into()))?;
                runtime.block_on(async move {
                    let interval = match interval_seconds {
                        60 => Interval::I1m,
                        300 => Interval::I5m,
                        900 => Interval::I15m,
                        1800 => Interval::I30m,
                        3600 => Interval::I1h,
                        _ => Interval::D1,
                    };
                    Ticker::new(&client, symbol)
                        .history(
                            Some(if interval_seconds < 86_400 {
                                Range::D5
                            } else {
                                Range::M6
                            }),
                            Some(interval),
                            false,
                        )
                        .await
                        .map_err(|_| Error("yfinance history request failed".into()))
                })
            })
            .map_err(|_| Error("cannot start yfinance history worker".into()))?;
        let candles = thread
            .join()
            .map_err(|_| Error("yfinance history worker stopped unexpectedly".into()))??;
        // Yahoo can append an irregular latest-price point after its current
        // intraday bucket. Provider coverage, rather than our later wall clock,
        // determines whether that bucket has actually completed.
        let coverage_end = candles.iter().map(|c| c.ts).max().unwrap_or(now).min(now);
        let mut bars = Vec::new();
        for candle in candles {
            let closes_at = candle.ts + ChronoDuration::seconds(i64::from(interval_seconds));
            if !completed_history_window(candle.ts, closes_at, coverage_end, interval_seconds, now)
            {
                continue;
            }
            let currency = candle.currency.code().to_owned();
            require(
                currency == self.config.base_currency.as_str(),
                "yfinance bar currency differs from EUR",
            )?;
            bars.push(CompletedBar {
                bar_id: format!("{}_{}", instrument, candle.ts.timestamp_millis()),
                instrument_id: instrument.clone(),
                interval_seconds,
                opens_at: candle.ts,
                closes_at,
                open: *candle.ohlc.open.as_decimal(),
                high: *candle.ohlc.high.as_decimal(),
                low: *candle.ohlc.low.as_decimal(),
                close: *candle.ohlc.close.as_decimal(),
                volume: candle.volume.as_ref().map(|volume| *volume.as_decimal()),
                provider: "yfinance_rs".into(),
                received_at: now,
                completed: true,
            });
        }
        // Five trading days of minute bars can exceed the domain's bounded
        // snapshot. Keep the most recent completed window in chronological order.
        if bars.len() > 2048 {
            bars.drain(..bars.len() - 2048);
        }
        let observed_at = bars
            .last()
            .map(|bar| bar.closes_at)
            .ok_or_else(|| Error("yfinance returned no completed bars".into()))?;
        let snapshot = BarSnapshot {
            snapshot_id: format!("yf_bars_{}_{}", instrument, now.timestamp_millis()),
            instrument_id: instrument.clone(),
            provider: "yfinance_rs".into(),
            interval_seconds,
            observed_at,
            received_at: now,
            bars,
        };
        snapshot.validate(now)?;
        Ok(snapshot)
    }

    /// Normalize Yahoo's latest news into a bounded, source-linked research
    /// observation. Missing links remain metadata-only; no empty success is
    /// manufactured on provider failure.
    pub fn latest_news(
        &self,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<Vec<YfinanceNewsItem>> {
        let symbol = self.symbol_for(instrument)?.to_owned();
        let client = self.client.clone();
        let thread = thread::Builder::new()
            .name("trady-yfinance-news".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| Error("cannot start yfinance news runtime".into()))?;
                runtime.block_on(async move {
                    Ticker::new(&client, symbol)
                        .news()
                        .await
                        .map_err(|_| Error("yfinance news request failed".into()))
                })
            })
            .map_err(|_| Error("cannot start yfinance news worker".into()))?;
        let articles = thread
            .join()
            .map_err(|_| Error("yfinance news worker stopped unexpectedly".into()))??;
        let mut result = Vec::with_capacity(articles.len().min(64));
        for article in articles.into_iter().take(64) {
            require(article.published_at <= now, "yfinance news is future-dated")?;
            result.push(YfinanceNewsItem {
                id: format!("yf_news_{}", article.uuid),
                instrument_id: instrument.clone(),
                title: article.title,
                publisher: article.publisher,
                canonical_url: article.link,
                published_at: article.published_at,
                fetched_at: now,
            });
        }
        Ok(result)
    }

    /// Bounded SDK WebSocket capture for SHADOW research. QuoteUpdate contains
    /// last trades, without a two-sided book or a market-state assertion. This
    /// response is deliberately separate from latest_venue_quote.
    pub fn capture_stream(&self, seconds: u64) -> Result<serde_json::Value> {
        require(
            (1..=30).contains(&seconds),
            "Yahoo stream capture duration is unbounded",
        )?;
        require(
            self.config.enabled
                && !self.config.bindings.is_empty()
                && self.config.bindings.len() <= 8,
            "Yahoo stream requires a small reviewed crosswalk",
        )?;
        let client = self.client.clone();
        let config = self.config.clone();
        let crosswalk_hash = self.crosswalk_hash()?;
        let worker = thread::Builder::new().name("trady-yfinance-stream".into()).spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()
                .map_err(|_| Error("cannot start Yahoo stream runtime".into()))?;
            runtime.block_on(async move {
                let symbols: Vec<_> = config.bindings.keys().map(|id| config.symbols[id].clone()).collect();
                let (handle, mut receiver) = StreamBuilder::new(&client).symbols(symbols.clone())
                    .method(StreamMethod::Websocket)
                    .websocket_connect_timeout(Duration::from_secs(config.timeout_seconds.min(10)))
                    .start().await.map_err(|_| Error("Yahoo SDK WebSocket unavailable".into()))?;
                let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
                let mut samples = Vec::new();
                let mut latest = BTreeMap::new();
                let mut disconnected = false;
                let mut invalid = false;
                while samples.len() < 512 {
                    match tokio::time::timeout_at(deadline, receiver.recv()).await {
                        Ok(Some(update)) => {
                            let received_at = Utc::now();
                            let identity = config.symbols.iter().find(|(id, symbol)|
                                config.bindings.contains_key(*id) && symbol.as_str() == update.instrument.symbol.as_str());
                            let Some((id, symbol)) = identity else { invalid = true; break; };
                            if update.currency.code() != config.base_currency.as_str()
                                || update.price.as_ref().is_none_or(|price| *price.as_decimal() <= rust_decimal::Decimal::ZERO)
                                || update.ts > received_at + ChronoDuration::seconds(5) {
                                invalid = true; break;
                            }
                            latest.insert(symbol.clone(), update.ts);
                            samples.push(serde_json::json!({"instrument_id":id,"provider_symbol":symbol,
                                "broker_binding":config.bindings[id],"crosswalk_hash":crosswalk_hash,
                                "observed_at":update.ts,"received_at":received_at,
                                "age_seconds":(received_at-update.ts).num_seconds(),"sdk_projection":update}));
                        }
                        Ok(None) => { disconnected = true; break; }
                        Err(_) => break,
                    }
                }
                handle.abort();
                let now = Utc::now();
                let stale = latest.values().any(|at| (now-*at).num_seconds()
                    > i64::try_from(config.quote_max_age_seconds).unwrap_or(60));
                let missing: Vec<_> = symbols.iter().filter(|symbol| !latest.contains_key(*symbol)).collect();
                let (status, error) = if invalid { ("UNAVAILABLE", Some("INVALID_STREAM_IDENTITY_OR_TIME")) }
                    else if disconnected { ("UNAVAILABLE", Some("STREAM_DISCONNECTED")) }
                    else if !missing.is_empty() { ("UNAVAILABLE", Some("STREAM_SYMBOLS_UNAVAILABLE")) }
                    else if stale { ("STALE", Some("STREAM_TRADE_TIMESTAMP_STALE")) }
                    else { ("OK", None) };
                Ok(serde_json::json!({"provider_status":status,"error_code":error,
                    "role":"LAST_TRADE_RESEARCH_ONLY","transport":"WEBSOCKET_ONLY",
                    "capture_level":"YFINANCE_RS_QUOTE_UPDATE_PROJECTION",
                    "requested_duration_seconds":seconds,"truncated":samples.len() == 512,
                    "requested_symbols":symbols,"missing_symbols":missing,"samples":samples,
                    "received_at":now,"book_fields_exposed":false,"entry_authorized":false}))
            })
        }).map_err(|_| Error("cannot start Yahoo stream worker".into()))?;
        worker
            .join()
            .map_err(|_| Error("Yahoo stream worker stopped unexpectedly".into()))?
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
                    bid_size: quote
                        .bid
                        .as_ref()
                        .and_then(|level| level.size.as_ref())
                        .map(|size| *size.as_decimal()),
                    ask_size: quote
                        .ask
                        .as_ref()
                        .and_then(|level| level.size.as_ref())
                        .map(|size| *size.as_decimal()),
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

fn completed_history_window(
    opens_at: DateTime<Utc>,
    closes_at: DateTime<Utc>,
    provider_coverage: DateTime<Utc>,
    interval: u32,
    now: DateTime<Utc>,
) -> bool {
    closes_at <= now
        && (interval == 86_400
            || (opens_at.timestamp().rem_euclid(i64::from(interval)) == 0
                && opens_at.timestamp_subsec_nanos() == 0
                && closes_at <= provider_coverage))
}

#[cfg(test)]
mod completed_history_tests {
    use super::*;
    #[test]
    fn delayed_latest_price_point_and_forming_bucket_are_not_completed_bars() {
        let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        let now = at("2026-11-05T10:38:12Z");
        let coverage = at("2026-11-05T10:22:59Z");
        assert!(completed_history_window(
            at("2026-11-05T10:15:00Z"),
            at("2026-11-05T10:20:00Z"),
            coverage,
            300,
            now
        ));
        assert!(!completed_history_window(
            at("2026-11-05T10:20:00Z"),
            at("2026-11-05T10:25:00Z"),
            coverage,
            300,
            now
        ));
        assert!(!completed_history_window(
            at("2026-11-05T10:22:59Z"),
            at("2026-11-05T10:27:59Z"),
            coverage,
            300,
            now
        ));
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
        .is_some_and(market_state_is_tradable);
    let quote_quality = if !fresh {
        QuoteQuality::Stale
    } else if provider
        .bid
        .zip(provider.ask)
        .is_some_and(|(bid, ask)| ask < bid)
    {
        QuoteQuality::Crossed
    } else if fresh && book_valid && market_tradable {
        QuoteQuality::Executable
    } else if bid_ask_present(provider.bid, provider.ask) && fresh {
        QuoteQuality::Delayed
    } else if provider.price.is_some() {
        QuoteQuality::LastTradeOnly
    } else {
        QuoteQuality::Missing
    };
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
        bid_size: provider.bid_size,
        ask_size: provider.ask_size,
        source: "yfinance_rs".into(),
        quote_quality,
        crosswalk_hash: None,
        fresh,
        tradable: fresh && book_valid && market_tradable,
    })
}

fn bid_ask_present(bid: Option<rust_decimal::Decimal>, ask: Option<rust_decimal::Decimal>) -> bool {
    bid.is_some() && ask.is_some()
}

fn yahoo_symbol_matches_mic(symbol: &str, mic: &str) -> bool {
    let suffix = match mic {
        "XAMS" => ".AS",
        "XBRU" => ".BR",
        "XETR" => ".DE",
        "XLIS" => ".LS",
        "XMIL" => ".MI",
        "XOSL" => ".OL",
        "XPAR" => ".PA",
        _ => return false,
    };
    symbol.to_ascii_uppercase().ends_with(suffix)
}

fn market_state_is_tradable(state: &str) -> bool {
    MarketState::parse(state).entry_allowed()
}

fn market_state_label(state: &str) -> String {
    match MarketState::parse(state) {
        MarketState::Regular => "REGULAR".into(),
        MarketState::PreMarket => "PREMARKET".into(),
        MarketState::PostMarket => "POSTMARKET".into(),
        MarketState::Auction => "AUCTION".into(),
        MarketState::Closed => "CLOSED".into(),
        MarketState::Halted => "HALTED".into(),
        MarketState::Unknown => state.trim().to_ascii_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::collections::BTreeMap;

    fn config(enabled: bool) -> YfinanceDataConfig {
        let instrument = InstrumentId::new("AIR_PA_EQ").expect("instrument");
        YfinanceDataConfig {
            config_version: 1,
            enabled,
            base_currency: Currency::new("EUR").expect("currency"),
            timeout_seconds: 10,
            quote_max_age_seconds: 10,
            symbols: BTreeMap::from([(instrument.clone(), "AIR.PA".into())]),
            bindings: BTreeMap::from([(
                instrument,
                YfinanceInstrumentBinding {
                    broker_instrument_id: "demo_air_pa".into(),
                    isin: "FR0000120404".into(),
                    exchange_mic: "XPAR".into(),
                },
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
    fn unknown_missing_and_nonregular_market_states_cannot_authorize_a_quote() {
        let now = Utc::now();
        for state in [
            None,
            Some("UNKNOWN"),
            Some("HALTED"),
            Some("CLOSED"),
            Some("PREMARKET"),
            Some("POSTMARKET"),
        ] {
            let snapshot = normalize_snapshot(
                &InstrumentId::new("AIR_PA_EQ").unwrap(),
                "AIR.PA",
                &Currency::new("EUR").unwrap(),
                ProviderQuote {
                    currency: "EUR".into(),
                    price: Some(Decimal::TEN),
                    bid: Some(Decimal::TEN),
                    ask: Some(Decimal::new(1001, 2)),
                    bid_size: None,
                    ask_size: None,
                    observed_at: Some(now),
                    market_state: state.map(String::from),
                },
                now,
                10,
            )
            .unwrap();
            assert!(!snapshot.tradable, "{state:?}");
            assert_ne!(snapshot.quote_quality, QuoteQuality::Executable);
        }
        let mut mismatched = config(true);
        mismatched
            .bindings
            .values_mut()
            .next()
            .unwrap()
            .exchange_mic = "XAMS".into();
        assert!(mismatched.validate().is_err());
        let mut no_binding = config(true);
        no_binding.bindings.clear();
        assert!(no_binding.validate().is_err());
        let mut too_old = config(true);
        too_old.quote_max_age_seconds = 61;
        assert!(too_old.validate().is_err());
    }

    #[test]
    fn normalizes_fresh_executable_quote_and_market_state() {
        let now = DateTime::parse_from_rfc3339("2026-11-03T10:00:10Z")
            .expect("time")
            .to_utc();
        let instrument = InstrumentId::new("AIR_PA_EQ").expect("instrument");
        let provider = ProviderQuote {
            currency: "EUR".into(),
            price: Some(Decimal::new(18423, 2)),
            bid: Some(Decimal::new(18420, 2)),
            ask: Some(Decimal::new(18425, 2)),
            bid_size: Some(Decimal::new(500, 0)),
            ask_size: Some(Decimal::new(300, 0)),
            observed_at: Some(now - ChronoDuration::seconds(2)),
            market_state: Some("REGULAR".into()),
        };
        let snapshot = normalize_snapshot(
            &instrument,
            "AIR.PA",
            &Currency::new("EUR").expect("currency"),
            provider.clone(),
            now,
            10,
        )
        .expect("snapshot");
        assert_eq!(snapshot.bid_size, Some(Decimal::new(500, 0)));
        assert_eq!(snapshot.ask_size, Some(Decimal::new(300, 0)));
        let quote = normalize_quote(
            &instrument,
            &Currency::new("EUR").expect("currency"),
            provider,
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
                bid_size: None,
                ask_size: None,
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
                bid_size: None,
                ask_size: None,
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
                bid_size: None,
                ask_size: None,
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

    #[test]
    fn news_normalization_never_invents_missing_provider_links() {
        let now = DateTime::parse_from_rfc3339("2026-11-03T10:00:10Z")
            .expect("time")
            .to_utc();
        let instrument = InstrumentId::new("AIR_PA_EQ").expect("instrument");
        let (rows, skipped) = normalize_news_evidence(
            &[
                YfinanceNewsItem {
                    id: "yf_news_1".into(),
                    instrument_id: instrument.clone(),
                    title: "Headline".into(),
                    publisher: Some("Publisher".into()),
                    canonical_url: Some("https://example.com/article".into()),
                    published_at: now,
                    fetched_at: now,
                },
                YfinanceNewsItem {
                    id: "yf_news_2".into(),
                    instrument_id: instrument,
                    title: "No link".into(),
                    publisher: None,
                    canonical_url: None,
                    published_at: now,
                    fetched_at: now,
                },
            ],
            now,
        )
        .expect("evidence");
        assert_eq!(rows.len(), 1);
        assert_eq!(skipped, 1);
        assert_eq!(rows[0].source_kind, EvidenceSourceKind::YfinanceNews);
    }

    #[test]
    fn polling_scheduler_bounds_batches_and_backs_off_failures() {
        let mut scheduler = YfinancePollScheduler::new(YfinancePollConfig {
            interval_seconds: 5,
            jitter_seconds: 0,
            max_concurrency: 1,
            max_attempts: 3,
            retry_backoff_seconds: 2,
        })
        .expect("scheduler");
        let first = InstrumentId::new("AIR_PA_EQ").expect("id");
        let second = InstrumentId::new("ASML_AS_EQ").expect("id");
        let now = DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc();
        assert_eq!(
            scheduler.due(&[first.clone(), second.clone()], now).len(),
            1
        );
        scheduler.record_failure(&first, now).expect("failure");
        assert!(scheduler.due(std::slice::from_ref(&first), now).is_empty());
        assert_eq!(scheduler.attempts(&first), 1);
        scheduler.record_success(&first, now).expect("success");
        assert!(scheduler.due(&[first], now).is_empty());
    }
}

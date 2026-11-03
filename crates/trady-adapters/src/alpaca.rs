//! Alpaca market-data adapter.
//!
//! This module is read-only. It reads IEX stock quotes/bars and owns the
//! authenticated quote stream; it deliberately exposes no order or account
//! mutation operation. Trading 212 remains Trady's execution broker.

use chrono::{DateTime, Utc};
use reqwest::{Url, blocking::Client, redirect::Policy};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    net::{TcpStream, ToSocketAddrs},
    str::FromStr,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use trady_domain::{Currency, Error, InstrumentId, Quote, Result, require};
use tungstenite::{Error as WebSocketError, Message, client_tls, stream::MaybeTlsStream};

const PAPER_DATA_URL: &str = "https://data.alpaca.markets";
const LIVE_DATA_URL: &str = "https://data.alpaca.markets";
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
pub const IEX_STREAM_URL: &str = "wss://stream.data.alpaca.markets/v2/iex";
pub const MAX_IEX_STREAM_SYMBOLS: usize = 30;
const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(1);
const STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const STREAM_CONNECT_BACKOFF_MAX: Duration = Duration::from_secs(30);
const STREAM_MAX_MESSAGE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlpacaEnvironment {
    Paper,
    Live,
}

impl AlpacaEnvironment {
    fn data_base_url(self) -> &'static str {
        if matches!(self, Self::Live) {
            LIVE_DATA_URL
        } else {
            PAPER_DATA_URL
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlpacaFeed {
    #[default]
    Iex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlpacaDataConfig {
    pub config_version: u32,
    pub enabled: bool,
    pub environment: AlpacaEnvironment,
    #[serde(default)]
    pub feed: AlpacaFeed,
    pub base_currency: Currency,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_quote_max_age_seconds")]
    pub quote_max_age_seconds: u64,
    #[serde(default = "default_history_limit")]
    pub history_limit: u32,
    #[serde(default = "default_stream_start_lead_seconds")]
    pub stream_start_lead_seconds: u64,
}

fn default_timeout_seconds() -> u64 {
    15
}
fn default_quote_max_age_seconds() -> u64 {
    10
}
fn default_history_limit() -> u32 {
    500
}
fn default_stream_start_lead_seconds() -> u64 {
    1_800
}

impl AlpacaDataConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1,
            "unsupported Alpaca config version",
        )?;
        require(
            self.base_currency.as_str() == "USD",
            "Alpaca equity data is USD only",
        )?;
        require(
            matches!(self.feed, AlpacaFeed::Iex),
            "only the Alpaca IEX feed is supported",
        )?;
        require(
            (1..=60).contains(&self.timeout_seconds),
            "invalid Alpaca timeout",
        )?;
        require(
            (1..=300).contains(&self.quote_max_age_seconds),
            "invalid Alpaca quote age",
        )?;
        require(
            (1..=10_000).contains(&self.history_limit),
            "invalid Alpaca history limit",
        )?;
        require(
            (0..=7_200).contains(&self.stream_start_lead_seconds),
            "invalid Alpaca stream start lead",
        )?;
        Ok(())
    }
}

/// Fixed subscription plan for Alpaca Basic. The symbol set is deliberately
/// derived from the owner-approved universe and cannot be widened by config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IexStreamSubscription {
    pub symbols: Vec<String>,
    pub trades: bool,
    pub quotes: bool,
}

impl Default for IexStreamSubscription {
    fn default() -> Self {
        Self {
            symbols: trady_domain::iex::SYMBOLS
                .iter()
                .map(ToString::to_string)
                .collect(),
            trades: true,
            quotes: true,
        }
    }
}

impl IexStreamSubscription {
    pub fn validate(&self) -> Result<()> {
        let unique: BTreeSet<_> = self.symbols.iter().map(String::as_str).collect();
        require(
            self.symbols.len() == MAX_IEX_STREAM_SYMBOLS
                && unique.len() == MAX_IEX_STREAM_SYMBOLS
                && trady_domain::iex::SYMBOLS
                    .iter()
                    .all(|symbol| unique.contains(symbol))
                && (self.trades || self.quotes),
            "IEX stream must subscribe to exactly the approved 30 symbols",
        )
    }

    pub fn auth_message(&self, key: &str, secret: &str) -> Result<Value> {
        require(
            !key.trim().is_empty()
                && !secret.trim().is_empty()
                && !key.bytes().any(|b| b.is_ascii_control())
                && !secret.bytes().any(|b| b.is_ascii_control()),
            "invalid Alpaca stream credentials",
        )?;
        Ok(serde_json::json!({"action":"auth","key":key,"secret":secret}))
    }

    pub fn subscribe_message(&self) -> Result<Value> {
        self.validate()?;
        let mut message = serde_json::Map::new();
        message.insert("action".into(), Value::String("subscribe".into()));
        let symbols: Vec<Value> = self.symbols.iter().cloned().map(Value::String).collect();
        if self.trades {
            message.insert("trades".into(), Value::Array(symbols.clone()));
        }
        if self.quotes {
            message.insert("quotes".into(), Value::Array(symbols));
        }
        Ok(Value::Object(message))
    }
}

/// Quotes received from the authenticated IEX stream. The cache is shared by
/// the stream thread and the synchronous executor quote path.
#[derive(Clone, Default)]
pub struct IexQuoteCache {
    quotes: Arc<RwLock<BTreeMap<String, Quote>>>,
}

impl IexQuoteCache {
    pub fn latest(
        &self,
        instrument_id: &InstrumentId,
        now: DateTime<Utc>,
        max_age_seconds: u64,
    ) -> Result<Quote> {
        let symbol = symbol_for(instrument_id)?;
        let quote = self
            .quotes
            .read()
            .map_err(|_| Error("IEX quote cache lock failed".into()))?
            .get(&symbol)
            .cloned()
            .ok_or_else(|| Error("IEX stream quote unavailable".into()))?;
        let age = now.signed_duration_since(quote.observed_at).num_seconds();
        require(
            age >= 0
                && age <= i64::try_from(max_age_seconds).unwrap_or(i64::MAX)
                && quote.received_at >= quote.observed_at
                && quote.received_at <= now
                && quote.bid > Decimal::ZERO
                && quote.ask >= quote.bid
                && quote.tradable,
            "IEX stream quote is stale or invalid",
        )?;
        Ok(quote)
    }

    fn insert(&self, quote: Quote) -> Result<()> {
        let symbol = symbol_for(&quote.instrument_id)?;
        self.quotes
            .write()
            .map_err(|_| Error("IEX quote cache lock failed".into()))?
            .insert(symbol, quote);
        Ok(())
    }

    pub fn symbols(&self) -> Result<Vec<String>> {
        Ok(self
            .quotes
            .read()
            .map_err(|_| Error("IEX quote cache lock failed".into()))?
            .keys()
            .cloned()
            .collect())
    }

    pub fn quote_age_seconds(
        &self,
        instrument_id: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<Option<u64>> {
        let symbol = symbol_for(instrument_id)?;
        let age = self
            .quotes
            .read()
            .map_err(|_| Error("IEX quote cache lock failed".into()))?
            .get(&symbol)
            .map(|quote| now.signed_duration_since(quote.observed_at).num_seconds());
        Ok(age.and_then(|seconds| u64::try_from(seconds).ok()))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IexStreamHealth {
    pub connected: bool,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_quote_at: Option<DateTime<Utc>>,
    pub reconnects: u64,
    pub last_error: Option<String>,
}

/// A blocking, reconnecting IEX stream supervisor. It owns no broker client
/// and stops promptly when the holding lifecycle drops its handle.
pub struct IexStreamHandle {
    config: AlpacaDataConfig,
    api_key: String,
    secret_key: String,
    subscription: IexStreamSubscription,
    cache: IexQuoteCache,
    stop: Arc<AtomicBool>,
    health: Arc<Mutex<IexStreamHealth>>,
    thread: Option<JoinHandle<()>>,
}

impl IexStreamHandle {
    pub fn new(config: AlpacaDataConfig, api_key: String, secret_key: String) -> Result<Self> {
        config.validate()?;
        require(config.enabled, "Alpaca IEX stream is disabled")?;
        require(
            !api_key.trim().is_empty()
                && !secret_key.trim().is_empty()
                && !api_key.bytes().any(|b| b.is_ascii_control())
                && !secret_key.bytes().any(|b| b.is_ascii_control()),
            "invalid Alpaca stream credentials",
        )?;
        let subscription = IexStreamSubscription::default();
        subscription.validate()?;
        Ok(Self {
            config,
            api_key,
            secret_key,
            subscription,
            cache: IexQuoteCache::default(),
            stop: Arc::new(AtomicBool::new(false)),
            health: Arc::new(Mutex::new(IexStreamHealth::default())),
            thread: None,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        require(
            !self.stop.load(Ordering::Acquire),
            "Alpaca IEX stream has already been stopped",
        )?;
        if self.thread.is_some() {
            return Ok(());
        }
        let config = self.config.clone();
        let api_key = self.api_key.clone();
        let secret_key = self.secret_key.clone();
        let subscription = self.subscription.clone();
        let cache = self.cache.clone();
        let stop = self.stop.clone();
        let health = self.health.clone();
        let thread_stop = stop.clone();
        let thread_cache = cache.clone();
        let thread_health = health.clone();
        let thread = thread::Builder::new()
            .name("alpaca-iex-stream".into())
            .spawn(move || {
                stream_loop(
                    config,
                    api_key,
                    secret_key,
                    subscription,
                    thread_cache,
                    thread_stop,
                    thread_health,
                );
            })
            .map_err(|_| Error("cannot start Alpaca IEX stream thread".into()))?;
        self.thread = Some(thread);
        Ok(())
    }

    pub fn started(&self) -> bool {
        self.thread.is_some()
    }

    pub fn cache(&self) -> IexQuoteCache {
        self.cache.clone()
    }

    pub fn health(&self) -> Result<IexStreamHealth> {
        self.health
            .lock()
            .map_err(|_| Error("IEX stream health lock failed".into()))
            .map(|health| health.clone())
    }

    pub fn stop(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| Error("Alpaca IEX stream thread panicked".into()))?;
        }
        Ok(())
    }
}

impl Drop for IexStreamHandle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn stream_loop(
    config: AlpacaDataConfig,
    api_key: String,
    secret_key: String,
    subscription: IexStreamSubscription,
    cache: IexQuoteCache,
    stop: Arc<AtomicBool>,
    health: Arc<Mutex<IexStreamHealth>>,
) {
    let mut backoff = Duration::from_secs(1);
    while !stop.load(Ordering::Acquire) {
        match connect_and_read(
            &config,
            &api_key,
            &secret_key,
            &subscription,
            &cache,
            &stop,
            &health,
        ) {
            Ok(()) if stop.load(Ordering::Acquire) => break,
            Ok(()) => set_stream_error(&health, "IEX stream closed by provider"),
            Err(error) if !stop.load(Ordering::Acquire) => {
                set_stream_error(&health, &error.to_string())
            }
            Err(_) => break,
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        if let Ok(mut current) = health.lock() {
            current.connected = false;
            current.reconnects = current.reconnects.saturating_add(1);
        }
        let started = std::time::Instant::now();
        while !stop.load(Ordering::Acquire) && started.elapsed() < backoff {
            thread::sleep(Duration::from_millis(100));
        }
        backoff = (backoff * 2).min(STREAM_CONNECT_BACKOFF_MAX);
    }
    if let Ok(mut current) = health.lock() {
        current.connected = false;
    }
}

fn connect_and_read(
    config: &AlpacaDataConfig,
    api_key: &str,
    secret_key: &str,
    subscription: &IexStreamSubscription,
    cache: &IexQuoteCache,
    stop: &AtomicBool,
    health: &Mutex<IexStreamHealth>,
) -> Result<()> {
    let (mut socket, _) = connect_iex()?;
    set_stream_timeout(socket.get_mut(), STREAM_READ_TIMEOUT)
        .map_err(|_| Error("cannot set Alpaca IEX WebSocket timeout".into()))?;
    socket
        .send(Message::text(
            subscription.auth_message(api_key, secret_key)?.to_string(),
        ))
        .map_err(|_| Error("Alpaca IEX WebSocket authentication send failed".into()))?;
    wait_for_authentication(&mut socket, stop)?;
    socket
        .send(Message::text(subscription.subscribe_message()?.to_string()))
        .map_err(|_| Error("Alpaca IEX subscription send failed".into()))?;
    if let Ok(mut current) = health.lock() {
        current.connected = true;
        current.last_error = None;
    }
    let mut saw_message = false;
    while !stop.load(Ordering::Acquire) {
        match socket.read() {
            Ok(message) => {
                saw_message = true;
                let now = Utc::now();
                if let Ok(mut current) = health.lock() {
                    current.last_message_at = Some(now);
                }
                if let Some(last_quote) = parse_stream_message(message, cache, config)?
                    && let Ok(mut current) = health.lock()
                {
                    current.last_quote_at = Some(last_quote);
                }
            }
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(WebSocketError::ConnectionClosed) => break,
            Err(error) => {
                return Err(Error(format!("Alpaca IEX WebSocket read failed: {error}")));
            }
        }
    }
    if stop.load(Ordering::Acquire) {
        let _ = socket.close(None);
    }
    require(
        saw_message || stop.load(Ordering::Acquire),
        "Alpaca IEX stream produced no messages",
    )
}

fn connect_iex() -> Result<(
    tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    tungstenite::handshake::client::Response,
)> {
    let url =
        Url::parse(IEX_STREAM_URL).map_err(|_| Error("invalid Alpaca IEX WebSocket URL".into()))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error("Alpaca IEX WebSocket host missing".into()))?;
    let port = url.port_or_known_default().unwrap_or(443);
    let mut stream = None;
    for address in (host, port)
        .to_socket_addrs()
        .map_err(|_| Error("Alpaca IEX WebSocket DNS failed".into()))?
    {
        if let Ok(candidate) = TcpStream::connect_timeout(&address, STREAM_CONNECT_TIMEOUT) {
            stream = Some(candidate);
            break;
        }
    }
    let stream = stream.ok_or_else(|| Error("Alpaca IEX WebSocket connection failed".into()))?;
    stream
        .set_read_timeout(Some(STREAM_CONNECT_TIMEOUT))
        .map_err(|_| Error("cannot set Alpaca IEX connect timeout".into()))?;
    stream
        .set_write_timeout(Some(STREAM_CONNECT_TIMEOUT))
        .map_err(|_| Error("cannot set Alpaca IEX connect timeout".into()))?;
    stream
        .set_nodelay(true)
        .map_err(|_| Error("cannot set Alpaca IEX WebSocket nodelay".into()))?;
    client_tls(IEX_STREAM_URL, stream)
        .map_err(|_| Error("Alpaca IEX WebSocket handshake failed".into()))
}

fn wait_for_authentication(
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    stop: &AtomicBool,
) -> Result<()> {
    while !stop.load(Ordering::Acquire) {
        match socket.read() {
            Ok(Message::Text(text)) => {
                let value: Value = serde_json::from_str(text.as_ref())
                    .map_err(|_| Error("invalid Alpaca IEX authentication response".into()))?;
                if authentication_succeeded(&value) {
                    return Ok(());
                }
                if authentication_failed(&value) {
                    return Err(Error("Alpaca IEX authentication rejected".into()));
                }
            }
            Ok(Message::Binary(bytes)) => {
                let text = std::str::from_utf8(bytes.as_ref())
                    .map_err(|_| Error("invalid Alpaca IEX authentication bytes".into()))?;
                let value: Value = serde_json::from_str(text)
                    .map_err(|_| Error("invalid Alpaca IEX authentication response".into()))?;
                if authentication_succeeded(&value) {
                    return Ok(());
                }
                if authentication_failed(&value) {
                    return Err(Error("Alpaca IEX authentication rejected".into()));
                }
            }
            Err(WebSocketError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(WebSocketError::ConnectionClosed) => {
                return Err(Error(
                    "Alpaca IEX WebSocket closed during authentication".into(),
                ));
            }
            Err(error) => {
                return Err(Error(format!(
                    "Alpaca IEX authentication read failed: {error}"
                )));
            }
            Ok(_) => {}
        }
    }
    Err(Error(
        "Alpaca IEX stream stopped during authentication".into(),
    ))
}

fn authentication_succeeded(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item.get("T").and_then(Value::as_str) == Some("success")
                && item.get("msg").and_then(Value::as_str) == Some("authenticated")
        })
    })
}

fn authentication_failed(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item.get("T").and_then(Value::as_str) == Some("error"))
    })
}

fn parse_stream_message(
    message: Message,
    cache: &IexQuoteCache,
    config: &AlpacaDataConfig,
) -> Result<Option<DateTime<Utc>>> {
    let text = match message {
        Message::Text(text) => text.to_string(),
        Message::Binary(bytes) => std::str::from_utf8(bytes.as_ref())
            .map_err(|_| Error("invalid Alpaca IEX binary message".into()))?
            .to_owned(),
        Message::Close(_) => return Ok(None),
        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => return Ok(None),
    };
    require(
        text.len() <= STREAM_MAX_MESSAGE_BYTES,
        "Alpaca IEX message exceeds configured bound",
    )?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| Error("invalid Alpaca IEX stream message".into()))?;
    let mut latest_quote = None;
    for item in value
        .as_array()
        .ok_or_else(|| Error("Alpaca IEX stream message must be an array".into()))?
    {
        if item.get("T").and_then(Value::as_str) != Some("q") {
            continue;
        }
        let symbol = item
            .get("S")
            .and_then(Value::as_str)
            .ok_or_else(|| Error("Alpaca IEX quote symbol missing".into()))?;
        require(
            trady_domain::iex::SYMBOLS.contains(&symbol),
            "Alpaca IEX stream returned a symbol outside IEX30",
        )?;
        let observed_at = timestamp(item.get("t"))?;
        let bid = decimal_value(item.get("bp"))?;
        let ask = decimal_value(item.get("ap"))?;
        require(
            bid > Decimal::ZERO && ask >= bid,
            "Alpaca IEX stream quote is crossed or empty",
        )?;
        let received_at = Utc::now();
        let quote = Quote {
            instrument_id: InstrumentId::new(format!("{symbol}_US_EQ"))?,
            currency: config.base_currency.clone(),
            bid,
            ask,
            observed_at,
            received_at,
            tradable: received_at.signed_duration_since(observed_at).num_seconds()
                <= config.quote_max_age_seconds as i64,
        };
        cache.insert(quote)?;
        latest_quote = Some(received_at);
    }
    Ok(latest_quote)
}

fn set_stream_timeout(stream: &mut MaybeTlsStream<TcpStream>, timeout: Duration) -> io::Result<()> {
    match stream {
        MaybeTlsStream::Plain(stream) => {
            stream.set_read_timeout(Some(timeout))?;
            stream.set_write_timeout(Some(timeout))
        }
        MaybeTlsStream::NativeTls(stream) => {
            stream.get_mut().set_read_timeout(Some(timeout))?;
            stream.get_mut().set_write_timeout(Some(timeout))
        }
        #[allow(unreachable_patterns)]
        _ => Ok(()),
    }
}

fn set_stream_error(health: &Mutex<IexStreamHealth>, error: &str) {
    if let Ok(mut current) = health.lock() {
        current.connected = false;
        current.last_error = Some(error.chars().take(256).collect());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AlpacaBar {
    pub instrument_id: InstrumentId,
    pub observed_at: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AlpacaLatestQuote {
    pub quote: Quote,
    pub bid_size: Decimal,
    pub ask_size: Decimal,
    pub feed: AlpacaFeed,
}

pub struct AlpacaMarketData {
    config: AlpacaDataConfig,
    api_key: String,
    secret_key: String,
    client: Client,
}

impl AlpacaMarketData {
    pub fn new(config: AlpacaDataConfig, api_key: String, secret_key: String) -> Result<Self> {
        config.validate()?;
        require(
            !api_key.trim().is_empty()
                && !secret_key.trim().is_empty()
                && !api_key.bytes().any(|b| b.is_ascii_control())
                && !secret_key.bytes().any(|b| b.is_ascii_control()),
            "invalid Alpaca credentials",
        )?;
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(config.timeout_seconds.min(10)))
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|_| Error("cannot initialize Alpaca HTTPS client".into()))?;
        Ok(Self {
            config,
            api_key,
            secret_key,
            client,
        })
    }

    pub fn config(&self) -> &AlpacaDataConfig {
        &self.config
    }

    pub fn iex_stream_subscription(&self) -> Result<IexStreamSubscription> {
        require(
            matches!(self.config.feed, AlpacaFeed::Iex),
            "IEX feed required",
        )?;
        let subscription = IexStreamSubscription::default();
        subscription.validate()?;
        Ok(subscription)
    }

    pub fn latest_quote(
        &self,
        instrument_id: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<AlpacaLatestQuote> {
        let symbol = symbol_for(instrument_id)?;
        let value = self.get_json(&format!(
            "{}/v2/stocks/{}/quotes/latest?feed={}",
            self.config.environment.data_base_url(),
            symbol,
            feed_name(self.config.feed)
        ))?;
        let received_at = Utc::now().max(now);
        let quote = value
            .get("quote")
            .ok_or_else(|| Error("Alpaca quote missing".into()))?;
        let observed_at = timestamp(quote.get("t"))?;
        let bid = decimal_value(quote.get("bp"))?;
        let ask = decimal_value(quote.get("ap"))?;
        require(
            bid > Decimal::ZERO && ask >= bid,
            "Alpaca quote is crossed or empty",
        )?;
        let age = received_at.signed_duration_since(observed_at).num_seconds();
        require(
            age >= 0 && age <= self.config.quote_max_age_seconds as i64,
            "Alpaca quote is stale or future-dated",
        )?;
        Ok(AlpacaLatestQuote {
            quote: Quote {
                instrument_id: instrument_id.clone(),
                currency: self.config.base_currency.clone(),
                bid,
                ask,
                observed_at,
                received_at,
                tradable: true,
            },
            bid_size: decimal_value(quote.get("bs"))?,
            ask_size: decimal_value(quote.get("as"))?,
            feed: self.config.feed,
        })
    }

    pub fn daily_bars(
        &self,
        instrument_id: &InstrumentId,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
    ) -> Result<Vec<AlpacaBar>> {
        require(
            start < end.unwrap_or_else(Utc::now),
            "invalid Alpaca history window",
        )?;
        let symbol = symbol_for(instrument_id)?;
        let mut url = Url::parse(&format!(
            "{}/v2/stocks/{}/bars",
            self.config.environment.data_base_url(),
            symbol
        ))
        .map_err(|_| Error("invalid Alpaca data URL".into()))?;
        url.query_pairs_mut()
            .append_pair("timeframe", "1Day")
            .append_pair("start", &start.to_rfc3339())
            .append_pair("limit", &self.config.history_limit.to_string())
            .append_pair("feed", feed_name(self.config.feed));
        if let Some(end) = end {
            url.query_pairs_mut().append_pair("end", &end.to_rfc3339());
        }
        let value = self.get_json(url.as_str())?;
        let rows = value["bars"]
            .as_array()
            .ok_or_else(|| Error("Alpaca bars missing".into()))?;
        require(
            rows.len() <= self.config.history_limit as usize,
            "Alpaca history exceeds configured limit",
        )?;
        rows.iter()
            .map(|row| {
                Ok(AlpacaBar {
                    instrument_id: instrument_id.clone(),
                    observed_at: timestamp(row.get("t"))?,
                    open: decimal_value(row.get("o"))?,
                    high: decimal_value(row.get("h"))?,
                    low: decimal_value(row.get("l"))?,
                    close: decimal_value(row.get("c"))?,
                    volume: decimal_value(row.get("v"))?,
                })
            })
            .collect()
    }

    fn get_json(&self, raw_url: &str) -> Result<Value> {
        let url = Url::parse(raw_url).map_err(|_| Error("invalid Alpaca request URL".into()))?;
        require(
            url.scheme() == "https" && url.host_str().is_some(),
            "Alpaca requests require HTTPS",
        )?;
        let response = self
            .client
            .get(url)
            .header("APCA-API-KEY-ID", &self.api_key)
            .header("APCA-API-SECRET-KEY", &self.secret_key)
            .send()
            .map_err(|_| Error("Alpaca request failed".into()))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .map_err(|_| Error("Alpaca response read failed".into()))?;
        require(
            bytes.len() <= MAX_RESPONSE_BYTES,
            "Alpaca response exceeds size limit",
        )?;
        require(status.is_success(), "Alpaca request returned an error")?;
        serde_json::from_slice(&bytes).map_err(|_| Error("invalid Alpaca response JSON".into()))
    }
}

fn feed_name(feed: AlpacaFeed) -> &'static str {
    match feed {
        AlpacaFeed::Iex => "iex",
    }
}

fn symbol_for(instrument: &InstrumentId) -> Result<String> {
    trady_domain::iex::symbol(instrument).map(str::to_owned)
}

fn timestamp(value: Option<&Value>) -> Result<DateTime<Utc>> {
    let text = value
        .and_then(Value::as_str)
        .ok_or_else(|| Error("Alpaca timestamp missing".into()))?;
    DateTime::parse_from_rfc3339(text)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| Error("invalid Alpaca timestamp".into()))
}

fn decimal_value(value: Option<&Value>) -> Result<Decimal> {
    let value = value.ok_or_else(|| Error("Alpaca numeric field missing".into()))?;
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return Err(Error("invalid Alpaca numeric field".into())),
    };
    Decimal::from_str(&text).map_err(|_| Error("invalid Alpaca decimal".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn config_rejects_non_us_currency_and_bad_limits() {
        let mut config = AlpacaDataConfig {
            config_version: 1,
            enabled: true,
            environment: AlpacaEnvironment::Paper,
            feed: AlpacaFeed::Iex,
            base_currency: Currency::new("EUR").unwrap(),
            timeout_seconds: 15,
            quote_max_age_seconds: 10,
            history_limit: 500,
            stream_start_lead_seconds: 1_800,
        };
        assert!(config.validate().is_err());
        config.base_currency = Currency::new("USD").unwrap();
        config.history_limit = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn decimal_and_symbol_parsing_are_bounded() {
        assert_eq!(
            symbol_for(&InstrumentId::new("META_US_EQ").unwrap()).unwrap(),
            "META"
        );
        assert!(symbol_for(&InstrumentId::new("xom_US_EQ").unwrap()).is_err());
        assert_eq!(
            decimal_value(Some(&json!("1.2300"))).unwrap(),
            Decimal::new(123, 2)
        );
        assert!(decimal_value(Some(&json!([1]))).is_err());
    }

    #[test]
    fn iex_stream_plan_is_exact_and_cannot_be_widened() {
        let plan = IexStreamSubscription::default();
        plan.validate().unwrap();
        let message = plan.subscribe_message().unwrap();
        assert_eq!(message["action"], "subscribe");
        assert_eq!(message["quotes"].as_array().unwrap().len(), 30);
        assert_eq!(message["trades"].as_array().unwrap().len(), 30);
        let mut altered = plan;
        altered.symbols.push("SPY".into());
        assert!(altered.validate().is_err());
    }

    #[test]
    fn stream_quote_messages_update_the_shared_cache() {
        let config = AlpacaDataConfig {
            config_version: 1,
            enabled: true,
            environment: AlpacaEnvironment::Paper,
            feed: AlpacaFeed::Iex,
            base_currency: Currency::new("USD").unwrap(),
            timeout_seconds: 15,
            quote_max_age_seconds: 10,
            history_limit: 500,
            stream_start_lead_seconds: 1_800,
        };
        let cache = IexQuoteCache::default();
        let observed = Utc::now();
        let message = json!([{
            "T":"q", "S":"NVDA", "bp":"100.10", "ap":"100.20",
            "bs":"10", "as":"12", "t":observed.to_rfc3339()
        }]);
        let received = parse_stream_message(Message::text(message.to_string()), &cache, &config)
            .unwrap()
            .unwrap();
        assert!(received >= observed);
        let quote = cache
            .latest(&InstrumentId::new("NVDA_US_EQ").unwrap(), Utc::now(), 10)
            .unwrap();
        assert_eq!(quote.bid, Decimal::new(10010, 2));
        assert_eq!(cache.symbols().unwrap(), vec!["NVDA"]);
    }

    #[test]
    fn stream_quote_messages_reject_symbols_outside_the_universe() {
        let config = AlpacaDataConfig {
            config_version: 1,
            enabled: true,
            environment: AlpacaEnvironment::Paper,
            feed: AlpacaFeed::Iex,
            base_currency: Currency::new("USD").unwrap(),
            timeout_seconds: 15,
            quote_max_age_seconds: 10,
            history_limit: 500,
            stream_start_lead_seconds: 1_800,
        };
        let message = json!([{
            "T":"q", "S":"SPY", "bp":"100.10", "ap":"100.20",
            "bs":"10", "as":"12", "t":"2026-11-03T12:00:00Z"
        }]);
        assert!(
            parse_stream_message(
                Message::text(message.to_string()),
                &IexQuoteCache::default(),
                &config
            )
            .is_err()
        );
    }
}

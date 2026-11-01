//! Trading 212 v0 HTTP adapter. Production endpoints are fixed HTTPS origins.
//! Read access and executor mutation access are separate concrete types.

mod autonomous;
mod dto;
mod http;
pub use autonomous::AutonomousTrading212;
#[cfg(test)]
mod autonomous_tests;
#[cfg(test)]
mod tests;

use crate::{SystemClock, credentials::CredentialStore};
use chrono::{DateTime, Duration as TimeDelta, Utc};
use dto::{AccountSummary, History, InstrumentMetadata, Order, PositionDto, invalid};
use http::{Bucket, Http};
use reqwest::Method;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use trady_application::{
    BrokerInformationPort, BrokerPort, Clock,
    broker::{
        BrokerExecutionPort, BrokerFailure, BrokerFailureKind, BrokerOrderRequest, BrokerResult,
        CancellationReceipt, OrderKind,
    },
};
use trady_domain::{
    AccountId, AccountSnapshot, BrokerOrder, Capabilities, Currency, Error, InstrumentId, IntentId,
    Mode, Money, Result,
};

const API: &str = "/api/v0/equity";
static SNAPSHOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Demo,
    Live,
}
impl Environment {
    pub fn credential_names(self) -> (&'static str, &'static str) {
        match self {
            Self::Demo => ("TRADY_T212_DEMO_API_KEY", "TRADY_T212_DEMO_API_SECRET"),
            Self::Live => ("TRADY_T212_LIVE_API_KEY", "TRADY_T212_LIVE_API_SECRET"),
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            Self::Demo => "https://demo.trading212.com",
            Self::Live => "https://live.trading212.com",
        }
    }
    pub fn mode(self) -> Mode {
        match self {
            Self::Demo => Mode::Paper,
            Self::Live => Mode::Live,
        }
    }
}

/// Non-secret deployment binding. No arbitrary URLs, credentials or mode fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trading212Config {
    pub config_version: u32,
    pub environment: Environment,
    pub account_id: AccountId,
    pub broker_account_id: i64,
    pub base_currency: Currency,
    pub timeout_seconds: u64,
}
impl Trading212Config {
    pub fn validate(&self) -> BrokerResult<()> {
        let prefix = match self.environment {
            Environment::Demo => "paper_",
            Environment::Live => "live_",
        };
        if self.config_version != 1
            || self.broker_account_id <= 0
            || !(1..=30).contains(&self.timeout_seconds)
            || !self.account_id.as_str().starts_with(prefix)
        {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
        }
        Ok(())
    }
}

/// Never serializable; Debug redacts both values. Use account-scoped, least-privilege keys.
pub struct Credentials {
    key: String,
    secret: String,
}
impl Credentials {
    /// Resolve the API key and secret for exactly one explicitly configured environment.
    pub fn from_store(environment: Environment, store: &CredentialStore) -> Result<Self> {
        let (key_name, secret_name) = environment.credential_names();
        Self::new(
            store.required(key_name)?.into_inner(),
            store.required(secret_name)?.into_inner(),
        )
        .map_err(|error| Error(error.to_string()))
    }

    pub fn new(key: String, secret: String) -> BrokerResult<Self> {
        if key.is_empty()
            || secret.is_empty()
            || key.contains(':')
            || [&key, &secret]
                .iter()
                .any(|s| s.bytes().any(|b| b.is_ascii_control()) || s.trim() != s.as_str())
        {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
        }
        Ok(Self { key, secret })
    }
}
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credentials([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerInstrument {
    pub instrument_id: InstrumentId,
    pub currency: Currency,
    pub isin: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentCatalog {
    pub instruments: Vec<BrokerInstrument>,
    /// Provider rows whose ticker cannot be represented by `InstrumentId`.
    /// These rows are excluded from the typed catalog and never become order
    /// candidates.
    pub rejected_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerTax {
    pub charged_at: Option<DateTime<Utc>>,
    pub currency: Currency,
    pub name: String,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerFillWalletImpact {
    pub currency: Option<Currency>,
    pub fx_rate: Option<Decimal>,
    pub net_value: Option<Decimal>,
    pub realised_profit_loss: Option<Decimal>,
    pub taxes: Vec<BrokerTax>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerFill {
    pub broker_order_id: String,
    pub instrument_id: InstrumentId,
    pub id: Option<i64>,
    pub filled_at: Option<DateTime<Utc>>,
    pub price: Option<Decimal>,
    pub quantity: Option<Decimal>,
    pub trading_method: Option<String>,
    #[serde(rename = "type")]
    pub fill_type: Option<String>,
    pub wallet_impact: Option<BrokerFillWalletImpact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryQuery {
    pub instrument_id: Option<InstrumentId>,
    pub cursor: Option<i64>,
    pub limit: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPage {
    /// Total order quantities, not the per-fill quantities in history records.
    pub orders: Vec<BrokerOrder>,
    /// Individual fill records returned alongside historical orders.
    pub fills: Vec<BrokerFill>,
    pub next_cursor: Option<i64>,
}

/// Exposes only typed information operations. Snapshot collection is not atomic.
pub struct Trading212Reader<C: Clock = SystemClock> {
    config: Trading212Config,
    http: Http,
    clock: C,
    bound_at: Option<Instant>,
    metadata: Option<(Instant, BTreeMap<InstrumentId, BrokerInstrument>, usize)>,
}
impl Trading212Reader<SystemClock> {
    pub fn new(config: Trading212Config, credentials: Credentials) -> BrokerResult<Self> {
        Self::with_clock(config, credentials, SystemClock)
    }

    /// Build a reader immediately after `discover_account` has validated the
    /// provider-owned account and currency. The binding is valid for the same
    /// short observation window as a normal account read, so metadata and
    /// history reads do not issue a redundant account request and trip the
    /// broker's one-summary-per-five-seconds bucket.
    pub fn new_bound(config: Trading212Config, credentials: Credentials) -> BrokerResult<Self> {
        let mut reader = Self::new(config, credentials)?;
        reader.bound_at = Some(Instant::now());
        Ok(reader)
    }
}
impl<C: Clock> Trading212Reader<C> {
    pub fn with_clock(
        config: Trading212Config,
        credentials: Credentials,
        clock: C,
    ) -> BrokerResult<Self> {
        config.validate()?;
        let http = Http::new(&config, credentials)?;
        Ok(Self {
            config,
            http,
            clock,
            bound_at: None,
            metadata: None,
        })
    }

    pub fn config(&self) -> &Trading212Config {
        &self.config
    }

    /// Read the provider-owned account identity before a cold-start config has
    /// a broker account number. The caller must bind the returned ID into a
    /// fresh, normally validated reader before reading positions or history.
    /// This method deliberately does not infer any order or execution
    /// capability from the account response.
    pub fn discover_account(&mut self) -> BrokerResult<(i64, Currency, Money)> {
        let summary: AccountSummary = self.http.json(
            Bucket::Summary,
            Method::GET,
            &format!("{API}/account/summary"),
            None,
        )?;
        if summary.id <= 0
            || summary.currency != self.config.base_currency
            || summary.cash.available_to_trade.0 < rust_decimal::Decimal::ZERO
        {
            return Err(BrokerFailure::new(BrokerFailureKind::AccountMismatch));
        }
        self.bound_at = Some(Instant::now());
        Ok((
            summary.id,
            summary.currency.clone(),
            Money {
                amount: summary.cash.available_to_trade.0,
                currency: summary.currency,
            },
        ))
    }

    /// Checks the actual broker account number and primary currency on every call.
    pub fn spendable_cash(&mut self) -> BrokerResult<Money> {
        self.bound_at = None;
        let summary: AccountSummary = self.http.json(
            Bucket::Summary,
            Method::GET,
            &format!("{API}/account/summary"),
            None,
        )?;
        if summary.id != self.config.broker_account_id
            || summary.currency != self.config.base_currency
        {
            return Err(BrokerFailure::new(BrokerFailureKind::AccountMismatch));
        }
        if summary.cash.available_to_trade.0 < rust_decimal::Decimal::ZERO {
            return Err(invalid());
        }
        self.bound_at = Some(Instant::now());
        Ok(Money {
            amount: summary.cash.available_to_trade.0,
            currency: summary.currency,
        })
    }

    pub fn pending_orders(&mut self) -> BrokerResult<Vec<BrokerOrder>> {
        self.ensure_binding()?;
        let orders: Vec<Order> =
            self.http
                .json(Bucket::Orders, Method::GET, &format!("{API}/orders"), None)?;
        let mut ids = BTreeSet::new();
        orders
            .iter()
            .map(|o| {
                if !ids.insert(o.id) {
                    return Err(invalid());
                }
                o.normalize()
            })
            .collect()
    }

    /// None means absent from the pending endpoint, never proof of cancellation or nonexecution.
    pub fn pending_order(&mut self, broker_order_id: &str) -> BrokerResult<Option<BrokerOrder>> {
        let id = numeric_id(broker_order_id)?;
        self.ensure_binding()?;
        let order: Order = match self.http.json(
            Bucket::Order,
            Method::GET,
            &format!("{API}/orders/{id}"),
            None,
        ) {
            Ok(order) => order,
            Err(e) if e.http_status == Some(404) => return Ok(None),
            Err(e) => return Err(e),
        };
        if order.id != id {
            return Err(invalid());
        }
        order.normalize().map(Some)
    }

    /// Caches the provider's metadata for ten minutes. This is not capability evidence.
    /// Rows with provider tickers outside the domain identifier grammar are
    /// excluded and counted; valid rows remain available to the caller.
    pub fn instruments_with_rejections(&mut self) -> BrokerResult<InstrumentCatalog> {
        self.ensure_binding()?;
        if self
            .metadata
            .as_ref()
            .is_none_or(|(at, _, _)| at.elapsed() >= Duration::from_secs(600))
        {
            let rows: Vec<InstrumentMetadata> = self.http.json(
                Bucket::Instruments,
                Method::GET,
                &format!("{API}/metadata/instruments"),
                None,
            )?;
            let mut metadata = BTreeMap::new();
            let mut rejected_count = 0;
            for row in rows {
                let id = match InstrumentId::new(row.ticker) {
                    Ok(id) => id,
                    Err(_) => {
                        rejected_count += 1;
                        continue;
                    }
                };
                let instrument = BrokerInstrument {
                    instrument_id: id.clone(),
                    currency: row.currency_code,
                    isin: row.isin,
                    kind: row.kind,
                };
                if metadata.insert(id, instrument).is_some() {
                    return Err(invalid());
                }
            }
            self.metadata = Some((Instant::now(), metadata, rejected_count));
        }
        let (_, metadata, rejected_count) = self.metadata.as_ref().ok_or_else(invalid)?;
        Ok(InstrumentCatalog {
            instruments: metadata.values().cloned().collect(),
            rejected_count: *rejected_count,
        })
    }

    pub fn instruments(&mut self) -> BrokerResult<Vec<BrokerInstrument>> {
        Ok(self.instruments_with_rejections()?.instruments)
    }

    pub fn historical_orders(&mut self, query: &HistoryQuery) -> BrokerResult<HistoryPage> {
        if !(1..=50).contains(&query.limit) || query.cursor.is_some_and(|v| v < 0) {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
        }
        self.ensure_binding()?;
        let mut path = format!("{API}/history/orders?limit={}", query.limit);
        if let Some(cursor) = query.cursor {
            path.push_str(&format!("&cursor={cursor}"));
        }
        if let Some(instrument) = &query.instrument_id {
            path.push_str(&format!("&ticker={instrument}"));
        }
        let history: History = self.http.json(Bucket::History, Method::GET, &path, None)?;
        if history.items.len() > query.limit as usize {
            return Err(invalid());
        }
        let next_cursor = history
            .next_page_path
            .map(|path| next_cursor(&path, query))
            .transpose()?;
        let mut orders = BTreeMap::new();
        let mut fills_with_id = BTreeMap::new();
        let mut fills_without_id = Vec::new();
        for row in history.items {
            let order = row.order.normalize()?;
            if query
                .instrument_id
                .as_ref()
                .is_some_and(|id| id != &order.instrument_id)
            {
                return Err(invalid());
            }
            if let Some(previous) = orders.insert(order.broker_order_id.clone(), order.clone())
                && previous != order
            {
                return Err(invalid());
            }
            if let Some(fill) = row.fill {
                let fill = fill.normalize(&order)?;
                if let Some(id) = fill.id {
                    if let Some(previous) = fills_with_id.insert(id, fill.clone())
                        && previous != fill
                    {
                        return Err(invalid());
                    }
                } else if !fills_without_id.contains(&fill) {
                    fills_without_id.push(fill);
                }
            }
        }
        let mut fills: Vec<_> = fills_with_id.into_values().collect();
        fills.extend(fills_without_id);
        let mut filled_by_order = BTreeMap::<String, Decimal>::new();
        for fill in &fills {
            if fill
                .fill_type
                .as_deref()
                .is_none_or(|fill_type| fill_type == "TRADE")
                && let Some(quantity) = fill.quantity
            {
                let total = filled_by_order
                    .entry(fill.broker_order_id.clone())
                    .or_insert(Decimal::ZERO);
                *total += quantity;
            }
        }
        for (broker_order_id, total) in filled_by_order {
            if orders
                .get(&broker_order_id)
                .is_some_and(|order| total > order.filled_quantity)
            {
                return Err(invalid());
            }
        }
        Ok(HistoryPage {
            orders: orders.into_values().collect(),
            fills,
            next_cursor,
        })
    }

    pub fn read_snapshot(&mut self, account: &AccountId) -> BrokerResult<AccountSnapshot> {
        self.check_account(account)?;
        // Stamp the beginning, so multiple HTTP calls cannot refresh old observations.
        let observed_at = self.clock.now();
        let spendable_cash = self.spendable_cash()?;
        let positions: Vec<PositionDto> = self.http.json(
            Bucket::Positions,
            Method::GET,
            &format!("{API}/positions"),
            None,
        )?;
        let positions = positions
            .into_iter()
            .map(PositionDto::normalize)
            .collect::<BrokerResult<_>>()?;
        let orders = self.pending_orders()?;
        let elapsed = self.clock.now().signed_duration_since(observed_at);
        if elapsed < TimeDelta::zero() || elapsed > TimeDelta::seconds(60) {
            return Err(invalid());
        }
        let sequence = SNAPSHOT_SEQUENCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| invalid())?;
        let snapshot = AccountSnapshot {
            snapshot_id: format!(
                "t212_{}_{}_{}_{}",
                std::process::id(),
                observed_at.timestamp(),
                observed_at.timestamp_subsec_nanos(),
                sequence
            ),
            account_id: account.clone(),
            mode: self.config.environment.mode(),
            spendable_cash,
            positions,
            orders,
            observed_at,
        };
        snapshot.validate().map_err(|_| invalid())?;
        Ok(snapshot)
    }

    fn check_account(&self, account: &AccountId) -> BrokerResult<()> {
        if account != &self.config.account_id {
            return Err(BrokerFailure::new(BrokerFailureKind::AccountMismatch));
        }
        Ok(())
    }

    fn ensure_binding(&mut self) -> BrokerResult<()> {
        if self
            .bound_at
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(60))
        {
            self.spendable_cash()?;
        }
        Ok(())
    }
}

impl<C: Clock> BrokerInformationPort for Trading212Reader<C> {
    fn snapshot(&mut self, account: &AccountId) -> Result<AccountSnapshot> {
        self.read_snapshot(account)
            .map_err(|e| Error(e.to_string()))
    }
}
impl<C: Clock> BrokerPort for Trading212Reader<C> {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn find_order(&mut self, _intent: &IntentId) -> Result<Option<BrokerOrder>> {
        Err(Error("Trading 212 has no client-intent lookup; reconcile stored broker IDs/history, never infer a match".into()))
    }
}

/// Low-level executor transport. No CLI or model tool exposes this type's mutations.
/// It does not implement the durable live execution/protection state machine.
pub struct Trading212Executor<C: Clock = SystemClock> {
    reader: Trading212Reader<C>,
}
impl Trading212Executor<SystemClock> {
    pub fn new(config: Trading212Config, credentials: Credentials) -> BrokerResult<Self> {
        Ok(Self {
            reader: Trading212Reader::new(config, credentials)?,
        })
    }
}
impl<C: Clock> Trading212Executor<C> {
    pub fn information(&mut self) -> &mut Trading212Reader<C> {
        &mut self.reader
    }
}
impl<C: Clock> Trading212Executor<C> {
    /// Caller has just consumed a fresh account binding for this single mutation.
    fn submit_bound(&mut self, request: &BrokerOrderRequest) -> BrokerResult<BrokerOrder> {
        self.submit_bound_before(request, None)
    }

    fn submit_bound_before(
        &mut self,
        request: &BrokerOrderRequest,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> BrokerResult<BrokerOrder> {
        request.validate()?;
        self.reader.check_account(&request.account_id)?;
        let body = dto::order_body(request)?;
        let instruments = self.reader.instruments()?;
        let instrument = instruments
            .iter()
            .find(|i| i.instrument_id == request.instrument_id)
            .ok_or_else(|| BrokerFailure::new(BrokerFailureKind::Unsupported))?;
        if instrument.currency != request.instrument_currency || instrument.kind != "STOCK" {
            return Err(BrokerFailure::new(BrokerFailureKind::Unsupported));
        }
        let (bucket, endpoint) = match request.kind {
            OrderKind::Market { .. } => (Bucket::Market, "market"),
            OrderKind::Limit { .. } => (Bucket::Limit, "limit"),
            OrderKind::Stop { .. } => (Bucket::Stop, "stop"),
            OrderKind::StopLimit { .. } => (Bucket::StopLimit, "stop_limit"),
        };
        if expires_at.is_some_and(|expiry| self.reader.clock.now() >= expiry) {
            return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
        }
        let order: Order = self.reader.http.json(
            bucket,
            Method::POST,
            &format!("{API}/orders/{endpoint}"),
            Some(body),
        )?;
        let normalized = order.normalize().and_then(|o| {
            if order.matches(request, &o) {
                Ok(o)
            } else {
                Err(invalid())
            }
        });
        normalized.map_err(|mut e| {
            e.outcome_unknown = true;
            e.broker_order_id = (order.id > 0).then(|| order.id.to_string());
            e
        })
    }
}
impl<C: Clock> BrokerExecutionPort for Trading212Executor<C> {
    fn submit_order(&mut self, request: &BrokerOrderRequest) -> BrokerResult<BrokerOrder> {
        request.validate()?;
        self.reader.check_account(&request.account_id)?;
        self.reader.spendable_cash()?;
        self.submit_bound(request)
    }

    fn cancel_order(
        &mut self,
        account: &AccountId,
        broker_order_id: &str,
    ) -> BrokerResult<CancellationReceipt> {
        self.reader.check_account(account)?;
        let id = numeric_id(broker_order_id)?;
        self.reader.spendable_cash()?;
        self.reader
            .http
            .request(
                Bucket::Cancel,
                Method::DELETE,
                &format!("{API}/orders/{id}"),
                None,
            )
            .map_err(|mut e| {
                e.broker_order_id = Some(id.to_string());
                e
            })?;
        Ok(CancellationReceipt {
            broker_order_id: id.to_string(),
        })
    }
}

fn numeric_id(value: &str) -> BrokerResult<i64> {
    let id = value
        .parse::<i64>()
        .map_err(|_| BrokerFailure::new(BrokerFailureKind::InvalidRequest))?;
    if id <= 0 || value != id.to_string() {
        return Err(BrokerFailure::new(BrokerFailureKind::InvalidRequest));
    }
    Ok(id)
}

fn next_cursor(path: &str, query: &HistoryQuery) -> BrokerResult<i64> {
    // Never forward a provider-supplied URL with an Authorization header.
    if !path.starts_with("/api/v0/equity/history/orders?") {
        return Err(invalid());
    }
    let base = reqwest::Url::parse("https://live.trading212.com").map_err(|_| invalid())?;
    let url = base.join(path).map_err(|_| invalid())?;
    if url.origin() != base.origin()
        || url.path() != "/api/v0/equity/history/orders"
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let mut params = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if !matches!(key.as_ref(), "limit" | "cursor" | "ticker")
            || params
                .insert(key.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(invalid());
        }
    }
    if params.get("limit").and_then(|v| v.parse::<u8>().ok()) != Some(query.limit)
        || params.get("ticker").map(String::as_str)
            != query.instrument_id.as_ref().map(InstrumentId::as_str)
    {
        return Err(invalid());
    }
    let cursor: i64 = params
        .get("cursor")
        .ok_or_else(invalid)?
        .parse()
        .map_err(|_| invalid())?;
    if cursor < 0 || Some(cursor) == query.cursor {
        return Err(invalid());
    }
    Ok(cursor)
}

//! Cold-start account discovery, artifact registration, and the supervised first-order gate.
//!
//! This module is deliberately separate from `autonomous`: a cold start may
//! discover and record evidence without silently turning that evidence into
//! execution authority.  The first-order path can only mutate a demo account
//! after a fresh quote, cost record, protection evidence, and an explicit
//! operator approval have all been resolved.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration as StdDuration,
};
use trady_adapters::{
    autonomous::{MarketInputs, TradingInputsConfig},
    credentials::CredentialStore,
    opencode::llm::{OpenCodeGoConfig, OpenCodeGoProvider},
    trading212::{
        BrokerFill, BrokerInstrument, Credentials, Environment, HistoryQuery, Trading212Config,
        Trading212Reader,
    },
};
use trady_application::{
    autonomous::TradingConfig,
    contracts::parse_json_bounded,
    harness::{CancellationToken, ChatMessage, LlmProvider, ModelRequest},
    model_lab::write_immutable,
    optimization::{valid_hash, valid_id},
};
use trady_domain::{
    AccountId, Capabilities, Capability, Currency, Error, InstrumentId, Mode, Money, Quote, Result,
    Side, Support, require,
};
use trady_policy::Policy;

pub const HELP: &str = "\nCold-start account setup:\n  trady account cold-start --config FILE [--llm-review]\n  trady account cold-start capability-evidence --config FILE --file FILE\n  trady account cold-start first-order --config FILE --require-manual-approval [--approve]\n  trady account cold-start reconcile --config FILE --intent FILE --broker-order-id ID\n\nThe cold-start command discovers the broker account, records immutable account/data/universe/model/formula/capability artifacts, and leaves the seed deployment shadow-only. capability-evidence imports a separately reviewed, account-bound capability/crosswalk record and writes an immutable copy; it never changes UNKNOWN to VERIFIED by itself. The first-order command refreshes the account and reviewed input service, then fails closed unless one fresh protected demo order is explicitly approved. Reconcile binds a broker response or fill to an immutable intent and preserves unknown fees, FX and cost fields instead of guessing.\n";

const MAX_CONFIG_BYTES: usize = 2 * 1024 * 1024;
const MAX_HISTORY_BYTES: usize = 96 * 1024 * 1024;
const MAX_HISTORY_PAGES: usize = 20;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ColdStartConfig {
    schema_version: u32,
    account: AccountConfig,
    broker: BrokerBootstrapConfig,
    market_data: MarketDataConfig,
    universe: UniverseConfig,
    risk: RiskConfig,
    artifacts: ArtifactConfig,
    #[serde(default)]
    inputs: Option<TradingInputsConfig>,
    #[serde(default)]
    research_artifacts: ResearchArtifacts,
    #[serde(default)]
    review: ReviewConfig,
    /// A private, independently reviewed capability/crosswalk record. The
    /// cold-start config may reference it, but cannot manufacture it.
    #[serde(default)]
    capability_evidence_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountConfig {
    account_id: AccountId,
    base_currency: Currency,
    environment: Environment,
    owner_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrokerBootstrapConfig {
    timeout_seconds: u64,
    #[serde(default)]
    broker_account_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarketDataConfig {
    history_path: PathBuf,
    provider: String,
    source_permissions: String,
    #[serde(default)]
    expected_symbols: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct UniverseConfig {
    source_id: String,
    source_url: String,
    candidate_limit: usize,
    #[serde(default)]
    preferred_symbols: Vec<String>,
    #[serde(default)]
    excluded_symbols: Vec<String>,
    #[serde(default = "default_dynamic_research")]
    selection_mode: String,
    #[serde(default)]
    rights_reference: String,
}

fn default_dynamic_research() -> String {
    "LLM_NEWS_X_RESEARCH_WITH_BROKER_INTERSECTION".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RiskConfig {
    entry_cash_fraction: Decimal,
    max_initial_loss_fraction: Decimal,
    max_order_value: Money,
    max_daily_orders: u8,
    require_protection: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactConfig {
    directory: PathBuf,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ResearchArtifacts {
    dataset_report: Option<PathBuf>,
    candidate_model: Option<PathBuf>,
    native_model: Option<PathBuf>,
    evaluation_report: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ReviewConfig {
    enabled: bool,
    config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
struct RunSummary {
    schema_version: u32,
    run_id: String,
    status: String,
    entry_authorized: bool,
    artifact_directory: PathBuf,
    account: Value,
    market_data: Value,
    universe: Value,
    models: Value,
    formula: Value,
    capabilities: Value,
    runtime_review: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapabilityObservation {
    support: Support,
    evidence_id: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    method: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityReview {
    reviewer_id: String,
    review_id: String,
    reviewed_at: DateTime<Utc>,
    review_hash: String,
    independent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewedCapabilitySet {
    cash_account: CapabilityObservation,
    instrument: CapabilityObservation,
    fractional_orders: CapabilityObservation,
    quotes: CapabilityObservation,
    market_orders: CapabilityObservation,
    limit_orders: CapabilityObservation,
    protective_orders: CapabilityObservation,
    order_cancellation: CapabilityObservation,
    partial_fill_reporting: CapabilityObservation,
    fee_reporting: CapabilityObservation,
    fx_reporting: CapabilityObservation,
}

impl ReviewedCapabilitySet {
    fn runtime(&self) -> Capabilities {
        let to_capability = |observation: &CapabilityObservation| Capability {
            support: observation.support,
            evidence_id: Some(observation.evidence_id.clone()),
            valid_until: Some(observation.valid_until),
        };
        Capabilities {
            cash_account: to_capability(&self.cash_account),
            instrument: to_capability(&self.instrument),
            fractional_orders: to_capability(&self.fractional_orders),
            protective_orders: to_capability(&self.protective_orders),
            quotes: to_capability(&self.quotes),
        }
    }

    fn entry_operations(&self) -> [(&'static str, &CapabilityObservation); 7] {
        [
            ("market_orders", &self.market_orders),
            ("limit_orders", &self.limit_orders),
            ("protective_orders", &self.protective_orders),
            ("order_cancellation", &self.order_cancellation),
            ("partial_fill_reporting", &self.partial_fill_reporting),
            ("fee_reporting", &self.fee_reporting),
            ("fx_reporting", &self.fx_reporting),
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewedCapabilityEvidence {
    schema_version: u32,
    kind: String,
    evidence_id: String,
    account_id: AccountId,
    broker_account_id: i64,
    environment: Environment,
    instrument_id: InstrumentId,
    /// The crosswalk's share-class identity. This is never inferred from a
    /// ticker prefix or silently replaced with the historical dataset key.
    instrument_isin: String,
    currency: Currency,
    venue: String,
    provider: String,
    source_hash: String,
    observed_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    review: CapabilityReview,
    capabilities: ReviewedCapabilitySet,
}

impl ReviewedCapabilityEvidence {
    fn validate(
        &self,
        config: &ColdStartConfig,
        broker_id: i64,
        broker_instrument: &BrokerInstrument,
        now: DateTime<Utc>,
    ) -> Result<()> {
        require(
            self.schema_version == 1
                && self.kind == "broker_capability_evidence"
                && valid_id(&self.evidence_id)
                && self.account_id == config.account.account_id
                && self.broker_account_id == broker_id
                && self.environment == config.account.environment
                && self.instrument_id == broker_instrument.instrument_id
                && self.instrument_isin == broker_instrument.isin
                && self.currency == broker_instrument.currency
                && self.venue.len() == 4
                && self
                    .venue
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && valid_id(&self.provider)
                && valid_hash(&self.source_hash)
                && self.observed_at <= self.review.reviewed_at
                && self.review.reviewed_at <= now
                && now < self.valid_until
                && valid_id(&self.review.reviewer_id)
                && valid_id(&self.review.review_id)
                && valid_hash(&self.review.review_hash)
                && self.review.independent,
            "capability evidence is unbound, stale or not independently reviewed",
        )?;
        let observations = [
            &self.capabilities.cash_account,
            &self.capabilities.instrument,
            &self.capabilities.fractional_orders,
            &self.capabilities.quotes,
            &self.capabilities.market_orders,
            &self.capabilities.limit_orders,
            &self.capabilities.protective_orders,
            &self.capabilities.order_cancellation,
            &self.capabilities.partial_fill_reporting,
            &self.capabilities.fee_reporting,
            &self.capabilities.fx_reporting,
        ];
        for observation in observations {
            require(
                valid_id(&observation.evidence_id)
                    && observation.observed_at >= self.observed_at
                    && observation.observed_at <= now
                    && observation.valid_until <= self.valid_until
                    && observation.valid_until > now
                    && !observation.method.trim().is_empty()
                    && !observation.detail.trim().is_empty(),
                "capability observation is invalid or outside the reviewed evidence window",
            )?;
        }
        Ok(())
    }
}

impl ColdStartConfig {
    fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 1,
            "unsupported cold-start config version",
        )?;
        require(
            self.account.owner_id.len() <= 96
                && !self.account.owner_id.is_empty()
                && self
                    .account
                    .owner_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "invalid cold-start owner identity",
        )?;
        require(
            self.account.environment == Environment::Demo
                && self.account.account_id.as_str().starts_with("paper_"),
            "cold-start is restricted to a paper_ demo namespace",
        )?;
        require(
            (1..=30).contains(&self.broker.timeout_seconds),
            "broker timeout must be between one and thirty seconds",
        )?;
        require(
            self.market_data.provider.len() <= 256
                && !self.market_data.provider.trim().is_empty()
                && !self.market_data.source_permissions.trim().is_empty(),
            "market-data provenance and permissions are required",
        )?;
        require(
            !self.universe.source_id.is_empty()
                && !self.universe.source_url.is_empty()
                && (1..=5).contains(&self.universe.candidate_limit),
            "universe source and candidate bound are required",
        )?;
        require(
            self.risk.entry_cash_fraction > Decimal::ZERO
                && self.risk.entry_cash_fraction <= Decimal::ONE
                && self.risk.max_initial_loss_fraction > Decimal::ZERO
                && self.risk.max_initial_loss_fraction <= Decimal::new(2, 2)
                && self.risk.max_order_value.amount > Decimal::ZERO
                && self.risk.max_order_value.currency == self.account.base_currency
                && (1..=1).contains(&self.risk.max_daily_orders),
            "cold-start risk policy exceeds the single-order bound",
        )?;
        if let Some(inputs) = &self.inputs {
            let trading = trading_config(self)?;
            inputs.validate(&trading)?;
        }
        Ok(())
    }
}

fn trading_config(config: &ColdStartConfig) -> Result<TradingConfig> {
    Ok(TradingConfig {
        config_version: 1,
        account_id: config.account.account_id.clone(),
        mode: Mode::Paper,
        base_currency: config.account.base_currency.clone(),
        owner_id: config.account.owner_id.clone(),
        policy: Policy::default(),
        capabilities: Capabilities::default(),
        approved_skills: BTreeMap::new(),
        live_release: None,
        limits_authority: None,
    })
}

pub(super) fn run(args: &[String]) -> Result<Value> {
    match args.first().map(String::as_str) {
        Some("cold-start") if args.get(1).map(String::as_str) == Some("capability-evidence") => {
            import_capability_evidence(&args[2..])
        }
        Some("cold-start") if args.get(1).map(String::as_str) == Some("first-order") => {
            first_order(&args[2..])
        }
        Some("cold-start") if args.get(1).map(String::as_str) == Some("reconcile") => {
            reconcile(&args[2..])
        }
        Some("cold-start") => cold_start(&args[1..]),
        Some("help") | Some("--help") if args.len() == 1 => Ok(json!({"help": HELP})),
        _ => Err(Error("unsupported account command; see help".into())),
    }
}

fn options(args: &[String], first_order: bool) -> Result<(PathBuf, bool, bool)> {
    let mut config = None;
    let mut llm_review = false;
    let mut manual = false;
    let mut approve = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                require(config.is_none(), "duplicate --config")?;
                config = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| Error("missing --config value".into()))?,
                ));
            }
            "--llm-review" => llm_review = true,
            "--require-manual-approval" => manual = true,
            "--approve" => approve = true,
            _ => return Err(Error("unknown account option".into())),
        }
        i += 1;
    }
    let config = config.ok_or_else(|| Error("--config is required".into()))?;
    if first_order {
        require(manual, "first-order requires --require-manual-approval")?;
    } else {
        require(
            !manual && !approve,
            "manual approval flags are only valid for first-order",
        )?;
    }
    Ok((config, llm_review, approve))
}

fn reconcile_options(args: &[String]) -> Result<(PathBuf, PathBuf, String)> {
    let mut config = None;
    let mut intent = None;
    let mut broker_order_id = None;
    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        i += 1;
        let value = args
            .get(i)
            .ok_or_else(|| Error("missing reconcile option value".into()))?
            .clone();
        i += 1;
        match key {
            "--config" if config.is_none() => config = Some(PathBuf::from(value)),
            "--intent" if intent.is_none() => intent = Some(PathBuf::from(value)),
            "--broker-order-id" if broker_order_id.is_none() => broker_order_id = Some(value),
            _ => return Err(Error("unknown or duplicate reconcile option".into())),
        }
    }
    Ok((
        config.ok_or_else(|| Error("reconcile requires --config".into()))?,
        intent.ok_or_else(|| Error("reconcile requires --intent".into()))?,
        broker_order_id.ok_or_else(|| Error("reconcile requires --broker-order-id".into()))?,
    ))
}

fn load(path: &Path) -> Result<ColdStartConfig> {
    let bytes = bounded_file(path, MAX_CONFIG_BYTES)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| Error("cold-start config is not UTF-8".into()))?;
    let config: ColdStartConfig =
        serde_json::from_value(parse_json_bounded(text, MAX_CONFIG_BYTES)?)
            .map_err(|_| Error("invalid cold-start configuration fields".into()))?;
    config.validate()?;
    Ok(config)
}

fn bounded_file(path: &Path, max: usize) -> Result<Vec<u8>> {
    let meta =
        fs::symlink_metadata(path).map_err(|_| Error("cold-start input unavailable".into()))?;
    require(
        meta.is_file() && !meta.file_type().is_symlink(),
        "cold-start input must be a regular file",
    )?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| Error("cold-start input unavailable".into()))?
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error("cold-start input read failed".into()))?;
    require(bytes.len() <= max, "cold-start input exceeds size limit")?;
    Ok(bytes)
}

fn load_reviewed_capability_evidence(
    config: &ColdStartConfig,
    config_base: &Path,
    broker_id: i64,
    broker_instrument: &BrokerInstrument,
    now: DateTime<Utc>,
) -> Result<ReviewedCapabilityEvidence> {
    let path = config
        .capability_evidence_path
        .as_ref()
        .ok_or_else(|| Error("reviewed capability evidence is not configured".into()))?;
    let path = resolve(config_base, path);
    let bytes = bounded_file(&path, MAX_CONFIG_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error("capability evidence is not UTF-8".into()))?;
    let evidence: ReviewedCapabilityEvidence =
        serde_json::from_value(parse_json_bounded(text, MAX_CONFIG_BYTES)?)
            .map_err(|_| Error("invalid reviewed capability evidence fields".into()))?;
    evidence.validate(config, broker_id, broker_instrument, now)?;
    Ok(evidence)
}

fn hash_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn hash_value(value: &Value) -> Result<String> {
    Ok(hash_bytes(
        &serde_json::to_vec(value).map_err(|_| Error("artifact encoding failed".into()))?,
    ))
}

fn write_value(path: &Path, value: &Value) -> Result<String> {
    write_immutable(path, value)?;
    let bytes = bounded_file(path, MAX_HISTORY_BYTES.min(MAX_CONFIG_BYTES))?;
    Ok(hash_bytes(&bytes))
}

fn resolve(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn bootstrap_reader(config: &ColdStartConfig) -> Result<(Trading212Reader, i64, Currency, Money)> {
    let credentials =
        Credentials::from_store(config.account.environment, &CredentialStore::load()?)?;
    // A positive sentinel is used only to pass the normal adapter constructor;
    // discover_account immediately replaces it with the provider-owned ID.
    let probe_config = Trading212Config {
        config_version: 1,
        environment: config.account.environment,
        account_id: config.account.account_id.clone(),
        broker_account_id: config.broker.broker_account_id.unwrap_or(1),
        base_currency: config.account.base_currency.clone(),
        timeout_seconds: config.broker.timeout_seconds,
    };
    let mut probe = Trading212Reader::new(probe_config.clone(), credentials)
        .map_err(|e| Error(e.to_string()))?;
    let (broker_id, currency, cash) = probe.discover_account().map_err(|e| Error(e.to_string()))?;
    if let Some(expected) = config.broker.broker_account_id {
        require(
            expected == broker_id,
            "configured broker account identity differs from discovery",
        )?;
    }
    let bound_config = Trading212Config {
        broker_account_id: broker_id,
        ..probe_config
    };
    let credentials =
        Credentials::from_store(config.account.environment, &CredentialStore::load()?)?;
    let reader =
        Trading212Reader::new_bound(bound_config, credentials).map_err(|e| Error(e.to_string()))?;
    Ok((reader, broker_id, currency, cash))
}

fn history(
    reader: &mut Trading212Reader,
) -> Result<(Vec<trady_domain::BrokerOrder>, Vec<BrokerFill>, bool)> {
    let mut cursor = None;
    let mut orders = Vec::new();
    let mut fills = Vec::new();
    let mut pages = 0;
    while pages < MAX_HISTORY_PAGES {
        let page = reader
            .historical_orders(&HistoryQuery {
                instrument_id: None,
                cursor,
                limit: 50,
            })
            .map_err(|e| Error(e.to_string()))?;
        orders.extend(page.orders);
        for fill in page.fills {
            if !fills.contains(&fill) {
                fills.push(fill);
            }
        }
        cursor = page.next_cursor;
        pages += 1;
        if cursor.is_none() {
            break;
        }
    }
    Ok((orders, fills, cursor.is_some()))
}

#[allow(clippy::too_many_arguments)]
fn account_report(
    config: &ColdStartConfig,
    broker_id: i64,
    currency: &Currency,
    cash: &Money,
    snapshot: &trady_domain::AccountSnapshot,
    orders: &[trady_domain::BrokerOrder],
    fills: &[BrokerFill],
    history_truncated: bool,
    history_error: Option<&str>,
    now: DateTime<Utc>,
) -> Value {
    let filled_quantity = fills
        .iter()
        .filter_map(|fill| fill.quantity)
        .sum::<Decimal>();
    json!({
        "schema_version": 1,
        "kind": "cold_start_account_snapshot",
        "immutable": true,
        "observed_at": now,
        "local_account_id": config.account.account_id,
        "broker_account_id": broker_id,
        "environment": config.account.environment,
        "base_currency": currency,
        "available_cash": cash,
        "snapshot": snapshot,
        "historical_orders": orders,
        "historical_fills": {"supported": true, "source": "Trading212 history/orders fill", "count": fills.len(), "quantity": filled_quantity, "items": fills},
        "fees": {"supported": true, "source": "Trading212 fill.walletImpact.taxes", "observed": fills.iter().any(|fill| fill.wallet_impact.as_ref().is_some_and(|impact| !impact.taxes.is_empty()))},
        "fx": {"supported": true, "source": "Trading212 fill.walletImpact.fxRate", "observed": fills.iter().any(|fill| fill.wallet_impact.as_ref().and_then(|impact| impact.fx_rate).is_some())},
        "history_truncated": history_truncated,
        "history_error": history_error,
        "entry_authorized": false
    })
}

fn market_report(config: &ColdStartConfig, now: DateTime<Utc>) -> Result<Value> {
    let path = resolve(Path::new("."), &config.market_data.history_path);
    let bytes = bounded_file(&path, MAX_HISTORY_BYTES)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error("historical market data is not valid JSON".into()))?;
    let instruments = value
        .get("instruments")
        .and_then(Value::as_array)
        .ok_or_else(|| Error("historical market data has no instruments".into()))?;
    let mut rows = 0usize;
    let mut duplicates = 0usize;
    let mut missing_windows = 0usize;
    let mut first = None;
    let mut last = None;
    let mut symbols = Vec::new();
    for instrument in instruments {
        let id = instrument
            .get("instrument_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let bars = instrument
            .get("bars")
            .and_then(Value::as_array)
            .ok_or_else(|| Error("market instrument has no bars".into()))?;
        let mut seen = BTreeSet::new();
        let mut previous = None;
        for bar in bars {
            let date = bar
                .get("date")
                .and_then(Value::as_str)
                .ok_or_else(|| Error("market bar has no date".into()))?;
            let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| Error("market bar date is invalid".into()))?;
            for key in ["open", "high", "low", "close", "volume"] {
                require(
                    bar.get(key)
                        .and_then(Value::as_f64)
                        .is_some_and(f64::is_finite),
                    "market bar contains a non-finite field",
                )?;
            }
            require(
                bar["high"].as_f64().unwrap_or(0.0) >= bar["low"].as_f64().unwrap_or(0.0),
                "market bar high/low relationship is invalid",
            )?;
            if !seen.insert(date.to_owned()) {
                duplicates += 1;
            }
            if let Some(old) = previous {
                if parsed <= old {
                    return Err(Error(
                        "market timestamps are not strictly increasing".into(),
                    ));
                }
                if parsed.signed_duration_since(old).num_days() > 5 {
                    missing_windows += 1;
                }
            }
            previous = Some(parsed);
            first = Some(first.map_or(parsed, |v: NaiveDate| v.min(parsed)));
            last = Some(last.map_or(parsed, |v: NaiveDate| v.max(parsed)));
            rows += 1;
        }
        if !id.is_empty() {
            symbols.push(id);
        }
    }
    if !config.market_data.expected_symbols.is_empty() {
        for expected in &config.market_data.expected_symbols {
            require(
                symbols.iter().any(|s| s == expected),
                "expected market instrument is absent",
            )?;
        }
    }
    Ok(json!({
        "schema_version":1,"kind":"historical_ohlcv_integrity","immutable":true,"observed_at":now,
        "path":path,"sha256":hash_bytes(&bytes),"provider":config.market_data.provider,
        "source_permissions":config.market_data.source_permissions,"instruments":symbols,"rows":rows,
        "duplicate_records":duplicates,"missing_candle_windows":missing_windows,
        "first_date":first,"last_date":last,
        "timestamp_consistent":duplicates == 0,"training_dataset_ready":rows > 0 && duplicates == 0,
        "deployment_eligible":false
    }))
}

fn instrument_json(instrument: &BrokerInstrument) -> Value {
    json!({"instrument_id":instrument.instrument_id,"currency":instrument.currency,"isin":instrument.isin,"kind":instrument.kind})
}

fn universe_report(
    config: &ColdStartConfig,
    instruments: &[BrokerInstrument],
    metadata_rejected_count: usize,
    metadata_error: Option<&str>,
    now: DateTime<Utc>,
) -> Value {
    let excluded: BTreeSet<_> = config
        .universe
        .excluded_symbols
        .iter()
        .map(String::as_str)
        .collect();
    let mut supported = Vec::new();
    let mut rejected = Vec::new();
    for instrument in instruments {
        let symbol = instrument
            .instrument_id
            .as_str()
            .split('_')
            .next()
            .unwrap_or(instrument.instrument_id.as_str());
        let allowed = instrument.kind == "STOCK"
            && !excluded.contains(symbol)
            && !excluded.contains(instrument.instrument_id.as_str());
        if allowed {
            supported.push(instrument);
        } else {
            rejected.push(instrument);
        }
    }
    let mut candidates = Vec::new();
    for preferred in &config.universe.preferred_symbols {
        let exact = format!("{preferred}_US_EQ");
        let instrument = supported
            .iter()
            .find(|i| i.instrument_id.as_str() == exact)
            .or_else(|| {
                let mut matches = supported
                    .iter()
                    .filter(|i| i.instrument_id.as_str().starts_with(preferred));
                let first = matches.next();
                first.filter(|_| matches.next().is_none())
            });
        if let Some(instrument) = instrument
            && !candidates
                .iter()
                .any(|i: &&BrokerInstrument| i.instrument_id == instrument.instrument_id)
        {
            candidates.push(*instrument);
        }
    }
    for instrument in &supported {
        if candidates.len() >= config.universe.candidate_limit {
            break;
        }
        if !candidates
            .iter()
            .any(|i| i.instrument_id == instrument.instrument_id)
        {
            candidates.push(*instrument);
        }
    }
    candidates.truncate(config.universe.candidate_limit);
    json!({
        "schema_version":1,"kind":"dynamic_universe_initialization","immutable":true,"observed_at":now,
        "source_id":config.universe.source_id,"source_url":config.universe.source_url,"rights_reference":config.universe.rights_reference,
        "selection_mode":config.universe.selection_mode,"llm_research_required":true,
        "broker_instrument_count":instruments.len().saturating_add(metadata_rejected_count),"broker_typed_instrument_count":instruments.len(),"broker_metadata_rejected_count":metadata_rejected_count,"supported_stock_count":supported.len(),"excluded_count":rejected.len(),
        "broker_metadata_error":metadata_error,
        "candidate_limit":config.universe.candidate_limit,"candidate_pool":candidates.iter().map(|i|instrument_json(i)).collect::<Vec<_>>(),
        "owner_allowlist":[],"allowlist_status":"DYNAMIC_RESEARCH_REVIEW_REQUIRED",
        "excluded":rejected.iter().map(|i|instrument_json(i)).collect::<Vec<_>>(),"deployment_eligible":false
    })
}

fn file_hash(path: &Option<PathBuf>) -> Option<String> {
    path.as_ref()
        .and_then(|p| bounded_file(p, MAX_HISTORY_BYTES).ok())
        .map(|b| hash_bytes(&b))
}

fn seed_artifacts(
    config: &ColdStartConfig,
    root: &Path,
    market_hash: &str,
    account_hash: &str,
    now: DateTime<Utc>,
) -> Result<(Value, Value, Value, Value)> {
    let dataset_hash = file_hash(&config.research_artifacts.dataset_report);
    let candidate_hash = file_hash(&config.research_artifacts.candidate_model);
    let native_hash = file_hash(&config.research_artifacts.native_model);
    let evaluation_hash = file_hash(&config.research_artifacts.evaluation_report);
    let cost_model = json!({
        "schema_version":1,"kind":"execution_cost_model","model_id":"seed_execution_cost_v1","status":"SHADOW_ONLY",
        "components":{"spread_bps":"20","broker_fee_bps":"0","fx_buffer_bps":"15","slippage_bps":"10"},
        "labels":{"fees":"unavailable_until_first_fill","fx":"unavailable_until_first_fill","slippage":"unavailable_until_first_fill"},
        "provenance":{"market_data_hash":market_hash,"account_snapshot_hash":account_hash},"approved":false,"created_at":now
    });
    let cost_hash = hash_value(&cost_model)?;
    let market_model = json!({
        "schema_version":1,"kind":"market_prediction_model","model_id":"seed_market_prediction_v1","status":"SHADOW_ONLY",
        "artifact_sha256":native_hash.or(candidate_hash.clone()),"dataset_report_sha256":dataset_hash,
        "evaluation_report_sha256":evaluation_hash,"feature_schema":"daily_replay_features_v1",
        "features":["return_1","return_5","return_20","volatility_20","range_20","volume_ratio_20","distance_sma_20"],
        "label_definition":"gross_barrier_1pct_5bars_v1","research_only":true,"approved":false,"created_at":now
    });
    let market_model_hash = hash_value(&market_model)?;
    let formula = json!({
        "schema_version":1,"kind":"deterministic_baseline_formula","revision_id":"seed_formula_v1","status":"SHADOW_ONLY",
        "parameters":{"entry_cash_fraction":config.risk.entry_cash_fraction,"max_initial_loss_fraction":config.risk.max_initial_loss_fraction,"reward_to_risk":"1.5","max_holding_sessions":5,"price_tick":"0.01","quantity_step":"0.01"},
        "inputs":["account_snapshot","quote","execution_cost_model","market_prediction_model","research_decision"],
        "outputs":["entry_limit","stop_loss","take_profit","quantity"],"cost_model_hash":cost_hash,"prediction_model_hash":market_model_hash,
        "parity":{"rust":true,"lean":false,"report":"pending_runtime_parity"},"approved":false,"created_at":now
    });
    let formula_hash = hash_value(&formula)?;
    let decision = json!({
        "schema_version":1,"kind":"trading_decision_model","model_id":"seed_decision_v1","status":"SHADOW_ONLY",
        "rule":"enter only when reviewed research candidate, calibrated prediction and protected cost-adjusted reward/risk pass",
        "formula_hash":formula_hash,"market_prediction_model_hash":market_model_hash,"execution_cost_model_hash":cost_hash,
        "candidate_limit":config.universe.candidate_limit,"approved":false,"created_at":now
    });
    let formula_report = json!({
        "schema_version":1,"kind":"formula_reproducibility_report","formula_hash":formula_hash,"formula":formula,
        "mathematical_parity":"PENDING_INDEPENDENT_REVIEW","implementation_parity":"PENDING_INDEPENDENT_REVIEW","deployment_eligible":false
    });
    let cost_path = root.join("execution-cost-model.json");
    let market_path = root.join("market-prediction-model.json");
    let decision_path = root.join("trading-decision-model.json");
    let formula_path = root.join("seed-formula.json");
    let cost_file_hash = write_value(&cost_path, &cost_model)?;
    let market_file_hash = write_value(&market_path, &market_model)?;
    let decision_file_hash = write_value(&decision_path, &decision)?;
    let formula_file_hash = write_value(&formula_path, &formula_report)?;
    Ok((
        json!({"path":market_path,"sha256":market_file_hash,"artifact":market_model,"model_hash":market_model_hash}),
        json!({"path":cost_path,"sha256":cost_file_hash,"artifact":cost_model,"model_hash":cost_hash}),
        json!({"path":decision_path,"sha256":decision_file_hash,"artifact":decision}),
        json!({"path":formula_path,"sha256":formula_file_hash,"artifact":formula_report}),
    ))
}

fn capability_report(
    config: &ColdStartConfig,
    instruments: &[BrokerInstrument],
    snapshot: &trady_domain::AccountSnapshot,
    now: DateTime<Utc>,
    run_id: &str,
) -> (Value, Capabilities) {
    let expiry = now + Duration::minutes(10);
    let verified = |detail: &str| CapabilityObservation {
        support: Support::Verified,
        evidence_id: format!(
            "cap_{}_{}",
            run_id,
            hash_bytes(detail.as_bytes())[..12].to_owned()
        ),
        observed_at: now,
        valid_until: expiry,
        method: "direct_read_only_observation".into(),
        detail: detail.into(),
    };
    let unknown = |detail: &str| CapabilityObservation {
        support: Support::Unknown,
        evidence_id: format!("cap_{}_unknown", run_id),
        observed_at: now,
        valid_until: expiry,
        method: "no_mutating_probe".into(),
        detail: detail.into(),
    };
    let cash = verified(&format!(
        "demo account currency {}, nonnegative available cash {}",
        snapshot.spendable_cash.currency.as_str(),
        snapshot.spendable_cash.amount
    ));
    let instrument = if instruments.iter().any(|i| i.kind == "STOCK") {
        verified("broker metadata returned stock instruments")
    } else {
        unknown("broker metadata returned no stock instruments")
    };
    let quote = unknown(
        "quote availability requires the reviewed input service; metadata is not quote evidence",
    );
    let market = unknown("order mutation support is not inferred from account API success");
    let limit = unknown("order mutation support is not inferred from account API success");
    let stop = unknown("protective order support requires an explicit broker capability test");
    let cancel = unknown("cancellation support requires an explicit broker capability test");
    let partial = unknown("partial-fill reporting requires a broker fill record");
    let fees = unknown("fee and FX reporting require a genuine fill");
    let capabilities = Capabilities {
        cash_account: cash_to_cap(&cash),
        instrument: cash_to_cap(&instrument),
        fractional_orders: Capability::default(),
        protective_orders: cash_to_cap(&stop),
        quotes: cash_to_cap(&quote),
    };
    let report = json!({
        "schema_version":1,"kind":"broker_capability_discovery","immutable":true,"observed_at":now,
        "environment":config.account.environment,"account_id":config.account.account_id,"snapshot_id":snapshot.snapshot_id,
        "cash_account":cash,"instrument_availability":instrument,"quote_availability":quote,
        "market_orders":market,"limit_orders":limit,"stop_loss":stop,"take_profit":stop,
        "order_cancellation":cancel,"partial_fill_reporting":partial,"fee_reporting":fees,"fx_reporting":fees,
        "entry_authorized":false,"capabilities_for_runtime":capabilities
    });
    (report, capabilities)
}

fn cash_to_cap(value: &CapabilityObservation) -> Capability {
    Capability {
        support: value.support,
        evidence_id: Some(value.evidence_id.clone()),
        valid_until: Some(value.valid_until),
    }
}

#[allow(clippy::too_many_arguments)]
fn review_request(
    root: &Path,
    run_id: &str,
    account_hash: &str,
    market_hash: &str,
    universe_hash: &str,
    models: &Value,
    formula: &Value,
    capabilities: &Value,
    account: &Value,
    market: &Value,
    universe: &Value,
    now: DateTime<Utc>,
) -> Result<(Value, String)> {
    let request = json!({
        "schema_version":1,"kind":"independent_runtime_review_request","run_id":run_id,"created_at":now,
        "artifact_hashes":{"account":account_hash,"market_data":market_hash,"universe":universe_hash,"models":hash_value(models)?,"formula":hash_value(formula)?,"capabilities":hash_value(capabilities)?},
        "questions":["Are the three model boundaries separate and reproducible?","Does the formula preserve protection and risk bounds?","Does the dynamic universe remain broker-intersected and bounded?","Are any claims of execution performance unsupported by the cold-start evidence?"],
        "artifact_payloads":{"account":account,"market_data":market,"universe":universe,"models":models,"formula":formula,"capabilities":capabilities},
        "required_verdict":"APPROVE_SHADOW_ONLY | REJECT | REQUEST_CHANGES","order_authority":false
    });
    let path = root.join("independent-review-request.json");
    let hash = write_value(&path, &request)?;
    Ok((json!({"path":path,"sha256":hash,"request":request}), hash))
}

fn capability_evidence_options(args: &[String]) -> Result<(PathBuf, PathBuf)> {
    let mut config = None;
    let mut file = None;
    let mut i = 0;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or_else(|| Error("missing capability-evidence option value".into()))?;
        match args[i].as_str() {
            "--config" if config.is_none() => config = Some(PathBuf::from(value)),
            "--file" if file.is_none() => file = Some(PathBuf::from(value)),
            _ => {
                return Err(Error(
                    "unknown or duplicate capability-evidence option".into(),
                ));
            }
        }
        i += 2;
    }
    Ok((
        config.ok_or_else(|| Error("capability-evidence requires --config".into()))?,
        file.ok_or_else(|| Error("capability-evidence requires --file".into()))?,
    ))
}

/// Import an operator/reviewer-authored capability record after binding it to
/// the current broker account and exact instrument metadata. The command only
/// persists the record; it never promotes an observation or sends an order.
fn import_capability_evidence(args: &[String]) -> Result<Value> {
    let (config_path, evidence_path) = capability_evidence_options(args)?;
    let config = load(&config_path)?;
    let bytes = bounded_file(&evidence_path, MAX_CONFIG_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error("capability evidence is not UTF-8".into()))?;
    let evidence: ReviewedCapabilityEvidence =
        serde_json::from_value(parse_json_bounded(text, MAX_CONFIG_BYTES)?)
            .map_err(|_| Error("invalid reviewed capability evidence fields".into()))?;
    let (mut reader, broker_id, _currency, _cash) = bootstrap_reader(&config)?;
    let instruments = reader
        .instruments_with_rejections()
        .map_err(|e| Error(e.to_string()))?
        .instruments;
    let broker_instrument = instruments
        .iter()
        .find(|instrument| instrument.instrument_id == evidence.instrument_id)
        .ok_or_else(|| {
            Error("reviewed capability instrument is not in the broker catalog".into())
        })?;
    evidence.validate(&config, broker_id, broker_instrument, Utc::now())?;
    let destination_root =
        resolve(Path::new("."), &config.artifacts.directory).join("capability-evidence");
    fs::create_dir_all(&destination_root)
        .map_err(|_| Error("capability evidence artifact directory unavailable".into()))?;
    let destination = destination_root.join(format!("{}.json", evidence.evidence_id));
    let hash = write_value(
        &destination,
        &serde_json::to_value(&evidence)
            .map_err(|_| Error("capability evidence encoding failed".into()))?,
    )?;
    Ok(json!({
        "status":"CAPABILITY_EVIDENCE_IMPORTED",
        "entry_authorized":false,
        "path":destination,
        "sha256":hash,
        "evidence_id":evidence.evidence_id,
        "broker_account_id":broker_id,
        "instrument_id":evidence.instrument_id,
        "instrument_isin":evidence.instrument_isin,
        "order_mutations":0
    }))
}

const REVIEW_ARRAY_SAMPLE: usize = 8;
const REVIEW_MAX_STRING_CHARS: usize = 8_192;

fn compact_review_value(value: &Value, path: &str, omitted: &mut Vec<Value>) -> Value {
    match value {
        Value::Object(object) => {
            let mut compact = Map::new();
            for (key, child) in object {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                compact.insert(
                    key.clone(),
                    compact_review_value(child, &child_path, omitted),
                );
            }
            Value::Object(compact)
        }
        Value::Array(items) if items.len() > REVIEW_ARRAY_SAMPLE * 4 => {
            let hash = hash_value(value).unwrap_or_default();
            omitted.push(json!({
                "path": path,
                "kind": "array",
                "count": items.len(),
                "sha256": hash
            }));
            let sample = items
                .iter()
                .take(REVIEW_ARRAY_SAMPLE)
                .enumerate()
                .map(|(index, child)| {
                    compact_review_value(child, &format!("{path}[{index}]"), omitted)
                })
                .collect::<Vec<_>>();
            json!({
                "omitted": true,
                "count": items.len(),
                "sha256": hash,
                "sample": sample
            })
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, child)| {
                    compact_review_value(child, &format!("{path}[{index}]"), omitted)
                })
                .collect(),
        ),
        Value::String(text) if text.chars().count() > REVIEW_MAX_STRING_CHARS => {
            let hash = hash_value(value).unwrap_or_default();
            omitted.push(json!({
                "path": path,
                "kind": "string",
                "chars": text.chars().count(),
                "sha256": hash
            }));
            let prefix = text
                .chars()
                .take(REVIEW_MAX_STRING_CHARS)
                .collect::<String>();
            json!({
                "omitted": true,
                "chars": text.chars().count(),
                "sha256": hash,
                "prefix": prefix
            })
        }
        _ => value.clone(),
    }
}

fn selected_fields(value: &Value, fields: &[&str]) -> Value {
    let mut selected = Map::new();
    for field in fields {
        if let Some(child) = value.get(*field) {
            selected.insert((*field).into(), child.clone());
        }
    }
    Value::Object(selected)
}

fn project_review_request(request: &Value) -> Value {
    if request.get("kind").and_then(Value::as_str) != Some("independent_runtime_review_request") {
        return compact_review_request(request);
    }
    let mut projected = Map::new();
    for field in [
        "schema_version",
        "kind",
        "run_id",
        "created_at",
        "artifact_hashes",
        "questions",
        "required_verdict",
        "order_authority",
    ] {
        if let Some(value) = request.get(field) {
            projected.insert(field.into(), value.clone());
        }
    }
    let Some(payloads) = request.get("artifact_payloads").and_then(Value::as_object) else {
        return Value::Object(projected);
    };
    let mut projected_payloads = Map::new();
    if let Some(account) = payloads.get("account") {
        projected_payloads.insert(
            "account".into(),
            selected_fields(
                account,
                &[
                    "schema_version",
                    "kind",
                    "immutable",
                    "environment",
                    "local_account_id",
                    "broker_account_id",
                    "base_currency",
                    "available_cash",
                    "historical_fills",
                    "fees",
                    "fx",
                    "entry_authorized",
                ],
            ),
        );
    }
    if let Some(market) = payloads.get("market_data") {
        projected_payloads.insert(
            "market_data".into(),
            selected_fields(
                market,
                &[
                    "schema_version",
                    "kind",
                    "immutable",
                    "provider",
                    "source_permissions",
                    "instruments",
                    "rows",
                    "duplicate_records",
                    "missing_candle_windows",
                    "first_date",
                    "last_date",
                    "timestamp_consistent",
                    "training_dataset_ready",
                    "deployment_eligible",
                ],
            ),
        );
    }
    if let Some(universe) = payloads.get("universe") {
        let mut selected = selected_fields(
            universe,
            &[
                "schema_version",
                "kind",
                "immutable",
                "source_id",
                "source_url",
                "rights_reference",
                "selection_mode",
                "llm_research_required",
                "broker_instrument_count",
                "broker_typed_instrument_count",
                "broker_metadata_rejected_count",
                "supported_stock_count",
                "excluded_count",
                "candidate_limit",
                "candidate_pool",
                "owner_allowlist",
                "allowlist_status",
                "deployment_eligible",
            ],
        );
        if let Some(excluded) = universe.get("excluded") {
            selected["excluded_summary"] = json!({
                "omitted": true,
                "count": excluded.as_array().map_or(0, Vec::len),
                "sha256": hash_value(excluded).unwrap_or_default()
            });
        }
        projected_payloads.insert("universe".into(), selected);
    }
    if let Some(models) = payloads.get("models").and_then(Value::as_object) {
        let mut projected_models = Map::new();
        for (name, model) in models {
            let mut selected = selected_fields(model, &["path", "sha256", "model_hash"]);
            if let Some(artifact) = model.get("artifact") {
                selected["artifact"] = selected_fields(
                    artifact,
                    &[
                        "schema_version",
                        "kind",
                        "model_id",
                        "status",
                        "approved",
                        "deployment_eligible",
                        "research_only",
                        "artifact_sha256",
                        "dataset_report_sha256",
                        "evaluation_report_sha256",
                        "formula_hash",
                        "market_prediction_model_hash",
                        "execution_cost_model_hash",
                        "labels",
                    ],
                );
            }
            projected_models.insert(name.clone(), selected);
        }
        projected_payloads.insert("models".into(), Value::Object(projected_models));
    }
    if let Some(formula) = payloads.get("formula") {
        let mut selected = selected_fields(formula, &["path", "sha256"]);
        selected["artifact"] = selected_fields(
            formula.get("artifact").unwrap_or(formula),
            &[
                "schema_version",
                "kind",
                "formula_hash",
                "implementation_parity",
                "mathematical_parity",
                "deployment_eligible",
                "approved",
                "status",
                "cost_model_hash",
                "prediction_model_hash",
            ],
        );
        projected_payloads.insert("formula".into(), selected);
    }
    if let Some(capabilities) = payloads.get("capabilities") {
        let report = capabilities.get("report").unwrap_or(capabilities);
        let mut selected = Map::new();
        for name in [
            "cash_account",
            "instrument_availability",
            "fractional_orders",
            "quote_availability",
            "market_orders",
            "limit_orders",
            "stop_loss",
            "take_profit",
            "order_cancellation",
            "partial_fill_reporting",
            "fee_reporting",
            "fx_reporting",
        ] {
            if let Some(value) = report.get(name) {
                selected.insert(
                    name.into(),
                    selected_fields(value, &["support", "evidence_id", "valid_until"]),
                );
            }
        }
        projected_payloads.insert("capabilities".into(), Value::Object(selected));
    }
    projected.insert(
        "artifact_payloads".into(),
        Value::Object(projected_payloads),
    );
    projected.insert(
        "review_transport".into(),
        json!({
            "bounded": true,
            "hashes_are_pinned": true,
            "instruction": "Review only the bounded gate projection. Omitted universe rows remain hash-bound and are not observed detail."
        }),
    );
    Value::Object(projected)
}

fn compact_review_request(request: &Value) -> Value {
    let mut omitted = Vec::new();
    let mut compact = compact_review_value(request, "", &mut omitted);
    if let Some(object) = compact.as_object_mut() {
        object.insert(
            "review_transport".into(),
            json!({
                "bounded": true,
                "hashes_are_pinned": true,
                "omitted_fields": omitted,
                "instruction": "Review bounded summaries against artifact_hashes; omitted content remains hash-bound and must not be treated as observed detail."
            }),
        );
    }
    compact
}

fn review_sections(request: &Value) -> Vec<Value> {
    let projection = project_review_request(request);
    if request.get("kind").and_then(Value::as_str) != Some("independent_runtime_review_request") {
        return vec![projection];
    }
    let common = json!({
        "run_id": projection.get("run_id"),
        "artifact_hashes": projection.get("artifact_hashes"),
        "hashes_are_pinned": true
    });
    let payloads = projection
        .get("artifact_payloads")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let first = json!({
        "focus":"identity, account binding, market data, universe and freshness",
        "common":common,
        "facts":{
            "account":payloads.get("account").map(|account| json!({
                "broker_account_id":account.get("broker_account_id"),
                "environment":account.get("environment"),
                "base_currency":account.get("base_currency"),
                "available_cash":account.get("available_cash"),
                "historical_fill_count":account.get("historical_fills").and_then(|v| v.get("count")),
                "fees_observed":account.get("fees").and_then(|v| v.get("observed")),
                "fx_observed":account.get("fx").and_then(|v| v.get("observed")),
                "entry_authorized":account.get("entry_authorized")
            })),
            "market_data":payloads.get("market_data").map(|market| json!({
                "provider":market.get("provider"),
                "rows":market.get("rows"),
                "first_date":market.get("first_date"),
                "last_date":market.get("last_date"),
                "training_dataset_ready":market.get("training_dataset_ready"),
                "deployment_eligible":market.get("deployment_eligible")
            })),
            "universe":payloads.get("universe").map(|universe| json!({
                "broker_instrument_count":universe.get("broker_instrument_count"),
                "supported_stock_count":universe.get("supported_stock_count"),
                "excluded_count":universe.get("excluded_count"),
                "candidate_pool":universe.get("candidate_pool"),
                "owner_allowlist":universe.get("owner_allowlist"),
                "allowlist_status":universe.get("allowlist_status"),
                "excluded_summary":universe.get("excluded_summary"),
                "deployment_eligible":universe.get("deployment_eligible")
            }))
        }
    });
    let mut models = Map::new();
    if let Some(model_map) = payloads.get("models").and_then(Value::as_object) {
        for (name, model) in model_map {
            models.insert(
                name.clone(),
                selected_fields(
                    model.get("artifact").unwrap_or(&Value::Null),
                    &[
                        "kind",
                        "model_id",
                        "status",
                        "approved",
                        "deployment_eligible",
                        "research_only",
                        "artifact_sha256",
                        "dataset_report_sha256",
                        "evaluation_report_sha256",
                        "formula_hash",
                        "execution_cost_model_hash",
                    ],
                ),
            );
        }
    }
    let mut capabilities = Map::new();
    if let Some(capability_map) = payloads.get("capabilities").and_then(Value::as_object) {
        for (name, capability) in capability_map {
            capabilities.insert(
                name.clone(),
                selected_fields(capability, &["support", "evidence_id", "valid_until"]),
            );
        }
    }
    let second = json!({
        "focus":"models, formula, cost model and broker capabilities",
        "common":common,
        "facts":{
            "models":models,
            "formula":payloads.get("formula"),
            "capabilities":capabilities
        }
    });
    vec![first, second]
}

fn terse_review_section(section: &Value) -> String {
    let focus = section
        .get("focus")
        .and_then(Value::as_str)
        .unwrap_or("gate evidence");
    let hashes = section
        .get("common")
        .and_then(|value| value.get("artifact_hashes"));
    let facts = section
        .get("facts")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if focus.starts_with("identity") {
        let account = facts.get("account").cloned().unwrap_or(Value::Null);
        let market = facts.get("market_data").cloned().unwrap_or(Value::Null);
        let universe = facts.get("universe").cloned().unwrap_or(Value::Null);
        return format!(
            "focus={focus}; hashes={}; account={}; market={}; universe={}",
            serde_json::to_string(hashes.unwrap_or(&Value::Null)).unwrap_or_default(),
            serde_json::to_string(&account).unwrap_or_default(),
            serde_json::to_string(&market).unwrap_or_default(),
            serde_json::to_string(&universe).unwrap_or_default()
        );
    }
    let mut model_status = Map::new();
    if let Some(models) = facts.get("models").and_then(Value::as_object) {
        for (name, model) in models {
            model_status.insert(
                name.clone(),
                selected_fields(
                    model,
                    &["status", "approved", "deployment_eligible", "research_only"],
                ),
            );
        }
    }
    let capabilities = facts
        .get("capabilities")
        .and_then(Value::as_object)
        .map(|items| {
            items
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        value.get("support").cloned().unwrap_or(Value::Null),
                    )
                })
                .collect::<Map<_, _>>()
        })
        .unwrap_or_default();
    format!(
        "focus={focus}; hashes={}; models={}; formula={}; capabilities={}",
        serde_json::to_string(hashes.unwrap_or(&Value::Null)).unwrap_or_default(),
        serde_json::to_string(&model_status).unwrap_or_default(),
        serde_json::to_string(facts.get("formula").unwrap_or(&Value::Null)).unwrap_or_default(),
        serde_json::to_string(&capabilities).unwrap_or_default()
    )
}

fn call_opencode(config: &ColdStartConfig, request: &Value) -> Value {
    if !config.review.enabled {
        return json!({"status":"NOT_REQUESTED","provider":"opencode-go","entry_authorized":false});
    }
    let mut provider_config = config
        .review
        .config_path
        .as_ref()
        .and_then(|p| bounded_file(p, MAX_CONFIG_BYTES).ok())
        .and_then(|b| serde_json::from_slice::<OpenCodeGoConfig>(&b).ok())
        .unwrap_or_default();
    // Runtime review uses bounded summaries and a separate finalization turn.
    // Non-streaming avoids accepting a truncated SSE transcript as a verdict.
    provider_config.stream = false;
    let mut provider = match OpenCodeGoProvider::from_env(provider_config) {
        Ok(p) => p,
        Err(error) => {
            return json!({"status":"UNAVAILABLE","provider":"opencode-go","error":error.to_string(),"entry_authorized":false});
        }
    };
    let cancel = CancellationToken::default();
    let session_id = format!(
        "cold_start_review_{}",
        request
            .get("run_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    let mut findings = Vec::new();
    let mut usages = Vec::new();
    for (index, section) in review_sections(request).into_iter().enumerate() {
        let prompt = format!(
            "Check this small evidence section. Do not reason aloud or use tools. Return at most three short findings immediately; the final turn will decide the verdict.\n{}",
            terse_review_section(&section)
        );
        let analysis_request = ModelRequest {
            session_id: session_id.clone(),
            messages: vec![
                ChatMessage::text("system", "You are a terse deployment gate checker. Do not invent evidence or authorize orders. Emit concise findings immediately.".into()),
                ChatMessage::text("user", prompt),
            ],
            tools: vec![],
            max_output_tokens: 800,
            timeout: StdDuration::from_secs(30),
            native_tools: false,
            force_final: true,
        };
        let analysis = match provider.complete(&analysis_request, &cancel, &mut |_| {}) {
            Ok(completion) => completion,
            Err(error) => {
                return json!({"status":"UNAVAILABLE","provider":"opencode-go","error":error.code,"retryable":error.retryable,"turn":format!("analysis_{}",index + 1),"entry_authorized":false});
            }
        };
        findings.push(analysis.message.content.clone().unwrap_or_default());
        usages.push(analysis.usage);
    }
    let preliminary = findings.join("\n");
    let final_prompt = format!(
        "Finalize immediately. Return exactly one verdict token (APPROVE_SHADOW_ONLY, REJECT, or REQUEST_CHANGES) followed by at most five short reasons. Do not reason aloud. APPROVE_SHADOW_ONLY authorizes shadow evidence only and never an order.\nFindings:\n{}",
        preliminary.chars().take(16_000).collect::<String>()
    );
    let final_request = ModelRequest {
        session_id: session_id.clone(),
        messages: vec![
            ChatMessage::text("system", "You are a terse final deployment gate checker. Output immediately; missing or unavailable evidence remains unavailable and no order is authorized.".into()),
            ChatMessage::text("user", final_prompt),
        ],
        tools: vec![],
        max_output_tokens: 800,
        timeout: StdDuration::from_secs(30),
        native_tools: false,
        force_final: true,
    };
    let final_completion = match provider.complete(&final_request, &cancel, &mut |_| {}) {
        Ok(completion) => Ok((completion, 3u8)),
        Err(error) if error.code == "provider_unfinished_output" => {
            let retry_prompt = format!(
                "Return one verdict token only: APPROVE_SHADOW_ONLY, REJECT, or REQUEST_CHANGES. Then give at most three short reasons. Do not reason aloud. Missing evidence cannot be approved. Facts: {}",
                preliminary.chars().take(6_000).collect::<String>()
            );
            let retry_request = ModelRequest {
                session_id: format!("{session_id}_final_retry"),
                messages: vec![
                    ChatMessage::text("system", "You are a bounded deployment gate checker. Respond immediately with the token and short reasons.".into()),
                    ChatMessage::text("user", retry_prompt),
                ],
                tools: vec![],
                max_output_tokens: 1_600,
                timeout: StdDuration::from_secs(30),
                native_tools: false,
                force_final: true,
            };
            provider
                .complete(&retry_request, &cancel, &mut |_| {})
                .map(|completion| (completion, 4u8))
                .map_err(|_| error)
        }
        Err(error) => Err(error),
    };
    match final_completion {
        Ok(completion) => json!({
            "status":"REVIEWED",
            "provider":"opencode-go",
            "model":provider.identity().model,
            "verdict":completion.0.message.content,
            "preliminary":preliminary,
            "turns":completion.1,
            "usage":{"analysis":usages,"final":completion.0.usage},
            "entry_authorized":false
        }),
        Err(error) => {
            json!({"status":"UNAVAILABLE","provider":"opencode-go","error":error.code,"retryable":error.retryable,"turn":"finalization","preliminary":preliminary,"entry_authorized":false})
        }
    }
}

fn cold_start(args: &[String]) -> Result<Value> {
    let (config_path, requested_review, _) = options(args, false)?;
    let config = load(&config_path)?;
    let now = Utc::now();
    let run_id = format!("cold_start_{}", now.format("%Y%m%dT%H%M%SZ"));
    let root = resolve(&config.artifacts.directory, Path::new(&run_id));
    fs::create_dir_all(&root)
        .map_err(|_| Error("cold-start artifact directory unavailable".into()))?;

    let (mut reader, broker_id, currency, cash) = bootstrap_reader(&config)?;
    require(
        currency == config.account.base_currency,
        "discovered account currency differs from configuration",
    )?;
    let stopped = std::sync::atomic::AtomicBool::new(false);
    let snapshot = super::broker_read::snapshot(&mut reader, &config.account.account_id, &stopped)?;
    let (orders, fills, history_truncated, history_error) = match history(&mut reader) {
        Ok((orders, fills, truncated)) => (orders, fills, truncated, None),
        Err(error) => (Vec::new(), Vec::new(), false, Some(error.to_string())),
    };
    let (instruments, metadata_rejected_count, metadata_error) =
        match reader.instruments_with_rejections() {
            Ok(value) => (value.instruments, value.rejected_count, None),
            Err(error) => (Vec::new(), 0, Some(error.to_string())),
        };
    let account = account_report(
        &config,
        broker_id,
        &currency,
        &cash,
        &snapshot,
        &orders,
        &fills,
        history_truncated,
        history_error.as_deref(),
        now,
    );
    let account_path = root.join("account-snapshot.json");
    let account_hash = write_value(&account_path, &account)?;
    let market = market_report(&config, now)?;
    let market_path = root.join("market-data-report.json");
    let market_hash = write_value(&market_path, &market)?;
    let universe = universe_report(
        &config,
        &instruments,
        metadata_rejected_count,
        metadata_error.as_deref(),
        now,
    );
    let universe_path = root.join("universe-report.json");
    let universe_hash = write_value(&universe_path, &universe)?;
    let (market_model, cost_model, decision_model, formula) =
        seed_artifacts(&config, &root, &market_hash, &account_hash, now)?;
    let models = json!({"market_prediction":market_model,"execution_cost":cost_model,"trading_decision":decision_model});
    let capabilities = capability_report(&config, &instruments, &snapshot, now, &run_id);
    let capability_path = root.join("broker-capabilities.json");
    let capability_hash = write_value(&capability_path, &capabilities.0)?;
    let (review, _review_hash) = review_request(
        &root,
        &run_id,
        &account_hash,
        &market_hash,
        &universe_hash,
        &models,
        &formula,
        &capabilities.0,
        &account,
        &market,
        &universe,
        now,
    )?;
    let mut runtime_review = if requested_review || config.review.enabled {
        call_opencode(&config, review.get("request").unwrap_or(&Value::Null))
    } else {
        json!({"status":"NOT_REQUESTED","provider":"opencode-go","entry_authorized":false})
    };
    if runtime_review.get("status").and_then(Value::as_str) == Some("REVIEWED") {
        let review_path = root.join("independent-runtime-review.json");
        let review_hash = write_value(&review_path, &runtime_review)?;
        runtime_review["path"] = json!(review_path);
        runtime_review["sha256"] = json!(review_hash);
    }
    let summary = RunSummary {
        schema_version: 1,
        run_id,
        status: "INITIALIZED_SHADOW_ONLY".into(),
        entry_authorized: false,
        artifact_directory: root,
        account,
        market_data: json!({"path":market_path,"sha256":market_hash,"report":market}),
        universe: json!({"path":universe_path,"sha256":universe_hash,"report":universe}),
        models,
        formula,
        capabilities: json!({"path":capability_path,"sha256":capability_hash,"report":capabilities.0,"runtime":capabilities.1}),
        runtime_review,
    };
    let summary_value = serde_json::to_value(&summary)
        .map_err(|_| Error("cold-start summary encoding failed".into()))?;
    let summary_path = summary.artifact_directory.join("cold-start-summary.json");
    let summary_hash = write_value(&summary_path, &summary_value)?;
    Ok(
        json!({"status":summary.status,"run_id":summary.run_id,"artifact_directory":summary.artifact_directory,"summary_path":summary_path,"summary_sha256":summary_hash,"entry_authorized":false,"account":summary.account,"market_data":summary.market_data,"universe":summary.universe,"models":summary.models,"formula":summary.formula,"capabilities":summary.capabilities,"runtime_review":summary.runtime_review}),
    )
}

fn first_order(args: &[String]) -> Result<Value> {
    let (config_path, _, approved) = options(args, true)?;
    let config = load(&config_path)?;
    let now = Utc::now();
    let (mut reader, broker_id, currency, cash) = bootstrap_reader(&config)?;
    let stopped = std::sync::atomic::AtomicBool::new(false);
    let snapshot = super::broker_read::snapshot(&mut reader, &config.account.account_id, &stopped)?;
    require(
        snapshot.positions.is_empty(),
        "first-order requires a flat account",
    )?;
    require(
        snapshot.orders.iter().all(|o| o.status.terminal()),
        "first-order requires no pending orders",
    )?;
    let (instruments, metadata_rejected_count, metadata_error) =
        match reader.instruments_with_rejections() {
            Ok(value) => (value.instruments, value.rejected_count, None),
            Err(error) => (Vec::new(), 0, Some(error.to_string())),
        };
    let universe = universe_report(
        &config,
        &instruments,
        metadata_rejected_count,
        metadata_error.as_deref(),
        now,
    );
    let mut blockers = if approved {
        Vec::new()
    } else {
        vec!["manual_approval_required".to_owned()]
    };
    if metadata_error.is_some() {
        blockers.push("broker_instrument_metadata_unavailable".into());
    }
    let root = resolve(&config.artifacts.directory, Path::new("first-order"));
    fs::create_dir_all(&root)
        .map_err(|_| Error("first-order artifact directory unavailable".into()))?;
    let intent_path = root.join(format!("intent_{}.json", now.format("%Y%m%dT%H%M%S%3fZ")));
    let persist_blocked = |instrument: Option<&str>,
                           quote: Option<&Quote>,
                           costs: Option<&Value>,
                           blockers: &[String]|
     -> Result<Value> {
        let intent = json!({
            "schema_version":1,"kind":"supervised_first_order_intent","intent_id":format!("first_order_{}",now.timestamp()),
            "account_id":config.account.account_id,"broker_account_id":broker_id,"instrument_id":instrument,
            "side":Side::Buy,"quantity":null,"entry_limit":null,"stop_loss":null,"take_profit":null,
            "protection_required":config.risk.require_protection,"quote":quote,"costs":costs,
            "account_snapshot":snapshot,"created_at":now,"approval_required":!approved,"approved":approved,
            "blockers":blockers,"order_mutations":0
        });
        let hash = write_value(&intent_path, &intent)?;
        Ok(
            json!({"status":"FIRST_ORDER_BLOCKED","entry_authorized":false,"intent_path":intent_path,"intent_sha256":hash,"blockers":blockers,"approval_required":true,"broker_account_id":broker_id,"instrument_id":instrument,"proposed_order":intent,"order_mutations":0,"note":"An immutable intent was recorded; no broker mutation was sent."}),
        )
    };
    let candidate = universe["candidate_pool"]
        .as_array()
        .and_then(|v| v.first())
        .and_then(|v| v["instrument_id"].as_str());
    let Some(candidate) = candidate else {
        blockers.push("dynamic_universe_produced_no_candidate".into());
        return persist_blocked(None, None, None, &blockers);
    };
    let instrument = InstrumentId::new(candidate)?;
    let broker_instrument = instruments
        .iter()
        .find(|value| value.instrument_id == instrument);
    let reviewed_capabilities = if let Some(broker_instrument) = broker_instrument {
        match load_reviewed_capability_evidence(
            &config,
            config_path.parent().unwrap_or(Path::new(".")),
            broker_id,
            broker_instrument,
            now,
        ) {
            Ok(evidence) => Some(evidence),
            Err(_) => {
                blockers.push("reviewed_capability_evidence_unavailable".into());
                None
            }
        }
    } else {
        blockers.push("broker_instrument_identity_unavailable".into());
        None
    };
    let mut quote = None;
    let mut costs = None;
    if let Some(input_config) = config.inputs.clone() {
        let token = trady_adapters::credentials::optional("TRADY_INPUTS_SERVICE_TOKEN")?
            .map(|v| v.into_inner());
        let trading = trading_config(&config)?;
        match MarketInputs::new(input_config, &trading, token) {
            Ok(mut market) => {
                match market.quote(&instrument) {
                    Ok(value) => quote = Some(value),
                    Err(_) => blockers.push("fresh_quote_unavailable".into()),
                }
                match market.current_exit_costs(&instrument, now) {
                    Ok(value) => costs = value,
                    Err(_) => blockers.push("fresh_execution_costs_unavailable".into()),
                }
            }
            Err(_) => blockers.push("reviewed_input_service_unavailable".into()),
        }
    } else {
        blockers.push("reviewed_input_service_unconfigured".into());
    }
    let capabilities = reviewed_capabilities
        .as_ref()
        .map(|evidence| evidence.capabilities.runtime())
        .unwrap_or_else(|| {
            capability_report(&config, &instruments, &snapshot, now, "first_order").1
        });
    if let Some(evidence) = &reviewed_capabilities {
        for (name, observation) in evidence.capabilities.entry_operations() {
            if observation.support != Support::Verified || observation.valid_until <= now {
                let blocker = match name {
                    "protective_orders" => "protective_orders_unverified",
                    "market_orders" => "market_order_capability_unverified",
                    "limit_orders" => "limit_order_capability_unverified",
                    "order_cancellation" => "order_cancellation_unverified",
                    "partial_fill_reporting" => "partial_fill_reporting_unverified",
                    "fee_reporting" => "fee_reporting_unverified",
                    "fx_reporting" => "fx_reporting_unverified",
                    _ => "broker_capability_unverified",
                };
                blockers.push(blocker.into());
            }
        }
    }
    if !capabilities.cash_account.usable(now) {
        blockers.push("cash_account_unverified".into());
    }
    if !capabilities.instrument.usable(now) {
        blockers.push("instrument_capability_unverified".into());
    }
    if !capabilities.protective_orders.usable(now) {
        blockers.push("protective_orders_unverified".into());
    }
    if !capabilities.quotes.usable(now) {
        blockers.push("quote_capability_unverified".into());
    }
    let Some(quote) = quote else {
        blockers.push("fresh_quote_unavailable".into());
        return persist_blocked(Some(instrument.as_str()), None, None, &blockers);
    };
    require(
        quote.currency == currency
            && quote.instrument_id == instrument
            && quote.ask > quote.bid
            && quote.ask > Decimal::ZERO,
        "fresh quote is incompatible",
    )?;
    let Some(costs) = costs else {
        blockers.push("fresh_execution_costs_unavailable".into());
        return persist_blocked(Some(instrument.as_str()), Some(&quote), None, &blockers);
    };
    let max_value = cash.amount * config.risk.entry_cash_fraction;
    let max_value = max_value.min(config.risk.max_order_value.amount);
    let quantity = (max_value / quote.ask).round_dp(2);
    require(
        quantity > Decimal::ZERO,
        "risk policy produced zero quantity",
    )?;
    if quantity.fract() != Decimal::ZERO
        && !reviewed_capabilities.as_ref().is_some_and(|evidence| {
            evidence.capabilities.fractional_orders.support == Support::Verified
                && evidence.capabilities.fractional_orders.valid_until > now
        })
    {
        blockers.push("fractional_order_capability_unverified".into());
    }
    let stop = quote.ask * (Decimal::ONE - config.risk.max_initial_loss_fraction);
    let target = quote.ask
        * (Decimal::ONE
            + config.risk.max_initial_loss_fraction * Decimal::from(3u8) / Decimal::from(2u8));
    let mut intent = json!({
        "schema_version":1,"kind":"supervised_first_order_intent","intent_id":format!("first_order_{}",now.timestamp()),
        "account_id":config.account.account_id,"broker_account_id":broker_id,"instrument_id":instrument,"side":Side::Buy,
        "quantity":quantity,"entry_limit":quote.ask,"stop_loss":stop,"take_profit":target,"protection_required":config.risk.require_protection,
        "quote":quote,"costs":costs,"account_snapshot":snapshot,"created_at":now,"approval_required":!approved,"approved":approved,
        "order_mutations":0,
        "capability_evidence":reviewed_capabilities.as_ref().map(|evidence| json!({
            "evidence_id":evidence.evidence_id,
            "source_hash":evidence.source_hash,
            "review_id":evidence.review.review_id,
            "review_hash":evidence.review.review_hash,
            "reviewer_id":evidence.review.reviewer_id,
            "instrument_id":evidence.instrument_id,
            "instrument_isin":evidence.instrument_isin,
            "venue":evidence.venue,
            "valid_until":evidence.valid_until
        }))
    });
    let runtime_review = if config.review.enabled {
        call_opencode(&config, &intent)
    } else {
        json!({"status":"NOT_REQUESTED","provider":"opencode-go","entry_authorized":false})
    };
    if runtime_review.get("status").and_then(Value::as_str) != Some("REVIEWED") {
        blockers.push("independent_runtime_review_unavailable".into());
    } else if !runtime_review
        .get("verdict")
        .and_then(Value::as_str)
        .is_some_and(|verdict| verdict.contains("APPROVE_SHADOW_ONLY"))
    {
        blockers.push("independent_runtime_review_not_approved".into());
    }
    intent["runtime_review"] = runtime_review;
    let intent_hash = write_value(&intent_path, &intent)?;
    if !blockers.is_empty() || !approved {
        return Ok(
            json!({"status":"FIRST_ORDER_BLOCKED","entry_authorized":false,"intent_path":intent_path,"intent_sha256":intent_hash,"blockers":blockers,"approval_required":true,"broker_account_id":broker_id,"instrument_id":instrument,"proposed_order":intent,"order_mutations":0,"note":"An intent was recorded; no broker mutation was sent."}),
        );
    }
    // The low-level adapter cannot attach a stop-loss to the entry request.
    // Keep the one-order promise fail-closed until the broker's attached
    // protection capability has been independently verified.
    Err(Error("first-order blocked: this broker adapter has no verified attached-protection mutation; intent recorded for review".into()))
}

fn reconcile(args: &[String]) -> Result<Value> {
    let (config_path, intent_path, broker_order_id) = reconcile_options(args)?;
    let config = load(&config_path)?;
    let intent_bytes = bounded_file(&intent_path, MAX_CONFIG_BYTES)?;
    let intent: Value = serde_json::from_slice(&intent_bytes)
        .map_err(|_| Error("first-order intent is not valid JSON".into()))?;
    require(
        intent.get("account_id").and_then(Value::as_str)
            == Some(config.account.account_id.as_str()),
        "first-order intent account differs from configuration",
    )?;
    require(
        broker_order_id.parse::<i64>().is_ok_and(|id| id > 0),
        "broker order ID must be a positive decimal identifier",
    )?;
    let (mut reader, broker_id, _currency, _cash) = bootstrap_reader(&config)?;
    let stopped = std::sync::atomic::AtomicBool::new(false);
    let snapshot = super::broker_read::snapshot(&mut reader, &config.account.account_id, &stopped)?;
    let pending = reader
        .pending_order(&broker_order_id)
        .map_err(|e| Error(e.to_string()))?;
    let (historical, historical_fills, history_error) = match history(&mut reader) {
        Ok((orders, fills, _)) => (
            orders
                .into_iter()
                .find(|o| o.broker_order_id == broker_order_id),
            fills
                .into_iter()
                .filter(|fill| fill.broker_order_id == broker_order_id)
                .collect::<Vec<_>>(),
            None,
        ),
        Err(error) => (None, Vec::new(), Some(error.to_string())),
    };
    let order = pending.or(historical);
    let order_terminal = order.as_ref().is_some_and(|o| o.status.terminal());
    let filled = order
        .as_ref()
        .filter(|o| o.filled_quantity > Decimal::ZERO)
        .cloned();
    let order_found = order.is_some();
    let fill_observed = filled.is_some();
    let fees_observed = historical_fills.iter().any(|fill| {
        fill.wallet_impact
            .as_ref()
            .is_some_and(|impact| !impact.taxes.is_empty())
    });
    let fx_observed = historical_fills.iter().any(|fill| {
        fill.wallet_impact
            .as_ref()
            .and_then(|impact| impact.fx_rate)
            .is_some()
    });
    let now = Utc::now();
    let mut reconciliation = json!({
        "schema_version":1,"kind":"first_order_reconciliation","immutable":true,"reconciled_at":now,
        "account_id":config.account.account_id,"broker_account_id":broker_id,"broker_order_id":broker_order_id,
        "intent_path":intent_path,"intent_sha256":hash_bytes(&intent_bytes),"account_snapshot":snapshot,
        "broker_order":order,"filled_order":filled,"historical_fills":historical_fills,"order_terminal":order_terminal,
        "history_error":history_error,"fees":{"supported":true,"observed":fees_observed},
        "fx":{"supported":true,"observed":fx_observed},
        "execution_cost":{"status":"UNKNOWN_UNTIL_FILL_PRICE_FEES_FX_AND_COSTS"},
        "execution_record_status":if fill_observed {"FILL_OBSERVED_COST_RECONCILIATION_PENDING"} else if order_found {"ORDER_OBSERVED_FILL_NOT_CONFIRMED"} else {"BROKER_RESPONSE_NOT_FOUND_OUTCOME_UNKNOWN"},
        "entry_authorized":false,"order_mutations":0
    });
    reconciliation["runtime_review"] = if config.review.enabled {
        call_opencode(&config, &reconciliation)
    } else {
        json!({"status":"NOT_REQUESTED","provider":"opencode-go","entry_authorized":false})
    };
    let root = resolve(&config.artifacts.directory, Path::new("first-order"));
    fs::create_dir_all(&root)
        .map_err(|_| Error("first-order artifact directory unavailable".into()))?;
    let path = root.join(format!(
        "reconciliation_{}.json",
        now.format("%Y%m%dT%H%M%S%3fZ")
    ));
    let hash = write_value(&path, &reconciliation)?;
    Ok(
        json!({"status":"RECONCILIATION_RECORDED","entry_authorized":false,"path":path,"sha256":hash,"broker_account_id":broker_id,"broker_order_id":broker_order_id,"execution_record_status":reconciliation["execution_record_status"],"order_mutations":0,"reconciliation":reconciliation}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use trady_domain::{AccountSnapshot, BrokerOrder, Position};

    #[test]
    fn first_order_requires_the_manual_approval_switch() {
        assert!(options(&["--config".into(), "config.json".into()], true).is_err());
        assert!(
            options(
                &[
                    "--config".into(),
                    "config.json".into(),
                    "--require-manual-approval".into()
                ],
                true
            )
            .is_ok()
        );
    }

    #[test]
    fn reconciliation_requires_an_exact_provider_order_identifier() {
        let parsed = reconcile_options(&[
            "--config".into(),
            "cold.json".into(),
            "--intent".into(),
            "intent.json".into(),
            "--broker-order-id".into(),
            "123".into(),
        ])
        .unwrap();
        assert_eq!(parsed.0, PathBuf::from("cold.json"));
        assert_eq!(parsed.1, PathBuf::from("intent.json"));
        assert_eq!(parsed.2, "123");
        assert!(reconcile_options(&["--config".into(), "cold.json".into()]).is_err());
    }

    #[test]
    fn dynamic_universe_never_adds_non_stock_or_excluded_instruments() {
        let config: ColdStartConfig = serde_json::from_value(json!({
            "schema_version":1,
            "account":{"account_id":"paper_test","base_currency":"EUR","environment":"demo","owner_id":"owner"},
            "broker":{"timeout_seconds":10},
            "market_data":{"history_path":"history.json","provider":"provider","source_permissions":"rights"},
            "universe":{"source_id":"source","source_url":"https://source.example/","candidate_limit":1,"preferred_symbols":["GOOD"],"excluded_symbols":["BAD"],"rights_reference":"terms"},
            "risk":{"entry_cash_fraction":"0.02","max_initial_loss_fraction":"0.01","max_order_value":{"amount":"100","currency":"EUR"},"max_daily_orders":1,"require_protection":true},
            "artifacts":{"directory":"var/test"}
        }))
        .unwrap();
        let instruments = vec![
            BrokerInstrument {
                instrument_id: InstrumentId::new("GOOD_US_EQ").unwrap(),
                currency: Currency::new("USD").unwrap(),
                isin: "US1".into(),
                kind: "STOCK".into(),
            },
            BrokerInstrument {
                instrument_id: InstrumentId::new("BAD_US_EQ").unwrap(),
                currency: Currency::new("USD").unwrap(),
                isin: "US2".into(),
                kind: "STOCK".into(),
            },
            BrokerInstrument {
                instrument_id: InstrumentId::new("GOOD_BOND").unwrap(),
                currency: Currency::new("USD").unwrap(),
                isin: "US3".into(),
                kind: "ETF".into(),
            },
        ];
        let report = universe_report(&config, &instruments, 0, None, Utc::now());
        assert_eq!(report["candidate_pool"].as_array().unwrap().len(), 1);
        assert_eq!(report["candidate_pool"][0]["instrument_id"], "GOOD_US_EQ");
        assert_eq!(report["excluded_count"], 2);
        assert_eq!(report["owner_allowlist"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn dynamic_universe_prefers_exact_us_broker_identity_over_prefix_lookalikes() {
        let config: ColdStartConfig = serde_json::from_value(json!({
            "schema_version":1,
            "account":{"account_id":"paper_test","base_currency":"EUR","environment":"demo","owner_id":"owner"},
            "broker":{"timeout_seconds":10},
            "market_data":{"history_path":"history.json","provider":"provider","source_permissions":"rights"},
            "universe":{"source_id":"source","source_url":"https://source.example/","candidate_limit":1,"preferred_symbols":["PG"],"rights_reference":"terms"},
            "risk":{"entry_cash_fraction":"0.02","max_initial_loss_fraction":"0.01","max_order_value":{"amount":"100","currency":"EUR"},"max_daily_orders":1,"require_protection":true},
            "artifacts":{"directory":"var/test"}
        }))
        .unwrap();
        let instruments = vec![
            BrokerInstrument {
                instrument_id: InstrumentId::new("PGC_US_EQ").unwrap(),
                currency: Currency::new("USD").unwrap(),
                isin: "US7046991078".into(),
                kind: "STOCK".into(),
            },
            BrokerInstrument {
                instrument_id: InstrumentId::new("PG_US_EQ").unwrap(),
                currency: Currency::new("USD").unwrap(),
                isin: "US7427181091".into(),
                kind: "STOCK".into(),
            },
        ];
        let report = universe_report(&config, &instruments, 0, None, Utc::now());
        assert_eq!(report["candidate_pool"][0]["instrument_id"], "PG_US_EQ");
        assert_eq!(report["candidate_pool"][0]["isin"], "US7427181091");
    }

    #[test]
    fn capability_discovery_keeps_mutations_and_quotes_unknown() {
        let config: ColdStartConfig = serde_json::from_value(json!({
            "schema_version":1,
            "account":{"account_id":"paper_test","base_currency":"EUR","environment":"demo","owner_id":"owner"},
            "broker":{"timeout_seconds":10},
            "market_data":{"history_path":"history.json","provider":"provider","source_permissions":"rights"},
            "universe":{"source_id":"source","source_url":"https://source.example/","candidate_limit":1,"rights_reference":"terms"},
            "risk":{"entry_cash_fraction":"0.02","max_initial_loss_fraction":"0.01","max_order_value":{"amount":"100","currency":"EUR"},"max_daily_orders":1,"require_protection":true},
            "artifacts":{"directory":"var/test"}
        }))
        .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap();
        let snapshot = AccountSnapshot {
            snapshot_id: "snapshot".into(),
            account_id: config.account.account_id.clone(),
            mode: Mode::Paper,
            spendable_cash: Money {
                amount: Decimal::new(5000, 0),
                currency: Currency::new("EUR").unwrap(),
            },
            positions: Vec::<Position>::new(),
            orders: Vec::<BrokerOrder>::new(),
            observed_at: now,
        };
        let (report, runtime) = capability_report(&config, &[], &snapshot, now, "test");
        assert_eq!(report["market_orders"]["support"], "UNKNOWN");
        assert_eq!(report["stop_loss"]["support"], "UNKNOWN");
        assert_eq!(runtime.quotes.support, Support::Unknown);
        assert_eq!(runtime.protective_orders.support, Support::Unknown);
        assert_eq!(runtime.cash_account.support, Support::Verified);
    }

    #[test]
    fn reviewed_capability_evidence_binds_the_exact_broker_isin() {
        let config: ColdStartConfig = serde_json::from_value(json!({
            "schema_version":1,
            "account":{"account_id":"paper_test","base_currency":"EUR","environment":"demo","owner_id":"owner"},
            "broker":{"timeout_seconds":10},
            "market_data":{"history_path":"history.json","provider":"provider","source_permissions":"rights"},
            "universe":{"source_id":"source","source_url":"https://source.example/","candidate_limit":1,"rights_reference":"terms"},
            "risk":{"entry_cash_fraction":"0.02","max_initial_loss_fraction":"0.01","max_order_value":{"amount":"100","currency":"EUR"},"max_daily_orders":1,"require_protection":true},
            "artifacts":{"directory":"var/test"}
        }))
        .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap();
        let observation = |support| {
            json!({
                "support":support,"evidence_id":"cap-review-1","observed_at":"2026-11-01T11:00:00Z",
                "valid_until":"2026-11-01T13:00:00Z","method":"reviewed_provider_evidence","detail":"account-bound evidence"
            })
        };
        let mut capabilities = serde_json::Map::new();
        for name in [
            "cash_account",
            "instrument",
            "fractional_orders",
            "quotes",
            "market_orders",
            "limit_orders",
            "protective_orders",
            "order_cancellation",
            "partial_fill_reporting",
            "fee_reporting",
            "fx_reporting",
        ] {
            capabilities.insert(name.into(), observation("VERIFIED"));
        }
        let evidence: ReviewedCapabilityEvidence = serde_json::from_value(json!({
            "schema_version":1,"kind":"broker_capability_evidence","evidence_id":"capability-review-1",
            "account_id":"paper_test","broker_account_id":52211463,"environment":"demo",
            "instrument_id":"PGC_US_EQ","instrument_isin":"US7427181091","currency":"USD","venue":"XNAS",
            "provider":"broker","source_hash":"a".repeat(64),"observed_at":"2026-11-01T11:00:00Z","valid_until":"2026-11-01T13:00:00Z",
            "review":{"reviewer_id":"reviewer-1","review_id":"review-1","reviewed_at":"2026-11-01T11:05:00Z","review_hash":"b".repeat(64),"independent":true},
            "capabilities":capabilities
        }))
        .unwrap();
        let broker = BrokerInstrument {
            instrument_id: InstrumentId::new("PGC_US_EQ").unwrap(),
            currency: Currency::new("USD").unwrap(),
            isin: "US7427181091".into(),
            kind: "STOCK".into(),
        };
        assert!(evidence.validate(&config, 52211463, &broker, now).is_ok());
        let mismatched = BrokerInstrument {
            isin: "US7046991078".into(),
            ..broker
        };
        assert!(
            evidence
                .validate(&config, 52211463, &mismatched, now)
                .is_err()
        );
    }

    #[test]
    fn capability_evidence_options_require_both_paths() {
        assert!(capability_evidence_options(&["--config".into(), "cold.json".into()]).is_err());
        let (config, file) = capability_evidence_options(&[
            "--config".into(),
            "cold.json".into(),
            "--file".into(),
            "evidence.json".into(),
        ])
        .unwrap();
        assert_eq!(config, PathBuf::from("cold.json"));
        assert_eq!(file, PathBuf::from("evidence.json"));
    }

    #[test]
    fn opencode_review_compacts_large_arrays_with_hashes() {
        let excluded = (0..100)
            .map(|index| json!({"instrument_id":format!("TEST{index}_US_EQ"),"kind":"ETF"}))
            .collect::<Vec<_>>();
        let request = json!({
            "artifact_hashes":{"universe":"a".repeat(64)},
            "artifact_payloads":{"universe":{"excluded":excluded,"candidate_pool":[{"instrument_id":"PGC_US_EQ"}]}}
        });
        let compact = compact_review_request(&request);
        let text = serde_json::to_string(&compact).unwrap();
        assert!(text.len() < 20_000);
        assert_eq!(
            compact["artifact_payloads"]["universe"]["excluded"]["omitted"],
            true
        );
        assert_eq!(
            compact["artifact_payloads"]["universe"]["excluded"]["count"],
            100
        );
        assert_eq!(compact["review_transport"]["hashes_are_pinned"], true);
        assert_eq!(
            compact["artifact_payloads"]["universe"]["candidate_pool"][0]["instrument_id"],
            "PGC_US_EQ"
        );
    }

    #[test]
    fn opencode_review_projection_keeps_gate_fields_without_full_artifact_payloads() {
        let excluded = (0..6_769)
            .map(|index| json!({"instrument_id":format!("TEST{index}_US_EQ"),"kind":"ETF"}))
            .collect::<Vec<_>>();
        let request = json!({
            "schema_version":1,
            "kind":"independent_runtime_review_request",
            "run_id":"cold_start_test",
            "artifact_hashes":{"universe":"a".repeat(64)},
            "questions":["review"],
            "required_verdict":"APPROVE_SHADOW_ONLY | REJECT | REQUEST_CHANGES",
            "order_authority":false,
            "artifact_payloads":{
                "universe":{"excluded":excluded,"excluded_count":6769,"candidate_pool":[{"instrument_id":"PGC_US_EQ"}],"deployment_eligible":false},
                "models":{"market_prediction":{"artifact":{"status":"SHADOW_ONLY","approved":false}}},
                "capabilities":{"quotes":{"support":"UNKNOWN"}}
            }
        });
        let projection = project_review_request(&request);
        let text = serde_json::to_string(&projection).unwrap();
        assert!(text.len() < 10_000);
        assert_eq!(
            projection["artifact_payloads"]["universe"]["excluded_summary"]["count"],
            6769
        );
        assert_eq!(
            projection["artifact_payloads"]["universe"]["candidate_pool"][0]["instrument_id"],
            "PGC_US_EQ"
        );
        assert_eq!(
            projection["artifact_payloads"]["models"]["market_prediction"]["artifact"]["status"],
            "SHADOW_ONLY"
        );
        assert_eq!(projection["review_transport"]["hashes_are_pinned"], true);
    }
}

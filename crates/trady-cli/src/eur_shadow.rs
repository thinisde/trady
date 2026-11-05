//! Durable EUR SHADOW deployment. Capture runs as analyst, the coordinator as
//! executor, and delivery as gateway. No broker executor is imported here.
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};
use trady_adapters::{
    credentials::CredentialStore,
    ibkr_smart::IbkrSmartConfig,
    telegram::{BotToken, TelegramClient},
    trading212::{Credentials, Environment, Trading212Config, Trading212Reader},
    yfinance::{YfinanceDataConfig, YfinanceMarketData},
};
use trady_application::{
    eur_runtime::EurRuntimeConfig,
    eur_shadow::{
        ObservationStatus, ProviderObservation, ShadowCrosswalk, ShadowSettings, assess_quote,
    },
    harness::{content_hash, new_id},
    telegram::{LifecycleEvent, LifecycleEventKind},
};
use trady_domain::{
    CoordinatorEvent, CoordinatorEventKind, CoordinatorState, Error, ExecutionStage, Result,
    require,
};
use trady_storage::target::{PgTargetOutbox, PgTargetStore};

const MAX_FILE: u64 = 8 * 1024 * 1024;
const LEASE_SECONDS: i32 = 180;

#[derive(Clone)]
struct Loaded {
    config: EurRuntimeConfig,
    settings: ShadowSettings,
    crosswalk: ShadowCrosswalk,
    config_path: PathBuf,
    config_hash: String,
    crosswalk_hash: String,
    cycle_id: String,
}

pub fn configured(path: &str) -> Result<bool> {
    let value: Value = read_json(Path::new(path))?;
    Ok(value.get("shadow").is_some_and(|value| !value.is_null()))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(MAX_FILE + 1).read_to_end(&mut bytes))
        .map_err(|_| Error("EUR configuration or evidence file unavailable".into()))?;
    require(
        bytes.len() as u64 <= MAX_FILE,
        "EUR input exceeds size bound",
    )?;
    serde_json::from_slice(&bytes)
        .map_err(|_| Error("invalid EUR configuration or evidence fields".into()))
}

pub(super) fn resolve(config: &Path, reference: &str) -> PathBuf {
    let value = Path::new(reference);
    if value.is_absolute() {
        return value.to_owned();
    }
    if reference.starts_with("config/") || reference.starts_with("docs/") {
        config
            .parent()
            .unwrap_or(Path::new("."))
            .parent()
            .unwrap_or(Path::new("."))
            .join(value)
    } else {
        config.parent().unwrap_or(Path::new(".")).join(value)
    }
}

fn hash<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(content_hash(
        &serde_json::to_string(value).map_err(|_| Error("EUR hash encoding failed".into()))?,
    ))
}

fn load(path: &str) -> Result<Loaded> {
    let config_path = PathBuf::from(path);
    let config: EurRuntimeConfig = read_json(&config_path)?;
    config.validate(Utc::now())?;
    require(
        config.execution_stage == ExecutionStage::Shadow
            && !config.permits_broker_mutation()
            && config.telegram_enabled,
        "durable shadow command requires mutation-free SHADOW and Telegram",
    )?;
    let settings = config
        .shadow
        .clone()
        .ok_or_else(|| Error("durable shadow settings missing".into()))?;
    settings.validate()?;
    require(
        settings.ibkr_config.is_some(),
        "IBKR SMART Live configuration is required by the EUR source policy; quote fallback is prohibited",
    )?;
    let crosswalk_value: Value = read_json(&resolve(&config_path, &settings.crosswalk_config))?;
    let crosswalk: ShadowCrosswalk = serde_json::from_value(crosswalk_value.clone())
        .map_err(|_| Error("invalid shadow crosswalk".into()))?;
    crosswalk.validate(Utc::now())?;
    let crosswalk_hash = hash(&crosswalk_value)?;
    // Configuration authority includes referenced file contents, so changing a
    // provider binding or source registry cannot reuse an old daily decision.
    let mut provider_hashes = std::collections::BTreeMap::new();
    for (name, reference) in [
        ("yfinance", config.yfinance_config.as_deref()),
        ("research", config.research_config.as_deref()),
        ("broker", config.trading212_config.as_deref()),
        ("opencode", config.opencode_config.as_deref()),
    ] {
        let reference =
            reference.ok_or_else(|| Error("required EUR provider configuration missing".into()))?;
        let document: Value = read_json(&resolve(&config_path, reference))?;
        provider_hashes.insert(name, hash(&document)?);
    }
    for (name, reference) in [
        ("risk", settings.risk_config.as_deref()),
        ("universe", config.universe_config.as_deref()),
        ("current_bars", settings.current_bars_config.as_deref()),
    ] {
        if let Some(reference) = reference {
            let document: Value = read_json(&resolve(&config_path, reference))?;
            provider_hashes.insert(name, hash(&document)?);
        }
    }
    if let Some(reference) = &settings.current_bars_config {
        let bars: trady_adapters::ibkr_bars::CurrentBarsConfig =
            read_json(&resolve(&config_path, reference))?;
        bars.validate()?;
        require(bars.enabled, "current-bar source disabled")?;
    }
    if let Some(reference) = &settings.ibkr_config {
        let document: Value = read_json(&resolve(&config_path, reference))?;
        let ibkr: IbkrSmartConfig = serde_json::from_value(document.clone())
            .map_err(|_| Error("invalid IBKR shadow source configuration".into()))?;
        for identity in &crosswalk.members {
            ibkr.validate_identity(identity)?;
        }
        provider_hashes.insert("ibkr", hash(&document)?);
    }
    let config_hash = hash(&json!({"runtime":config,"crosswalk_hash":crosswalk_hash,
        "provider_configuration_hashes":provider_hashes}))?;
    let cycle_id = settings.cycle_id(&config_hash, Utc::now())?;
    Ok(Loaded {
        config,
        settings,
        crosswalk,
        config_path,
        config_hash,
        crosswalk_hash,
        cycle_id,
    })
}

fn store(loaded: &Loaded, role: &str, credential: &str) -> Result<PgTargetStore> {
    PgTargetStore::connect(
        &super::secret(credential)?,
        role,
        loaded.config.owner_id.clone(),
        loaded.config.account_id.to_string(),
    )
}

fn observation_id(loaded: &Loaded, slot: &str) -> String {
    if slot.starts_with("alpha_history_") && loaded.settings.decision_interval_seconds.is_some() {
        return format!(
            "eur_{}_{}_{}_{}",
            loaded.settings.deployment_id,
            Utc::now().format("%Y%m%d"),
            &loaded.config_hash[..12],
            slot
        );
    }
    format!("{}_{}", loaded.cycle_id, slot)
}

pub(super) fn current_result(path: &str) -> Result<Option<Value>> {
    let loaded = load(path)?;
    latest_result(
        &loaded,
        &mut store(&loaded, "trady_executor", "TRADY_EXECUTOR_DSN")?,
    )
}
pub(super) fn configuration_hash(path: &str) -> Result<String> {
    Ok(load(path)?.config_hash)
}

fn recent_cursor(loaded: &Loaded, repository: &mut PgTargetStore) -> Result<Option<Value>> {
    if loaded.settings.decision_interval_seconds.is_none() {
        return Ok(None);
    }
    let rows = repository.list_documents(
        "research_cycle_cursor",
        Utc::now() - chrono::Duration::days(2),
        1000,
    )?;
    let rows = rows
        .as_array()
        .ok_or_else(|| Error("EUR research cursor list invalid".into()))?;
    require(rows.len() < 1000, "EUR research cursor list truncated")?;
    Ok(rows
        .iter()
        .rev()
        .find(|row| row["document"]["configuration_hash"] == loaded.config_hash)
        .map(|row| row["document"].clone()))
}

fn latest_result(loaded: &Loaded, repository: &mut PgTargetStore) -> Result<Option<Value>> {
    if loaded.settings.decision_interval_seconds.is_none() {
        return repository.read_document("shadow_result", &loaded.cycle_id);
    }
    let since = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| Error("invalid EUR day".into()))?
        .and_utc();
    let rows = repository.list_documents("shadow_result", since, 1000)?;
    let rows = rows
        .as_array()
        .ok_or_else(|| Error("EUR result list invalid".into()))?;
    require(rows.len() < 1000, "EUR result list truncated")?;
    Ok(rows
        .iter()
        .rev()
        .find(|row| row["document"]["configuration_hash"] == loaded.config_hash)
        .map(|row| row["document"].clone()))
}

fn observe<F>(
    store: &mut PgTargetStore,
    loaded: &Loaded,
    slot: &str,
    provider: &str,
    endpoint: &str,
    rights: &str,
    fetch: F,
) -> Result<ProviderObservation>
where
    F: FnOnce() -> Result<Value>,
{
    let id = observation_id(loaded, slot);
    if let Some(value) = store.read_document("provider_snapshot", &id)? {
        let existing: ProviderObservation = serde_json::from_value(value)
            .map_err(|_| Error("invalid durable EUR provider observation".into()))?;
        existing.validate()?;
        return Ok(existing);
    }
    let requested_at = Utc::now();
    let (response, status, error_code) = match fetch() {
        Ok(response)
            if response.get("provider_status").and_then(Value::as_str) == Some("UNAVAILABLE") =>
        {
            let code = response
                .get("error_code")
                .and_then(Value::as_str)
                .unwrap_or("PROVIDER_UNAVAILABLE")
                .to_owned();
            (response, ObservationStatus::Unavailable, Some(code))
        }
        Ok(response)
            if response.get("provider_status").and_then(Value::as_str) == Some("STALE") =>
        {
            let code = response
                .get("error_code")
                .and_then(Value::as_str)
                .unwrap_or("PROVIDER_STALE")
                .to_owned();
            (response, ObservationStatus::Stale, Some(code))
        }
        Ok(response) if response.get("fresh").and_then(Value::as_bool) == Some(false) => (
            response,
            ObservationStatus::Stale,
            Some("QUOTE_STALE".into()),
        ),
        Ok(response) => (response, ObservationStatus::Ok, None),
        Err(_) => (
            json!({"untrusted_error_detail_omitted":true}),
            ObservationStatus::Unavailable,
            Some("PROVIDER_UNAVAILABLE".into()),
        ),
    };
    let received_at = Utc::now();
    let namespace = hash(&json!([loaded.config.owner_id, loaded.config.account_id]))?;
    let (response, parts) = chunk_payload(response, &namespace)?;
    for (part_id, part) in parts {
        store.append_document("provider_snapshot", &part_id, &part)?;
    }
    let value = ProviderObservation {
        observation_id: id.clone(),
        provider: provider.into(),
        endpoint: endpoint.into(),
        source_id: slot.into(),
        requested_at,
        received_at,
        published_at: None,
        response_hash: hash(&response)?,
        rights_reference: rights.into(),
        status,
        error_code,
        capture_level: "PROVIDER_OR_LIBRARY_RESPONSE".into(),
        response,
    };
    value.validate()?;
    store.append_document(
        "provider_snapshot",
        &id,
        &serde_json::to_value(&value)
            .map_err(|_| Error("EUR observation encoding failed".into()))?,
    )?;
    Ok(value)
}

fn chunk_payload(response: Value, namespace: &str) -> Result<(Value, Vec<(String, Value)>)> {
    if serde_json::to_vec(&response)
        .map_err(|_| Error("provider payload encoding failed".into()))?
        .len()
        <= 3_000_000
    {
        return Ok((response, Vec::new()));
    }
    let full_hash = hash(&response)?;
    let rows = response
        .as_array()
        .ok_or_else(|| Error("oversized provider object cannot be stored losslessly".into()))?;
    let mut parts = Vec::new();
    let mut chunk = Vec::new();
    let mut bytes = 0usize;
    let mut push = |chunk: Vec<Value>| -> Result<()> {
        let value = Value::Array(chunk);
        let response_hash = hash(&value)?;
        let id = format!("payload_{}", hash(&json!([namespace, response_hash]))?);
        parts.push((id, json!({"response_hash":response_hash,"response":value})));
        Ok(())
    };
    for row in rows {
        let size = serde_json::to_vec(row)
            .map_err(|_| Error("provider row encoding failed".into()))?
            .len()
            + 1;
        require(
            size <= 1_000_000,
            "individual provider row exceeds durable payload bound",
        )?;
        if bytes + size > 1_000_000 && !chunk.is_empty() {
            push(std::mem::take(&mut chunk))?;
            bytes = 0;
        }
        bytes += size;
        chunk.push(row.clone());
    }
    if !chunk.is_empty() {
        push(chunk)?;
    }
    let ids: Vec<_> = parts.iter().map(|(id, _)| id.clone()).collect();
    Ok((
        json!({"encoding":"JSON_ARRAY_CHUNKS_V1","row_count":rows.len(),
        "response_hash":full_hash,"parts":ids}),
        parts,
    ))
}

fn materialize(repository: &mut PgTargetStore, observation: &ProviderObservation) -> Result<Value> {
    observation.validate()?;
    if observation.response["encoding"] != "JSON_ARRAY_CHUNKS_V1" {
        return Ok(observation.response.clone());
    }
    let parts = observation.response["parts"]
        .as_array()
        .filter(|parts| parts.len() <= 128)
        .ok_or_else(|| Error("invalid provider chunk manifest".into()))?;
    let mut rows = Vec::new();
    for id in parts {
        let id = id
            .as_str()
            .ok_or_else(|| Error("invalid provider chunk ID".into()))?;
        let part = repository
            .read_document("provider_snapshot", id)?
            .ok_or_else(|| Error("durable provider payload chunk missing".into()))?;
        require(
            part["response_hash"].as_str() == Some(&hash(&part["response"])?),
            "provider chunk hash mismatch",
        )?;
        rows.extend(
            part["response"]
                .as_array()
                .ok_or_else(|| Error("invalid provider chunk array".into()))?
                .iter()
                .cloned(),
        );
    }
    require(
        observation.response["row_count"].as_u64() == Some(rows.len() as u64)
            && observation.response["response_hash"].as_str() == Some(&hash(&rows)?),
        "provider catalog reconstruction differs",
    )?;
    Ok(Value::Array(rows))
}

fn provider_path(loaded: &Loaded, value: Option<&str>, name: &str) -> Result<PathBuf> {
    Ok(resolve(
        &loaded.config_path,
        value.ok_or_else(|| Error(format!("{name} configuration is missing")))?,
    ))
}

pub fn command(action: &str, args: &[String]) -> Result<Value> {
    require(
        (args.len() == 2 || (args.len() == 3 && args[2] == "--once")) && args[0] == "--config",
        "usage: trady eur capture|diagnose-inputs|daily|gateway|report --config FILE [--once]",
    )?;
    let once = args.len() == 3;
    match action {
        "capture" => repeat(&args[1], once, load, capture, |l| {
            l.settings.interval_seconds
        }),
        "daily" => run_once(&load(&args[1])?),
        "diagnose-inputs" => {
            let mut loaded = load(&args[1])?;
            // Operator diagnostics capture new evidence without replacing an
            // immutable daily decision or enqueueing another daily summary.
            loaded.cycle_id = new_id("eur_input_probe");
            let mut result = capture(&loaded)?;
            result["diagnostic_only"] = json!(true);
            Ok(result)
        }
        "gateway" => repeat(&args[1], once, load_gateway, gateway, |s| {
            s.interval_seconds
        }),
        "report" => report(&load(&args[1])?),
        _ => Err(Error("unknown EUR shadow command".into())),
    }
}

fn repeat<T>(
    path: &str,
    once: bool,
    loader: fn(&str) -> Result<T>,
    action: fn(&T) -> Result<Value>,
    interval: fn(&T) -> u32,
) -> Result<Value> {
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = stopped.clone();
    ctrlc::set_handler(move || stop.store(true, std::sync::atomic::Ordering::Relaxed))
        .map_err(|_| Error("cannot install EUR service stop handler".into()))?;
    loop {
        let loaded = loader(path)?;
        let result = action(&loaded);
        if once {
            return result;
        }
        match result {
            Ok(value) => super::output(value)?,
            Err(error) => super::output(
                json!({"status":"EUR_SERVICE_DEGRADED","error":error.to_string(),"entry_authorized":false}),
            )?,
        }
        for _ in 0..interval(&loaded) * 10 {
            if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(json!({"status":"STOPPED"}));
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

fn capture(loaded: &Loaded) -> Result<Value> {
    require(loaded.config.enabled, "EUR shadow deployment is disabled")?;
    let mut repository = store(loaded, "trady_analyst", "TRADY_ANALYST_DSN")?;
    let mut selected = loaded.clone();
    if let Some(cursor) = recent_cursor(loaded, &mut repository)? {
        selected.cycle_id = cursor["cycle_id"]
            .as_str()
            .ok_or_else(|| Error("EUR research cursor missing cycle".into()))?
            .into();
    }
    let loaded = &selected;
    process_research_request(loaded, &mut repository)?;
    // Freeze daily research only after the reviewed primary session opens.
    // Midnight capture would otherwise freeze yesterday's incomplete daily
    // bar projection and consume the day's source slots before entry time.
    if !loaded.cycle_id.starts_with("eur_input_probe_") {
        let calendar = repository.read_document(
            "provider_snapshot",
            &observation_id(loaded, "broker_sessions"),
        )?;
        let Some(calendar) = calendar else {
            return Ok(
                json!({"status":"EUR_ANALYST_WAITING_FOR_BROKER_CALENDAR","cycle_id":loaded.cycle_id,"entry_authorized":false}),
            );
        };
        let calendar: ProviderObservation = serde_json::from_value(calendar)
            .map_err(|_| Error("invalid captured session calendar".into()))?;
        calendar.validate()?;
        let now = Utc::now();
        let sessions: Result<Vec<_>> = loaded
            .crosswalk
            .members
            .iter()
            .map(|identity| {
                trady_application::eur_shadow::session_from_catalog(
                    identity,
                    &loaded.config.session_deadlines,
                    &calendar.response,
                    calendar.received_at,
                    now,
                )
            })
            .collect();
        let sessions = sessions?;
        if sessions
            .iter()
            .all(|s| now < s.opens_at || now >= s.flatten_at)
        {
            return Ok(
                json!({"status":"EUR_ANALYST_WAITING_FOR_PRIMARY_SESSION","cycle_id":loaded.cycle_id,"entry_authorized":false}),
            );
        }
    }
    let data_path = provider_path(loaded, loaded.config.yfinance_config.as_deref(), "Yahoo")?;
    let data_config: YfinanceDataConfig = read_json(&data_path)?;
    for identity in &loaded.crosswalk.members {
        let binding = data_config
            .bindings
            .get(&identity.internal_instrument_id)
            .ok_or_else(|| Error("reviewed Yahoo broker binding is missing".into()))?;
        require(
            binding.broker_instrument_id == identity.broker_ticker
                && binding.isin == identity.isin
                && binding.exchange_mic == identity.exchange_mic
                && data_config.symbols.get(&identity.internal_instrument_id)
                    == Some(&identity.yahoo_symbol),
            "Yahoo configuration differs from reviewed shadow identity",
        )?;
    }
    let data = YfinanceMarketData::new(data_config)?;
    let mut observations = Vec::new();
    let stream = observe(
        &mut repository,
        loaded,
        "yf_stream",
        "yfinance_rs",
        "wss://streamer.finance.yahoo.com/",
        "PRIVATE_SHADOW_LAST_TRADE_RESEARCH_ONLY",
        || data.capture_stream(10),
    )?;
    observations.push(stream.observation_id);
    let mut budget = trady_storage::research::PgResearchRepository::connect(&super::secret(
        "TRADY_ANALYST_DSN",
    )?)?;
    for identity in &loaded.crosswalk.members {
        let id = identity.internal_instrument_id.as_str();
        for kind in ["bars", "news"] {
            let slot = format!("yf_{kind}_{id}");
            let observation = observe(
                &mut repository,
                loaded,
                &slot,
                "yfinance_rs",
                if kind == "bars" {
                    "https://query1.finance.yahoo.com/v8/finance/chart"
                } else {
                    "https://query1.finance.yahoo.com/v1/finance/search"
                },
                "PRIVATE_SHADOW_RESEARCH_METADATA_ONLY",
                || {
                    if kind == "bars" {
                        serde_json::to_value(data.completed_bars(
                            &identity.internal_instrument_id,
                            loaded.settings.bar_interval_seconds.unwrap_or(300),
                            Utc::now(),
                        )?)
                        .map_err(|_| Error("Yahoo bar encoding failed".into()))
                    } else {
                        serde_json::to_value(
                            data.latest_news(&identity.internal_instrument_id, Utc::now())?,
                        )
                        .map_err(|_| Error("Yahoo news encoding failed".into()))
                    }
                },
            )?;
            observations.push(observation.observation_id);
        }
        let slot = format!("alpha_history_{id}");
        let source = trady_application::research::SourceConfig {
            id: slot.clone(),
            kind: trady_domain::research::SourceKind::AlphaVantage,
            enabled: true,
            url: "https://www.alphavantage.co/query".into(),
            allowed_hosts: vec!["www.alphavantage.co".into()],
            credential_env: Some("TRADY_ALPHA_VANTAGE_API_KEY".into()),
            interval_seconds: 86400,
            daily_request_limit: 20,
            query: None,
            x_account: None,
            x_rule: None,
            rights: trady_domain::research::SourceRights {
                store_content: false,
                llm_processing: false,
                retention_hours: 24,
                terms_reference: "ALPHA_PRIVATE_HISTORICAL_CROSSCHECK_SHARED_BUDGET".into(),
            },
        };
        let observation = observe(
            &mut repository,
            loaded,
            &slot,
            "alpha_vantage",
            &source.url,
            &source.rights.terms_reference,
            || {
                use trady_application::research::ResearchRepository;
                let state =
                    trady_application::research::prepare_state(&source, budget.state(&source.id)?)?;
                let (_, allowed) = trady_application::research::reserve_source_requests(
                    &source,
                    &mut budget,
                    state,
                    Utc::now(),
                    1,
                )?;
                require(allowed, "shared Alpha Vantage request budget exhausted")?;
                trady_adapters::alpha_vantage::daily_history(
                    &identity.alpha_vantage_symbol,
                    &super::secret("TRADY_ALPHA_VANTAGE_API_KEY")?,
                )
            },
        )?;
        observations.push(observation.observation_id);
    }
    let research_path =
        provider_path(loaded, loaded.config.research_config.as_deref(), "research")?;
    let research: trady_application::research::ResearchConfig = read_json(&research_path)?;
    research.validate()?;
    for source in &research.sources {
        let slot = format!("research_{}", source.id);
        let observation = observe(
            &mut repository,
            loaded,
            &slot,
            "research_collector",
            &source.url,
            &source.rights.terms_reference,
            || {
                if !source.enabled || source.activation_gap(Utc::now()).is_some() {
                    return Err(Error(
                        "research source is unavailable under its reviewed scope".into(),
                    ));
                }
                let path = research_path.to_string_lossy().into_owned();
                let collection = super::news::run(&[
                    "collect".into(),
                    "--once".into(),
                    "--source".into(),
                    source.id.clone(),
                    "--config".into(),
                    path.clone(),
                ])?;
                let status = collection
                    .pointer("/reports/0/status")
                    .and_then(Value::as_str);
                require(
                    matches!(status, Some("COMPLETE" | "PARTIAL" | "DEFERRED")),
                    "research collection unavailable",
                )?;
                let evidence = if source.rights.llm_processing {
                    super::news::run(&[
                        "read".into(),
                        "--source".into(),
                        source.id.clone(),
                        "--config".into(),
                        path,
                        "--limit".into(),
                        "20".into(),
                        "--for-model".into(),
                    ])?
                } else {
                    json!({"items":[],"processing_blocked":"SOURCE_LLM_RIGHTS_NOT_APPROVED"})
                };
                Ok(json!({"collection":collection,"model_permitted_evidence":evidence}))
            },
        )?;
        observations.push(observation.observation_id);
    }
    let manifest = json!({"schema_version":1,"cycle_id":loaded.cycle_id,"configuration_hash":loaded.config_hash,
        "crosswalk_hash":loaded.crosswalk_hash,"observation_ids":observations,"entry_authorized":false});
    repository.append_document(
        "provider_snapshot",
        &observation_id(loaded, "analyst_manifest"),
        &manifest,
    )?;
    // The executor may finish after the pre-open daily timer returns. The next
    // analyst cycle creates the immutable, sanitized daily review handoff.
    let handoff = if repository
        .read_document("daily_report", &loaded.cycle_id)?
        .is_some()
    {
        Some(super::eur_workflow::run(
            "handoff",
            &[
                "--config".into(),
                loaded.config_path.to_string_lossy().into_owned(),
            ],
        )?)
    } else {
        None
    };
    Ok(
        json!({"status":"EUR_ANALYST_INPUTS_PERSISTED","cycle_id":loaded.cycle_id,
        "observations":observations.len(),"review_handoff":handoff,"entry_authorized":false}),
    )
}

fn process_research_request(loaded: &Loaded, repository: &mut PgTargetStore) -> Result<()> {
    let request_id = observation_id(loaded, "research_request");
    let response_id = observation_id(loaded, "research_response");
    if repository
        .read_document("provider_snapshot", &response_id)?
        .is_some()
    {
        return Ok(());
    }
    let Some(document) = repository.read_document("provider_snapshot", &request_id)? else {
        return Ok(());
    };
    if repository
        .read_document("research_cycle_expiry", &loaded.cycle_id)?
        .is_some()
    {
        let attempted = repository
            .read_document(
                "provider_snapshot",
                &observation_id(loaded, "analyst_attempt"),
            )?
            .is_some();
        repository.append_document("provider_snapshot", &response_id, &json!({"status":if attempted {"UNKNOWN_AFTER_ANALYST_RESTART"} else {"EXPIRED_BEFORE_ANALYST_DISPATCH"},"entry_authorized":false}))?;
        return Ok(());
    }
    let request: trady_application::analyst::AnalystRequest =
        serde_json::from_value(document["request"].clone())
            .map_err(|_| Error("invalid durable research request".into()))?;
    request.validate(Utc::now())?;
    require(
        request.run.account_id == loaded.config.account_id.as_str()
            && request.run.session_id == loaded.cycle_id
            && document["configuration_hash"] == loaded.config_hash,
        "research request scope changed",
    )?;
    let attempt_id = observation_id(loaded, "analyst_attempt");
    if repository
        .read_document("provider_snapshot", &attempt_id)?
        .is_some()
    {
        repository.append_document(
            "provider_snapshot",
            &response_id,
            &json!({"status":"UNKNOWN_AFTER_ANALYST_RESTART","entry_authorized":false}),
        )?;
        return Ok(());
    }
    repository.append_document(
        "provider_snapshot",
        &attempt_id,
        &json!({"started_at":Utc::now(),"request_hash":hash(&document)?}),
    )?;
    let tools = super::eur_decision::CapturedTools(
        serde_json::from_value(document["tools"].clone())
            .map_err(|_| Error("invalid scoped research tools".into()))?,
    );
    let llm = read_json(&provider_path(
        loaded,
        loaded.config.opencode_config.as_deref(),
        "analyst",
    )?)?;
    let selection = (|| {
        let mut analyst = trady_adapters::opencode::eur_analyst::OpenCodeEurAnalyst::new(
            llm,
            super::secret("OPENCODE_API_KEY")?,
        )?
        .with_tools(tools);
        trady_application::eur_runtime::analyst_select_candidate(
            &mut analyst,
            &request,
            vec![],
            Utc::now(),
        )
    })();
    let response = match selection {
        Ok(selection) => {
            json!({"status":"COMPLETED","selection":selection,"request_hash":hash(&document)?})
        }
        Err(_) => {
            json!({"status":"ANALYST_FAILED_OR_RESPONSE_INVALID","request_hash":hash(&document)?,"entry_authorized":false})
        }
    };
    repository.append_document("provider_snapshot", &response_id, &response)?;
    Ok(())
}

pub fn autonomous(action: &str, args: &[String], path: &str) -> Result<Value> {
    let loaded = load(path)?;
    if action == "check" {
        let mut blockers = Vec::new();
        if !loaded.config.enabled {
            blockers.push("RUNTIME_DISABLED");
        }
        let mut repository = store(&loaded, "trady_executor", "TRADY_EXECUTOR_DSN")?;
        if repository
            .read_document(
                "provider_snapshot",
                &observation_id(&loaded, "analyst_manifest"),
            )?
            .is_none()
        {
            blockers.push("ANALYST_INPUTS_NOT_CAPTURED");
        }
        return Ok(
            json!({"status":if blockers.is_empty(){"EUR_SHADOW_READY"}else{"EUR_SHADOW_BLOCKED"},
            "blockers":blockers,"cycle_id":loaded.cycle_id,"postgresql_verified":true,
            "real_market_inputs":true,"broker_mutations":0,"entry_authorized":false}),
        );
    }
    require(
        action == "run" && (args.len() == 3 || (args.len() == 4 && args[3] == "--once")),
        "unsupported EUR shadow autonomous arguments",
    )?;
    repeat(path, args.len() == 4, load, run_once, |l| {
        l.settings.interval_seconds
    })
}

fn broker(loaded: &Loaded) -> Result<Trading212Reader> {
    let path = provider_path(loaded, loaded.config.trading212_config.as_deref(), "broker")?;
    let config: Trading212Config = read_json(&path)?;
    require(
        config.environment == Environment::Demo
            && config.broker_account_id == 52211463
            && config.account_id == loaded.config.account_id
            && config.base_currency.as_str() == "EUR",
        "shadow broker binding differs from the approved demo account",
    )?;
    let credentials = Credentials::from_store(Environment::Demo, &CredentialStore::load()?)?;
    Trading212Reader::new_bound(config, credentials).map_err(|error| Error(error.to_string()))
}

fn prepare_cycle(loaded: &Loaded, repository: &mut PgTargetStore) -> Result<Option<Loaded>> {
    let mut selected = loaded.clone();
    if let Some(seconds) = loaded.settings.decision_interval_seconds {
        if let Some(cursor) = recent_cursor(loaded, repository)? {
            let id = cursor["cycle_id"]
                .as_str()
                .ok_or_else(|| Error("EUR cycle cursor invalid".into()))?;
            let expiry: chrono::DateTime<Utc> =
                serde_json::from_value(cursor["expires_at"].clone())
                    .map_err(|_| Error("EUR cycle expiry invalid".into()))?;
            let complete = repository.read_document("shadow_result", id)?.is_some();
            if !complete && Utc::now() < expiry {
                selected.cycle_id = id.into();
            } else if !complete
                && repository
                    .read_document("research_cycle_expiry", id)?
                    .is_none()
            {
                repository.append_document("research_cycle_expiry", id, &json!({"cycle_id":id,"expired_at":Utc::now(),"reason":"RESEARCH_DEADLINE_EXCEEDED","entry_authorized":false}))?;
            }
            if !complete && Utc::now() >= expiry && id == loaded.cycle_id {
                return Ok(None);
            }
        }
        if repository
            .read_document("research_cycle_cursor", &selected.cycle_id)?
            .is_none()
        {
            let lifetime =
                seconds.min(loaded.settings.bar_interval_seconds.unwrap_or(300) * 2 + 15);
            repository.append_document("research_cycle_cursor", &selected.cycle_id, &json!({"cycle_id":selected.cycle_id,"configuration_hash":loaded.config_hash,"expires_at":Utc::now()+chrono::Duration::seconds(i64::from(lifetime)),"entry_authorized":false}))?;
        }
    }
    Ok(Some(selected))
}

fn run_once(loaded: &Loaded) -> Result<Value> {
    require(loaded.config.enabled, "EUR shadow deployment is disabled")?;
    let mut repository = store(loaded, "trady_executor", "TRADY_EXECUTOR_DSN")?;
    repository.acquire_executor_lease(&new_id("eur_worker"), LEASE_SECONDS)?;
    let result = (|| {
        if super::eur_operation::protocol_needs_execution(&mut repository)? {
            return Ok(
                json!({"status":"SHADOW_DEFERRED_FOR_ACTIVE_PROTOCOL","broker_mutations":0,"entry_authorized":false}),
            );
        }
        match prepare_cycle(loaded, &mut repository)? {
            Some(selected) => run_fenced(&selected, &mut repository),
            None => Ok(
                json!({"status":"WAITING_NEXT_RESEARCH_BUCKET","cycle_id":loaded.cycle_id,"entry_authorized":false}),
            ),
        }
    })();
    let release = repository.release_executor_lease();
    let value = result?;
    release?;
    Ok(value)
}

fn run_fenced(loaded: &Loaded, repository: &mut PgTargetStore) -> Result<Value> {
    let mut reader = broker(loaded)?;
    // Always reconcile current broker state, including on an already completed cycle.
    let account = reader
        .read_snapshot(&loaded.config.account_id)
        .map_err(|e| Error(e.to_string()))?;
    repository.append_document(
        "reconciliation",
        &new_id("eur_reconcile"),
        &json!({
        "cycle_id":loaded.cycle_id,"configuration_hash":loaded.config_hash,"account":account,
        "broker_account_id":52211463,"broker_mutations":0}),
    )?;
    let existing = repository.read_document("shadow_result", &loaded.cycle_id)?;
    if let Some(result) = existing {
        finalize_result(loaded, repository, &result)?;
        return Ok(json!({
        "status":"EUR_SHADOW_RECONCILED","cycle_id":loaded.cycle_id,"selection_reused":true,
        "new_selection":false,"broker_mutations":0,"entry_authorized":false,"result":result}));
    }
    let started = observe(
        repository,
        loaded,
        "start",
        "eur_coordinator",
        "local://eur-shadow",
        "OWNER_APPROVED_SHADOW_IMPLEMENTATION",
        || {
            Ok(json!({"configuration_hash":loaded.config_hash,
            "crosswalk_hash":loaded.crosswalk_hash,"policy_hash":hash(&loaded.config.policy)?}))
        },
    )?;
    let catalog = observe(
        repository,
        loaded,
        "broker_catalog",
        "trading212_demo",
        "https://demo.trading212.com/api/v0/equity/metadata/instruments",
        "DEMO_ACCOUNT_METADATA_ONLY",
        || reader.catalog_snapshot().map_err(|e| Error(e.to_string())),
    )?;
    let exchanges = observe(
        repository,
        loaded,
        "broker_sessions",
        "trading212_demo",
        "https://demo.trading212.com/api/v0/equity/metadata/exchanges",
        "DEMO_ACCOUNT_METADATA_ONLY",
        || {
            reader
                .exchanges_snapshot()
                .map_err(|e| Error(e.to_string()))
        },
    )?;
    let catalog_response = materialize(repository, &catalog)?;
    let now = Utc::now();
    let sessions: Vec<_> = loaded
        .crosswalk
        .members
        .iter()
        .filter_map(|identity| {
            trady_application::eur_shadow::session_from_catalog(
                identity,
                &loaded.config.session_deadlines,
                &exchanges.response,
                exchanges.received_at,
                now,
            )
            .ok()
        })
        .collect();
    if sessions.len() == loaded.crosswalk.members.len() && sessions.iter().all(|s| now < s.opens_at)
    {
        return Ok(
            json!({"status":"EUR_SHADOW_WAITING_FOR_SESSION","cycle_id":loaded.cycle_id,
            "next_session_open":sessions.iter().map(|s|s.opens_at).min(),"broker_mutations":0,"entry_authorized":false}),
        );
    }
    let account_observation = observe(
        repository,
        loaded,
        "account_snapshot",
        "trading212_demo",
        "https://demo.trading212.com/api/v0/equity/account/summary",
        "PRIVATE_ACCOUNT_RECONCILIATION",
        || {
            serde_json::to_value(&account)
                .map_err(|_| Error("account snapshot encoding failed".into()))
        },
    )?;
    let mut gaps = Vec::<String>::new();
    let analyst = repository.read_document(
        "provider_snapshot",
        &observation_id(loaded, "analyst_manifest"),
    )?;
    if analyst.is_none() {
        return Ok(
            json!({"status":"EUR_SHADOW_WAITING_FOR_ANALYST_INPUTS","cycle_id":loaded.cycle_id,"broker_mutations":0,"entry_authorized":false}),
        );
    }
    let data_path = provider_path(loaded, loaded.config.yfinance_config.as_deref(), "Yahoo")?;
    let data = YfinanceMarketData::new(read_json(&data_path)?)?;
    let current_bars = if let Some(reference) = &loaded.settings.current_bars_config {
        repository.renew_executor_lease(LEASE_SECONDS)?;
        let settings: trady_adapters::ibkr_bars::CurrentBarsConfig =
            read_json(&resolve(&loaded.config_path, reference))?;
        let quote_path = resolve(
            &loaded.config_path,
            loaded
                .settings
                .ibkr_config
                .as_deref()
                .ok_or_else(|| Error("current-bar quote identity missing".into()))?,
        );
        let quotes: IbkrSmartConfig = read_json(&quote_path)?;
        let raw = observe(
            repository,
            loaded,
            "ibkr_current_bars_wire",
            "ibkr_smart_trades",
            "tcp://127.0.0.1/ib-gateway",
            "OWNER_APPROVED_CURRENT_TRADE_CANDLES_ONLY",
            || {
                let directory = Path::new(&loaded.settings.state_directory);
                if !directory.exists() {
                    private_dir(directory)?;
                }
                settings.capture(
                    &quote_path,
                    &resolve(&loaded.config_path, &loaded.settings.crosswalk_config),
                    &directory.join(format!("{}.json", new_id("ibkr_current_bars"))),
                )
            },
        )?;
        repository.renew_executor_lease(LEASE_SECONDS)?;
        Some((settings, quotes, raw))
    } else {
        None
    };
    let ibkr = if let Some(reference) = &loaded.settings.ibkr_config {
        let config: IbkrSmartConfig = read_json(&resolve(&loaded.config_path, reference))?;
        let raw = observe(
            repository,
            loaded,
            "ibkr_smart_wire",
            "ibkr_tws_api",
            "tcp://127.0.0.1/ib-gateway",
            "IBKR_PRIVATE_API_SHADOW_SMART_COMPOSITE",
            || {
                let directory = Path::new(&loaded.settings.state_directory);
                if !directory.exists() {
                    private_dir(directory)?;
                }
                let metadata = fs::symlink_metadata(directory)
                    .map_err(|_| Error("IBKR state directory unavailable".into()))?;
                require(
                    metadata.is_dir() && !metadata.file_type().is_symlink(),
                    "IBKR state directory invalid",
                )?;
                config.capture(
                    &resolve(&loaded.config_path, &loaded.settings.crosswalk_config),
                    &directory.join(format!("{}.json", new_id("ibkr_capture"))),
                )
            },
        )?;
        Some((config, raw))
    } else {
        None
    };
    let mut candidates = Vec::new();
    let mut decision_observations = std::collections::BTreeMap::new();
    let mut deadlines = std::collections::BTreeMap::new();
    let mut observations = vec![
        catalog.observation_id.clone(),
        exchanges.observation_id.clone(),
        account_observation.observation_id.clone(),
    ];
    if let Some((_, raw)) = &ibkr {
        observations.push(raw.observation_id.clone());
    }
    if let Some((_, _, raw)) = &current_bars {
        observations.push(raw.observation_id.clone());
    }
    for identity in &loaded.crosswalk.members {
        let slot = format!("quote_{}", identity.internal_instrument_id);
        let (provider, endpoint, rights) = if ibkr.is_some() {
            (
                "ibkr_tws_api",
                "tcp://127.0.0.1/ib-gateway",
                "IBKR_PRIVATE_API_SHADOW_SMART_COMPOSITE",
            )
        } else {
            (
                "yfinance_rs",
                "https://query1.finance.yahoo.com/v7/finance/quote",
                "PRIVATE_SHADOW_DIAGNOSTIC_OBSERVATION",
            )
        };
        let quote = observe(
            repository,
            loaded,
            &slot,
            provider,
            endpoint,
            rights,
            || {
                if let Some((config, raw)) = &ibkr {
                    let now = Utc::now();
                    let mut quote = config.normalize(
                        identity,
                        &raw.response,
                        now,
                        loaded.config.eligibility.quote_max_age_seconds,
                    )?;
                    let session = trady_application::eur_shadow::session_from_catalog(
                        identity,
                        &loaded.config.session_deadlines,
                        &exchanges.response,
                        exchanges.received_at,
                        now,
                    )
                    .ok();
                    quote["market_state"] = json!(match &session {
                        Some(s) if now >= s.opens_at && now < s.closes_at => "REGULAR",
                        Some(_) => "CLOSED",
                        None => "UNKNOWN",
                    });
                    quote["market_state_source"] = json!(exchanges.observation_id);
                    quote["raw_capture_id"] = json!(raw.observation_id);
                    return Ok(quote);
                }
                serde_json::to_value(
                    data.latest_snapshot(&identity.internal_instrument_id, Utc::now())?,
                )
                .map_err(|_| Error("Yahoo observation encoding failed".into()))
            },
        )?;
        observations.push(quote.observation_id.clone());
        decision_observations.insert(quote.observation_id.clone(), quote.response.clone());
        if let Ok(session) = trady_application::eur_shadow::session_from_catalog(
            identity,
            &loaded.config.session_deadlines,
            &exchanges.response,
            exchanges.received_at,
            Utc::now(),
        ) {
            decision_observations.insert(
                format!("session:{}", identity.internal_instrument_id),
                serde_json::to_value(&session)
                    .map_err(|_| Error("session encoding failed".into()))?,
            );
            deadlines.insert(
                identity.internal_instrument_id.to_string(),
                session.flatten_at,
            );
        }
        if let Some((settings, quotes, raw)) = &current_bars {
            let bars = observe(
                repository,
                loaded,
                &format!("ibkr_bars_{}", identity.internal_instrument_id),
                "ibkr_smart_trades",
                "tcp://127.0.0.1/ib-gateway",
                "OWNER_APPROVED_CURRENT_TRADE_CANDLES_ONLY",
                || {
                    serde_json::to_value(settings.normalize(
                        quotes,
                        identity,
                        &loaded.crosswalk_hash,
                        &raw.response,
                        Utc::now(),
                    )?)
                    .map_err(|_| Error("current-bar encoding failed".into()))
                },
            )?;
            observations.push(bars.observation_id.clone());
            if bars.status == ObservationStatus::Ok {
                decision_observations.insert(
                    format!("bars:{}", identity.internal_instrument_id),
                    bars.response,
                );
            }
        } else if let Some(value) = repository.read_document(
            "provider_snapshot",
            &observation_id(
                loaded,
                &format!("yf_bars_{}", identity.internal_instrument_id),
            ),
        )? {
            let bars: ProviderObservation =
                serde_json::from_value(value).map_err(|_| Error("invalid recorded bars".into()))?;
            bars.validate()?;
            if bars.status == ObservationStatus::Ok {
                decision_observations.insert(
                    format!("bars:{}", identity.internal_instrument_id),
                    bars.response,
                );
            }
        }
        let assessed_at = Utc::now();
        let mut candidate = assess_quote(
            identity,
            &quote,
            assessed_at,
            loaded.config.eligibility.quote_max_age_seconds,
        )?;
        let rows = catalog_response.as_array();
        let matches: Vec<_> = rows
            .into_iter()
            .flatten()
            .filter(|row| {
                row.get("ticker").and_then(Value::as_str) == Some(&identity.broker_ticker)
            })
            .collect();
        if matches.len() != 1
            || matches[0].get("isin").and_then(Value::as_str) != Some(&identity.isin)
            || matches[0].get("currencyCode").and_then(Value::as_str) != Some("EUR")
            || matches[0].get("workingScheduleId").and_then(Value::as_i64)
                != Some(identity.working_schedule_id)
        {
            candidate
                .blockers
                .push("CURRENT_CATALOG_IDENTITY_CHANGED".into());
            candidate.research_eligible = false;
        }
        let session = session_blockers(loaded, identity, &exchanges.response, assessed_at);
        if !session.is_empty() {
            candidate.research_eligible = false;
            candidate.blockers.extend(session);
        }
        candidates.push(candidate);
        repository.renew_executor_lease(LEASE_SECONDS)?;
    }
    if let Some(manifest) = analyst.as_ref() {
        for id in manifest["observation_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            observations.push(id.to_owned());
            match repository.read_document("provider_snapshot", id)? {
                Some(value) if value["status"] == "OK" => (),
                _ => gaps.push(format!("SOURCE_UNAVAILABLE:{id}")),
            }
        }
    }
    let research_id = format!("{}_research", loaded.cycle_id);
    let decision = if let Some(value) = repository.read_document("research_result", &research_id)? {
        value
    } else {
        let input_id = observation_id(loaded, "research_inputs");
        let frozen = if let Some(value) = repository.read_document("research_inputs", &input_id)? {
            value
        } else {
            let value = json!({"candidates":candidates,"observations":decision_observations,"deadlines":deadlines,"as_of":Utc::now()});
            repository.append_document("research_inputs", &input_id, &value)?;
            value
        };
        let frozen_candidates = serde_json::from_value::<
            Vec<trady_application::eur_shadow::ShadowCandidate>,
        >(frozen["candidates"].clone())
        .map_err(|_| Error("invalid frozen research candidates".into()))?;
        let frozen_observations = serde_json::from_value(frozen["observations"].clone())
            .map_err(|_| Error("invalid frozen research observations".into()))?;
        let frozen_deadlines = serde_json::from_value(frozen["deadlines"].clone())
            .map_err(|_| Error("invalid frozen research deadlines".into()))?;
        let research_at = serde_json::from_value(frozen["as_of"].clone())
            .map_err(|_| Error("invalid frozen research timestamp".into()))?;
        let universe = loaded
            .config
            .universe_config
            .as_ref()
            .map(|p| {
                read_json::<trady_domain::EurUniverseSnapshot>(&resolve(&loaded.config_path, p))
            })
            .transpose()?;
        if let Some(universe) = &universe {
            loaded.config.validate_universe(universe, Utc::now())?;
        }
        let costs = loaded
            .settings
            .risk_config
            .as_ref()
            .map(|p| {
                read_json::<super::eur_decision::ReviewedCosts>(&resolve(&loaded.config_path, p))
            })
            .transpose()?;
        let llm: trady_adapters::opencode::llm::OpenCodeGoConfig = read_json(&provider_path(
            loaded,
            loaded.config.opencode_config.as_deref(),
            "analyst",
        )?)?;
        repository.renew_executor_lease(LEASE_SECONDS)?;
        // The analyst receives its own DSN and LLM credential. The executor
        // publishes a scoped immutable handoff, never acquires the LLM key.
        let mut risk_quotes = Vec::new();
        let need_risk = frozen_candidates.iter().any(|c| c.research_eligible)
            && costs.is_some()
            && universe.is_some()
            && repository
                .read_document(
                    "provider_snapshot",
                    &observation_id(loaded, "research_response"),
                )?
                .is_some();
        if need_risk {
            let response = repository
                .read_document(
                    "provider_snapshot",
                    &observation_id(loaded, "research_response"),
                )?
                .unwrap_or_default();
            for c in &frozen_candidates {
                if response["selection"]["selected_instrument_id"].as_str()
                    == Some(c.instrument_id.as_str())
                    && let Ok((q, ids)) = refresh_risk_quote(loaded, repository, &c.instrument_id)
                {
                    observations.extend(ids);
                    risk_quotes.push((c.instrument_id.clone(), q));
                }
            }
        }
        let decision = super::eur_decision::decide(
            &loaded.config,
            &loaded.cycle_id,
            &frozen_candidates,
            &frozen_observations,
            &account,
            universe.as_ref(),
            costs.as_ref(),
            &frozen_deadlines,
            llm,
            research_at,
            &mut |id| {
                let q = risk_quotes
                    .iter()
                    .find(|(instrument, _)| instrument == id)
                    .ok_or_else(|| Error("fresh risk quote unavailable".into()))?
                    .1
                    .clone();
                Ok((q, observation_id(loaded, &format!("risk_quote_{id}"))))
            },
            &mut |request, tools| {
                let request_id = observation_id(loaded, "research_request");
                if repository
                    .read_document("provider_snapshot", &request_id)?
                    .is_none()
                {
                    repository.append_document("provider_snapshot",&request_id,&json!({"request":request,"tools":tools.0,"configuration_hash":loaded.config_hash}))?;
                }
                match repository.read_document(
                    "provider_snapshot",
                    &observation_id(loaded, "research_response"),
                )? {
                    Some(response) if response["status"] == "COMPLETED" => {
                        serde_json::from_value(response["selection"].clone())
                            .map_err(|_| Error("invalid durable research selection".into()))
                    }
                    Some(_) => Err(Error(
                        "analyst outcome unavailable; no automatic retry".into(),
                    )),
                    None => Err(Error("EUR_RESEARCH_PENDING".into())),
                }
            },
        );
        let value = match decision {
            Ok(value) => value,
            Err(error) if error.0 == "EUR_RESEARCH_PENDING" => {
                return Ok(
                    json!({"status":"EUR_SHADOW_WAITING_FOR_ANALYST_SELECTION","cycle_id":loaded.cycle_id,"entry_authorized":false,"broker_mutations":0}),
                );
            }
            Err(error) => return Err(error),
        };
        repository.renew_executor_lease(LEASE_SECONDS)?;
        repository.append_document("research_result", &research_id, &value)?;
        value
    };
    let selected = !decision["selection"]["selected_instrument_id"].is_null();
    let planned = !decision["risk_plan"].is_null();
    let completed_at = Utc::now();
    let result = json!({"schema_version":1,"cycle_id":loaded.cycle_id,"account_id":loaded.config.account_id,
        "code_hash":code_fingerprint(),
        "broker_account_id":52211463,"runtime_mode":"SHADOW","configuration_hash":loaded.config_hash,
        "crosswalk_hash":loaded.crosswalk_hash,"policy_hash":hash(&loaded.config.policy)?,
        "started_at":started.requested_at,"completed_at":completed_at,"candidates":candidates,
        "provider_observation_ids":observations,"source_gaps":gaps,
        "selected_instrument_id":decision["selection"]["selected_instrument_id"],"selection_failure":decision["selection"]["failure"],
        "model_run":decision["selection"]["model"],"selection":decision["selection"],"analyst_invoked":decision["analyst_invoked"],
        "strategy_signals":decision["strategy_signals"],"strategy_assessments":decision["strategy_assessments"],"strategy_status":if selected {"RESEARCH_SELECTED"} else {"BLOCKED_INPUTS"},
        "risk_plan":decision["risk_plan"],"risk_cost_model":decision["risk_cost_model"],"risk_blockers":decision["risk_blockers"],
        "risk_plan_status":if planned {"CALCULATED_SHADOW_ONLY"}else {"NOT_CALCULATED_BLOCKED_INPUTS"},"coordinator_state":if planned {"RISK_APPROVED"}else {"REJECTED"},
        "broker_mutations":0,"entry_authorized":false,"reconciled_flat":account.positions.is_empty()&&account.orders.is_empty()});
    // Persist the immutable decision before projections. Interrupted finalization
    // reuses its original timestamps and payloads instead of rebinding IDs.
    repository.append_document("shadow_result", &loaded.cycle_id, &result)?;
    finalize_result(loaded, repository, &result)?;
    Ok(
        json!({"status":if planned {"EUR_SHADOW_RISK_CALCULATED"}else if selected {"EUR_SHADOW_SELECTED_RISK_BLOCKED"}else {"EUR_SHADOW_COMPLETED_WITH_BLOCKED_SELECTION"},"cycle_id":loaded.cycle_id,
        "new_selection":true,"broker_mutations":0,"entry_authorized":false,"result":result}),
    )
}

fn refresh_risk_quote(
    loaded: &Loaded,
    repository: &mut PgTargetStore,
    id: &trady_domain::InstrumentId,
) -> Result<(trady_domain::Quote, Vec<String>)> {
    let identity = loaded
        .crosswalk
        .members
        .iter()
        .find(|m| &m.internal_instrument_id == id)
        .ok_or_else(|| Error("risk quote identity unavailable".into()))?;
    let reference = loaded
        .settings
        .ibkr_config
        .as_ref()
        .ok_or_else(|| Error("risk requires IBKR SMART Live".into()))?;
    let config: IbkrSmartConfig = read_json(&resolve(&loaded.config_path, reference))?;
    let raw = observe(
        repository,
        loaded,
        "risk_ibkr_wire",
        "ibkr_tws_api",
        "tcp://127.0.0.1/ib-gateway",
        "IBKR_PRIVATE_API_SMART_COMPOSITE",
        || {
            config.capture(
                &resolve(&loaded.config_path, &loaded.settings.crosswalk_config),
                &Path::new(&loaded.settings.state_directory)
                    .join(format!("{}.json", new_id("risk_ibkr"))),
            )
        },
    )?;
    let normalized = observe(
        repository,
        loaded,
        &format!("risk_quote_{id}"),
        "ibkr_tws_api",
        "tcp://127.0.0.1/ib-gateway",
        "IBKR_PRIVATE_API_SMART_COMPOSITE",
        || {
            let mut value = config.normalize(
                identity,
                &raw.response,
                Utc::now(),
                loaded.config.eligibility.quote_max_age_seconds,
            )?;
            value["raw_capture_id"] = json!(raw.observation_id);
            Ok(value)
        },
    )?;
    let q: trady_application::eur_shadow::ShadowQuote =
        serde_json::from_value(normalized.response.clone())
            .map_err(|_| Error("fresh IBKR risk quote invalid".into()))?;
    require(
        normalized.status == ObservationStatus::Ok && normalized.response["fresh"] == true,
        "IBKR risk quote stale",
    )?;
    let quote = trady_domain::Quote {
        instrument_id: id.clone(),
        currency: loaded.config.base_currency.clone(),
        bid: q.bid.ok_or_else(|| Error("IBKR bid unavailable".into()))?,
        ask: q.ask.ok_or_else(|| Error("IBKR ask unavailable".into()))?,
        observed_at: q.observed_at,
        received_at: q.received_at,
        tradable: true,
    };
    Ok((quote, vec![raw.observation_id, normalized.observation_id]))
}

fn code_fingerprint() -> String {
    content_hash(concat!(
        include_str!("eur_shadow.rs"),
        include_str!("eur_decision.rs"),
        include_str!("../../trady-application/src/eur_shadow.rs"),
        include_str!("../../trady-adapters/src/yfinance.rs"),
        include_str!("../../trady-adapters/src/ibkr_smart.rs"),
        include_str!("../../trady-adapters/src/alpha_vantage.rs"),
        include_str!("../../trady-adapters/src/trading212/mod.rs"),
        include_str!("../../trady-storage/src/target.rs")
    ))
}

fn finalize_result(loaded: &Loaded, repository: &mut PgTargetStore, result: &Value) -> Result<()> {
    let completed_at: DateTime<Utc> = serde_json::from_value(result["completed_at"].clone())
        .map_err(|_| Error("invalid persisted EUR completion time".into()))?;
    let started_at: DateTime<Utc> = serde_json::from_value(result["started_at"].clone())
        .map_err(|_| Error("invalid persisted EUR start time".into()))?;
    let evidence: Vec<String> = result["provider_observation_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(64)
        .map(str::to_owned)
        .collect();
    let mut transitions = vec![
        (
            "research",
            CoordinatorEventKind::StartResearch,
            CoordinatorState::Idle,
            CoordinatorState::Researching,
        ),
        (
            "ranked",
            CoordinatorEventKind::CandidatesRanked,
            CoordinatorState::Researching,
            CoordinatorState::CandidatesRanked,
        ),
    ];
    if !result["risk_plan"].is_null() {
        transitions.extend([
            (
                "validated",
                CoordinatorEventKind::CandidateValidated,
                CoordinatorState::CandidatesRanked,
                CoordinatorState::CandidateValidated,
            ),
            (
                "signal",
                CoordinatorEventKind::SignalProduced,
                CoordinatorState::CandidateValidated,
                CoordinatorState::SignalReady,
            ),
            (
                "risk",
                CoordinatorEventKind::RiskApproved,
                CoordinatorState::SignalReady,
                CoordinatorState::RiskApproved,
            ),
        ]);
    } else {
        transitions.push((
            "rejected",
            CoordinatorEventKind::Reject,
            CoordinatorState::CandidatesRanked,
            CoordinatorState::Rejected,
        ));
    }
    for (suffix, kind, from, to) in transitions {
        let event = CoordinatorEvent {
            event_id: format!("{}_{}", loaded.cycle_id, suffix),
            kind,
            from,
            to,
            account_id: loaded.config.account_id.to_string(),
            instrument_id: result["selected_instrument_id"]
                .as_str()
                .map(|s| trady_domain::InstrumentId::new(s.to_owned()))
                .transpose()?,
            policy_revision: loaded.config.policy.revision.clone(),
            evidence_snapshot_ids: evidence.clone(),
            occurred_at: if suffix == "research" {
                started_at
            } else {
                completed_at
            },
            reason: Some(
                if suffix == "rejected" {
                    "no executable input authority"
                } else {
                    "durable EUR shadow research and deterministic risk evaluation"
                }
                .into(),
            ),
        };
        event.validate(Utc::now())?;
        let document = json!({"event":event,"configuration_hash":loaded.config_hash,"actor":"eur_shadow_coordinator"});
        if repository
            .read_document("coordinator_event", &event.event_id)?
            .is_none()
        {
            repository.append_document("coordinator_event", &event.event_id, &document)?;
        }
    }
    repository.append_document("selection", &loaded.cycle_id, result)?;
    if !result["risk_plan"].is_null() {
        repository.append_document(
            "trade_plan",
            &format!("{}_plan", loaded.cycle_id),
            &result["risk_plan"],
        )?;
    }
    repository.append_document("daily_report", &loaded.cycle_id, result)?;
    repository.append_document("shadow_cycle", &loaded.cycle_id, result)?;
    publish_result(loaded, repository, result)
}

fn session_blockers(
    loaded: &Loaded,
    identity: &trady_application::eur_shadow::ShadowIdentity,
    response: &Value,
    now: DateTime<Utc>,
) -> Vec<String> {
    match trady_application::eur_shadow::session_from_catalog(
        identity,
        &loaded.config.session_deadlines,
        response,
        now,
        now,
    ) {
        Ok(window) if now >= window.opens_at && now < window.flatten_at => vec![],
        Ok(_) => vec!["SESSION_CLOSED_OR_FLATTEN_DEADLINE_REACHED".into()],
        Err(_) => vec!["SESSION_CALENDAR_UNAVAILABLE".into()],
    }
}

fn chats() -> Result<Vec<i64>> {
    let chat = super::secret("TRADY_TELEGRAM_WHITELIST_USER_ID")?
        .parse::<i64>()
        .map_err(|_| Error("invalid private Telegram owner ID".into()))?;
    require(
        chat > 0 && chat < (1_i64 << 52),
        "invalid private Telegram owner ID",
    )?;
    Ok(vec![chat])
}

fn publish_result(loaded: &Loaded, repository: &mut PgTargetStore, result: &Value) -> Result<()> {
    let occurred_at: DateTime<Utc> = serde_json::from_value(result["completed_at"].clone())
        .map_err(|_| Error("invalid persisted EUR cycle timestamp".into()))?;
    let blocked = result["candidates"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|candidate| {
            format!(
                "{}: {}",
                candidate["instrument_id"].as_str().unwrap_or("unknown"),
                candidate["blockers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let event = LifecycleEvent {
        event_id: format!("{}_summary", loaded.cycle_id),
        kind: LifecycleEventKind::DailyReport,
        account_id: loaded.config.account_id.clone(),
        session_id: loaded.cycle_id.clone(),
        instrument_id: None,
        evidence_ids: vec![loaded.cycle_id.clone()],
        occurred_at,
        text: format!(
            "EUR SHADOW completed. No broker orders submitted.\nSelected: {}. Analyst invoked: {}. Risk: {}.\n{blocked}\nBroker flat: {}. Source gaps: {}.",
            result["selected_instrument_id"],
            result["analyst_invoked"],
            result["risk_plan_status"],
            result["reconciled_flat"],
            result["source_gaps"].as_array().map_or(0, Vec::len)
        ),
    };
    repository.publish_lifecycle(
        &loaded.settings.deployment_id,
        &event,
        &chats()?,
        Utc::now(),
    )
}

fn gateway_settings(config: EurRuntimeConfig) -> Result<ShadowSettings> {
    config.validate(Utc::now())?;
    require(
        config.telegram_enabled
            && matches!(
                config.execution_stage,
                ExecutionStage::Shadow | ExecutionStage::Demo
            ),
        "EUR notifications disabled or invalid gateway environment",
    )?;
    let settings = config
        .shadow
        .ok_or_else(|| Error("EUR gateway deployment scope missing".into()))?;
    settings.validate()?;
    Ok(settings)
}
fn load_gateway(path: &str) -> Result<ShadowSettings> {
    // Delivery of already-journaled events must not depend on current entry
    // review expiry, provider files or the entry enabled flag.
    gateway_settings(read_json(Path::new(path))?)
}
fn gateway(settings: &ShadowSettings) -> Result<Value> {
    let mut outbox = PgTargetOutbox::connect(
        &super::secret("TRADY_GATEWAY_DSN")?,
        settings.deployment_id.clone(),
        chats()?,
    )?;
    let token = BotToken::new(super::secret("TRADY_TELEGRAM_BOT_TOKEN")?)?;
    let client = TelegramClient::new(token, 1)
        .map_err(|_| Error("Telegram transport unavailable".into()))?;
    client
        .identity()
        .map_err(|_| Error("Telegram identity unavailable".into()))?;
    let mut sent = 0;
    let mut unknown = 0;
    for _ in 0..20 {
        let Some(delivery) = outbox.claim()? else {
            break;
        };
        let text = delivery.payload["text"]
            .as_str()
            .filter(|text| text.encode_utf16().count() <= 4096)
            .ok_or_else(|| Error("invalid persisted Telegram delivery text".into()))?;
        let delivered = client.send_message(delivery.chat_id, text).is_ok();
        outbox.finish(&delivery, delivered)?;
        if delivered { sent += 1 } else { unknown += 1 }
    }
    Ok(
        json!({"status":"EUR_GATEWAY_DRAINED","sent":sent,"unknown":unknown,
        "broker_mutations":0,"entry_authorized":false}),
    )
}

fn report(loaded: &Loaded) -> Result<Value> {
    let mut repository = store(loaded, "trady_analyst", "TRADY_ANALYST_DSN")?;
    Ok(
        json!({"cycle_id":loaded.cycle_id,"report":repository.read_document("daily_report",&loaded.cycle_id)?,
        "telegram_deliveries":repository.telegram_delivery_summary(&loaded.settings.deployment_id)?,
        "entry_authorized":false}),
    )
}

/// Export an immutable, private capsule. A manifest is written last, after all
/// files are synced. Existing directories are never overwritten.
pub fn export_bundle(path: &str, root: &Path) -> Result<Value> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut loaded = load(path)?;
    let mut repository = store(&loaded, "trady_analyst", "TRADY_ANALYST_DSN")?;
    let result = latest_result(&loaded, &mut repository)?
        .ok_or_else(|| Error("no completed EUR shadow decision to export".into()))?;
    loaded.cycle_id = result["cycle_id"]
        .as_str()
        .ok_or_else(|| Error("completed EUR cycle missing".into()))?
        .into();
    require(
        result["configuration_hash"].as_str() == Some(&loaded.config_hash),
        "capture configuration differs from completed decision",
    )?;
    private_dir(root)?;
    private_dir(&root.join("provider-snapshots"))?;
    let mut inputs = BTreeMap::<String, Value>::new();
    let mut index = BTreeMap::<String, String>::new();
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    let mut queue: Vec<String> = result["provider_observation_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    queue.push(observation_id(&loaded, "start"));
    let mut seen = BTreeSet::new();
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        require(
            seen.len() <= 256,
            "capture has too many observation records",
        )?;
        let value = repository
            .read_document("provider_snapshot", &id)?
            .ok_or_else(|| Error("recorded provider snapshot is missing".into()))?;
        if value.get("observation_id").is_some() {
            let observation: ProviderObservation = serde_json::from_value(value.clone())
                .map_err(|_| Error("recorded observation schema differs".into()))?;
            observation.validate()?;
            if observation.response["encoding"] == "JSON_ARRAY_CHUNKS_V1" {
                queue.extend(
                    observation.response["parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned),
                );
                materialize(&mut repository, &observation)?;
            }
            let source = observation.source_id.as_str();
            let group = if source.starts_with("yf_") {
                Some("yfinance")
            } else if source.starts_with("alpha_") {
                Some("alpha_vantage")
            } else if source.contains("rss") {
                Some("rss")
            } else if source.starts_with("research_x_") {
                Some("x")
            } else if source == "broker_catalog" {
                Some("broker_catalog")
            } else if source == "broker_sessions" {
                Some("session_calendar")
            } else if source.starts_with("quote_") {
                Some("quote_observations")
            } else {
                None
            };
            if let Some(group) = group {
                groups.entry(group.into()).or_default().push(id.clone());
            }
        } else {
            require(
                value["response_hash"].as_str() == Some(&hash(&value["response"])?),
                "catalog payload chunk hash differs",
            )?;
        }
        let key = format!("snapshot_{:03}", index.len());
        let file = format!("provider-snapshots/{key}.json");
        write_capture_file(root, &key, &file, &value, &mut inputs)?;
        index.insert(id, key);
    }
    for group in [
        "yfinance",
        "alpha_vantage",
        "rss",
        "x",
        "broker_catalog",
        "session_calendar",
        "quote_observations",
    ] {
        let ids = groups.remove(group).unwrap_or_default();
        write_capture_file(
            root,
            group,
            &format!("provider-snapshots/{group}-index.json"),
            &json!({"observation_ids":ids,"status":if ids.is_empty(){"UNAVAILABLE"}else{"RECORDED"}}),
            &mut inputs,
        )?;
    }
    let mut provider_configs = BTreeMap::new();
    for (kind, slot) in [
        ("provider_snapshot", "research_request"),
        ("provider_snapshot", "research_response"),
        ("research_inputs", "research_inputs"),
    ] {
        if let Some(value) = repository.read_document(kind, &observation_id(&loaded, slot))? {
            write_capture_file(
                root,
                slot,
                &format!("provider-snapshots/{slot}.json"),
                &value,
                &mut inputs,
            )?;
        }
    }
    for (name, reference) in [
        ("yfinance", loaded.config.yfinance_config.as_deref()),
        ("research", loaded.config.research_config.as_deref()),
        ("broker", loaded.config.trading212_config.as_deref()),
        ("opencode", loaded.config.opencode_config.as_deref()),
    ] {
        let reference =
            reference.ok_or_else(|| Error("provider configuration unavailable".into()))?;
        let value: Value = read_json(&resolve(&loaded.config_path, reference))?;
        reject_configuration_secrets(&value)?;
        provider_configs.insert(name, value);
    }
    if let Some(reference) = &loaded.settings.ibkr_config {
        let value: Value = read_json(&resolve(&loaded.config_path, reference))?;
        reject_configuration_secrets(&value)?;
        provider_configs.insert("ibkr", value);
    }
    for (name, reference) in [
        ("risk", loaded.settings.risk_config.as_deref()),
        ("universe", loaded.config.universe_config.as_deref()),
        (
            "current_bars",
            loaded.settings.current_bars_config.as_deref(),
        ),
    ] {
        if let Some(reference) = reference {
            let value: Value = read_json(&resolve(&loaded.config_path, reference))?;
            reject_configuration_secrets(&value)?;
            provider_configs.insert(name, value);
        }
    }
    let crosswalk: Value = read_json(&resolve(
        &loaded.config_path,
        &loaded.settings.crosswalk_config,
    ))?;
    let deliveries = repository.telegram_delivery_summary(&loaded.settings.deployment_id)?;
    let cost_document = match provider_configs.get("risk") {
        Some(costs) => json!({"status":"REVIEWED_MODEL_INPUTS","reviewed_costs":costs,
            "scope":"SHADOW_COST_MODEL_NOT_INDEPENDENT_RELEASE_ACCEPTANCE","same_currency_fx":"EUR_TO_EUR_IDENTITY",
            "fees_assumed_zero":false}),
        None => {
            json!({"status":"UNAVAILABLE","reason":"NO_REVIEWED_TRANSACTION_COST_OBSERVATION_IN_SHADOW_RUN",
            "same_currency_fx":"EUR_TO_EUR_IDENTITY","fees_assumed_zero":false})
        }
    };
    let account_snapshot = repository
        .read_document(
            "provider_snapshot",
            &observation_id(&loaded, "account_snapshot"),
        )?
        .and_then(|value| value.get("response").cloned())
        .unwrap_or(Value::Null);
    write_capture_file(
        root,
        "capture_context",
        "capture-context.json",
        &json!({"schema_version":1,
        "runtime":loaded.config,"crosswalk":crosswalk,"provider_configurations":provider_configs,
        "result":result,"account_snapshot":account_snapshot,"observation_index":index,"telegram_deliveries":deliveries}),
        &mut inputs,
    )?;
    write_capture_file(
        root,
        "cost_fx",
        "provider-snapshots/cost-fx.json",
        &cost_document,
        &mut inputs,
    )?;
    write_capture_file(root, "selection", "selection.json", &result, &mut inputs)?;
    write_capture_file(
        root,
        "strategy_snapshots",
        "strategy-snapshots.json",
        &json!({"status":result["strategy_status"],
        "model_run":result["model_run"],"risk_plan_status":result["risk_plan_status"]}),
        &mut inputs,
    )?;
    let manifest = json!({"schema_version":1,"run_id":loaded.cycle_id,"inputs":inputs});
    private_json(&root.join("manifest.json"), &manifest)?;
    Ok(
        json!({"status":"EUR_CAPTURE_EXPORTED","manifest":root.join("manifest.json"),"snapshots":index.len(),
        "manifest_hash":hash(&manifest)?,"paper_day_passed":false,"broker_mutations":0,"entry_authorized":false}),
    )
}
fn reject_configuration_secrets(value: &Value) -> Result<()> {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            require(
                !["password", "secret", "token", "api_key", "dsn"]
                    .contains(&key.to_ascii_lowercase().as_str())
                    || value.is_null(),
                "a provider configuration contains an inline credential",
            )?;
            reject_configuration_secrets(value)?;
        }
    } else if let Some(array) = value.as_array() {
        for value in array {
            reject_configuration_secrets(value)?;
        }
    }
    Ok(())
}
fn write_capture_file(
    root: &Path,
    key: &str,
    name: &str,
    value: &Value,
    inputs: &mut std::collections::BTreeMap<String, Value>,
) -> Result<()> {
    let digest = private_json(&root.join(name), value)?;
    inputs.insert(key.into(), json!({"path":name,"sha256":digest}));
    Ok(())
}
pub(super) fn private_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|_| {
        Error("cannot create new private artifact directory; existing paths are preserved".into())
    })
}
pub(super) fn private_json(path: &Path, value: &Value) -> Result<String> {
    use std::io::Write;
    let bytes = serde_json::to_string_pretty(value)
        .map_err(|_| Error("artifact encoding failed".into()))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| Error("artifact already exists or cannot be created".into()))?;
    file.write_all(bytes.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| Error("artifact could not be synced".into()))?;
    Ok(content_hash(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the disposable PostgreSQL runner"]
    fn durable_intraday_cursor_resumes_and_expires_without_repeating_model_calls() {
        assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
        let mut config: EurRuntimeConfig =
            serde_json::from_str(include_str!("../../../config/eur-shadow.example.json")).unwrap();
        config.account_id = trady_domain::AccountId::new(new_id("paper_cursor")).unwrap();
        let mut settings = config.shadow.clone().unwrap();
        settings.decision_interval_seconds = Some(300);
        let hash = content_hash(config.account_id.as_str());
        let mut loaded = Loaded {
            config: config.clone(),
            settings: settings.clone(),
            crosswalk: serde_json::from_str(include_str!(
                "../../../docs/evidence/eur-shadow-review-2026-11-04/crosswalk.json"
            ))
            .unwrap(),
            config_path: PathBuf::from("/unused-fixture.json"),
            config_hash: hash.clone(),
            crosswalk_hash: "a".repeat(64),
            cycle_id: settings.cycle_id(&hash, Utc::now()).unwrap(),
        };
        let mut repository = PgTargetStore::connect(
            &std::env::var("TRADY_EXECUTOR_DSN").unwrap(),
            "trady_executor",
            config.owner_id.clone(),
            config.account_id.to_string(),
        )
        .unwrap();
        repository
            .acquire_executor_lease(&new_id("cursor_test"), 120)
            .unwrap();
        let old = settings
            .cycle_id(&hash, Utc::now() - chrono::Duration::minutes(5))
            .unwrap();
        repository.append_document("research_cycle_cursor",&old,&json!({"cycle_id":old,"configuration_hash":hash,"expires_at":Utc::now()+chrono::Duration::seconds(120),"synthetic_fixture":true})).unwrap();
        assert_eq!(
            prepare_cycle(&loaded, &mut repository)
                .unwrap()
                .unwrap()
                .cycle_id,
            old
        );
        repository.append_document("shadow_result",&old,&json!({"cycle_id":old,"configuration_hash":hash,"risk_plan":null,"completed_at":Utc::now(),"synthetic_fixture":true})).unwrap();
        assert_eq!(
            latest_result(&loaded, &mut repository).unwrap().unwrap()["cycle_id"],
            old
        );
        assert_eq!(
            prepare_cycle(&loaded, &mut repository)
                .unwrap()
                .unwrap()
                .cycle_id,
            loaded.cycle_id
        );
        loaded.cycle_id = new_id("expired_research_fixture");
        repository
            .append_document(
                "research_cycle_expiry",
                &loaded.cycle_id,
                &json!({"synthetic_fixture":true,"entry_authorized":false}),
            )
            .unwrap();
        repository
            .append_document(
                "provider_snapshot",
                &observation_id(&loaded, "research_request"),
                &json!({"request":{},"synthetic_fixture":true}),
            )
            .unwrap();
        process_research_request(&loaded, &mut repository).unwrap();
        assert_eq!(
            repository
                .read_document(
                    "provider_snapshot",
                    &observation_id(&loaded, "research_response")
                )
                .unwrap()
                .unwrap()["status"],
            "EXPIRED_BEFORE_ANALYST_DISPATCH"
        );
        assert!(
            repository
                .read_document(
                    "provider_snapshot",
                    &observation_id(&loaded, "analyst_attempt")
                )
                .unwrap()
                .is_none()
        );
        repository.release_executor_lease().unwrap();
    }
    #[test]
    fn pending_delivery_survives_disabled_entry_and_missing_review_files() {
        let mut config: EurRuntimeConfig =
            serde_json::from_str(include_str!("../../../config/eur-shadow.example.json")).unwrap();
        config.enabled = false;
        config.shadow.as_mut().unwrap().crosswalk_config =
            "/entry-evidence-no-longer-available.json".into();
        config.shadow.as_mut().unwrap().ibkr_config =
            Some("/quote-provider-no-longer-available.json".into());
        assert!(gateway_settings(config.clone()).is_ok());
        config.telegram_enabled = false;
        assert!(gateway_settings(config.clone()).is_err());
        config.telegram_enabled = true;
        config.live_enabled = true;
        assert!(gateway_settings(config).is_err());
    }
}

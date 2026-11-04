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

fn resolve(config: &Path, reference: &str) -> PathBuf {
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
    let crosswalk_value: Value = read_json(&resolve(&config_path, &settings.crosswalk_config))?;
    let crosswalk: ShadowCrosswalk = serde_json::from_value(crosswalk_value.clone())
        .map_err(|_| Error("invalid shadow crosswalk".into()))?;
    crosswalk.validate(Utc::now())?;
    let config_hash = hash(&config)?;
    let crosswalk_hash = hash(&crosswalk_value)?;
    let cycle_id = format!(
        "eur_{}_{}_{}",
        settings.deployment_id,
        Utc::now().format("%Y%m%d"),
        &config_hash[..12]
    );
    require(
        cycle_id.len() <= 120,
        "EUR deployment identifier is too long",
    )?;
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
    format!("{}_{}", loaded.cycle_id, slot)
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
        "usage: trady eur capture|gateway|report --config FILE [--once]",
    )?;
    let once = args.len() == 3;
    match action {
        "capture" => repeat(&args[1], once, capture),
        "gateway" => repeat(&args[1], once, gateway),
        "report" => report(&load(&args[1])?),
        _ => Err(Error("unknown EUR shadow command".into())),
    }
}

fn repeat(path: &str, once: bool, action: fn(&Loaded) -> Result<Value>) -> Result<Value> {
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = stopped.clone();
    ctrlc::set_handler(move || stop.store(true, std::sync::atomic::Ordering::Relaxed))
        .map_err(|_| Error("cannot install EUR service stop handler".into()))?;
    loop {
        let loaded = load(path)?;
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
        for _ in 0..loaded.settings.interval_seconds * 10 {
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
                            86_400,
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
                let evidence = super::news::run(&[
                    "read".into(),
                    "--source".into(),
                    source.id.clone(),
                    "--config".into(),
                    path,
                    "--limit".into(),
                    "20".into(),
                    "--for-model".into(),
                ])?;
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
    Ok(
        json!({"status":"EUR_ANALYST_INPUTS_PERSISTED","cycle_id":loaded.cycle_id,
        "observations":observations.len(),"entry_authorized":false}),
    )
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
    repeat(path, args.len() == 4, run_once)
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

fn run_once(loaded: &Loaded) -> Result<Value> {
    require(loaded.config.enabled, "EUR shadow deployment is disabled")?;
    let mut repository = store(loaded, "trady_executor", "TRADY_EXECUTOR_DSN")?;
    let worker = new_id("eur_worker");
    repository.acquire_executor_lease(&worker, LEASE_SECONDS)?;
    let result = run_fenced(loaded, &mut repository);
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
    let mut gaps = Vec::<String>::new();
    let analyst = repository.read_document(
        "provider_snapshot",
        &observation_id(loaded, "analyst_manifest"),
    )?;
    if analyst.is_none() {
        gaps.push("ANALYST_INPUTS_NOT_CAPTURED".into());
    }
    let data_path = provider_path(loaded, loaded.config.yfinance_config.as_deref(), "Yahoo")?;
    let data = YfinanceMarketData::new(read_json(&data_path)?)?;
    let mut candidates = Vec::new();
    let mut observations = vec![
        catalog.observation_id.clone(),
        exchanges.observation_id.clone(),
    ];
    for identity in &loaded.crosswalk.members {
        let slot = format!("quote_{}", identity.internal_instrument_id);
        let quote = observe(
            repository,
            loaded,
            &slot,
            "yfinance_rs",
            "https://query1.finance.yahoo.com/v7/finance/quote",
            "PRIVATE_SHADOW_DIAGNOSTIC_OBSERVATION",
            || {
                serde_json::to_value(
                    data.latest_snapshot(&identity.internal_instrument_id, Utc::now())?,
                )
                .map_err(|_| Error("Yahoo observation encoding failed".into()))
            },
        )?;
        observations.push(quote.observation_id.clone());
        let mut candidate = assess_quote(
            identity,
            &quote,
            Utc::now(),
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
        let session = session_blockers(loaded, identity, &exchanges.response, Utc::now());
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
    let completed_at = Utc::now();
    let result = json!({"schema_version":1,"cycle_id":loaded.cycle_id,"account_id":loaded.config.account_id,
        "broker_account_id":52211463,"runtime_mode":"SHADOW","configuration_hash":loaded.config_hash,
        "crosswalk_hash":loaded.crosswalk_hash,"policy_hash":hash(&loaded.config.policy)?,
        "started_at":started.requested_at,"completed_at":completed_at,"candidates":candidates,
        "provider_observation_ids":observations,"source_gaps":gaps,
        "selected_instrument_id":null,"selection_failure":{"kind":"OPERATIONAL_INPUTS_UNAVAILABLE",
            "code":"NO_EXECUTABLE_EUR_CANDIDATE","message":"No reviewed executable quote and order rules; candidate blockers are retained."},
        "model_run":null,"analyst_invoked":false,"strategy_status":"BLOCKED_INPUTS",
        "risk_plan_status":"NOT_CALCULATED_BLOCKED_INPUTS","coordinator_state":"REJECTED",
        "broker_mutations":0,"entry_authorized":false,"reconciled_flat":account.positions.is_empty()&&account.orders.is_empty()});
    // Persist the immutable decision before projections. Interrupted finalization
    // reuses its original timestamps and payloads instead of rebinding IDs.
    repository.append_document("shadow_result", &loaded.cycle_id, &result)?;
    finalize_result(loaded, repository, &result)?;
    Ok(
        json!({"status":"EUR_SHADOW_COMPLETED_WITH_BLOCKED_SELECTION","cycle_id":loaded.cycle_id,
        "new_selection":true,"broker_mutations":0,"entry_authorized":false,"result":result}),
    )
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
    for (suffix, kind, from, to) in [
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
        (
            "rejected",
            CoordinatorEventKind::Reject,
            CoordinatorState::CandidatesRanked,
            CoordinatorState::Rejected,
        ),
    ] {
        let event = CoordinatorEvent {
            event_id: format!("{}_{}", loaded.cycle_id, suffix),
            kind,
            from,
            to,
            account_id: loaded.config.account_id.to_string(),
            instrument_id: None,
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
                    "durable EUR shadow research"
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
    let mut reasons = Vec::new();
    let venue = response.as_array().into_iter().flatten().find(|venue| {
        venue["id"].as_i64() == Some(identity.broker_exchange_id)
            && venue["name"].as_str() == Some(&identity.exchange)
    });
    let schedule = venue
        .and_then(|v| v["workingSchedules"].as_array())
        .into_iter()
        .flatten()
        .find(|s| s["id"].as_i64() == Some(identity.working_schedule_id));
    let date = now.format("%Y-%m-%d").to_string();
    let mut opens = Vec::new();
    let mut closes = Vec::new();
    for event in schedule
        .and_then(|s| s["timeEvents"].as_array())
        .into_iter()
        .flatten()
    {
        if let Some(time) = event["date"]
            .as_str()
            .filter(|time| time.starts_with(&date))
            .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
            .map(|time| time.with_timezone(&Utc))
        {
            match event["type"].as_str() {
                Some("OPEN") => opens.push(time),
                Some("CLOSE") => closes.push(time),
                _ => (),
            }
        }
    }
    if opens.len() != 1 || closes.len() != 1 {
        reasons.push("SESSION_CALENDAR_UNAVAILABLE".into());
        return reasons;
    }
    let deadline = loaded
        .config
        .session_deadlines
        .iter()
        .find(|d| {
            d.exchange_mic == identity.exchange_mic && d.timezone == identity.session_timezone
        })
        .and_then(|d| d.flatten_at(closes[0]).ok());
    match deadline {
        Some(deadline) if now >= opens[0] && now < deadline => (),
        _ => reasons.push("SESSION_CLOSED_OR_FLATTEN_DEADLINE_REACHED".into()),
    }
    reasons
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
            "EUR SHADOW completed. No broker orders submitted.\nSelection blocked by input/execution evidence.\n{blocked}\nBroker flat: {}. Source gaps: {}.",
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

fn gateway(loaded: &Loaded) -> Result<Value> {
    require(loaded.config.enabled, "EUR gateway deployment is disabled")?;
    let mut outbox = PgTargetOutbox::connect(
        &super::secret("TRADY_GATEWAY_DSN")?,
        loaded.settings.deployment_id.clone(),
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

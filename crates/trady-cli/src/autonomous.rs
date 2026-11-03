//! Explicit deployment entrypoint. Enabling a reviewed config runs the executor, never chat tools.

#[path = "autonomous_shadow.rs"]
mod shadow;
#[path = "autonomous_worker.rs"]
mod worker;

use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use trady_adapters::{
    SystemClock,
    alpaca::{AlpacaDataConfig, IexStreamHandle},
    autonomous::TradingInputsConfig,
    credentials::CredentialStore,
    perpetual::{ApprovedArtifact, MAX_ARTIFACT_BYTES, NativePerpetualAdvisor},
    screening::ApprovedUniverseConfig,
    trading212::{AutonomousTrading212, Credentials, Trading212Config},
    yfinance::YfinanceDataConfig,
};
use trady_application::{
    autonomous::*,
    contracts::parse_json_bounded,
    optimization::{
        DonchianIntegrityManifest, DonchianIntegrityReport, DonchianReleaseState,
        verify_donchian_integrity,
    },
    watchdog::{IexWatchdogObservation, WatchdogStatus},
};
use trady_domain::{perpetual::ModelManifest, *};
use trady_storage::autonomous::PgTradingJournal;

pub const HELP: &str = "\nAutonomous execution (explicit deployment config):\n  trady autonomous check --config FILE\n  trady autonomous run --config FILE [--once]\n  trady autonomous analyst-service --config FILE\n  trady autonomous holding --config FILE --position ID\n  trady autonomous review-holding --config FILE --request FILE\n  trady autonomous recover-order --config FILE --intent ID --broker-order ID\n\nRun requires enabled=true, restricted database credentials, approved inputs, formula and model artifacts.\nLive execution uses analyst_socket with a separately managed analyst identity.\nShadow requires shadow_directory and records proposals through a read-only broker reader.\nHolding reviews queue checked target updates; the executor rechecks the current plan, quote and old exit triggers.\nThe broker environment selects matching demo/live API credentials.\nUnknown submissions stay blocked until exact-ID reconciliation; no submission is automatically replayed.\n";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelFile {
    manifest: ModelManifest,
    artifact_path: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeConfig {
    config_version: u32,
    enabled: bool,
    /// Lifecycle stage is separate from the broker environment mode. Missing
    /// legacy values default to SHADOW so an old config cannot gain mutation
    /// authority by deserialization alone.
    #[serde(default)]
    execution_stage: ExecutionStage,
    interval_seconds: u64,
    trading: TradingConfig,
    broker: Trading212Config,
    /// Alpaca Basic is the authoritative IEX quote plane for the fixed 30.
    #[serde(default)]
    alpaca_market_data: Option<AlpacaDataConfig>,
    /// Explicit Yahoo Finance symbol mappings for Euronext quotes.
    #[serde(default)]
    yfinance_market_data: Option<YfinanceDataConfig>,
    inputs: TradingInputsConfig,
    universe: ApprovedUniverseConfig,
    models: Vec<ModelFile>,
    controls: OwnerControls,
    /// Connect to a separately managed analyst identity instead of spawning a child.
    #[serde(default)]
    analyst_socket: Option<String>,
    #[serde(default)]
    shadow_directory: Option<String>,
    /// Exact deployment-owned source, calendar, model and formula release scope.
    #[serde(default)]
    release_candidate: Option<trady_storage::weekly::ReleaseCandidate>,
    #[serde(default)]
    watchdog: trady_application::watchdog::IexWatchdogConfig,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HoldingReviewRequest {
    context: trady_domain::perpetual::AssessmentContext,
    proposed: trady_policy::holding::HoldingPlan,
    research: EntryCandidate,
    screening: trady_application::screening::ScreeningSnapshot,
    review_reason: String,
}
impl RuntimeConfig {
    fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1 && (5..=30).contains(&self.interval_seconds),
            "invalid autonomous runtime configuration",
        )?;
        self.trading.validate_for_management()?;
        match self.execution_stage {
            ExecutionStage::Shadow => {
                require(
                    self.trading.mode != Mode::Live,
                    "live deployments require execution_stage LIVE",
                )?;
            }
            ExecutionStage::SupervisedDemo
            | ExecutionStage::AutonomousDemo
            | ExecutionStage::Paused
            | ExecutionStage::ManagementOnly => {
                require(
                    self.trading.mode == Mode::Paper,
                    "demo execution stages require paper broker mode",
                )?;
            }
            ExecutionStage::Live => {
                require(
                    self.trading.mode == Mode::Live,
                    "live execution stage requires live broker mode",
                )?;
            }
        }
        self.broker.validate().map_err(|e| Error(e.to_string()))?;
        if let Some(alpaca) = &self.alpaca_market_data {
            alpaca.validate()?;
            if alpaca.enabled {
                require(
                    alpaca.base_currency.as_str() == "USD",
                    "IEX Alpaca market data must remain USD-denominated",
                )?;
                require(
                    self.inputs.screening.owner_allowlist.len() == 30
                        && self.inputs.screening.owner_allowlist.iter().all(|id| {
                            id.as_str().ends_with("_US_EQ")
                                && trady_domain::iex::SYMBOLS
                                    .iter()
                                    .any(|s| id.as_str() == format!("{s}_US_EQ"))
                        }),
                    "enabled IEX market data requires the exact IEX30 owner allowlist",
                )?;
            }
        }
        if let Some(yfinance) = &self.yfinance_market_data {
            yfinance.validate()?;
            if yfinance.enabled {
                require(
                    self.trading.base_currency == yfinance.base_currency,
                    "yfinance market data currency must match the trading account",
                )?;
                require(
                    yfinance.symbols.keys().all(|instrument| {
                        self.inputs.screening.owner_allowlist.contains(instrument)
                    }),
                    "yfinance mappings must remain inside the owner instrument allowlist",
                )?;
            }
        }
        self.watchdog.validate()?;
        self.inputs.validate(&self.trading)?;
        self.universe.validate()?;
        if let Some(socket) = &self.analyst_socket {
            require(
                Path::new(socket).is_absolute(),
                "analyst socket must use an absolute path",
            )?;
        }
        require(
            self.trading.mode != Mode::Live || self.analyst_socket.is_some(),
            "live execution requires the separately managed analyst socket",
        )?;
        if self.trading.mode == Mode::Live {
            let release = self.release_candidate.as_ref().ok_or_else(|| {
                Error("live deployment requires release_candidate bindings".into())
            })?;
            release.validate()?;
            require(
                release.mode == Mode::Live
                    && release.account_id == self.trading.account_id
                    && release.owner_id == self.trading.owner_id,
                "live release configuration scope differs",
            )?;
            for model in &self.models {
                require(
                    release.model_hashes.get(&model.manifest.model_revision_id)
                        == Some(&model.manifest.artifact_sha256),
                    "native model differs from approved release binding",
                )?;
            }
        }
        require(
            ((self.trading.mode == Mode::Shadow
                && self.trading.account_id != self.broker.account_id)
                || (self.trading.account_id == self.broker.account_id
                    && self.trading.mode == self.broker.environment.mode()))
                && self.trading.base_currency == self.broker.base_currency,
            "runtime broker/account/mode/currency mismatch",
        )?;
        require(self.models.len() <= 32, "too many configured model files")?;
        Ok(())
    }
}
fn bounded_file(path: &Path, max_bytes: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|_| Error("cannot open deployment input".into()))?;
    require(
        file.metadata()
            .map_err(|_| Error("cannot inspect deployment input".into()))?
            .is_file(),
        "deployment input is not a regular file",
    )?;
    let mut bytes = vec![];
    file.take((max_bytes + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error("cannot read deployment input".into()))?;
    require(
        bytes.len() <= max_bytes,
        "deployment input exceeds size limit",
    )?;
    Ok(bytes)
}
fn load(path: &str) -> Result<RuntimeConfig> {
    let bytes = bounded_file(Path::new(path), 2 * 1024 * 1024)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error("deployment configuration is not UTF-8".into()))?;
    let config: RuntimeConfig = serde_json::from_value(parse_json_bounded(text, 2 * 1024 * 1024)?)
        .map_err(|_| Error("invalid autonomous deployment fields".into()))?;
    config.validate()?;
    Ok(config)
}

fn deployment_root(path: &str) -> Result<PathBuf> {
    let config_path = fs::canonicalize(path)
        .map_err(|_| Error("cannot resolve autonomous deployment path".into()))?;
    let directory = config_path
        .parent()
        .ok_or_else(|| Error("autonomous deployment has no parent directory".into()))?;
    Ok(
        if directory.file_name().and_then(|name| name.to_str()) == Some("config") {
            directory
                .parent()
                .ok_or_else(|| Error("autonomous deployment has no repository root".into()))?
                .to_path_buf()
        } else {
            directory.to_path_buf()
        },
    )
}

fn read_required(path: &Path, message: &'static str) -> Result<Vec<u8>> {
    fs::read(path).map_err(|_| Error(message.into()))
}

/// Resolve and verify the deployment-owned Donchian integrity manifest.  A
/// non-Donchian legacy authority is intentionally outside this check; the
/// focused IEX30 authority always carries the deterministic strategy binding.
fn donchian_integrity(
    config: &RuntimeConfig,
    config_path: &str,
) -> Result<Option<DonchianIntegrityReport>> {
    if config
        .trading
        .limits_authority
        .as_ref()
        .and_then(|authority| authority.strategy.as_ref())
        .is_none()
    {
        return Ok(None);
    }
    let root = deployment_root(config_path)?;
    let config_bytes = read_required(
        Path::new(config_path),
        "autonomous deployment configuration is unreadable",
    )?;
    let authority_bytes = read_required(
        &root.join("config/formulas/long-donchian-atr-v1.authority.json"),
        "canonical Donchian authority is missing",
    )?;
    let spec_bytes = read_required(
        &root.join("config/formulas/long-donchian-atr-v1.json"),
        "canonical Donchian specification is missing",
    )?;
    let universe_bytes = read_required(
        &root.join("config/iex30.universe.json"),
        "canonical IEX30 universe is missing",
    )?;
    let implementation_bytes = read_required(
        &root.join("crates/trady-policy/src/limits/donchian.rs"),
        "registered Donchian implementation is missing",
    )?;
    let manifest_bytes = read_required(
        &root.join("config/formulas/long-donchian-atr-v1.integrity.json"),
        "Donchian integrity manifest is missing",
    )?;
    let manifest: DonchianIntegrityManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| Error("Donchian integrity manifest is unreadable".into()))?;
    let authority = config
        .trading
        .limits_authority
        .as_ref()
        .ok_or_else(|| Error("Donchian runtime authority is missing".into()))?;
    verify_donchian_integrity(
        &manifest,
        &authority_bytes,
        &spec_bytes,
        &universe_bytes,
        &config_bytes,
        &implementation_bytes,
        authority,
        &config.trading.policy,
    )
    .map(Some)
}

fn models(config: &RuntimeConfig, config_path: &str) -> Result<NativePerpetualAdvisor> {
    let root = Path::new(config_path).parent().unwrap_or(Path::new("."));
    let mut artifacts = vec![];
    for model in &config.models {
        if model.manifest.valid_until <= Utc::now() {
            continue;
        }
        let path = Path::new(&model.artifact_path);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        artifacts.push(ApprovedArtifact {
            manifest: model.manifest.clone(),
            bytes: bounded_file(&path, MAX_ARTIFACT_BYTES)?,
        });
    }
    NativePerpetualAdvisor::from_approved_artifacts(artifacts, Utc::now())
}

fn configuration_check(config: &RuntimeConfig, path: &str) -> Result<Value> {
    let advisor = models(config, path)?;
    let now = Utc::now();
    let mut blockers = vec![];
    let mut integrity_error = None;
    let integrity = match donchian_integrity(config, path) {
        Ok(report) => report,
        Err(error) => {
            integrity_error = Some(error.to_string());
            blockers.push("authority_integrity_failed");
            None
        }
    };
    if !config.enabled {
        blockers.push("runtime_disabled");
    }
    if advisor.model_count() == 0 {
        blockers.push("approved_model_registry_empty");
    }
    if config.inputs.uses_placeholder_endpoint() {
        blockers.push("approved_input_service_unconfigured");
    }
    if !config.universe.enabled {
        blockers.push("universe_source_disabled");
    }
    if config.inputs.screening.owner_allowlist.is_empty() {
        blockers.push("owner_allowlist_empty");
    }
    if config.trading.approved_skills.is_empty() {
        blockers.push("approved_skill_hashes_missing");
    }
    // Broker capabilities are unconditional for broker-mutating stages. A
    // record-only SHADOW run must be able to collect prospective evidence even
    // while the broker's protection/quote limitations keep paper/live entries
    // blocked. Fractional support is checked separately when sizing requests
    // fractions.
    let capabilities = &config.trading.capabilities;
    let entry_capability_pairs = [
        ("cash_account", &capabilities.cash_account),
        ("instrument", &capabilities.instrument),
        ("protective_orders", &capabilities.protective_orders),
        ("quotes", &capabilities.quotes),
    ];
    let broker_entry_requirements =
        config.trading.mode == Mode::Live || config.execution_stage.permits_broker_mutation();
    let unverified_entry_capabilities: Vec<_> = if broker_entry_requirements {
        entry_capability_pairs
            .iter()
            .filter_map(|(name, capability)| (!capability.usable(now)).then_some(*name))
            .collect()
    } else {
        vec![]
    };
    let unsupported_entry_capabilities: Vec<_> = entry_capability_pairs
        .iter()
        .filter_map(|(name, capability)| {
            (capability.support == Support::Unsupported).then_some(*name)
        })
        .collect();
    if broker_entry_requirements && !unverified_entry_capabilities.is_empty() {
        blockers.push("entry_capabilities_unverified");
    }
    if config
        .trading
        .limits_authority
        .as_ref()
        .is_none_or(|authority| {
            authority
                .validate_active(&config.trading.policy, now)
                .is_err()
        })
    {
        blockers.push("approved_formula_unavailable");
    }
    if config.trading.validate(now).is_err() {
        blockers.push("live_release_expired");
    }
    if config.enabled
        && config.trading.mode == Mode::Paper
        && !config.execution_stage.permits_broker_mutation()
    {
        blockers.push("execution_stage_not_mutating");
    }
    if config
        .alpaca_market_data
        .as_ref()
        .is_some_and(|data| data.enabled)
        && !config.watchdog.enabled
    {
        blockers.push("iex_watchdog_disabled");
    }
    let integrity_entries_paused = integrity
        .as_ref()
        .is_some_and(|report| report.entries_paused);
    if config.controls.pause_entries {
        blockers.push("entries_paused");
    }
    if config.controls.close_position {
        blockers.push("close_position_latched");
    }
    if broker_entry_requirements && integrity_entries_paused {
        blockers.push("authority_entries_paused");
    }
    let required_actions: Vec<Value> = blockers.iter().map(|code| {
        let required = match *code {
            "runtime_disabled" => "Review the deployment and explicitly enable the runtime.",
            "approved_model_registry_empty" => "Supply a current, independently approved native model manifest and its exact fitted artifact; TEST ONLY artifacts do not qualify.",
            "approved_input_service_unconfigured" => "Configure a deployment-owned HTTPS input service with reviewed sources, reviewer identities, account bindings and fresh market/research imports.",
            "universe_source_disabled" => "Configure and enable an approved constituent provider with instrument mappings, storage rights and current classification evidence.",
            "owner_allowlist_empty" => "Supply the owner's explicit instrument allowlist in inputs.screening.owner_allowlist.",
            "approved_skill_hashes_missing" => "Pin the exact reviewed runtime skill hashes in trading.approved_skills.",
            "entry_capabilities_unverified" => "Supply current verified broker/account/instrument evidence for every unverified_entry_capabilities field, with a nonempty evidence_id and future valid_until; explicitly UNSUPPORTED fields require a compatible broker capability profile. A successful metadata or account read alone does not verify protective orders or executable quotes.",
            "approved_formula_unavailable" => "Supply an active policy-compatible formula authority with matching prediction authority, proof, parity, independent evaluation and prospective shadow evidence.",
            "live_release_expired" => "Renew the independently reviewed release evidence for this exact deployment scope.",
            "execution_stage_not_mutating" => "Select SUPERVISED_DEMO for one approved test order or AUTONOMOUS_DEMO after the acceptance gate; SHADOW cannot submit broker mutations.",
            "iex_watchdog_disabled" => "Enable the IEX30 watchdog and keep its exact 30-symbol schedule active before enabling unattended entries.",
            "authority_integrity_failed" => "Restore the canonical Donchian authority, specification, IEX30 universe, runtime configuration and implementation hashes; live execution remains blocked until the integrity manifest verifies.",
            "entries_paused" => "Resume entries only after the independent release gates and current capability evidence are complete.",
            "close_position_latched" => "Resolve the owner close-position latch through flat reconciliation before enabling entries.",
            "authority_entries_paused" => "Complete the prospective shadow, paper-fill, risk and attributable release gates before unpausing this authority.",
            _ => unreachable!("every configuration blocker has a required action"),
        };
        json!({"code":code,"required":required})
    }).collect();
    Ok(json!({
        "status":"configuration_checked","account_id":config.trading.account_id,
        "mode":config.trading.mode,"execution_stage":config.execution_stage,
        "entry_authorized":false,
        "configuration_ready":blockers.is_empty(),"blockers":blockers,
        "required_actions":required_actions,"approved_models":advisor.model_count(),
        "unverified_entry_capabilities":unverified_entry_capabilities,
        "unsupported_entry_capabilities":unsupported_entry_capabilities,
        "entry_controls_enabled":config.enabled && !config.controls.pause_entries && !config.controls.close_position && (!broker_entry_requirements || !integrity_entries_paused),
        "authority_integrity":integrity,
        "authority_integrity_error":integrity_error,
        "order_submission_policy":if config.trading.mode == Mode::Shadow { "record_only" } else { "broker_execution" }
        ,"market_data":{"provider":if config.yfinance_market_data.as_ref().is_some_and(|data| data.enabled) {"yfinance-rs"} else if config.alpaca_market_data.as_ref().is_some_and(|data| data.enabled) {"alpaca"} else {"input_service"},"feed":if config.yfinance_market_data.as_ref().is_some_and(|data| data.enabled) {"yahoo_quote"} else if config.alpaca_market_data.as_ref().is_some_and(|data| data.enabled) {"iex"} else {"configured"},"universe":if config.yfinance_market_data.as_ref().is_some_and(|data| data.enabled) {"configured_euronext"} else {"IEX30"}}
        ,"watchdog":{"enabled":config.watchdog.enabled,"runtime_wired":true,"lifecycle":"holding_scoped","interval_seconds":config.watchdog.interval_seconds,"symbols":config.watchdog.symbols.len()}
    }))
}

pub fn run(args: &[String]) -> Result<Value> {
    if args == ["analyst-worker"] {
        worker::serve()?;
        return Ok(Value::Null);
    }
    require(
        args.len() >= 3 && args[1] == "--config",
        "usage: trady autonomous check|run|recover-order --config FILE",
    )?;
    let action = args[0].as_str();
    let path = &args[2];
    let config = load(path)?;
    if action == "analyst-service" {
        require(
            args.len() == 3,
            "usage: trady autonomous analyst-service --config FILE",
        )?;
        worker::serve_external(config, path)?;
        return Ok(json!({"status":"analyst_service_stopped"}));
    }
    if action == "holding" || action == "review-holding" {
        return holding_advice(action, args, &config, path);
    }
    if action == "check" {
        require(args.len() == 3, "unsupported autonomous check arguments")?;
        return configuration_check(&config, path);
    }
    if config.trading.mode == Mode::Live
        && config
            .trading
            .limits_authority
            .as_ref()
            .and_then(|authority| authority.strategy.as_ref())
            .is_some()
    {
        let integrity = donchian_integrity(&config, path)?.ok_or_else(|| {
            Error("live execution requires a deterministic authority integrity manifest".into())
        })?;
        require(
            integrity.status == DonchianReleaseState::Approved
                && integrity.live_enabled
                && !integrity.entries_paused,
            "live execution is blocked until the Donchian release gates approve this exact deployment",
        )?;
    }
    require(
        matches!(action, "run" | "recover-order"),
        "unsupported autonomous action",
    )?;
    if action == "recover-order" {
        require(
            args.len() == 7 && args[3] == "--intent" && args[5] == "--broker-order",
            "usage: trady autonomous recover-order --config FILE --intent ID --broker-order ID",
        )?;
    } else {
        require(
            args.len() == 3 || (args.len() == 4 && args[3] == "--once"),
            "unsupported autonomous run arguments",
        )?;
    }
    require(
        config.enabled,
        "autonomous runtime is disabled in deployment configuration",
    )?;
    if config.trading.mode == Mode::Shadow {
        require(
            action == "run",
            "shadow mode records proposals and has no broker orders to recover",
        )?;
        return shadow::run(config, path, args.len() == 4);
    }
    if action == "run" {
        require(
            config.execution_stage == ExecutionStage::AutonomousDemo,
            "autonomous run requires execution_stage AUTONOMOUS_DEMO",
        )?;
    } else {
        require(
            config.execution_stage.permits_broker_mutation()
                || config.execution_stage.management_only(),
            "recovery requires a supervised or management execution stage",
        )?;
    }
    // Run must perform the credential-free gate before loading any broker or
    // database secret. A paper/demo config with missing evidence therefore
    // fails without constructing a network client.
    let preflight = configuration_check(&config, path)?;
    require(
        preflight["configuration_ready"].as_bool() == Some(true),
        "autonomous preflight failed; inspect the configuration report",
    )?;
    let store = CredentialStore::load()?;
    let credentials = Credentials::from_store(config.broker.environment, &store)?;
    let mut broker = AutonomousTrading212::new(config.broker.clone(), credentials)
        .map_err(|e| Error(e.to_string()))?;
    let mut journal = PgTradingJournal::connect(store.required("TRADY_EXECUTOR_DSN")?.expose())?;
    if let Some(binding) = &config.release_candidate {
        journal.bind_live_release(binding.clone())?;
    }
    if action == "recover-order" {
        let intent = IntentId::new(&args[4])?;
        let worker = WorkerId::new(trady_application::harness::new_id("recovery"))?;
        let trader =
            AutonomousTrader::start(config.trading.clone(), worker, &mut journal, &SystemClock)?;
        let result = trady_application::autonomous::recover_order(
            &mut journal,
            &mut broker,
            &trader.fence,
            &intent,
            &args[6],
            &SystemClock,
        );
        let released = journal.release(&trader.fence);
        let report = result?;
        released?;
        return serde_json::to_value(report)
            .map_err(|_| Error("recovery output encoding failed".into()));
    }
    let once = args.len() == 4;
    let token = store
        .optional("TRADY_TRADING_INPUTS_TOKEN")?
        .map(|s| s.into_inner());
    let stopped = Arc::new(AtomicBool::new(false));
    let mut iex_stream = if let Some(alpaca) = config
        .alpaca_market_data
        .as_ref()
        .filter(|data| data.enabled)
    {
        let key = store.required("TRADY_ALPACA_API_KEY")?.into_inner();
        let secret = store.required("TRADY_ALPACA_SECRET_KEY")?.into_inner();
        Some(IexStreamHandle::new(alpaca.clone(), key, secret)?)
    } else {
        None
    };
    let mut inputs = if config.analyst_socket.is_some() {
        worker::IsolatedInputs::new_external(config.clone(), path, token, stopped.clone())?
    } else {
        worker::IsolatedInputs::new(
            config.clone(),
            path,
            store.required("TRADY_ANALYST_DSN")?.expose().to_owned(),
            token,
            stopped.clone(),
        )?
    };
    let signal = stopped.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
        .map_err(|_| Error("cannot install trading shutdown handler".into()))?;
    let worker = WorkerId::new(trady_application::harness::new_id("trader"))?;
    let trader =
        AutonomousTrader::start(config.trading.clone(), worker, &mut journal, &SystemClock)?;
    if let Some(stream) = &iex_stream {
        inputs.with_iex_stream_cache(stream.cache());
    }
    let result = (|| {
        let mut cycles = 0_u64;
        let mut holding_active = journal.load(&trader.fence)?.plan.is_some();
        let mut saw_holding = holding_active;
        let mut last_watchdog_bucket = None;
        let mut termination = "executor_stopped";
        loop {
            if stopped.load(Ordering::Relaxed) {
                break;
            }
            if let Some(stream) = iex_stream.as_mut()
                && !stream.started()
                && should_start_iex_stream(&mut inputs, &config, holding_active, Utc::now())
            {
                stream.start()?;
            }
            let report = trader.cycle(
                &mut journal,
                &mut broker,
                &mut inputs,
                &config.controls,
                &SystemClock,
            );
            cycles = cycles
                .checked_add(1)
                .ok_or_else(|| Error("cycle counter exhausted".into()))?;
            match report {
                Ok(report) => {
                    if !matches!(report.state, PositionState::Flat | PositionState::Closed) {
                        holding_active = true;
                        saw_holding = true;
                    } else if report.state == PositionState::Closed {
                        holding_active = false;
                    }
                    super::output(
                        serde_json::to_value(&report)
                            .map_err(|_| Error("cycle report encoding failed".into()))?,
                    )?;
                    if saw_holding && report.state == PositionState::Closed {
                        if let Some(stream) = iex_stream.as_mut() {
                            stream.stop()?;
                        }
                        termination = "holding_closed";
                        break;
                    }
                    if holding_active
                        && config.watchdog.enabled
                        && let Some(stream) = iex_stream.as_ref()
                    {
                        let state = journal.load(&trader.fence)?;
                        let actual_holding = state.plan.is_some()
                            && state
                                .last_snapshot
                                .as_ref()
                                .is_some_and(|snapshot| !snapshot.positions.is_empty());
                        if actual_holding {
                            let observation =
                                iex_watchdog_observation(&config, stream, &state, Utc::now())?;
                            if last_watchdog_bucket != Some(observation.bucket) {
                                journal.record_watchdog_observation(&trader.fence, &observation)?;
                                super::output(json!({
                                    "watchdog": observation,
                                    "entry_authorized": false,
                                }))?;
                                last_watchdog_bucket = Some(observation.bucket);
                            }
                        }
                    }
                }
                Err(error) if once => return Err(error),
                Err(error) => {
                    // Check fencing before continuing. A stale worker must not reacquire implicitly.
                    journal.renew(&trader.fence, 120)?;
                    super::output(
                        json!({"status":"cycle_failed","reason":error.to_string(),"entry_authorized":false}),
                    )?;
                }
            }
            if once {
                break;
            }
            for _ in 0..config.interval_seconds * 10 {
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Ok(
            json!({"status":termination,"cycles":cycles,"broker_protection_retained":true,"iex_stream_stopped":iex_stream.as_ref().is_none_or(|stream| !stream.started())}),
        )
    })();
    let released = journal.release(&trader.fence);
    let value = result?;
    released?;
    Ok(value)
}

fn holding_advice(
    action: &str,
    args: &[String],
    config: &RuntimeConfig,
    path: &str,
) -> Result<Value> {
    use trady_application::perpetual::{PerpetualRepository, assess, validate_review_for_scenario};
    use trady_application::screening::ScreeningRepository;
    use trady_domain::perpetual::{AssessmentInfluence, AssessmentStatus, PerpetualReview};
    use trady_storage::trading_research::PgTradingResearch;

    require(
        args.len() == 5,
        "usage: trady autonomous holding --config FILE --position ID | review-holding --config FILE --request FILE",
    )?;
    let credentials = CredentialStore::load()?;
    let mut repository = PgTradingResearch::connect(
        credentials.required("TRADY_ANALYST_DSN")?.expose(),
        config.trading.owner_id.clone(),
        config.trading.account_id.clone(),
    )?;
    if action == "holding" {
        require(args[3] == "--position", "holding requires --position ID")?;
        return serde_json::to_value(repository.load_verified_holding(&args[4])?)
            .map_err(|_| Error("cannot encode verified holding".into()));
    }
    require(
        args[3] == "--request",
        "review-holding requires --request FILE",
    )?;
    let bytes = bounded_file(Path::new(&args[4]), 4 * 1024 * 1024)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| Error("invalid holding request text".into()))?;
    let mut request: HoldingReviewRequest =
        serde_json::from_value(parse_json_bounded(text, 4 * 1024 * 1024)?)
            .map_err(|_| Error("invalid holding review request fields".into()))?;
    let now = Utc::now();
    let position_id = request
        .context
        .scenario
        .binding
        .arguments
        .position_id
        .clone()
        .ok_or_else(|| Error("holding request requires a position reference".into()))?;
    let holding = repository.load_verified_holding(&position_id)?;
    let calculation_id = holding.calculation_id(&request.context.scenario.binding, now)?;
    require(
        request
            .context
            .scenario
            .binding
            .arguments
            .scenario_calculation_id
            .is_empty()
            || request
                .context
                .scenario
                .binding
                .arguments
                .scenario_calculation_id
                == calculation_id,
        "holding request contains a stale calculation identity",
    )?;
    request
        .context
        .scenario
        .binding
        .arguments
        .scenario_calculation_id = calculation_id;
    holding.validate_context(&request.context, now)?;
    let retention = request
        .context
        .scenario
        .valid_until
        .min(request.context.features.valid_until);
    repository.store_screening_snapshot(&request.screening)?;
    repository.store_dossier(&request.research.dossier, retention)?;
    repository.store_context(&request.context, retention)?;
    let advisor = models(config, path)?;
    let args = request.context.scenario.binding.arguments.clone();
    let assessment = assess(&mut repository, &advisor, &args, Utc::now())?;
    require(
        assessment.status == AssessmentStatus::Ok,
        "native holding advisor abstained; plan is unchanged",
    )?;
    let review = PerpetualReview {
        assessment_id: assessment.assessment_id.clone(),
        influence: AssessmentInfluence::Used,
        reason: request.review_reason,
    };
    validate_review_for_scenario(
        &mut repository,
        &review,
        &request.context.scenario.binding,
        &request.context.scenario.market,
        Utc::now(),
    )?;
    let record = repository
        .load_assessment(&assessment.assessment_id)?
        .ok_or_else(|| Error("persisted holding assessment disappeared".into()))?;
    let mut update = HoldingUpdate {
        id: String::new(),
        position_id,
        expected_plan_version: holding.plan.holding_state.plan().version,
        proposed: request.proposed,
        created_at: now,
        valid_until: assessment.valid_until.min(retention),
        research: request.research,
        assessment: record,
        review,
    };
    update.id = format!(
        "holding_update_{}",
        trady_application::optimization::digest(&update)?
    );
    repository.queue_holding_update(&update)?;
    Ok(
        json!({"status":"holding_review_queued","update_id":update.id,"position_id":update.position_id,
        "valid_until":update.valid_until,"entry_authorized":false}),
    )
}

fn should_start_iex_stream(
    inputs: &mut worker::IsolatedInputs,
    config: &RuntimeConfig,
    holding_active: bool,
    now: chrono::DateTime<Utc>,
) -> bool {
    if holding_active {
        return true;
    }
    let Some(session) = inputs
        .session(&config.trading.account_id, now)
        .ok()
        .flatten()
    else {
        return false;
    };
    now >= session.opens_at
        - ChronoDuration::seconds(
            i64::try_from(
                config
                    .alpaca_market_data
                    .as_ref()
                    .map_or(1_800, |data| data.stream_start_lead_seconds),
            )
            .unwrap_or(1_800),
        )
        && now < session.closes_at
}

fn iex_watchdog_observation(
    config: &RuntimeConfig,
    stream: &IexStreamHandle,
    state: &TradingState,
    now: chrono::DateTime<Utc>,
) -> Result<IexWatchdogObservation> {
    let tick = config
        .watchdog
        .ticks(now)?
        .into_iter()
        .next()
        .ok_or_else(|| Error("IEX watchdog has no symbols".into()))?;
    let health = stream.health()?;
    let cache = stream.cache();
    let stale_symbols = config
        .watchdog
        .symbols
        .iter()
        .filter_map(|symbol| {
            let instrument = InstrumentId::new(format!("{symbol}_US_EQ")).ok()?;
            cache
                .latest(
                    &instrument,
                    now,
                    u64::from(config.watchdog.quote_max_age_seconds),
                )
                .is_err()
                .then_some(symbol.clone())
        })
        .collect::<Vec<_>>();
    let held_symbol = state
        .plan
        .as_ref()
        .map(|plan| trady_domain::iex::symbol(&plan.candidate.instrument_id).map(str::to_owned))
        .transpose()?;
    let quote_age_seconds = state.plan.as_ref().and_then(|plan| {
        cache
            .quote_age_seconds(&plan.candidate.instrument_id, now)
            .ok()
            .flatten()
    });
    let status = if !stream.started() {
        WatchdogStatus::Stopped
    } else if !health.connected || !stale_symbols.is_empty() {
        WatchdogStatus::Degraded
    } else {
        WatchdogStatus::Healthy
    };
    Ok(IexWatchdogObservation {
        bucket: tick.bucket,
        observed_at: now,
        status,
        holding: state.plan.is_some()
            && state
                .last_snapshot
                .as_ref()
                .is_some_and(|snapshot| !snapshot.positions.is_empty()),
        held_symbol,
        stream_connected: health.connected,
        stream_reconnects: health.reconnects,
        last_error: health.last_error,
        last_message_at: health.last_message_at,
        last_quote_at: health.last_quote_at,
        quote_age_seconds,
        stale_symbols,
    })
}

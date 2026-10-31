//! Explicit deployment entrypoint. Enabling a reviewed config runs the executor, never chat tools.

#[path = "autonomous_shadow.rs"]
mod shadow;
#[path = "autonomous_worker.rs"]
mod worker;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use trady_adapters::{
    SystemClock,
    autonomous::TradingInputsConfig,
    credentials::CredentialStore,
    perpetual::{ApprovedArtifact, MAX_ARTIFACT_BYTES, NativePerpetualAdvisor},
    screening::ApprovedUniverseConfig,
    trading212::{AutonomousTrading212, Credentials, Trading212Config},
};
use trady_application::{autonomous::*, contracts::parse_json_bounded};
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
    interval_seconds: u64,
    trading: TradingConfig,
    broker: Trading212Config,
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
        self.broker.validate().map_err(|e| Error(e.to_string()))?;
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
        let advisor = models(&config, path)?;
        let mut blockers = vec![];
        if !config.enabled {
            blockers.push("runtime_disabled");
        }
        if advisor.model_count() == 0 {
            blockers.push("approved_model_registry_empty");
        }
        if !config.universe.enabled {
            blockers.push("universe_source_disabled");
        }
        if config.trading.approved_skills.is_empty() {
            blockers.push("approved_skill_hashes_missing");
        }
        if config.trading.limits_authority.as_ref().is_none_or(|a| {
            a.validate_active(&config.trading.policy, Utc::now())
                .is_err()
        }) {
            blockers.push("approved_formula_unavailable");
        }
        if config.trading.validate(Utc::now()).is_err() {
            blockers.push("live_release_expired");
        }
        return Ok(
            json!({"status":"configuration_checked","account_id":config.trading.account_id,"mode":config.trading.mode,"entry_authorized":false,"blockers":blockers,"approved_models":advisor.model_count()}),
        );
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
    let trader = AutonomousTrader::start(config.trading, worker, &mut journal, &SystemClock)?;
    let result = (|| {
        let mut cycles = 0_u64;
        loop {
            if stopped.load(Ordering::Relaxed) {
                break;
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
                Ok(report) => super::output(
                    serde_json::to_value(report)
                        .map_err(|_| Error("cycle report encoding failed".into()))?,
                )?,
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
        Ok(json!({"status":"executor_stopped","cycles":cycles,"broker_protection_retained":true}))
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

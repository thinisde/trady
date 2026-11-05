//! EUR service entrypoint. Retired runtimes are rejected before credentials load.
use chrono::Utc;
use serde_json::{Value, json};
use std::fs;
use trady_application::eur_runtime::EurRuntimeConfig;
use trady_domain::{Error, ExecutionStage, Result, require};

pub const HELP: &str = "\nEUR autonomous service:\n  trady autonomous check --config EUR_CONFIG\n  trady autonomous run --config EUR_CONFIG [--once]\n\nThis entrypoint accepts only EUR configurations. SHADOW never submits orders.\nDEMO execution uses eur demo-run and the exact release/owner gates; LIVE remains blocked.\n";

pub fn run(args: &[String]) -> Result<Value> {
    require(
        args.len() >= 3 && args[1] == "--config",
        "usage: trady autonomous check|run --config EUR_CONFIG [--once]",
    )?;
    let action = args[0].as_str();
    require(
        (action == "check" && args.len() == 3)
            || (action == "run" && (args.len() == 3 || (args.len() == 4 && args[3] == "--once"))),
        "unsupported EUR autonomous arguments; legacy execution commands were retired",
    )?;
    let path = &args[2];
    let text = fs::read_to_string(path)
        .map_err(|_| Error("cannot read EUR runtime configuration".into()))?;
    let config: EurRuntimeConfig = serde_json::from_str(&text).map_err(|_| {
        Error("only EUR runtime configurations are supported; legacy runtimes were retired".into())
    })?;
    if config.shadow.is_some() {
        return super::eur_shadow::autonomous(action, args, path);
    }
    let validation = config.validate(Utc::now());
    if action == "check" {
        let mut blockers = Vec::new();
        if let Err(error) = validation {
            blockers.push(error.to_string());
        }
        if !config.enabled {
            blockers.push("runtime_disabled".into());
        }
        if config.execution_stage != ExecutionStage::Shadow {
            blockers.push("first_real_input_run_requires_SHADOW".into());
        }
        blockers.extend(
            [
                "reviewed_catalog_crosswalk_required",
                "real_input_shadow_orchestration_not_wired",
                "durable_gateway_delivery_not_wired",
            ]
            .into_iter()
            .map(String::from),
        );
        return Ok(json!({
            "status":"EUR_SHADOW_BLOCKED", "runtime_mode":config.execution_stage,
            "account_id":config.account_id, "broker_environment":config.broker.environment,
            "real_market_inputs":config.requires_real_inputs(), "postgresql_required":true,
            "telegram_required":config.telegram_enabled, "broker_mutations":0,
            "entry_authorized":false, "blockers":blockers, "config_path":path
        }));
    }
    validation?;
    require(
        config.enabled,
        "EUR runtime is disabled in deployment configuration",
    )?;
    require(
        config.execution_stage == ExecutionStage::Shadow,
        "EUR autonomous first run must remain in SHADOW mode",
    )?;
    Err(Error("EUR real-input shadow orchestration requires the reviewed catalog crosswalk, complete source capture, PostgreSQL and Telegram deployment bindings; no cycle has run".into()))
}

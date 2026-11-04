//! EUR target authority diagnostics and offline selection replay.

use chrono::Utc;
use serde_json::{Value, json};
use std::{fs, io::Read};
use trady_application::eur_runtime::{EurRuntimeConfig, select_candidate};
use trady_domain::{Error, FallbackAttempt, RankedCandidate, Result, SelectionRun, require};

pub const HELP: &str = "\nEUR day-trading target:\n  trady eur config --config FILE\n  trady eur gate-explain --config FILE\n  trady eur universe-check --config FILE\n  trady eur select --run FILE --candidates FILE [--fallback FILE]\n\nSelection replay is research-only; Trading 212 mutations remain behind the application order gate.\n";

pub fn run(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error("missing EUR command".into()))?;
    match command {
        "config" => {
            let path = option(args, "--config")?;
            require(args.len() == 3, "usage: trady eur config --config FILE")?;
            let config: EurRuntimeConfig = read_json(&path)?;
            config.validate(Utc::now())?;
            Ok(json!({
                "authority": "eur_intraday_v1",
                "account_id": config.account_id,
                "execution_stage": config.execution_stage,
                "opencode_model": config.opencode_model,
                "trading212_only": config.trading212_only,
                "enabled": config.enabled,
                "entry_authorized": false
            }))
        }
        "gate-explain" => {
            let path = option(args, "--config")?;
            require(
                args.len() == 3,
                "usage: trady eur gate-explain --config FILE",
            )?;
            let config: EurRuntimeConfig = read_json(&path)?;
            config.validate(Utc::now())?;
            Ok(json!({
                "gate_revision": "eur_order_gate_v1",
                "entry_authorized": false,
                "blockers": [
                    "target runtime is disabled in the checked-in example until an owner enables the reviewed demo run",
                    "fresh exact venue-compatible yfinance bid/ask is required",
                    "Trading 212 demo instrument and protection capability evidence is required",
                    "approved EUR universe and same-day session deadline are required",
                    "selection, deterministic strategy signal, cost-aware sizing and idempotent intent are required",
                    "a durable target journal and Telegram lifecycle sink are required before enabled mutation"
                ],
                "rotation": {
                    "enabled": config.policy.rotation_enabled,
                    "max_open_positions": config.policy.max_open_positions,
                    "minimum_net_improvement": config.policy.minimum_rotation_net_improvement_fraction
                }
            }))
        }
        "universe-check" => {
            let path = option(args, "--config")?;
            require(
                args.len() == 3,
                "usage: trady eur universe-check --config FILE",
            )?;
            let universe: trady_domain::EurUniverseSnapshot = read_json(&path)?;
            universe.validate(Utc::now())?;
            Ok(json!({
                "snapshot_id": universe.snapshot_id,
                "revision": universe.revision,
                "members": universe.members.len(),
                "currency": "EUR",
                "entry_authorized": false
            }))
        }
        "select" => select(args),
        _ => Err(Error(format!("unsupported EUR command\n{HELP}"))),
    }
}

fn select(args: &[String]) -> Result<Value> {
    let run_path = option(args, "--run")?;
    let candidates_path = option(args, "--candidates")?;
    let fallback_path = option_optional(args, "--fallback");
    let run: SelectionRun = read_json(&run_path)?;
    let candidates: Vec<RankedCandidate> = read_json(&candidates_path)?;
    let fallback: Vec<FallbackAttempt> = match fallback_path {
        Some(path) => read_json(&path)?,
        None => run.fallback_attempts.clone(),
    };
    let proposal = run.proposal.clone();
    let result = select_candidate(run.clone(), candidates, fallback, proposal, Utc::now())?;
    Ok(json!({
        "selection": result.clone(),
        "entry_authorized": false,
        "mandatory_selection_satisfied": result.selected_instrument_id.is_some(),
        "operational_failure": result.failure
    }))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    let mut file = fs::File::open(path).map_err(|_| Error("cannot read EUR input file".into()))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(2_000_000)
        .read_to_end(&mut bytes)
        .map_err(|_| Error("cannot read EUR input file".into()))?;
    require(
        bytes.len() <= 2_000_000,
        "EUR input file exceeds size limit",
    )?;
    serde_json::from_slice(&bytes).map_err(|_| Error("invalid EUR JSON input".into()))
}

fn option(args: &[String], name: &str) -> Result<String> {
    option_optional(args, name).ok_or_else(|| Error(format!("missing {name}")))
}

fn option_optional(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

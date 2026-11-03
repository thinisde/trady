//! Operator-visible IEX30 watchdog validation and schedule inspection.
use chrono::Utc;
use serde_json::{Value, json};
use std::fs;
use trady_application::watchdog::IexWatchdogConfig;
use trady_domain::{Error, Result, require};

pub const HELP: &str = "\nIEX30 Watchdog:\n  trady watchdog check --config FILE\n  trady watchdog schedule --config FILE\n";

pub fn run(args: &[String]) -> Result<Value> {
    require(
        args.len() == 3 && args[1] == "--config",
        "usage: trady watchdog check|schedule --config FILE",
    )?;
    let config: IexWatchdogConfig = serde_json::from_str(
        &fs::read_to_string(&args[2])
            .map_err(|_| Error("cannot read watchdog configuration".into()))?,
    )
    .map_err(|_| Error("invalid watchdog configuration".into()))?;
    config.validate()?;
    match args[0].as_str() {
        "check" => Ok(
            json!({"status":"valid","enabled":config.enabled,"runtime_wired":true,"lifecycle":"holding_scoped","universe":"IEX30","symbols":config.symbols,"interval_seconds":config.interval_seconds,"quote_max_age_seconds":config.quote_max_age_seconds,"x_source_ids":config.x_source_ids,"entry_authorized":false}),
        ),
        "schedule" => Ok(
            json!({"status":"scheduled","universe":"IEX30","ticks":config.ticks(Utc::now())?,"entry_authorized":false}),
        ),
        _ => Err(Error("unsupported watchdog action".into())),
    }
}

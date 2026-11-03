//! Read-only Alpaca stream, quote, and history commands.

use chrono::{Duration, Utc};
use serde_json::{Value, json};
use std::fs;
use trady_adapters::{
    alpaca::{AlpacaDataConfig, AlpacaMarketData, IEX_STREAM_URL},
    credentials::CredentialStore,
};
use trady_domain::{Error, InstrumentId, Result, require};

pub const HELP: &str = "\nAlpaca IEX market data (read-only; Trading 212 remains the execution broker):\n  trady alpaca stream-plan --config FILE\n  trady alpaca quote --config FILE --instrument SYMBOL_US_EQ\n  trady alpaca history --config FILE --instrument SYMBOL_US_EQ [--start RFC3339] [--end RFC3339]\n\nThe stream plan is fixed to the 30 approved IEX symbols. Alpaca commands never submit or cancel orders.\n";

pub fn run(args: &[String]) -> Result<Value> {
    require(
        args.len() >= 3,
        "usage: trady alpaca stream-plan|quote|history --config FILE",
    )?;
    let action = args[0].as_str();
    let config_path = option(args, "--config")?;
    let text = fs::read_to_string(config_path)
        .map_err(|_| Error("cannot read Alpaca configuration".into()))?;
    let config: AlpacaDataConfig = serde_json::from_str(&text)
        .map_err(|_| Error("invalid Alpaca configuration fields".into()))?;
    config.validate()?;
    if action == "stream-plan" {
        require(args.len() == 3, "unsupported Alpaca stream-plan arguments")?;
        let subscription = trady_adapters::alpaca::IexStreamSubscription::default();
        let message = subscription.subscribe_message()?;
        return Ok(json!({
            "provider":"alpaca", "feed":"iex", "stream_url":IEX_STREAM_URL,
            "symbols":subscription.symbols, "channels":["trades","quotes"],
            "authentication":"message", "auth_required":true,
            "subscription":message, "entry_authorized":false
        }));
    }
    require(config.enabled, "Alpaca market data is disabled")?;
    let store = CredentialStore::load()?;
    let api_key = store.required("TRADY_ALPACA_API_KEY")?.into_inner();
    let secret = store.required("TRADY_ALPACA_SECRET_KEY")?.into_inner();
    let data = AlpacaMarketData::new(config.clone(), api_key, secret)?;
    match action {
        "quote" => {
            let instrument = InstrumentId::new(option(args, "--instrument")?)?;
            Ok(
                json!({"provider":"alpaca","environment":config.environment,"feed":config.feed,"entry_authorized":false,"quote":data.latest_quote(&instrument, Utc::now())?}),
            )
        }
        "history" => {
            let instrument = InstrumentId::new(option(args, "--instrument")?)?;
            let end = optional(args, "--end")
                .map(|v| parse_time(&v))
                .transpose()?
                .unwrap_or_else(Utc::now);
            let start = optional(args, "--start")
                .map(|v| parse_time(&v))
                .transpose()?
                .unwrap_or_else(|| end - Duration::days(90));
            Ok(
                json!({"provider":"alpaca","environment":config.environment,"feed":config.feed,"entry_authorized":false,"bars":data.daily_bars(&instrument, start, Some(end))?}),
            )
        }
        _ => Err(Error("unsupported Alpaca action".into())),
    }
}

fn parse_time(value: &str) -> Result<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| Error("timestamp must be RFC3339".into()))
}

fn option(args: &[String], name: &str) -> Result<String> {
    optional(args, name).ok_or_else(|| Error(format!("missing {name}")))
}

fn optional(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

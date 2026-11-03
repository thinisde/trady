//! Read-only Yahoo Finance quote command.

use chrono::Utc;
use serde_json::{Value, json};
use std::fs;
use trady_adapters::yfinance::{YfinanceDataConfig, YfinanceMarketData};
use trady_domain::{Error, InstrumentId, Result, require};

pub const HELP: &str = "\nYahoo Finance Euronext market data (read-only):\n  trady yfinance quote --config FILE --instrument INSTRUMENT_ID\n\nThe configuration owns the exact internal-instrument to Yahoo-symbol mapping. Quotes require a fresh bid/ask and are never entry authorization.\n";

pub fn run(args: &[String]) -> Result<Value> {
    require(
        args.first().map(String::as_str) == Some("quote"),
        "usage: trady yfinance quote --config FILE --instrument INSTRUMENT_ID",
    )?;
    let config_path = option(args, "--config")?;
    let instrument = InstrumentId::new(option(args, "--instrument")?)?;
    require(args.len() == 5, "unsupported yfinance quote arguments")?;
    let text = fs::read_to_string(config_path)
        .map_err(|_| Error("cannot read yfinance configuration".into()))?;
    let config: YfinanceDataConfig = serde_json::from_str(&text)
        .map_err(|_| Error("invalid yfinance configuration fields".into()))?;
    config.validate()?;
    require(config.enabled, "yfinance market data is disabled")?;
    let data = YfinanceMarketData::new(config)?;
    let quote = data.latest_snapshot(&instrument, Utc::now())?;
    Ok(json!({
        "provider": "yfinance-rs",
        "symbol": data.symbol_for(&instrument)?,
        "entry_authorized": false,
        "quote": quote
    }))
}

fn option(args: &[String], name: &str) -> Result<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| Error(format!("missing {name}")))
}

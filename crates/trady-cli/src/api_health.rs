use chrono::Utc;
use serde_json::Value;
use trady_domain::{Error, Result, require};

pub const HELP: &str = "\n  trady api-health [--online] [--provider NAME]\n\nDefault: credential metadata only. --online sends one bounded GET per selected API using fixed routes. Results contain no credentials or raw provider payloads. X requires the existing durable cost gate; yfinance remains offline. No database, collection-setting or broker changes.\n";

pub fn run(args: &[String]) -> Result<Value> {
    let mut online = false;
    let mut provider = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        require(seen.insert(flag), "duplicate API health option")?;
        match flag {
            "--online" => {
                online = true;
                i += 1;
            }
            "--provider" => {
                provider = Some(
                    args.get(i + 1)
                        .ok_or_else(|| Error("API health provider missing".into()))?
                        .as_str(),
                );
                i += 2;
            }
            _ => return Err(Error("unknown API health option".into())),
        }
    }
    trady_adapters::api_health::run(online, provider, Utc::now())
}

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::{fs::File, io::Read};
use trady_application::{
    contracts::parse_json,
    stock_report::{ReportInput, generate},
};
use trady_domain::{Error, Result, require};

pub const HELP: &str = "\n  trady research-report FILE [--as-of RFC3339]\n\nBuild an offline US/EU stock research report for a 6..12 month moderate-risk horizon. Validates dated source references and period-matched financials; calculates growth, margins and optional trailing P/E. Missing or stale evidence remains visible. No network, database or broker operations.\n";

pub fn run(args: &[String]) -> Result<Value> {
    require(
        matches!(args.len(), 1 | 3),
        "research-report requires FILE [--as-of RFC3339]",
    )?;
    let as_of = if args.len() == 3 {
        require(args[1] == "--as-of", "unknown research-report option")?;
        DateTime::parse_from_rfc3339(&args[2])
            .map_err(|_| Error("invalid stock report cutoff".into()))?
            .with_timezone(&Utc)
    } else {
        Utc::now()
    };
    let mut bytes = vec![];
    File::open(&args[0])
        .map_err(|_| Error("stock report input unavailable".into()))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error("stock report input unreadable".into()))?;
    require(
        bytes.len() <= 1024 * 1024,
        "stock report input exceeds 1 MiB",
    )?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error("stock report input must be UTF-8".into()))?;
    let value = parse_json(text).map_err(|_| Error("invalid stock report JSON".into()))?;
    let input: ReportInput =
        serde_json::from_value(value).map_err(|_| Error("invalid stock report fields".into()))?;
    generate(&input, as_of)
}

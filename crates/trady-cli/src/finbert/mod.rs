use std::{fs::File, io::Read};

use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use trady_adapters::finbert::{FinBertClient, FinBertConfig};
use trady_application::sentiment::assess_sentiment;
use trady_domain::{Error, Result, require, research::SourceItem};
use trady_policy::{ConfidenceInputs, Policy, confidence, sentiment::sentiment_thresholds};

pub const HELP: &str = "\n  trady sentiment status --config FILE\n  trady sentiment score --config FILE --input FILE\n\nSentiment input is a news-read JSON object with items, optionally confidence_inputs.\nScoring requires an explicitly enabled FinBERT service; no order is authorized.\n";

#[derive(Deserialize)]
struct Input {
    items: Vec<SourceItem>,
    confidence_inputs: Option<ConfidenceInputs>,
}

fn read_bounded(path: &str, limit: u64) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    File::open(path)
        .map_err(|_| Error("cannot open sentiment input".into()))?
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|_| Error("cannot read sentiment input".into()))?;
    require(
        body.len() as u64 <= limit,
        "sentiment input exceeds size bound",
    )?;
    Ok(body)
}

pub fn run(args: &[String]) -> Result<Value> {
    require(
        (args.len() == 3 && args[0] == "status" && args[1] == "--config")
            || (args.len() == 5
                && args[0] == "score"
                && args[1] == "--config"
                && args[3] == "--input"),
        "use sentiment status --config FILE or sentiment score --config FILE --input FILE",
    )?;
    let config: FinBertConfig = serde_json::from_slice(&read_bounded(&args[2], 16384)?)
        .map_err(|_| Error("invalid FinBERT configuration JSON".into()))?;
    config.validate()?;
    if args[0] == "status" {
        return Ok(
            json!({"enabled":config.enabled,"network_checked":false,"entry_authorized":false}),
        );
    }
    let input: Input = serde_json::from_slice(&read_bounded(&args[4], 524288)?)
        .map_err(|_| Error("invalid sentiment source-item JSON".into()))?;
    let client = FinBertClient::new(config)?;
    let assessment = assess_sentiment(&client, &input.items, Utc::now())?;
    let thresholds = input
        .confidence_inputs
        .as_ref()
        .map(|inputs| {
            sentiment_thresholds(
                &confidence(inputs),
                &assessment,
                &Policy::default(),
                false,
                Utc::now(),
            )
        })
        .transpose()?;
    Ok(json!({"assessment":assessment,"thresholds":thresholds,
        "evidence_verified":false,"entry_authorized":false,
        "note":"Thresholds use supplied feature inputs and default policy; full readiness and plan activation remain required."}))
}

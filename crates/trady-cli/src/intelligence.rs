//! Explicit local owner commands for optional research and synthetic replay.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};
use trady_adapters::intelligence::{
    JsonFeatureSource, MemoryIntelligenceRepository, ReadOnlyFeatureSource,
};
use trady_application::{contracts::parse_json, intelligence::*};
use trady_domain::{Error, Mode, Result, intelligence::*, require};
use trady_storage::intelligence::PgIntelligenceRepository;

pub const HELP: &str = "\nResearch intelligence (all ten features enabled by default; sources require review):\n  trady intelligence status [--config FILE]\n  trady intelligence assess|tax-scenarios --config FILE --input FILE [--as-of RFC3339] [--mode PAPER|SHADOW]\n  trady intelligence replay --config FILE --input FILE\n  trady intelligence import --config FILE --input FILE\n  trady intelligence read --config FILE --feature OPTIONS|COORDINATION|EVENT|SHORT_INTEREST|INSIDER|CROSS_ASSET|ATTENTION|ACCOUNTING|MICROSTRUCTURE --entity ID [--as-of RFC3339]\n  trady intelligence invalidate --config FILE --target OBSERVATION|SOURCE|LINEAGE --id ID --reason CORRECTION|DELETED|RIGHTS_REVOKED|ENTITLEMENT_CHANGED\n  trady intelligence purge --config FILE\nNormalized imports require reviewed source rights and exact revision hashes.\nSet enabled to [] in configuration to disable every feature.\nDatabase commands use the analyst role and migrations through 11. Assess/replay use no network.\nTax is an ephemeral owner-only illustration; raw tax context never enters research storage or model tools.\n";

fn bytes(path: &str, max: usize) -> Result<String> {
    let file = File::open(path).map_err(|_| Error("intelligence input unavailable".into()))?;
    require(
        file.metadata()
            .map_err(|_| Error("intelligence input metadata unavailable".into()))?
            .is_file(),
        "intelligence input must be a regular file",
    )?;
    let mut out = String::new();
    file.take(max as u64 + 1)
        .read_to_string(&mut out)
        .map_err(|_| Error("intelligence input read failed".into()))?;
    require(out.len() <= max, "intelligence input exceeds byte limit")?;
    Ok(out)
}
pub(crate) fn configuration(path: Option<&str>) -> Result<IntelligenceConfig> {
    let text = path
        .map(|p| bytes(p, 262_144))
        .transpose()?
        .unwrap_or_else(|| include_str!("../../../config/intelligence.example.json").into());
    let config: IntelligenceConfig = serde_json::from_value(parse_json(&text)?)
        .map_err(|_| Error("invalid intelligence configuration".into()))?;
    config.validate()?;
    Ok(config)
}
fn repository() -> Result<PgIntelligenceRepository> {
    PgIntelligenceRepository::connect(&super::secret("TRADY_ANALYST_DSN")?)
}
fn required<'a>(options: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    options
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| Error(format!("{name} is required")))
}
fn enum_value<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_value(Value::String(value.into()))
        .map_err(|_| Error("invalid intelligence enum argument".into()))
}
fn timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.to_utc())
        .map_err(|_| Error("invalid intelligence timestamp".into()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayEvent {
    at: DateTime<Utc>,
    observation: FeatureObservation,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayQuery {
    at: DateTime<Utc>,
    feature: FeatureKind,
    entity_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replay {
    schema_version: u32,
    events: Vec<ReplayEvent>,
    queries: Vec<ReplayQuery>,
}

fn replay(config: &IntelligenceConfig, path: &str, now: DateTime<Utc>) -> Result<Value> {
    let replay: Replay = serde_json::from_value(parse_json(&bytes(path, config.max_input_bytes)?)?)
        .map_err(|_| Error("invalid intelligence replay".into()))?;
    require(
        replay.schema_version == 1
            && replay.events.len() <= 1024
            && replay.queries.len() <= 1024
            && replay.events.windows(2).all(|w| w[0].at <= w[1].at)
            && replay.queries.windows(2).all(|w| w[0].at <= w[1].at)
            && replay
                .events
                .iter()
                .all(|e| e.at <= now && e.observation.payload.kind() != FeatureKind::Tax)
            && replay
                .queries
                .iter()
                .all(|q| q.at <= now && q.feature != FeatureKind::Tax),
        "replay must be bounded, chronological, historical and free of private tax data",
    )?;
    let mut repository = MemoryIntelligenceRepository::default();
    let total = replay.events.len();
    let mut events = replay.events.into_iter().enumerate().peekable();
    let mut accepted = 0;
    let mut rejected_events = vec![];
    let mut results = vec![];
    for q in replay.queries {
        while events.peek().is_some_and(|(_, e)| e.at <= q.at) {
            let (index, event) = events
                .next()
                .ok_or_else(|| Error("replay cursor failed".into()))?;
            if ingest(&mut repository, config, &event.observation, event.at).is_ok() {
                accepted += 1;
            } else {
                rejected_events
                    .push(json!({"event_index":index,"at":event.at,"code":"INGESTION_REJECTED"}));
            }
        }
        results.push(read(
            &mut repository,
            config,
            ReadRequest {
                scope: &config.scope,
                feature: q.feature,
                entity: &q.entity_id,
                mode: Mode::Paper,
                audience: Audience::Owner,
                at: q.at,
                now,
            },
        )?);
    }
    for (index, event) in events {
        if ingest(&mut repository, config, &event.observation, event.at).is_ok() {
            accepted += 1;
        } else {
            rejected_events
                .push(json!({"event_index":index,"at":event.at,"code":"INGESTION_REJECTED"}));
        }
    }
    Ok(
        json!({"schema_version":1,"processed":total,"accepted":accepted,"rejected":rejected_events.len(),
        "rejected_events":rejected_events,"rejections":repository.rejections,
        "assessments":results,"entry_authorized":false}),
    )
}

pub fn run(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error(HELP.into()))?;
    let allowed: &[&str] = match command {
        "status" | "purge" => &["--config"],
        "assess" | "tax-scenarios" => &["--config", "--input", "--as-of", "--mode"],
        "replay" | "import" => &["--config", "--input"],
        "read" => &["--config", "--feature", "--entity", "--as-of"],
        "invalidate" => &["--config", "--target", "--id", "--reason"],
        _ => return Err(Error("unsupported intelligence command".into())),
    };
    let mut options = BTreeMap::new();
    require(
        (args.len() - 1).is_multiple_of(2),
        "intelligence flags require values",
    )?;
    for pair in args[1..].chunks_exact(2) {
        require(
            allowed.contains(&pair[0].as_str())
                && options.insert(pair[0].clone(), pair[1].clone()).is_none(),
            "unknown or duplicate intelligence option",
        )?;
    }
    let config = configuration(options.get("--config").map(String::as_str))?;
    let now = Utc::now();
    let at = options
        .get("--as-of")
        .map(|s| timestamp(s))
        .transpose()?
        .unwrap_or(now);
    require(at <= now, "intelligence decision cannot be future dated")?;
    let mode: Mode = options
        .get("--mode")
        .map(|s| enum_value(s))
        .transpose()?
        .unwrap_or(Mode::Paper);
    match command {
        "status" => Ok(
            json!({"schema_version":1,"registry_hash":config.revision()?,
            "features":FeatureKind::ALL.into_iter().map(|f|json!({"feature":f,"tool":f.tool(),
                "enabled":config.enabled.contains(&f),"authority":"RESEARCH_ONLY_UNCALIBRATED"})).collect::<Vec<_>>(),
            "sources":config.sources.iter().map(|s|json!({"source_id":s.source_id,"enabled":s.enabled,
                "rights_revision":s.rights_revision,"current":s.capability.current(now)})).collect::<Vec<_>>(),"entry_authorized":false}),
        ),
        "assess" | "tax-scenarios" => {
            let mut source = JsonFeatureSource {
                path: Path::new(required(&options, "--input")?),
            };
            let observation = source.observation(config.max_input_bytes)?;
            require(
                command != "tax-scenarios" || observation.payload.kind() == FeatureKind::Tax,
                "tax-scenarios requires tax input",
            )?;
            serde_json::to_value(assess_ephemeral(
                &config,
                &observation,
                mode,
                Audience::Owner,
                at,
                now,
            )?)
            .map_err(|_| Error("intelligence output serialization failed".into()))
        }
        "replay" => replay(&config, required(&options, "--input")?, now),
        "import" => {
            let mut source = JsonFeatureSource {
                path: Path::new(required(&options, "--input")?),
            };
            let observation = source.observation(config.max_input_bytes)?;
            let record = ingest(&mut repository()?, &config, &observation, now)?;
            Ok(
                json!({"observation_id":record.observation.observation_id,"ingested_at":record.ingested_at,
                "registry_hash":record.registry_hash,"entry_authorized":false}),
            )
        }
        "read" => {
            let feature: FeatureKind = enum_value(required(&options, "--feature")?)?;
            let result = read(
                &mut repository()?,
                &config,
                ReadRequest {
                    scope: &config.scope,
                    feature,
                    entity: required(&options, "--entity")?,
                    mode: Mode::Paper,
                    audience: Audience::Owner,
                    at,
                    now,
                },
            )?;
            serde_json::to_value(result)
                .map_err(|_| Error("intelligence output serialization failed".into()))
        }
        "invalidate" => {
            let reason: InvalidationReason = enum_value(required(&options, "--reason")?)?;
            let invalidation = Invalidation {
                scope: config.scope.clone(),
                target: enum_value(required(&options, "--target")?)?,
                target_id: required(&options, "--id")?.into(),
                at: now,
                reason,
                erase_content: matches!(
                    reason,
                    InvalidationReason::Deleted
                        | InvalidationReason::RightsRevoked
                        | InvalidationReason::EntitlementChanged
                ),
            };
            repository()?.invalidate(&invalidation)?;
            Ok(
                json!({"status":"INVALIDATED","target":invalidation.target,"target_id":invalidation.target_id,"entry_authorized":false}),
            )
        }
        "purge" => {
            repository()?.purge(now)?;
            Ok(json!({"status":"EXPIRED_CONTENT_ERASED","entry_authorized":false}))
        }
        _ => Err(Error("unsupported intelligence command".into())),
    }
}

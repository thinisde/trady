//! EUR authority diagnostics, bounded supervision and durable protocol routing.
use chrono::Utc;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use trady_adapters::{
    credentials::CredentialStore,
    trading212::{Credentials, Environment, InstrumentCatalog, Trading212Config, Trading212Reader},
};
use trady_application::{
    contracts::parse_json_bounded,
    eur_runtime::{
        EurRuntimeConfig, OrderGateSnapshot, OrderReadiness, ReadinessBlocker,
        configuration_readiness, select_candidate,
    },
    eur_universe::{CatalogIdentity, ReviewedInstrument, build_registry},
    harness::content_hash,
};
use trady_domain::{Error, FallbackAttempt, RankedCandidate, Result, SelectionRun, require};
use trady_storage::target::PgTargetStore;

pub const HELP: &str = "\nEUR target (disabled SHADOW by default; LIVE rejected):\n  trady eur config --config FILE\n  trady eur capture|diagnose-inputs|daily|gateway|report --config FILE [--once]\n  trady eur forward-paper --config FILE --stream DIR --out NEW_DIRECTORY [--exit-after-seconds N]\n  trady eur export --config FILE --out NEW_DIRECTORY\n  trady eur weekly|handoff --config FILE [--out FILE]\n  trady universe check --config FILE\n  trady universe snapshot --config FILE --review FILE [--catalog RECORDED_FILE] [--out FILE]\n  trady gate explain --config FILE [--snapshot FILE]\n  trady eur select --run FILE --candidates FILE [--fallback FILE]\n  trady eur demo-run --config FILE --bundle DIR --owner-approval FILE [--once]\n  trady eur demo-report --config FILE --out NEW_DIRECTORY\n  trady eur protocol-drive --config FILE --bundle DIR --owner-approval FILE --intent ID [--snapshot FILE] [--exit-reason REASON] [--once]\n  trady replay manifest-check --config FILE --manifest FILE\n  trady replay paper-day --config FILE --manifest FILE [--out NEW_DIRECTORY]\n  trady replay protocol-suite --out NEW_FILE\n\nSnapshot writes require the analyst PostgreSQL role. Crosswalk review must bind exact catalog IDs.\nManifest checking and synthetic protocol fixtures do not certify a complete paper-day replay.\n";

pub fn run(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error(HELP.into()))?;
    match command {
        "capability-attribution-check" => super::eur_attribution::command(&args[1..]),
        "config" => {
            let options = options(&args[1..], &["--config"], &["--config"])?;
            let config: EurRuntimeConfig = read_json(Path::new(&options["--config"]))?;
            config.validate(Utc::now())?;
            Ok(
                json!({"authority":"eur_intraday_v1","account_id":config.account_id,
                "enabled":config.enabled,"execution_stage":config.execution_stage,
                "live_enabled":config.live_enabled,"broker_environment":config.broker.environment,
                "entry_authorized":false,"broker_mutation_path_implemented":true,"verified_release_and_owner_approval_required":true}),
            )
        }
        "gate-explain" => gate_command(&[vec!["explain".into()], args[1..].to_vec()].concat()),
        "universe-check" => universe_command(&[vec!["check".into()], args[1..].to_vec()].concat()),
        "select" => select(&args[1..]),
        "protocol-drive" => super::eur_execute::run(&args[1..]),
        "demo-run" => super::eur_operation::run(&args[1..]),
        "demo-report" => super::eur_operation::report(&args[1..]),
        "forward-paper" => super::eur_paper::run(&args[1..]),
        "weekly" | "handoff" => super::eur_workflow::run(command, &args[1..]),
        "export" => {
            let values = options(&args[1..], &["--config", "--out"], &["--config", "--out"])?;
            super::eur_shadow::export_bundle(&values["--config"], Path::new(&values["--out"]))
        }
        "capture" | "diagnose-inputs" | "daily" | "gateway" | "report" => {
            super::eur_shadow::command(command, &args[1..])
        }
        _ => Err(Error(HELP.into())),
    }
}

pub fn gate_command(args: &[String]) -> Result<Value> {
    require(
        args.first().map(String::as_str) == Some("explain"),
        "usage: trady gate explain --config FILE [--snapshot FILE]",
    )?;
    let options = options(&args[1..], &["--config", "--snapshot"], &["--config"])?;
    let config: EurRuntimeConfig = read_json(Path::new(&options["--config"]))?;
    let now = Utc::now();
    let readiness = if let Some(path) = options.get("--snapshot") {
        let snapshot: OrderGateSnapshot = read_json(Path::new(path))?;
        snapshot.evaluate(&config, now)?
    } else {
        let mut blockers = configuration_readiness(&config, now)?;
        blockers.push(ReadinessBlocker::SnapshotUnavailable);
        OrderReadiness::Blocked { blockers }
    };
    Ok(json!({"gate_revision":"eur_order_gate_v1","checked_at":now,
        "entry_authorized":false,"readiness":readiness,
        "runtime_mode":config.execution_stage,"live_enabled":false}))
}

pub fn universe_command(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error(HELP.into()))?;
    let allowed = if command == "snapshot" {
        vec!["--config", "--review", "--catalog", "--out"]
    } else {
        vec!["--config"]
    };
    let required = if command == "snapshot" {
        vec!["--config", "--review"]
    } else {
        vec!["--config"]
    };
    let options = options(&args[1..], &allowed, &required)?;
    let path = Path::new(&options["--config"]);
    let config: EurRuntimeConfig = read_json(path)?;
    config.validate(Utc::now())?;
    match command {
        "check" => {
            let universe_path = config
                .universe_config
                .as_deref()
                .ok_or_else(|| Error("reviewed EUR universe path is missing".into()))?;
            let universe: trady_domain::EurUniverseSnapshot =
                read_json(&resolve(path, universe_path))?;
            config.validate_universe(&universe, Utc::now())?;
            Ok(
                json!({"status":"UNIVERSE_SCHEMA_VALID","snapshot_id":universe.snapshot_id,
                "members":universe.members.len(),"document_hash":hash(&universe)?,
                "catalog_identity_verified":false,"entry_authorized":false,
                "required":"universe snapshot with independent crosswalk review and exact broker catalog"}),
            )
        }
        "snapshot" => snapshot(&config, path, &options),
        _ => Err(Error(HELP.into())),
    }
}

fn snapshot(
    config: &EurRuntimeConfig,
    config_path: &Path,
    options: &BTreeMap<String, String>,
) -> Result<Value> {
    let review: Vec<ReviewedInstrument> = read_json(Path::new(&options["--review"]))?;
    // Validate reviews before loading any broker or database credential.
    for item in &review {
        item.binding.validate(Utc::now())?;
    }
    let (catalog, origin): (InstrumentCatalog, &str) = if let Some(path) = options.get("--catalog")
    {
        (read_json(Path::new(path))?, "RECORDED_CATALOG")
    } else {
        let broker_path = config
            .trading212_config
            .as_deref()
            .ok_or_else(|| Error("Trading 212 demo configuration is missing".into()))?;
        let broker: Trading212Config = read_json(&resolve(config_path, broker_path))?;
        broker
            .validate()
            .map_err(|_| Error("invalid Trading 212 demo binding".into()))?;
        require(
            broker.environment == Environment::Demo
                && broker.account_id == config.account_id
                && broker.base_currency == config.base_currency
                && broker.broker_account_id == 52211463,
            "EUR broker account, environment or currency differs from approved first-run scope",
        )?;
        let store = CredentialStore::load()?;
        let credentials = Credentials::from_store(broker.environment, &store)?;
        let mut reader =
            Trading212Reader::new(broker, credentials).map_err(|e| Error(e.to_string()))?;
        (
            reader
                .instruments_with_rejections()
                .map_err(|e| Error(e.to_string()))?,
            "TRADING212_DEMO_CATALOG",
        )
    };
    // Malformed provider tickers cannot be silently omitted from a reviewed release.
    require(
        catalog.rejected_count == catalog.rejected_instruments.len(),
        "recorded catalog is incomplete: rejection identities are missing",
    )?;
    let rows: Vec<_> = catalog
        .instruments
        .into_iter()
        .map(|row| CatalogIdentity {
            broker_ticker: row.instrument_id.to_string(),
            isin: row.isin,
            currency: row.currency.as_str().into(),
            kind: row.kind,
        })
        .collect();
    let mut registry = build_registry(&rows, &review, Utc::now())?;
    let mut rejected = catalog.rejected_instruments;
    rejected.sort_by(|a, b| {
        a.broker_ticker
            .cmp(&b.broker_ticker)
            .then(a.reason.cmp(&b.reason))
    });
    registry.catalog_hash = hash(&(&registry.catalog_hash, &rejected))?;
    for rejection in &rejected {
        registry
            .rejections
            .push(trady_application::eur_universe::CatalogRejection {
                broker_ticker: rejection.broker_ticker.clone(),
                reasons: vec![rejection.reason.clone()],
            });
    }
    registry.registry_hash = hash(&(
        &registry.catalog_hash,
        &registry.members,
        &registry.rejections,
    ))?;
    let id = format!("eur_registry_{}", &registry.registry_hash[..32]);
    let document =
        serde_json::to_value(&registry).map_err(|_| Error("registry encoding failed".into()))?;
    let credentials = CredentialStore::load()?;
    let mut database = PgTargetStore::connect(
        credentials.required("TRADY_ANALYST_DSN")?.expose(),
        "trady_analyst",
        config.owner_id.clone(),
        config.account_id.to_string(),
    )?;
    let inserted = database.append_document("instrument_registry", &id, &document)?;
    require(
        database.read_document("instrument_registry", &id)?.as_ref() == Some(&document),
        "PostgreSQL registry readback differs",
    )?;
    if let Some(path) = options.get("--out") {
        write_new(Path::new(path), &document)?;
    }
    Ok(
        json!({"status":"REGISTRY_SNAPSHOT_PERSISTED","snapshot_id":id,
        "registry_hash":registry.registry_hash,"catalog_hash":registry.catalog_hash,
        "members":registry.members.len(),"rejections":registry.rejections,"origin":origin,
        "new_record":inserted,"entry_authorized":false}),
    )
}

pub fn replay_command(args: &[String]) -> Result<Value> {
    if args.first().map(String::as_str) == Some("protocol-suite") {
        let values = options(&args[1..], &["--out"], &["--out"])?;
        let result = trady_application::eur_protocol::scenarios::run_all();
        super::eur_shadow::private_json(Path::new(&values["--out"]), &result)?;
        require(result["passed"] == true, "protocol regression suite failed")?;
        return Ok(
            json!({"status":"SYNTHETIC_PROTOCOL_SUITE_PASSED","artifact":values["--out"],
            "scenarios":result["scenarios"].as_array().map(Vec::len),"broker_capability_proven":false,"entry_authorized":false}),
        );
    }
    let paper_check = args.first().map(String::as_str) == Some("paper-day-check");
    let paper = paper_check || args.first().map(String::as_str) == Some("paper-day");
    require(
        paper || args.first().map(String::as_str) == Some("manifest-check"),
        "usage: trady replay manifest-check|paper-day --config FILE --manifest FILE [--out NEW_DIRECTORY]",
    )?;
    let options = options(
        &args[1..],
        &["--config", "--manifest", "--out"],
        &["--config", "--manifest"],
    )?;
    let config: EurRuntimeConfig = read_json(Path::new(&options["--config"]))?;
    config.validate(Utc::now())?;
    let path = Path::new(&options["--manifest"]);
    let manifest: ReplayManifest = read_json(path)?;
    let root = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|_| Error("replay directory is unavailable".into()))?;
    let required = [
        "yfinance",
        "alpha_vantage",
        "rss",
        "x",
        "broker_catalog",
        "session_calendar",
        "quote_observations",
        "cost_fx",
    ];
    require(
        manifest.schema_version == 1
            && manifest.inputs.len() <= 512
            && required
                .iter()
                .all(|key| manifest.inputs.contains_key(*key)),
        "replay manifest lacks required recorded provider inputs",
    )?;
    let mut documents = BTreeMap::new();
    let mut total = 0usize;
    for (key, input) in &manifest.inputs {
        let file = root
            .join(&input.path)
            .canonicalize()
            .map_err(|_| Error("recorded replay input unavailable".into()))?;
        require(
            file.starts_with(&root),
            "replay input escapes the manifest directory",
        )?;
        let bytes = read_bounded(&file, 4_000_000)?;
        total += bytes.len();
        require(
            total <= 128_000_000,
            "replay capsule exceeds aggregate size bound",
        )?;
        require(
            content_hash(
                std::str::from_utf8(&bytes)
                    .map_err(|_| Error("replay input is not UTF-8".into()))?,
            ) == input.sha256,
            "recorded replay input hash differs",
        )?;
        documents.insert(key.clone(), bytes);
    }
    if paper {
        let out = options
            .get("--out")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("paper-day-result"));
        return super::eur_replay::paper_day(&config, &manifest, &documents, &out, !paper_check);
    }
    Ok(
        json!({"status":"MANIFEST_INTEGRITY_CHECKED","run_id":manifest.run_id,
        "input_count":manifest.inputs.len(),"manifest_hash":hash(&manifest)?,
        "network_calls":0,"broker_mutations":0,"paper_day_passed":false,"entry_authorized":false}),
    )
}

#[derive(serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReplayManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub inputs: BTreeMap<String, ReplayInput>,
}
#[derive(serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReplayInput {
    pub path: String,
    pub sha256: String,
}

fn select(args: &[String]) -> Result<Value> {
    let options = options(
        args,
        &["--run", "--candidates", "--fallback"],
        &["--run", "--candidates"],
    )?;
    let run: SelectionRun = read_json(Path::new(&options["--run"]))?;
    let candidates: Vec<RankedCandidate> = read_json(Path::new(&options["--candidates"]))?;
    let fallback: Vec<FallbackAttempt> = options
        .get("--fallback")
        .map(|path| read_json(Path::new(path)))
        .transpose()?
        .unwrap_or_else(|| run.fallback_attempts.clone());
    let proposal = run.proposal.clone();
    let result = select_candidate(run, candidates, fallback, proposal, Utc::now())?;
    Ok(json!({"selection":result,"entry_authorized":false}))
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = read_bounded(path, 4_000_000)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Error("EUR input is not UTF-8".into()))?;
    serde_json::from_value(parse_json_bounded(text, 4_000_000)?)
        .map_err(|_| Error("invalid EUR input fields".into()))
}
fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|_| Error("EUR input file unavailable".into()))?;
    require(
        file.metadata().is_ok_and(|m| m.is_file()),
        "EUR input is not a regular file",
    )?;
    let mut bytes = vec![];
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error("cannot read EUR input file".into()))?;
    require(bytes.len() <= max, "EUR input exceeds size limit")?;
    Ok(bytes)
}
fn hash(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value)
        .map(|s| content_hash(&s))
        .map_err(|_| Error("EUR document encoding failed".into()))
}
fn write_new(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| Error("EUR artifact encoding failed".into()))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| {
            Error("cannot create a new EUR artifact (existing files are preserved)".into())
        })?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| Error("cannot persist EUR artifact".into()))
}
fn resolve(config_path: &Path, value: &str) -> PathBuf {
    let p = Path::new(value);
    if p.is_absolute() {
        return p.to_owned();
    }
    // Checked-in config/... references resolve from the checkout root; installed
    // relative references resolve beside their containing configuration.
    if value.starts_with("config/") {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(p)
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(p)
    }
}
pub(super) fn options(
    args: &[String],
    allowed: &[&str],
    required: &[&str],
) -> Result<BTreeMap<String, String>> {
    require(
        args.len().is_multiple_of(2),
        "EUR options require flag/value pairs",
    )?;
    let mut map = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        require(
            allowed.contains(&pair[0].as_str())
                && !pair[1].starts_with("--")
                && map.insert(pair[0].clone(), pair[1].clone()).is_none(),
            "unknown or duplicate EUR option",
        )?;
    }
    require(
        required.iter().all(|key| map.contains_key(*key)),
        "required EUR option is missing",
    )?;
    Ok(map)
}

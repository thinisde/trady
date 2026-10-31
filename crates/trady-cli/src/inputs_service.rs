//! Local operator import/review tooling and an explicitly started HTTP service.

use chrono::Utc;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use trady_adapters::inputs_service::*;
use trady_application::{
    model_lab::{read_document, write_immutable},
    optimization::digest,
};
use trady_domain::{AccountId, Error, Result, require};

pub const HELP: &str = "\n  trady inputs-service check --config FILE\n  trady inputs-service import --config FILE --file FILE\n  trady inputs-service review --config FILE --file FILE\n  trady inputs-service inspect --config FILE --id ID\n  trady inputs-service export-universe --config FILE --out FILE\n  trady inputs-service deadline --config FILE --account ID --sessions 1..5\n  trady inputs-service prune --config FILE\n  trady inputs-service serve --config FILE\n\nImports remain unverified until a separately authored exact-hash operator review.\nServe requires TRADY_INPUTS_SERVICE_TOKEN and binds loopback behind an approved TLS proxy.\nThe HTTP read token cannot import, review, approve models or trade. Prune removes expired licensed content.\n";

fn options(args: &[String], names: &[&str]) -> Result<BTreeMap<String, String>> {
    require(
        args.len() == names.len() * 2,
        "incorrect inputs-service options; see help",
    )?;
    let mut values = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        require(
            names.contains(&pair[0].as_str())
                && !pair[1].is_empty()
                && values.insert(pair[0].clone(), pair[1].clone()).is_none(),
            "unknown or duplicate inputs-service option",
        )?;
    }
    Ok(values)
}
pub fn run(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error("inputs-service command required".into()))?;
    let names: &[&str] = match command {
        "check" | "prune" | "serve" => &["--config"],
        "import" | "review" => &["--config", "--file"],
        "inspect" => &["--config", "--id"],
        "export-universe" => &["--config", "--out"],
        "deadline" => &["--config", "--account", "--sessions"],
        "help" | "--help" if args.len() == 1 => return Ok(json!({"help":HELP})),
        _ => return Err(Error("unsupported inputs-service command; see help".into())),
    };
    let p = options(&args[1..], names)?;
    let config = InputsServiceConfig::load(PathBuf::from(&p["--config"]))?;
    if command == "check" {
        return Ok(
            json!({"status":"CONFIGURATION_VALID","sources":config.sources.len(),"accounts":config.accounts.len(),
        "approved_model_configured":config.model.is_some(),"listen":config.listen.to_string()}),
        );
    }
    let now = Utc::now();
    match command {
        "import" => {
            let import: SourceImport = read_document(&p["--file"])?;
            let hash = ImportStore::open(config)?.import(&import, now)?;
            Ok(
                json!({"status":"STAGED_UNVERIFIED","id":import.id,"sha256":hash,"raw_document_sha256":import.raw_document_sha256}),
            )
        }
        "review" => {
            let review: SourceReview = read_document(&p["--file"])?;
            ImportStore::open(config)?.review(&review, now)?;
            Ok(
                json!({"status":"OPERATOR_REVIEW_RECORDED","id":review.import_id,"review_sha256":digest(&review)?}),
            )
        }
        "inspect" => {
            let store = ImportStore::open(config)?;
            let import = store.staged(&p["--id"])?;
            Ok(
                json!({"id":import.id,"kind":import.payload.kind(),"sha256":digest(&import)?,"source_id":import.source_id,
                "observed_at":import.observed_at,"available_at":import.available_at,"valid_until":import.valid_until,
                "retain_until":import.retain_until,"currently_reviewed_and_usable":store.resolve(&p["--id"],now).is_ok()}),
            )
        }
        "prune" => Ok(
            json!({"status":"EXPIRED_CONTENT_PRUNED","removed":ImportStore::open(config)?.prune_expired(now)?}),
        ),
        "export-universe" => {
            let document = InputsService::open(config)?.universe(now)?;
            write_immutable(&p["--out"], &document)?;
            Ok(
                json!({"status":"VERIFIED_UNIVERSE_EXPORTED","path":p["--out"],"snapshot_id":document.snapshot_id,"members":document.members.len()}),
            )
        }
        "deadline" => {
            let service = InputsService::open(config)?;
            let account = AccountId::new(&p["--account"])?;
            let sessions = p["--sessions"]
                .parse::<u8>()
                .map_err(|_| Error("invalid session count".into()))?;
            let binding = service.config().account(&account)?;
            let deadline = service
                .calendar(&binding.calendar_id, now)?
                .deadline(now, sessions)?;
            Ok(json!({"account_id":account,"exchange_sessions":sessions,"deadline_at":deadline}))
        }
        "serve" => {
            let token =
                trady_adapters::credentials::required("TRADY_INPUTS_SERVICE_TOKEN")?.into_inner();
            let server = HttpServer::bind(InputsService::open(config)?, token)?;
            let stop = Arc::new(AtomicBool::new(false));
            let signal = stop.clone();
            ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
                .map_err(|_| Error("input service signal handler unavailable".into()))?;
            server.serve_until(stop)?;
            Ok(json!({"status":"STOPPED"}))
        }
        _ => Err(Error("unsupported inputs-service command".into())),
    }
}

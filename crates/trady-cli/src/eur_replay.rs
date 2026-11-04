//! Offline provider assertion and explicitly synthetic order-protocol fixtures.
//! This module has no provider or broker transport dependency.
use super::{
    eur::ReplayManifest,
    eur_shadow::{private_dir, private_json},
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io::Write, path::Path};
use trady_application::{
    eur_runtime::EurRuntimeConfig,
    eur_shadow::{
        ProviderObservation, ShadowCandidate, ShadowCrosswalk, assess_quote, session_from_catalog,
    },
    harness::content_hash,
};
use trady_domain::{Error, Result, require};

fn decode(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).map_err(|_| Error("replay input is not valid JSON".into()))
}
fn digest(value: &impl serde::Serialize) -> Result<String> {
    Ok(content_hash(&serde_json::to_string(value).map_err(
        |_| Error("replay hash encoding failed".into()),
    )?))
}
fn private_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| Error("replay artifact already exists or is unavailable".into()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| Error("replay artifact sync failed".into()))
}
pub fn paper_day(
    config: &EurRuntimeConfig,
    manifest: &ReplayManifest,
    documents: &BTreeMap<String, Vec<u8>>,
    out: &Path,
) -> Result<Value> {
    let context = decode(
        documents
            .get("capture_context")
            .ok_or_else(|| Error("paper-day requires an exported capture context".into()))?,
    )?;
    let frozen: EurRuntimeConfig = serde_json::from_value(context["runtime"].clone())
        .map_err(|_| Error("frozen runtime configuration invalid".into()))?;
    require(
        config.policy == frozen.policy && config.base_currency == frozen.base_currency,
        "replay policy differs from captured policy",
    )?;
    let recorded = &context["result"];
    let captured_at: DateTime<Utc> = serde_json::from_value(recorded["completed_at"].clone())
        .map_err(|_| Error("capture timestamp unavailable".into()))?;
    frozen.validate(captured_at)?;
    let crosswalk: ShadowCrosswalk = serde_json::from_value(context["crosswalk"].clone())
        .map_err(|_| Error("frozen crosswalk invalid".into()))?;
    crosswalk.validate(captured_at)?;
    require(
        digest(&context["crosswalk"])? == recorded["crosswalk_hash"].as_str().unwrap_or_default(),
        "frozen crosswalk hash differs",
    )?;
    let mut provider_hashes = BTreeMap::new();
    for (name, value) in context["provider_configurations"]
        .as_object()
        .ok_or_else(|| Error("provider configuration manifest missing".into()))?
    {
        provider_hashes.insert(name, digest(value)?);
    }
    let config_hash = digest(
        &json!({"runtime":frozen,"crosswalk_hash":recorded["crosswalk_hash"],"provider_configuration_hashes":provider_hashes}),
    )?;
    require(
        config_hash == recorded["configuration_hash"].as_str().unwrap_or_default(),
        "capture configuration hash differs",
    )?;
    let mut observations = BTreeMap::new();
    let mut chunks = BTreeMap::new();
    for (id, key) in context["observation_index"]
        .as_object()
        .ok_or_else(|| Error("capture observation index missing".into()))?
    {
        let bytes = documents
            .get(
                key.as_str()
                    .ok_or_else(|| Error("invalid observation index key".into()))?,
            )
            .ok_or_else(|| Error("indexed observation input missing".into()))?;
        let value = decode(bytes)?;
        if value.get("observation_id").is_some() {
            let observation: ProviderObservation = serde_json::from_value(value)
                .map_err(|_| Error("provider observation schema invalid".into()))?;
            observation.validate()?;
            require(
                observation.observation_id == *id,
                "observation identity rebound",
            )?;
            observations.insert(id.clone(), observation);
        } else {
            require(
                value["response_hash"].as_str() == Some(&digest(&value["response"])?),
                "capture chunk hash mismatch",
            )?;
            chunks.insert(id.clone(), value);
        }
    }
    let mut catalog = None;
    let mut calendar = None;
    for observation in observations.values() {
        if observation.source_id == "broker_sessions" {
            calendar = Some(observation);
        }
        if observation.source_id == "broker_catalog" {
            let response = if observation.response["encoding"] == "JSON_ARRAY_CHUNKS_V1" {
                let mut rows = Vec::new();
                for id in observation.response["parts"]
                    .as_array()
                    .ok_or_else(|| Error("catalog chunk index invalid".into()))?
                {
                    let chunk = chunks
                        .get(
                            id.as_str()
                                .ok_or_else(|| Error("catalog chunk identity invalid".into()))?,
                        )
                        .ok_or_else(|| Error("catalog chunk missing".into()))?;
                    rows.extend(
                        chunk["response"]
                            .as_array()
                            .ok_or_else(|| Error("catalog chunk is not an array".into()))?
                            .iter()
                            .cloned(),
                    );
                }
                require(
                    observation.response["response_hash"].as_str() == Some(&digest(&rows)?)
                        && observation.response["row_count"].as_u64() == Some(rows.len() as u64),
                    "catalog reconstruction differs",
                )?;
                Value::Array(rows)
            } else {
                observation.response.clone()
            };
            catalog = Some(response);
        }
    }
    let catalog = catalog.ok_or_else(|| Error("recorded broker catalog missing".into()))?;
    let calendar =
        calendar.ok_or_else(|| Error("recorded broker session calendar missing".into()))?;
    let original: Vec<ShadowCandidate> = serde_json::from_value(recorded["candidates"].clone())
        .map_err(|_| Error("recorded candidate assessments invalid".into()))?;
    let mut candidates = Vec::new();
    let mut blockers = Vec::<String>::new();
    for identity in &crosswalk.members {
        let old = original
            .iter()
            .find(|c| c.instrument_id == identity.internal_instrument_id)
            .ok_or_else(|| Error("recorded candidate missing".into()))?;
        let observation = observations
            .get(&old.quote_observation_id)
            .ok_or_else(|| Error("recorded quote missing".into()))?;
        let at = old.assessed_at.unwrap_or(captured_at);
        if observation.provider == "ibkr_tws_api"
            && observation.status != trady_application::eur_shadow::ObservationStatus::Unavailable
        {
            let ibkr: trady_adapters::ibkr_smart::IbkrSmartConfig =
                serde_json::from_value(context["provider_configurations"]["ibkr"].clone())
                    .map_err(|_| Error("frozen IBKR source configuration missing".into()))?;
            let raw_id = observation.response["raw_capture_id"]
                .as_str()
                .ok_or_else(|| Error("IBKR raw capture reference missing".into()))?;
            let raw = observations
                .get(raw_id)
                .ok_or_else(|| Error("IBKR raw capture missing".into()))?;
            let normalized_at: DateTime<Utc> =
                serde_json::from_value(observation.response["normalized_at"].clone())
                    .map_err(|_| Error("IBKR normalization timestamp missing".into()))?;
            require(
                normalized_at <= at && raw.provider == "ibkr_tws_api",
                "IBKR replay source or timestamp rebound",
            )?;
            let mut derived = ibkr.normalize(
                identity,
                &raw.response,
                normalized_at,
                frozen.eligibility.quote_max_age_seconds,
            )?;
            let session = session_from_catalog(
                identity,
                &frozen.session_deadlines,
                &calendar.response,
                calendar.received_at,
                normalized_at,
            )
            .ok();
            derived["market_state"] = json!(match &session {
                Some(s) if normalized_at >= s.opens_at && normalized_at < s.closes_at => "REGULAR",
                Some(_) => "CLOSED",
                None => "UNKNOWN",
            });
            derived["market_state_source"] = json!(calendar.observation_id);
            derived["raw_capture_id"] = json!(raw_id);
            require(
                derived == observation.response,
                "IBKR normalized quote differs from its recorded callbacks",
            )?;
        }
        if old.assessed_at.is_none() {
            blockers.push("LEGACY_CAPTURE_LACKS_EXACT_ASSESSMENT_TIMESTAMP".into());
        }
        let mut candidate = assess_quote(
            identity,
            observation,
            at,
            frozen.eligibility.quote_max_age_seconds,
        )?;
        let matching: Vec<_> = catalog
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["ticker"].as_str() == Some(&identity.broker_ticker))
            .collect();
        if matching.len() != 1
            || matching[0]["isin"].as_str() != Some(&identity.isin)
            || matching[0]["currencyCode"] != "EUR"
            || matching[0]["workingScheduleId"].as_i64() != Some(identity.working_schedule_id)
        {
            candidate
                .blockers
                .push("CURRENT_CATALOG_IDENTITY_CHANGED".into());
            candidate.research_eligible = false;
        }
        match session_from_catalog(
            identity,
            &frozen.session_deadlines,
            &calendar.response,
            calendar.received_at,
            at,
        ) {
            Ok(session) if at >= session.opens_at && at < session.flatten_at => {}
            Ok(_) => {
                candidate
                    .blockers
                    .push("SESSION_CLOSED_OR_FLATTEN_DEADLINE_REACHED".into());
                candidate.research_eligible = false;
            }
            Err(_) => {
                candidate
                    .blockers
                    .push("SESSION_CALENDAR_UNAVAILABLE".into());
                candidate.research_eligible = false;
            }
        }
        candidate.assessed_at = old.assessed_at;
        require(
            serde_json::to_value(&candidate).ok() == serde_json::to_value(old).ok(),
            "replayed selection blockers differ from recorded decision",
        )?;
        candidates.push(candidate);
    }
    if candidates.iter().all(|c| !c.entry_eligible) {
        blockers.push("NO_RECORDED_EXECUTABLE_EUR_CANDIDATE".into());
    }
    if recorded["strategy_status"] != "READY" || recorded["risk_plan_status"] != "CALCULATED" {
        blockers.push("RECORDED_STRATEGY_OR_RISK_PLAN_UNAVAILABLE".into());
    }
    let costs = decode(&documents["cost_fx"])?;
    if costs["status"] != "VERIFIED" {
        blockers.push("REVIEWED_COST_AND_FX_INPUTS_UNAVAILABLE".into());
    }
    if observations
        .values()
        .any(|o| o.status != trady_application::eur_shadow::ObservationStatus::Ok)
    {
        blockers.push("PROVIDER_ERRORS_OR_STALE_OBSERVATIONS_RECORDED".into());
    }
    let suite = trady_application::eur_protocol::scenarios::run_all();
    require(
        suite["passed"] == true,
        "order-protocol regression fixture failed",
    )?;
    blockers.sort();
    blockers.dedup();
    private_dir(out)?;
    private_dir(&out.join("provider-snapshots"))?;
    let mut rebased = BTreeMap::new();
    for (i, (key, bytes)) in documents.iter().enumerate() {
        let name = format!("provider-snapshots/input_{i:03}.json");
        private_bytes(&out.join(&name), bytes)?;
        rebased.insert(key.clone(),json!({"path":name,"sha256":content_hash(std::str::from_utf8(bytes).map_err(|_|Error("capture input is not UTF-8".into()))?)}));
    }
    private_json(
        &out.join("manifest.json"),
        &json!({"schema_version":1,"run_id":manifest.run_id,"inputs":rebased}),
    )?;
    private_json(
        &out.join("selection.json"),
        &json!({"captured_decision":recorded,"replayed_candidates":candidates,"matches_recorded_decision":true}),
    )?;
    private_json(
        &out.join("strategy-snapshots.json"),
        &json!({"status":recorded["strategy_status"],"model_run":recorded["model_run"],"risk_plan_status":recorded["risk_plan_status"]}),
    )?;
    private_json(&out.join("restart-checks.json"), &suite)?;
    let mut ledger = String::new();
    for scenario in suite["scenarios"].as_array().into_iter().flatten() {
        for record in scenario["records"].as_array().into_iter().flatten() {
            ledger.push_str(&json!({"classification":"SYNTHETIC_PROTOCOL_FIXTURE","scenario":scenario["scenario"],"record":record}).to_string());
            ledger.push('\n');
        }
    }
    private_bytes(&out.join("order-ledger.jsonl"), ledger.as_bytes())?;
    let deliveries = &context["telegram_deliveries"];
    let mut outbox = String::new();
    for delivery in deliveries.as_array().into_iter().flatten() {
        outbox.push_str(&delivery.to_string());
        outbox.push('\n');
    }
    private_bytes(&out.join("telegram-outbox.jsonl"), outbox.as_bytes())?;
    private_json(
        &out.join("reconciliation.json"),
        &json!({"recorded_broker_reconciled_flat":recorded["reconciled_flat"],"recorded_broker_mutations":recorded["broker_mutations"],"real_order_fills_replayed":0,"synthetic_fill_reconciliation_passed":true}),
    )?;
    private_json(
        &out.join("capability-result.json"),
        &json!({"protection_protocol_simulation_passed":true,
        "broker_protection_verified":false,"quote_path_accepted":false,"independent_review":"PENDING","entry_authorized":false}),
    )?;
    let passed = blockers.is_empty();
    let summary = json!({"schema_version":1,"run_id":manifest.run_id,"status":if passed{"PAPER_DAY_PASSED"}else{"PAPER_DAY_BLOCKED"},
        "source_manifest_hash":digest(manifest)?,"configuration_hash":recorded["configuration_hash"],"policy_hash":recorded["policy_hash"],
        "recorded_code_hash":recorded["code_hash"],"replay_engine_hash":content_hash(concat!(include_str!("eur_replay.rs"),
            include_str!("../../trady-application/src/eur_protocol.rs"),include_str!("../../trady-application/src/eur_protocol/scenarios.rs"),
            include_str!("../../trady-adapters/src/ibkr_smart.rs"),include_str!("../../trady-application/src/eur_shadow.rs"),
            include_str!("../../trady-application/src/eur_runtime.rs"),include_str!("../../trady-domain/src/market.rs"))),
        "selection_reproduced":true,"protocol_scenarios_passed":suite["scenarios"].as_array().map(Vec::len),
        "paper_day_passed":passed,"blockers":blockers,"provider_calls":0,"broker_mutations":0,"entry_authorized":false});
    // REPLAY still has a durable PostgreSQL boundary, under a distinct replay
    // account. The provider assertion above performs no network access.
    let account = format!("replay_{}", &digest(manifest)?[..24]);
    let mut store = trady_storage::target::PgTargetStore::connect(
        &super::secret("TRADY_EXECUTOR_DSN")?,
        "trady_executor",
        frozen.owner_id,
        account,
    )?;
    store.acquire_executor_lease("offline_paper_day", 120)?;
    let id = format!("replay_{}", &digest(&summary)?[..32]);
    store.append_document("paper_day_report", &id, &summary)?;
    store.release_executor_lease()?;
    private_json(&out.join("paper-day-summary.json"), &summary)?;
    private_json(&out.join("hashes.json"), &artifact_hashes(out)?)?;
    require(
        passed,
        "paper-day acceptance blocked; inspect paper-day-summary.json; no broker mutation occurred",
    )?;
    Ok(summary)
}
fn artifact_hashes(path: &Path) -> Result<Value> {
    let mut hashes = BTreeMap::new();
    for entry in
        fs::read_dir(path).map_err(|_| Error("replay artifact directory unavailable".into()))?
    {
        let entry = entry.map_err(|_| Error("artifact entry unavailable".into()))?;
        if entry.path().is_file() {
            let bytes =
                fs::read(entry.path()).map_err(|_| Error("artifact readback failed".into()))?;
            hashes.insert(
                entry.file_name().to_string_lossy().into_owned(),
                content_hash(
                    std::str::from_utf8(&bytes)
                        .map_err(|_| Error("artifact encoding failed".into()))?,
                ),
            );
        }
    }
    Ok(json!(hashes))
}

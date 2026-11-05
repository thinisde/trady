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
    persist: bool,
) -> Result<Value> {
    let context = decode(
        documents
            .get("capture_context")
            .ok_or_else(|| Error("paper-day requires an exported capture context".into()))?,
    )?;
    let frozen: EurRuntimeConfig = serde_json::from_value(context["runtime"].clone())
        .map_err(|_| Error("frozen runtime configuration invalid".into()))?;
    let candle_source_matches = candle_source_matches(config, &frozen);
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
    if !candle_source_matches {
        blockers.push("PAPER_CURRENT_CANDLE_SOURCE_DIFFERS_FROM_RUNTIME".into());
    }
    for identity in &crosswalk.members {
        let old = original
            .iter()
            .find(|c| c.instrument_id == identity.internal_instrument_id)
            .ok_or_else(|| Error("recorded candidate missing".into()))?;
        let observation = observations
            .get(&old.quote_observation_id)
            .ok_or_else(|| Error("recorded quote missing".into()))?;
        let at = old.assessed_at.unwrap_or(captured_at);
        recorded_strategy_bars(&context, &observations, identity, at)?;
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
    if !candidates.iter().any(|c| {
        c.research_eligible
            && recorded["selected_instrument_id"].as_str() == Some(c.instrument_id.as_str())
    }) {
        blockers.push("NO_RECORDED_EXECUTABLE_EUR_CANDIDATE".into());
    }
    if recorded["strategy_status"] != "RESEARCH_SELECTED"
        || recorded["risk_plan_status"] != "CALCULATED_SHADOW_ONLY"
        || recorded["selection"]["failure"].is_object()
        || recorded["risk_plan"].is_null()
        || recorded["analyst_invoked"] != true
    {
        blockers.push("RECORDED_STRATEGY_OR_RISK_PLAN_UNAVAILABLE".into());
    }
    let costs = decode(&documents["cost_fx"])?;
    if costs["status"] != "VERIFIED" && costs["status"] != "REVIEWED_MODEL_INPUTS" {
        blockers.push("REVIEWED_COST_AND_FX_INPUTS_UNAVAILABLE".into());
    }
    let optional_source_gaps: Vec<_> = observations
        .values()
        .filter(|o| o.status != trady_application::eur_shadow::ObservationStatus::Ok)
        .map(|o| o.observation_id.clone())
        .collect();
    if observations.values().any(|o| {
        o.status != trady_application::eur_shadow::ObservationStatus::Ok
            && (matches!(
                o.source_id.as_str(),
                "broker_catalog" | "broker_sessions" | "account_snapshot" | "ibkr_smart_wire"
            ) || o.source_id.starts_with("quote_")
                || o.source_id.starts_with("risk_quote_")
                || o.source_id.starts_with("ibkr_bars_")
                || o.source_id == "ibkr_current_bars_wire"
                || (frozen
                    .shadow
                    .as_ref()
                    .is_none_or(|s| s.current_bars_config.is_none())
                    && o.source_id.starts_with("yf_bars_")))
    }) {
        blockers.push("PROVIDER_ERRORS_OR_STALE_OBSERVATIONS_RECORDED".into());
    }
    let paper_ledger_verified = match documents.get("paper_order_ledger") {
        Some(bytes) => decode(bytes)
            .and_then(|ledger| {
                super::eur_paper::validate_for_capture(&ledger, &context)?;
                let raw = documents
                    .get("paper_quote_stream")
                    .ok_or_else(|| Error("paper raw forward stream missing".into()))?;
                require(
                    ledger["stream_provenance_sha256"]
                        == content_hash(
                            std::str::from_utf8(raw)
                                .map_err(|_| Error("paper raw stream invalid".into()))?,
                        ),
                    "paper raw stream hash differs",
                )?;
                super::eur_paper::validate_stream_provenance(&ledger, &decode(raw)?)
            })
            .is_ok(),
        None => false,
    };
    if paper_ledger_verified {
        verify_recorded_risk(&context, &observations, calendar, &crosswalk)?;
    }
    if !paper_ledger_verified {
        blockers.push("REAL_INPUT_PAPER_ORDER_LEDGER_UNAVAILABLE_OR_INVALID".into());
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
        &json!({"recorded_broker_reconciled_flat":recorded["reconciled_flat"],"recorded_broker_mutations":recorded["broker_mutations"],"real_order_fills_replayed":0,"forward_paper_ledger_verified":paper_ledger_verified,"paper_order_fills_replayed":if paper_ledger_verified{2}else{0},"synthetic_fill_reconciliation_passed":true}),
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
        "paper_day_passed":passed,"forward_paper_ledger_verified":paper_ledger_verified,"optional_source_gaps":optional_source_gaps,"blockers":blockers,"provider_calls":0,"broker_mutations":0,"entry_authorized":false});
    // REPLAY still has a durable PostgreSQL boundary, under a distinct replay
    // account. The provider assertion above performs no network access.
    if persist {
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
    }
    private_json(&out.join("paper-day-summary.json"), &summary)?;
    private_json(&out.join("hashes.json"), &artifact_hashes(out)?)?;
    require(
        passed,
        "paper-day acceptance blocked; inspect paper-day-summary.json; no broker mutation occurred",
    )?;
    Ok(summary)
}

fn candle_source_matches(supplied: &EurRuntimeConfig, frozen: &EurRuntimeConfig) -> bool {
    match (&supplied.shadow, &frozen.shadow) {
        (Some(left), Some(right)) => {
            left.current_bars_config == right.current_bars_config
                && left.ibkr_config == right.ibkr_config
                && left.bar_interval_seconds.unwrap_or(300)
                    == right.bar_interval_seconds.unwrap_or(300)
        }
        (None, None) => true,
        _ => false,
    }
}

fn recorded_strategy_bars(
    context: &Value,
    observations: &BTreeMap<String, ProviderObservation>,
    identity: &trady_application::eur_shadow::ShadowIdentity,
    at: DateTime<Utc>,
) -> Result<Option<trady_domain::BarSnapshot>> {
    let config: EurRuntimeConfig = serde_json::from_value(context["runtime"].clone())
        .map_err(|_| Error("bar runtime missing".into()))?;
    let id = identity.internal_instrument_id.as_str();
    let current = config
        .shadow
        .as_ref()
        .is_some_and(|s| s.current_bars_config.is_some());
    let source = format!("{}_{id}", if current { "ibkr_bars" } else { "yf_bars" });
    let Some(observation) = observations.values().find(|o| o.source_id == source) else {
        return Ok(None);
    };
    if observation.status != trady_application::eur_shadow::ObservationStatus::Ok {
        return Ok(None);
    }
    let bars = serde_json::from_value(observation.response.clone())
        .map_err(|_| Error("recorded strategy bars invalid".into()))?;
    if current {
        let raw = observations
            .values()
            .find(|o| o.source_id == "ibkr_current_bars_wire")
            .ok_or_else(|| Error("current bars raw native capture missing".into()))?;
        let settings: trady_adapters::ibkr_bars::CurrentBarsConfig =
            serde_json::from_value(context["provider_configurations"]["current_bars"].clone())
                .map_err(|_| Error("current bars frozen configuration missing".into()))?;
        let quotes: trady_adapters::ibkr_smart::IbkrSmartConfig =
            serde_json::from_value(context["provider_configurations"]["ibkr"].clone())
                .map_err(|_| Error("current bars frozen quote binding missing".into()))?;
        let derived = settings.normalize(
            &quotes,
            identity,
            context["result"]["crosswalk_hash"]
                .as_str()
                .ok_or_else(|| Error("current bars frozen crosswalk hash missing".into()))?,
            &raw.response,
            at,
        )?;
        require(
            serde_json::to_value(&derived).ok() == Some(observation.response.clone()),
            "normalized strategy candles differ from native bar capture",
        )?;
    }
    Ok(Some(bars))
}

fn verify_recorded_risk(
    context: &Value,
    observations: &BTreeMap<String, ProviderObservation>,
    calendar: &ProviderObservation,
    crosswalk: &ShadowCrosswalk,
) -> Result<()> {
    use trady_domain::{BarSnapshot, Strategy, strategy::DonchianStrategy};
    let recorded = &context["result"];
    let config: EurRuntimeConfig = serde_json::from_value(context["runtime"].clone())
        .map_err(|_| Error("paper risk runtime missing".into()))?;
    let plan: trady_policy::intraday::IntradayTradePlan =
        serde_json::from_value(recorded["risk_plan"].clone())
            .map_err(|_| Error("paper risk plan missing".into()))?;
    let id = plan.instrument_id.as_str();
    let identity = crosswalk
        .members
        .iter()
        .find(|m| m.internal_instrument_id == plan.instrument_id)
        .ok_or_else(|| Error("paper risk identity missing".into()))?;
    let observation = |source: &str| {
        observations
            .values()
            .find(|o| o.source_id == source)
            .ok_or_else(|| Error("paper risk source missing".into()))
    };
    let bars: BarSnapshot =
        recorded_strategy_bars(context, observations, identity, plan.created_at)?
            .ok_or_else(|| Error("paper strategy bars missing".into()))?;
    let session = session_from_catalog(
        identity,
        &config.session_deadlines,
        &calendar.response,
        calendar.received_at,
        plan.created_at,
    )?;
    let signal = DonchianStrategy::intraday_default()
        .evaluate(&bars, session.flatten_at)?
        .ok_or_else(|| Error("paper strategy signal unavailable".into()))?;
    require(
        trady_application::eur_shadow::completed_bar_is_current(
            &bars,
            session.opens_at,
            plan.created_at,
        ),
        "paper strategy bars are stale or from a prior session",
    )?;
    require(
        serde_json::to_value(&signal).ok() == Some(recorded["strategy_signals"][id].clone()),
        "paper strategy signal differs from completed bars",
    )?;
    let universe: trady_domain::EurUniverseSnapshot =
        serde_json::from_value(context["provider_configurations"]["universe"].clone())
            .map_err(|_| Error("paper risk universe missing".into()))?;
    let binding = universe
        .member(&plan.instrument_id)
        .ok_or_else(|| Error("paper risk binding missing".into()))?;
    let costs: super::eur_decision::ReviewedCosts =
        serde_json::from_value(context["provider_configurations"]["risk"].clone())
            .map_err(|_| Error("paper risk costs missing".into()))?;
    require(
        recorded["risk_cost_model"]
            == serde_json::to_value(&costs)
                .map_err(|_| Error("paper risk cost encoding failed".into()))?,
        "paper risk costs rebound",
    )?;
    let source = observation(&format!("risk_quote_{id}"))?;
    let quote: trady_application::eur_shadow::ShadowQuote =
        serde_json::from_value(source.response.clone())
            .map_err(|_| Error("paper risk quote missing".into()))?;
    let raw_id = source.response["raw_capture_id"]
        .as_str()
        .ok_or_else(|| Error("paper risk raw quote missing".into()))?;
    let raw = observations
        .get(raw_id)
        .ok_or_else(|| Error("paper risk raw callbacks missing".into()))?;
    let ibkr: trady_adapters::ibkr_smart::IbkrSmartConfig =
        serde_json::from_value(context["provider_configurations"]["ibkr"].clone())
            .map_err(|_| Error("paper risk IBKR config missing".into()))?;
    let normalized = ibkr.normalize(
        identity,
        &raw.response,
        source.received_at,
        config.eligibility.quote_max_age_seconds,
    )?;
    require(
        normalized["bid"] == source.response["bid"]
            && normalized["ask"] == source.response["ask"]
            && source.response["fresh"] == true,
        "paper risk quote differs from SMART callbacks",
    )?;
    let quote = trady_domain::Quote {
        instrument_id: plan.instrument_id.clone(),
        currency: config.base_currency.clone(),
        bid: quote
            .bid
            .ok_or_else(|| Error("paper risk bid missing".into()))?,
        ask: quote
            .ask
            .ok_or_else(|| Error("paper risk ask missing".into()))?,
        observed_at: quote.observed_at,
        received_at: quote.received_at,
        tradable: true,
    };
    let account = serde_json::from_value(context["account_snapshot"].clone())
        .map_err(|_| Error("paper risk account missing".into()))?;
    let rules = trady_policy::InstrumentRules {
        price_tick: binding.price_tick,
        quantity_step: binding.quantity_step,
        minimum_quantity: binding.entry_minimum_quantity(quote.ask)?,
        currency_unit: rust_decimal::Decimal::new(1, 2),
    };
    let rebuilt = trady_application::eur_runtime::build_risk_plan(
        &account,
        &quote,
        &signal,
        &costs.entry,
        &costs.exit,
        &rules,
        session.flatten_at,
        &config.policy,
        plan.plan_id.clone(),
        plan.evidence_snapshot_ids.clone(),
        plan.created_at,
    )?;
    require(
        rebuilt == plan,
        "paper risk plan differs from deterministic source calculation",
    )
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

#[cfg(test)]
mod candle_source_tests {
    use super::*;
    #[test]
    fn supplied_current_candle_mode_cannot_be_downgraded_in_frozen_paper() {
        let mut supplied: EurRuntimeConfig =
            serde_json::from_str(include_str!("../../../config/eur-shadow.example.json")).unwrap();
        let legacy = supplied.clone();
        supplied.shadow.as_mut().unwrap().current_bars_config =
            Some("/opt/trady/config/current-bars.json".into());
        assert!(!candle_source_matches(&supplied, &legacy));
        assert!(candle_source_matches(&supplied, &supplied));
        let mut changed = supplied.clone();
        changed.shadow.as_mut().unwrap().current_bars_config =
            Some("/opt/trady/config/other.json".into());
        assert!(!candle_source_matches(&supplied, &changed));
        changed = supplied.clone();
        changed.shadow.as_mut().unwrap().ibkr_config =
            Some("/opt/trady/config/other-quote-binding.json".into());
        assert!(!candle_source_matches(&supplied, &changed));
        assert!(candle_source_matches(&legacy, &legacy));
    }
}

//! Bounded DEMO supervision. All broker mutations remain in protocol-drive.
use super::eur_execute::{approved, artifact, file_hash, read};
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::Path, thread, time::Duration};
use trady_adapters::{
    credentials::CredentialStore,
    ibkr_smart::IbkrSmartConfig,
    trading212::{Credentials, Environment, Trading212Config, Trading212Reader},
};
use trady_application::{
    eur_protocol::{ProtocolContext, ProtocolInput, ProtocolJournal, advance, recover},
    eur_runtime::{EurRuntimeConfig, OrderGateSnapshot},
    eur_shadow::{ShadowCrosswalk, ShadowIdentity, session_from_catalog},
    harness::new_id,
};
use trady_domain::{
    Capabilities, Capability, Error, ExecutionStage, MarketState, QuoteQuality, Result,
    SelectionRun, SessionWindow, Support, VenueQuote, require,
};
use trady_storage::target::PgTargetStore;

fn entry_intent(run_id: &str, cycle: &str) -> String {
    format!(
        "{}_{}",
        run_id,
        &trady_application::harness::content_hash(cycle)[..32]
    )
}
fn opening_covers_movements(opening: Option<DateTime<Utc>>, first: Option<DateTime<Utc>>) -> bool {
    opening.is_some_and(|at| first.is_none_or(|first| at <= first))
}

fn lifecycle_fill_coverage(
    retained: &std::collections::BTreeMap<String, Value>,
    native: &std::collections::BTreeMap<String, Value>,
    orders: &std::collections::BTreeSet<String>,
) -> (bool, std::collections::BTreeSet<String>) {
    let extra = native
        .iter()
        .filter(|(id, row)| {
            !retained.contains_key(*id) || !orders.contains(&row["order"]["id"].to_string())
        })
        .map(|(_, row)| row["order"]["id"].to_string())
        .collect();
    let complete = retained.len() == native.len()
        && retained.iter().all(|(id, row)| {
            native.get(id).is_some_and(|n| {
                n["fill"] == row["fill"]
                    && n["order"]["id"] == row["order"]["id"]
                    && n["order"]["side"] == row["order"]["side"]
                    && n["order"]["quantity"] == row["order"]["quantity"]
            })
        });
    (complete, extra)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DemoOperation {
    pub schema_version: u32,
    pub run_id: String,
    pub owner_id: String,
    pub account_id: String,
    pub broker_account_id: i64,
    pub environment: String,
    pub live_enabled: bool,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub source_runtime_config: String,
    pub artifact_directory: String,
    pub poll_seconds: u64,
    pub automatic_live_activation: bool,
    pub automatic_model_promotion: bool,
    pub max_daily_entries: u8,
    pub max_open_positions: u8,
    pub no_overnight: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_attribution_config: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_evidence_config: Option<String>,
}
impl DemoOperation {
    pub(super) fn validate(&self, config: &EurRuntimeConfig) -> Result<()> {
        require(
            self.schema_version == 1
                && !self.run_id.is_empty()
                && self.run_id.len() <= 48
                && self
                    .run_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && self.owner_id == config.owner_id
                && self.account_id == config.account_id.as_str()
                && self.broker_account_id == 52211463
                && self.environment == "demo"
                && !self.live_enabled
                && self.ends_at > self.starts_at
                && (self.ends_at - self.starts_at).num_seconds() <= 31 * 86400
                && (10..=60).contains(&self.poll_seconds)
                && !self.automatic_live_activation
                && !self.automatic_model_promotion
                && (1..=30).contains(&self.max_daily_entries)
                && self.max_open_positions == 1
                && self.no_overnight
                && self.max_daily_entries == config.policy.max_daily_entries
                && self.capability_attribution_config.is_some()
                    == self.capability_evidence_config.is_some()
                && (config.policy.revision != trady_policy::intraday::THIRTY_ENTRY_POLICY_VERSION
                    || self.capability_attribution_config.is_some())
                && config.policy.max_open_positions == 1
                && config.policy.no_overnight
                && Path::new(&self.source_runtime_config).is_absolute()
                && Path::new(&self.artifact_directory)
                    .starts_with("/var/lib/trady-executor/eur-demo")
                && !Path::new(&self.artifact_directory)
                    .components()
                    .any(|p| matches!(p, std::path::Component::ParentDir)),
            "invalid bounded DEMO operation",
        )
    }
}

pub(super) fn entry_operation(
    config: &EurRuntimeConfig,
    config_path: &Path,
    bundle: &Path,
    manifest: &Value,
) -> Result<Option<(DemoOperation, String)>> {
    let Some(path) = &config.operation_config else {
        return Ok(None);
    };
    let path = super::eur_shadow::resolve(config_path, path);
    require(
        file_hash(&path)? == file_hash(&artifact(bundle, manifest, "demo_operation")?)?,
        "entry operation policy differs from release",
    )?;
    let operation: DemoOperation = read(&path)?;
    operation.validate(config)?;
    require(
        Utc::now() >= operation.starts_at && Utc::now() < operation.ends_at,
        "new entry outside bounded DEMO operation",
    )?;
    for (role, reference) in [
        (
            "capability_attribution",
            operation.capability_attribution_config.as_ref(),
        ),
        (
            "capability_evidence",
            operation.capability_evidence_config.as_ref(),
        ),
    ] {
        if let Some(reference) = reference {
            require(
                file_hash(&super::eur_shadow::resolve(config_path, reference))?
                    == file_hash(&artifact(bundle, manifest, role)?)?,
                "capability attribution dependency differs from release",
            )?;
        }
    }
    let source_path = Path::new(&operation.source_runtime_config);
    require(
        file_hash(source_path)? == file_hash(&artifact(bundle, manifest, "shadow_runtime")?)?,
        "entry source runtime differs from release",
    )?;
    let source: EurRuntimeConfig = read(source_path)?;
    source.validate(Utc::now())?;
    require(
        source.execution_stage == ExecutionStage::Shadow
            && !source.live_enabled
            && source.account_id == config.account_id
            && source.owner_id == config.owner_id
            && source.policy == config.policy,
        "entry research authority changed",
    )?;
    let settings = source
        .shadow
        .as_ref()
        .ok_or_else(|| Error("entry research settings missing".into()))?;
    for (role, reference) in [
        ("yfinance_config", source.yfinance_config.as_ref()),
        ("research_config", source.research_config.as_ref()),
        ("opencode_config", source.opencode_config.as_ref()),
        ("broker_config", source.trading212_config.as_ref()),
        ("ibkr_config", settings.ibkr_config.as_ref()),
        ("forward_cost_model", settings.risk_config.as_ref()),
        ("universe_config", source.universe_config.as_ref()),
        ("crosswalk", Some(&settings.crosswalk_config)),
    ] {
        let reference = reference.ok_or_else(|| Error("entry source dependency missing".into()))?;
        require(
            file_hash(&super::eur_shadow::resolve(source_path, reference))?
                == file_hash(&artifact(bundle, manifest, role)?)?,
            "entry source dependency differs from release",
        )?;
    }
    if let Some(reference) = &settings.current_bars_config {
        require(
            file_hash(&super::eur_shadow::resolve(source_path, reference))?
                == file_hash(&artifact(bundle, manifest, "current_bars_config")?)?,
            "current strategy candle settings differ from release",
        )?;
    }
    let source_hash = super::eur_shadow::configuration_hash(&source_path.to_string_lossy())?;
    Ok(Some((operation, source_hash)))
}

pub(super) fn check_entry_catalog(identity: &ShadowIdentity, catalog: &Value) -> Result<()> {
    let rows: Vec<_> = catalog
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["ticker"] == identity.broker_ticker)
        .collect();
    require(
        rows.len() == 1
            && rows[0]["isin"] == identity.isin
            && rows[0]["currencyCode"] == identity.currency.as_str()
            && rows[0]["type"] == "STOCK"
            && rows[0]["workingScheduleId"].as_i64() == Some(identity.working_schedule_id),
        "current entry catalog identity/listing/schedule differs from accepted mapping",
    )
}

pub(super) fn venue_quote(
    identity: &ShadowIdentity,
    normalized: &Value,
    quote_id: String,
    session: &SessionWindow,
) -> Result<VenueQuote> {
    require(
        normalized["market_data_type"] == 1
            && normalized["source_venue"] == "SMART"
            && normalized["fresh"] == true,
        "entry requires current live SMART bid/ask",
    )?;
    let decimal = |key: &str| {
        serde_json::from_value(normalized[key].clone())
            .map_err(|_| Error("SMART quote field invalid".into()))
    };
    let quote = VenueQuote {
        quote_id,
        instrument_id: identity.internal_instrument_id.clone(),
        broker_instrument_id: identity.broker_ticker.clone(),
        isin: identity.isin.clone(),
        exchange_mic: identity.exchange_mic.clone(),
        currency: trady_domain::Currency::new("EUR")?,
        yahoo_symbol: identity.yahoo_symbol.clone(),
        bid: Some(decimal("bid")?),
        ask: Some(decimal("ask")?),
        bid_size: Some(decimal("bid_size")?),
        ask_size: Some(decimal("ask_size")?),
        last: None,
        volume: None,
        observed_at: serde_json::from_value(normalized["observed_at"].clone())
            .map_err(|_| Error("SMART provider time missing".into()))?,
        received_at: serde_json::from_value(normalized["received_at"].clone())
            .map_err(|_| Error("SMART receipt time missing".into()))?,
        market_state: if Utc::now() >= session.opens_at && Utc::now() < session.closes_at {
            MarketState::Regular
        } else {
            MarketState::Closed
        },
        source: "ibkr_tws_api".into(),
        quality: QuoteQuality::Executable,
    };
    quote.validate(Utc::now(), 10)?;
    Ok(quote)
}

fn protocol_states(store: &mut PgTargetStore) -> Result<Vec<Value>> {
    let rows = store.list_documents(
        "protocol_binding",
        DateTime::from_timestamp(0, 0).ok_or_else(|| Error("invalid epoch".into()))?,
        1000,
    )?;
    let rows = rows
        .as_array()
        .ok_or_else(|| Error("protocol binding list invalid".into()))?;
    require(rows.len() < 1000, "protocol discovery list truncated")?;
    let mut states = Vec::new();
    for row in rows {
        let id = row["record_id"]
            .as_str()
            .ok_or_else(|| Error("protocol binding ID missing".into()))?;
        let mut records = store.load(id)?;
        if records.is_empty() {
            // Binding publication precedes Reserve. An empty append-only
            // history proves this preparation never reached UNKNOWN/transport.
            let context: ProtocolContext =
                serde_json::from_value(row["document"]["context"].clone()).map_err(|_| {
                    Error(format!(
                        "legacy binding {id} lacks pre-submission context; operator review required"
                    ))
                })?;
            require(
                context.intent_id == id,
                "unsubmitted preparation identity differs",
            )?;
            let acquired_here = !store.has_executor_fence();
            if acquired_here {
                store.acquire_demo_broker_lease(&new_id("retire_preparation"), 120)?;
            }
            let retired = (|| {
                let current = store.load(id)?;
                if current.is_empty() {
                    advance(
                        store,
                        &context,
                        new_id("retire_exit"),
                        ProtocolInput::RequestExit {
                            reason: "UNSUBMITTED_PREPARATION_RETIRED".into(),
                        },
                        Utc::now(),
                    )?;
                }
                store.load(id)
            })();
            if acquired_here {
                let _ = store.release_executor_lease();
            }
            records = retired?;
        }
        let state = recover(&records)?;
        states.push(json!({"intent_id":id,"state":state,"release_sha256":row["document"]["release_sha256"]}));
    }
    Ok(states)
}

pub(super) fn protocol_needs_execution(store: &mut PgTargetStore) -> Result<bool> {
    let rows = store.list_documents(
        "protocol_binding",
        DateTime::from_timestamp(0, 0).ok_or_else(|| Error("invalid epoch".into()))?,
        1000,
    )?;
    let rows = rows
        .as_array()
        .ok_or_else(|| Error("protocol discovery invalid".into()))?;
    require(rows.len() < 1000, "protocol discovery truncated")?;
    for row in rows {
        let id = row["record_id"]
            .as_str()
            .ok_or_else(|| Error("protocol identity missing".into()))?;
        let records = store.load(id)?;
        if records.is_empty()
            || !matches!(
                recover(&records)?.phase,
                trady_application::eur_protocol::ProtocolPhase::Flat
                    | trady_application::eur_protocol::ProtocolPhase::Rejected
            )
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn store(config: &EurRuntimeConfig) -> Result<PgTargetStore> {
    PgTargetStore::connect(
        &super::secret("TRADY_EXECUTOR_DSN")?,
        "trady_executor",
        config.owner_id.clone(),
        config.account_id.to_string(),
    )
}

fn period_documents<F>(
    repository: &mut PgTargetStore,
    kind: &str,
    start: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    mut consume: F,
) -> Result<()>
where
    F: FnMut(&Value) -> Result<()>,
{
    if start >= cutoff {
        return Ok(());
    }
    let mut after = None;
    let mut after_id: Option<String> = None;
    loop {
        repository.renew_executor_lease(300)?;
        let page =
            repository.list_record_window(kind, start, cutoff, after, after_id.as_deref())?;
        let rows = page
            .as_array()
            .ok_or_else(|| Error("month evidence page invalid".into()))?;
        for row in rows {
            consume(row)?;
        }
        if rows.len() < 64 {
            break;
        }
        let row = rows.last().unwrap();
        after = Some(
            serde_json::from_value(row["created_at"].clone())
                .map_err(|_| Error("month evidence page clock invalid".into()))?,
        );
        after_id = Some(
            row["record_id"]
                .as_str()
                .ok_or_else(|| Error("month evidence page key missing".into()))?
                .to_string(),
        );
    }
    Ok(())
}

pub fn report(args: &[String]) -> Result<Value> {
    use rust_decimal::Decimal;
    use std::collections::{BTreeMap, BTreeSet};
    let values = super::eur::options(args, &["--config", "--out"], &["--config", "--out"])?;
    let config_path = Path::new(&values["--config"]);
    let config: EurRuntimeConfig = read(config_path)?;
    config.validate(Utc::now())?;
    let operation: DemoOperation = read(&super::eur_shadow::resolve(
        config_path,
        config
            .operation_config
            .as_ref()
            .ok_or_else(|| Error("month policy missing".into()))?,
    ))?;
    operation.validate(&config)?;
    let out = Path::new(&values["--out"]);
    super::eur_shadow::private_dir(out)?;
    if Utc::now() < operation.starts_at {
        let result = json!({"status":"MONTH_NOT_STARTED","run_id":operation.run_id,"completed_trades":0,"investment_return":null,"optimization_decision":"KEEP","broker_mutations":0,"live_enabled":false});
        super::eur_shadow::private_json(&out.join("month-report.json"), &result)?;
        return Ok(result);
    }
    let attribution = match (
        &operation.capability_attribution_config,
        &operation.capability_evidence_config,
    ) {
        (Some(registry), Some(evidence)) => {
            let (registry, facts) = super::eur_attribution::load_for_report(
                &super::eur_shadow::resolve(config_path, registry),
                &super::eur_shadow::resolve(config_path, evidence),
                &config.owner_id,
                config.account_id.as_str(),
                Utc::now(),
                (operation.starts_at, operation.ends_at),
            )?;
            require(
                registry.period_start == operation.starts_at
                    && registry.period_end == operation.ends_at,
                "capability attribution period differs from operation",
            )?;
            facts
        }
        (None, None) => super::eur_attribution::CapabilityAttribution::default(),
        _ => {
            return Err(Error(
                "incomplete capability attribution configuration".into(),
            ));
        }
    };
    let cutoff = Utc::now().min(operation.ends_at);
    let mut repository = store(&config)?;
    require(
        repository
            .physical_unresolved_protocols_for_audit()?
            .as_array()
            .is_some_and(Vec::is_empty),
        "month analytics deferred for physical-account recovery",
    )?;
    let states = protocol_states(&mut repository)?;
    require(
        states
            .iter()
            .all(|r| matches!(r["state"]["phase"].as_str(), Some("FLAT" | "REJECTED"))),
        "month analytics deferred while a protocol is active",
    )?;
    repository.acquire_demo_broker_lease(&new_id("month_analytics"), 300)?;
    let audit = (|| {
        require(
            repository
                .physical_unresolved_protocols_for_audit()?
                .as_array()
                .is_some_and(Vec::is_empty),
            "month analytics physical recovery priority changed",
        )?;
        require(
            protocol_states(&mut repository)?
                .iter()
                .all(|r| matches!(r["state"]["phase"].as_str(), Some("FLAT" | "REJECTED"))),
            "month analytics deferred: protocol changed before fence",
        )?;
        let broker: Trading212Config = read(&super::eur_shadow::resolve(
            config_path,
            config
                .trading212_config
                .as_ref()
                .ok_or_else(|| Error("DEMO broker config missing".into()))?,
        ))?;
        require(
            broker.environment == Environment::Demo && broker.broker_account_id == 52211463,
            "month report is DEMO only",
        )?;
        let mut reader = Trading212Reader::new_bound(
            broker,
            Credentials::from_store(Environment::Demo, &CredentialStore::load()?)?,
        )
        .map_err(|_| Error("DEMO cash collector unavailable".into()))?;
        let initial = reader
            .entry_snapshot_paced(&config.account_id)
            .map_err(|_| Error("analytics account read unavailable".into()))?;
        require(
            initial.positions.is_empty() && initial.orders.is_empty(),
            "analytics deferred while broker holding/orders exist",
        )?;
        if Utc::now() < operation.ends_at {
            let identities: ShadowCrosswalk = read(&super::eur_shadow::resolve(
                config_path,
                &config
                    .shadow
                    .as_ref()
                    .ok_or_else(|| Error("calendar configuration missing".into()))?
                    .crosswalk_config,
            ))?;
            let calendar = reader
                .exchanges_snapshot()
                .map_err(|_| Error("analytics calendar unavailable".into()))?;
            for identity in &identities.members {
                let session = session_from_catalog(
                    identity,
                    &config.session_deadlines,
                    &calendar,
                    Utc::now(),
                    Utc::now(),
                )?;
                require(
                    Utc::now() < session.opens_at || Utc::now() >= session.flatten_at,
                    "analytics deferred during the entry session",
                )?;
            }
        }
        let mut order_ids = BTreeSet::new();
        let mut completed = 0usize;
        let mut unresolved = Vec::new();
        let mut by_intent = Vec::new();
        for row in &states {
            let state = &row["state"];
            let at: DateTime<Utc> = serde_json::from_value(state["context"]["created_at"].clone())
                .map_err(|_| Error("protocol time missing".into()))?;
            if at < operation.starts_at || at >= cutoff {
                continue;
            }
            for progress in std::iter::once(&state["entry"])
                .chain(std::iter::once(&state["protection"]))
                .chain(state["exits"].as_array().into_iter().flatten())
            {
                if let Some(id) = progress["broker_id"].as_str() {
                    order_ids.insert(id.to_string());
                }
            }
            if state["phase"] == "FLAT"
                && state["entry"]["filled"]
                    .as_str()
                    .and_then(|s| s.parse::<Decimal>().ok())
                    .is_some_and(|q| q > Decimal::ZERO)
            {
                completed += 1;
            } else if state["phase"] != "REJECTED" && state["phase"] != "FLAT" {
                unresolved.push(row["intent_id"].clone());
            }
            by_intent.push(row.clone());
        }
        let mut fills = BTreeMap::<String, Value>::new();
        let mut unattributed = BTreeSet::new();
        period_documents(
            &mut repository,
            "protocol_broker_observation",
            operation.starts_at,
            cutoff,
            |row| {
                let evidence = &row["document"]["observation"]["evidence"];
                require(
                    evidence["provider"] == "TRADING212_DEMO",
                    "month economics requires native broker observations",
                )?;
                for response in evidence["responses"].as_array().into_iter().flatten() {
                    for item in response["response"]["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        if item["fill"].is_null() {
                            continue;
                        }
                        let at: DateTime<Utc> =
                            serde_json::from_value(item["fill"]["filledAt"].clone())
                                .map_err(|_| Error("native fill time missing".into()))?;
                        if at < operation.starts_at || at >= cutoff {
                            continue;
                        }
                        let id = item["fill"]["id"].to_string();
                        if let Some(old) = fills.get(&id) {
                            require(
                                old["fill"] == item["fill"],
                                "conflicting duplicate month fill",
                            )?;
                        }
                        let order = item["order"]["id"].to_string();
                        if order_ids.contains(&order) {
                            fills.insert(id, item.clone());
                        }
                    }
                }
                Ok(())
            },
        )?;
        let native_history = reader
            .eur_month_order_history(|| {
                repository.renew_executor_lease(300).map_err(|_| {
                    trady_application::broker::BrokerFailure::new(
                        trady_application::broker::BrokerFailureKind::Transport,
                    )
                })
            })
            .map_err(|_| Error("account-wide native fill coverage unavailable".into()))?;
        super::eur_shadow::private_json(&out.join("native-order-history.json"), &native_history)?;
        let mut native_fills = BTreeMap::<String, Value>::new();
        for page in native_history["pages"].as_array().into_iter().flatten() {
            for item in page["response"]["items"].as_array().into_iter().flatten() {
                if item["fill"].is_null() {
                    continue;
                }
                let at: DateTime<Utc> = serde_json::from_value(item["fill"]["filledAt"].clone())
                    .map_err(|_| Error("account-wide native fill time missing".into()))?;
                if at < operation.starts_at || at >= cutoff {
                    continue;
                }
                require(
                    item["fill"]["id"].as_u64().is_some_and(|id| id > 0)
                        && item["order"]["id"].as_u64().is_some_and(|id| id > 0),
                    "native fill/order ID missing",
                )?;
                let id = item["fill"]["id"].to_string();
                if let Some(old) = native_fills.insert(id.clone(), item.clone()) {
                    require(
                        old["fill"] == item["fill"] && old["order"]["id"] == item["order"]["id"],
                        "conflicting native history duplicate",
                    )?;
                }
            }
        }
        let (strategy_native, unknown, capability_coverage_complete) =
            super::eur_attribution::classify(
                &native_fills,
                &fills,
                &order_ids,
                &attribution,
                operation.starts_at,
                cutoff,
            )?;
        unattributed.extend(unknown);
        let (lifecycle_coverage_complete, extra) =
            lifecycle_fill_coverage(&fills, &strategy_native, &order_ids);
        unattributed.extend(extra);
        let fills = native_fills;
        let decimal = |value: &Value| -> Result<Decimal> {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
                .parse()
                .map_err(|_| Error("native EUR amount unavailable".into()))
        };
        let mut wallet_delta = Decimal::ZERO;
        let mut strategy_delta = Decimal::ZERO;
        let mut capability_delta = Decimal::ZERO;
        let mut unattributed_delta = Decimal::ZERO;
        let mut first_movement: Option<DateTime<Utc>> = None;
        for (id, row) in &fills {
            let wallet = &row["fill"]["walletImpact"];
            require(
                wallet["currency"] == "EUR"
                    && decimal(&wallet["fxRate"])? == Decimal::ONE
                    && wallet["taxes"].is_array(),
                "native month fill currency/FX/taxes unresolved",
            )?;
            let net = decimal(&wallet["netValue"])?;
            require(net >= Decimal::ZERO, "native wallet value sign unresolved")?;
            let delta = match row["order"]["side"].as_str() {
                Some("BUY") => -net,
                Some("SELL") => net,
                _ => return Err(Error("native fill side invalid".into())),
            };
            wallet_delta += delta;
            if strategy_native.contains_key(id) {
                strategy_delta += delta;
            } else if attribution.fills.contains_key(id) {
                capability_delta += delta;
            } else {
                unattributed_delta += delta;
            }
            let at: DateTime<Utc> = serde_json::from_value(row["fill"]["filledAt"].clone())
                .map_err(|_| Error("fill clock missing".into()))?;
            first_movement = Some(first_movement.map_or(at, |old| old.min(at)));
        }
        let mut opening_candidates = attribution.opening_accounts.clone();
        period_documents(
            &mut repository,
            "reconciliation",
            operation.starts_at,
            cutoff,
            |row| {
                let account = &row["document"]["account"];
                if account["account_id"] == config.account_id.as_str() {
                    opening_candidates.push(account.clone());
                }
                Ok(())
            },
        )?;
        let mut opening: Option<(DateTime<Utc>, Decimal, Value)> = None;
        for account in opening_candidates {
            if account["positions"] != json!([])
                || account["orders"] != json!([])
                || account["spendable_cash"]["currency"] != "EUR"
            {
                continue;
            }
            let at: DateTime<Utc> = serde_json::from_value(account["observed_at"].clone())
                .map_err(|_| Error("opening snapshot clock missing".into()))?;
            if at < operation.starts_at || at >= cutoff {
                continue;
            }
            if opening.as_ref().is_none_or(|(old, _, _)| at < *old) {
                opening = Some((at, decimal(&account["spendable_cash"]["amount"])?, account));
            }
        }
        let opening_cash = opening.as_ref().map(|(_, cash, _)| *cash);
        let cash_events = reader
            .eur_cash_events(operation.starts_at, || {
                repository.renew_executor_lease(300).map_err(|_| {
                    trady_application::broker::BrokerFailure::new(
                        trady_application::broker::BrokerFailureKind::Transport,
                    )
                })
            })
            .map_err(|_| Error("native cash-event history unavailable; no economic pass".into()))?;
        super::eur_shadow::private_json(&out.join("cash-events.json"), &cash_events)?;
        let mut event_delta = Decimal::ZERO;
        let mut references = BTreeMap::<String, Value>::new();
        let mut events_complete = true;
        for source in ["transactions", "dividends"] {
            events_complete &= cash_events["sources"][source]["complete"] == true;
            for page in cash_events["sources"][source]["pages"]
                .as_array()
                .into_iter()
                .flatten()
            {
                for item in page["response"]["items"].as_array().into_iter().flatten() {
                    let clock = if source == "transactions" {
                        "dateTime"
                    } else {
                        "paidOn"
                    };
                    let at: DateTime<Utc> = serde_json::from_value(item[clock].clone())
                        .map_err(|_| Error("cash event time missing".into()))?;
                    if at < operation.starts_at || at >= cutoff {
                        continue;
                    }
                    first_movement = Some(first_movement.map_or(at, |old| old.min(at)));
                    let reference = item["reference"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| Error("cash event reference missing".into()))?;
                    let key = format!("{source}:{reference}");
                    if let Some(old) = references.get(&key) {
                        require(old == item, "cash event conflict")?;
                        continue;
                    }
                    references.insert(key, item.clone());
                    require(item["currency"] == "EUR", "cash event FX unresolved")?;
                    let amount = decimal(&item["amount"])?;
                    event_delta += if source == "dividends" {
                        amount
                    } else {
                        match item["type"].as_str() {
                            Some("DEPOSIT" | "INTEREST_ON_FREE_CASH" | "LENDING_INTEREST")
                                if amount >= Decimal::ZERO =>
                            {
                                amount
                            }
                            Some("WITHDRAW" | "FEE") if amount >= Decimal::ZERO => -amount,
                            _ => {
                                events_complete = false;
                                Decimal::ZERO
                            }
                        }
                    };
                }
            }
        }
        let ending = reader
            .entry_snapshot_paced(&config.account_id)
            .map_err(|_| Error("closing account snapshot unavailable".into()))?;
        let expected = opening_cash.map(|cash| cash + wallet_delta + event_delta);
        let opening_temporal_coverage =
            opening_covers_movements(opening.as_ref().map(|(at, _, _)| *at), first_movement);
        let reconciled = opening_temporal_coverage
            && capability_coverage_complete
            && events_complete
            && native_history["complete"] == true
            && lifecycle_coverage_complete
            && unattributed.is_empty()
            && unresolved.is_empty()
            && ending.positions.is_empty()
            && ending.orders.is_empty()
            && expected == Some(ending.spendable_cash.amount);
        let result = json!({"schema_version":1,"run_id":operation.run_id,"environment":"demo","period_start":operation.starts_at,"cutoff":cutoff,"status":if reconciled{"RECONCILED_DEMO_FACTS"}else{"ECONOMIC_RECONCILIATION_BLOCKED"},
        "completed_protocols":completed,"native_fill_count":fills.len(),"account_wide_native_history_complete":native_history["complete"],"lifecycle_fill_coverage_complete":lifecycle_coverage_complete,"protocols":by_intent,"unresolved_intents":unresolved,"unattributed_order_ids":unattributed,"native_fills":fills.values().collect::<Vec<_>>(),
        "opening_cash_eur":opening_cash,"wallet_cash_delta_eur":wallet_delta,"strategy_wallet_delta_eur":strategy_delta,"capability_wallet_delta_eur":capability_delta,"unattributed_wallet_delta_eur":unattributed_delta,"capability_fill_ids":attribution.fills.keys().collect::<Vec<_>>(),"capability_coverage_complete":capability_coverage_complete,"strategy_native_fill_count":strategy_native.len(),"opening_observation":opening.as_ref().map(|(_,_,account)|account),"opening_precedes_period_movements":opening_temporal_coverage,"cash_events_net_eur":event_delta,"cash_events_complete":events_complete,"expected_flat_cash_eur":expected,"ending_account":ending,"cash_arithmetic_reconciled":reconciled,
        "cash_flow_adjusted_investment_return":null,"optimization_decision":"KEEP","promotion_requires_independent_offline_validation":true,"automatic_model_promotion":false,"live_activation_authorized":false,"broker_mutations":0});
        super::eur_shadow::private_json(&out.join("month-report.json"), &result)?;
        let result_id = new_id("month_report");
        let saved=repository.append_document("demo_month_report",&result_id,&json!({"schema_version":1,"run_id":operation.run_id,"cutoff":cutoff,"economic_reconciliation_complete":reconciled,"completed_protocols":completed,"native_fill_count":fills.len(),"optimization_decision":"KEEP","report_file":out.join("month-report.json"),"report_sha256":file_hash(&out.join("month-report.json"))?,"entry_authorized":false}));
        saved?;
        Ok(result)
    })();
    let _ = repository.release_executor_lease();
    audit
}

fn prepare(
    config: &EurRuntimeConfig,
    operation: &DemoOperation,
    bundle: &Path,
    manifest: &Value,
    out: &Path,
) -> Result<Option<Value>> {
    let Some(result) = super::eur_shadow::current_result(&operation.source_runtime_config)? else {
        return Ok(None);
    };
    if result["selection"]["selected_instrument_id"].is_null() || result["risk_plan"].is_null() {
        return Ok(None);
    }
    let selection: SelectionRun = serde_json::from_value(result["selection"].clone())
        .map_err(|_| Error("durable selection invalid".into()))?;
    let plan: trady_policy::intraday::IntradayTradePlan =
        serde_json::from_value(result["risk_plan"].clone())
            .map_err(|_| Error("durable risk plan invalid".into()))?;
    super::eur_shadow::private_dir(out)?;
    let universe: trady_domain::EurUniverseSnapshot = read(&super::eur_shadow::resolve(
        out,
        config
            .universe_config
            .as_ref()
            .ok_or_else(|| Error("universe missing".into()))?,
    ))?;
    config.validate_universe(&universe, Utc::now())?;
    let binding = universe
        .member(&plan.instrument_id)
        .ok_or_else(|| Error("selected binding missing".into()))?
        .clone();
    let crosswalk_path = artifact(bundle, manifest, "crosswalk")?;
    let crosswalk: ShadowCrosswalk = read(&crosswalk_path)?;
    crosswalk.validate(Utc::now())?;
    let identity = crosswalk
        .members
        .iter()
        .find(|m| m.internal_instrument_id == plan.instrument_id)
        .ok_or_else(|| Error("selected identity missing".into()))?;
    let broker: Trading212Config = read(&artifact(bundle, manifest, "broker_config")?)?;
    require(
        broker.environment == Environment::Demo
            && broker.broker_account_id == 52211463
            && broker.account_id == config.account_id,
        "DEMO account changed",
    )?;
    let mut reader = Trading212Reader::new_bound(
        broker,
        Credentials::from_store(Environment::Demo, &CredentialStore::load()?)?,
    )
    .map_err(|_| Error("DEMO reader unavailable".into()))?;
    let account = reader
        .read_snapshot(&config.account_id)
        .map_err(|_| Error("current account unavailable".into()))?;
    require(
        account.positions.is_empty() && account.orders.is_empty(),
        "new entry requires a flat broker account",
    )?;
    // The execution process alone obtains the entry catalog before its final
    // quote/gate. A duplicate preparation request would hit the50s bucket.
    let calendar = reader
        .exchanges_snapshot()
        .map_err(|_| Error("current calendar unavailable".into()))?;
    let session = session_from_catalog(
        identity,
        &config.session_deadlines,
        &calendar,
        Utc::now(),
        Utc::now(),
    )?;
    require(
        session.permits_entry(Utc::now(), config.eligibility.minimum_seconds_to_flatten),
        "entry session closed or ending",
    )?;
    let ibkr: IbkrSmartConfig = read(&artifact(bundle, manifest, "ibkr_config")?)?;
    let raw = ibkr.capture(&crosswalk_path, &out.join("quote-raw.json"))?;
    let normalized = ibkr.normalize(
        identity,
        &raw,
        Utc::now(),
        config.eligibility.quote_max_age_seconds,
    )?;
    let quote = venue_quote(identity, &normalized, new_id("operation_quote"), &session)?;
    let verified = |id: &str| Capability {
        support: Support::Verified,
        evidence_id: Some(id.into()),
        valid_until: Some(operation.ends_at),
    };
    let capabilities = Capabilities {
        cash_account: verified(&account.snapshot_id),
        instrument: verified(&manifest["artifacts"]["instrument_rules"]["sha256"].to_string()),
        fractional_orders: verified("reviewed_native_fractional_demo"),
        protective_orders: verified("reviewed_sequential_native_stop_protocol"),
        quotes: verified(&quote.quote_id),
    };
    let intent_id = entry_intent(&operation.run_id, &selection.session_id);
    let snapshot = OrderGateSnapshot {
        account,
        universe,
        binding,
        quote,
        session,
        capabilities,
        selection,
        plan,
        daily_entry_count: 0,
    };
    super::eur_shadow::private_json(
        &out.join("order-gate.json"),
        &serde_json::to_value(&snapshot)
            .map_err(|_| Error("order snapshot encoding failed".into()))?,
    )?;
    Ok(Some(
        json!({"intent_id":intent_id,"snapshot":out.join("order-gate.json"),"source_result":result["cycle_id"],"entry_authorized":false}),
    ))
}

fn collect_closed_session_report(
    config_path: &str,
    operation: &DemoOperation,
    repository: &mut PgTargetStore,
    now: DateTime<Utc>,
) -> Result<Option<Value>> {
    if now < operation.starts_at || (now < operation.ends_at && now.hour() < 19) {
        return Ok(None);
    }
    let reports = repository.list_documents("demo_month_report", operation.starts_at, 100)?;
    let reports = reports
        .as_array()
        .ok_or_else(|| Error("month report list invalid".into()))?;
    require(reports.len() < 100, "month report list truncated")?;
    let day = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    if reports.iter().any(|r| {
        r["document"]["run_id"] == operation.run_id
            && r["document"]["cutoff"]
                .as_str()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .is_some_and(|at| at >= operation.ends_at || (now < operation.ends_at && at >= day))
    }) {
        return Ok(None);
    }
    let out = Path::new(&operation.artifact_directory)
        .join("reports")
        .join(new_id("native_month_report"));
    fs::create_dir_all(out.parent().unwrap())
        .map_err(|_| Error("month report directory unavailable".into()))?;
    match report(&[
        "--config".into(),
        config_path.into(),
        "--out".into(),
        out.to_string_lossy().into_owned(),
    ]) {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            let value = json!({"status":"MONTH_REPORT_DEFERRED","reason":error.to_string(),"entry_authorized":false,"broker_mutations":0});
            if out.is_dir() {
                super::eur_shadow::private_json(&out.join("deferred.json"), &value)?;
            }
            Ok(Some(value))
        }
    }
}

pub fn run(args: &[String]) -> Result<Value> {
    let once = args.iter().any(|a| a == "--once");
    let args: Vec<String> = args
        .iter()
        .filter(|a| a.as_str() != "--once")
        .cloned()
        .collect();
    let values = super::eur::options(
        &args,
        &["--config", "--bundle", "--owner-approval"],
        &["--config", "--bundle", "--owner-approval"],
    )?;
    let bundle = Path::new(&values["--bundle"]);
    let manifest: Value = read(&bundle.join("manifest.json"))?;
    let config: EurRuntimeConfig = read(Path::new(&values["--config"]))?;
    config.validate(Utc::now())?;
    require(
        config.enabled && config.execution_stage == ExecutionStage::Demo && !config.live_enabled,
        "DEMO supervisor requires enabled DEMO only",
    )?;
    require(
        file_hash(Path::new(&values["--config"]))?
            == file_hash(&artifact(bundle, &manifest, "runtime")?)?
            && file_hash(
                &std::env::current_exe().map_err(|_| Error("binary unavailable".into()))?,
            )? == file_hash(&artifact(bundle, &manifest, "binary")?)?,
        "operation binary or config differs from exact release",
    )?;
    let operation_path = super::eur_shadow::resolve(
        Path::new(&values["--config"]),
        config
            .operation_config
            .as_ref()
            .ok_or_else(|| Error("bounded DEMO operation policy missing".into()))?,
    );
    require(
        file_hash(&operation_path)? == file_hash(&artifact(bundle, &manifest, "demo_operation")?)?,
        "operation policy differs from exact release",
    )?;
    let operation: DemoOperation = read(&operation_path)?;
    operation.validate(&config)?;
    require(
        file_hash(Path::new(&operation.source_runtime_config))?
            == file_hash(&artifact(bundle, &manifest, "shadow_runtime")?)?,
        "research runtime differs from exact release",
    )?;
    let source: EurRuntimeConfig = read(Path::new(&operation.source_runtime_config))?;
    require(
        source.execution_stage == ExecutionStage::Shadow
            && !source.live_enabled
            && source.account_id == config.account_id
            && source.owner_id == config.owner_id
            && source.policy == config.policy
            && source.approved_universe_snapshot_id == config.approved_universe_snapshot_id,
        "research and DEMO authorities differ",
    )?;
    loop {
        let authorized = approved(bundle, &manifest, &values["--owner-approval"])?;
        let mut repository = store(&config)?;
        let states = protocol_states(&mut repository)?;
        let open: Vec<_> = states
            .iter()
            .filter(|v| !matches!(v["state"]["phase"].as_str(), Some("FLAT" | "REJECTED")))
            .collect();
        require(
            open.len() <= 1,
            "multiple unfinished protocol intents require operator review",
        )?;
        let now = Utc::now();
        let month_report = if open.is_empty() && !once {
            collect_closed_session_report(&values["--config"], &operation, &mut repository, now)?
        } else {
            None
        };
        let checkpoint = if let Some(pending) = open.first() {
            require(
                pending["release_sha256"] == file_hash(&bundle.join("manifest.json"))?,
                "unfinished protocol is bound to another release; preserve its original recovery bundle",
            )?;
            let mut args = vec![
                "--config".into(),
                values["--config"].clone(),
                "--bundle".into(),
                values["--bundle"].clone(),
                "--owner-approval".into(),
                values["--owner-approval"].clone(),
                "--intent".into(),
                pending["intent_id"]
                    .as_str()
                    .ok_or_else(|| Error("pending intent invalid".into()))?
                    .into(),
            ];
            if now >= operation.ends_at {
                args.extend(["--exit-reason".into(), "DEMO_OPERATION_ENDED".into()]);
            }
            super::eur_execute::run(&args)?
        } else if now >= operation.ends_at {
            return Ok(
                json!({"status":"DEMO_OPERATION_ENDED_FLAT","month_report":month_report,"entry_authorized":false,"live_enabled":false}),
            );
        } else if !authorized {
            json!({"status":"DEMO_RELEASE_OR_OWNER_APPROVAL_BLOCKED","entry_authorized":false})
        } else if now < operation.starts_at {
            json!({"status":"WAITING_FOR_DEMO_OPERATION_START","entry_authorized":false})
        } else if repository
            .physical_unresolved_protocols()?
            .as_array()
            .is_none_or(|rows| !rows.is_empty())
        {
            json!({"status":"PHYSICAL_ACCOUNT_UNRESOLVED_INTENT","entry_authorized":false})
        } else if repository.daily_entry_count(now.date_naive())?
            >= i64::from(operation.max_daily_entries)
        {
            json!({"status":"DAILY_ENTRY_SLOT_SPENT","entry_authorized":false})
        } else {
            let path = Path::new(&operation.artifact_directory).join(new_id("entry_preparation"));
            fs::create_dir_all(Path::new(&operation.artifact_directory))
                .map_err(|_| Error("operation directory unavailable".into()))?;
            if let Some(prepared) = prepare(&config, &operation, bundle, &manifest, &path)? {
                super::eur_execute::run(&[
                    "--config".into(),
                    values["--config"].clone(),
                    "--bundle".into(),
                    values["--bundle"].clone(),
                    "--owner-approval".into(),
                    values["--owner-approval"].clone(),
                    "--intent".into(),
                    prepared["intent_id"]
                        .as_str()
                        .ok_or_else(|| Error("prepared intent missing".into()))?
                        .into(),
                    "--snapshot".into(),
                    prepared["snapshot"]
                        .as_str()
                        .ok_or_else(|| Error("prepared snapshot missing".into()))?
                        .into(),
                ])?
            } else {
                json!({"status":"WAITING_FOR_GENUINE_CURRENT_SELECTION_AND_RISK","entry_authorized":false})
            }
        };
        if once {
            return Ok(checkpoint);
        }
        println!("{checkpoint}");
        thread::sleep(Duration::from_secs(operation.poll_seconds));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn research_cycle_identity_and_opening_clock_are_not_relaxed_by_thirty_entries() {
        assert_eq!(
            entry_intent("month", "cycle_a"),
            entry_intent("month", "cycle_a")
        );
        assert_ne!(
            entry_intent("month", "cycle_a"),
            entry_intent("month", "cycle_b")
        );
        let at = Utc::now();
        assert!(opening_covers_movements(
            Some(at),
            Some(at + chrono::Duration::seconds(1))
        ));
        assert!(!opening_covers_movements(
            Some(at + chrono::Duration::seconds(1)),
            Some(at)
        ));
        assert!(!opening_covers_movements(None, Some(at)));
        assert!(!opening_covers_movements(None, None));
    }
    #[test]
    fn account_cash_equality_cannot_hide_untracked_or_missing_native_fills() {
        let row = json!({"fill":{"id":1,"walletImpact":{"netValue":"0"}},"order":{"id":11,"side":"BUY","quantity":"1"}});
        let retained = std::collections::BTreeMap::from([("1".into(), row.clone())]);
        let orders = std::collections::BTreeSet::from(["11".into()]);
        let mut native = retained.clone();
        assert_eq!(
            lifecycle_fill_coverage(&retained, &native, &orders),
            (true, Default::default())
        );
        native.insert("2".into(),json!({"fill":{"id":2,"walletImpact":{"netValue":"0"}},"order":{"id":12,"side":"SELL","quantity":"1"}}));
        assert_eq!(
            lifecycle_fill_coverage(&retained, &native, &orders),
            (false, std::collections::BTreeSet::from(["12".into()]))
        );
        native.clear();
        assert!(!lifecycle_fill_coverage(&retained, &native, &orders).0);
        let mut changed = row;
        changed["fill"]["walletImpact"]["netValue"] = json!("1");
        native.insert("1".into(), changed);
        assert!(!lifecycle_fill_coverage(&retained, &native, &orders).0);
    }
    #[test]
    fn entry_listing_requires_exact_identity_schedule_and_no_duplicates() {
        let crosswalk: ShadowCrosswalk = serde_json::from_str(include_str!(
            "../../../docs/evidence/eur-shadow-review-2026-11-04/crosswalk.json"
        ))
        .unwrap();
        let identity = &crosswalk.members[0];
        let row = json!({"ticker":identity.broker_ticker,"isin":identity.isin,"currencyCode":"EUR","type":"STOCK","workingScheduleId":identity.working_schedule_id});
        check_entry_catalog(identity, &json!([row.clone()])).unwrap();
        for (field, value) in [
            ("isin", json!("wrong")),
            ("currencyCode", json!("USD")),
            ("type", json!("ETF")),
            ("workingScheduleId", json!(-1)),
        ] {
            let mut changed = row.clone();
            changed[field] = value;
            assert!(check_entry_catalog(identity, &json!([changed])).is_err());
        }
        assert!(check_entry_catalog(identity, &json!([row.clone(), row])).is_err());
    }

    #[test]
    #[ignore = "requires the disposable PostgreSQL runner"]
    fn month_record_window_and_unsubmitted_restart_are_fenced() {
        assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
        let at = Utc::now();
        let account = new_id("paper_month_restart_fixture");
        let mut repository = PgTargetStore::connect(
            &std::env::var("TRADY_EXECUTOR_DSN").unwrap(),
            "trady_executor",
            "month_test_owner".into(),
            account.clone(),
        )
        .unwrap();
        repository
            .acquire_executor_lease("fixture_writer", 120)
            .unwrap();
        for i in 0..140 {
            repository
                .append_document(
                    "month_window_fixture",
                    &format!("row_{i:03}"),
                    &json!({"i":i,"classification":"SYNTHETIC_DATABASE_TEST"}),
                )
                .unwrap();
        }
        let mut found = std::collections::BTreeSet::new();
        period_documents(
            &mut repository,
            "month_window_fixture",
            at - chrono::Duration::seconds(1),
            Utc::now() + chrono::Duration::seconds(1),
            |row| {
                assert!(found.insert(row["record_id"].as_str().unwrap().to_string()));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(found.len(), 140);
        assert!(
            repository
                .list_record_window(
                    "month_window_fixture",
                    at - chrono::Duration::days(32),
                    at,
                    None,
                    None
                )
                .is_err()
        );
        assert!(
            repository
                .list_record_window(
                    "month_window_fixture",
                    at - chrono::Duration::seconds(1),
                    at,
                    None,
                    Some("unpaired")
                )
                .is_err()
        );
        let before = repository
            .list_record_window(
                "month_window_fixture",
                at - chrono::Duration::seconds(2),
                at - chrono::Duration::seconds(1),
                None,
                None,
            )
            .unwrap();
        assert_eq!(before, json!([]));
        let context:ProtocolContext=serde_json::from_value(json!({"runtime_mode":"DEMO","policy_revision":"eur_intraday_v1","intent_id":"unsubmitted_context",
            "account_id":account,"broker_account_id":52211463,"instrument_id":"AIR_PA_EQ","broker_ticker":"AIRp_EQ","isin":"NL0000235190",
            "policy_hash":"a".repeat(64),"configuration_hash":"b".repeat(64),"evidence_ids":["synthetic_database_test"],"quantity":"1","entry_limit":"200","initial_stop":"198","target":"204",
            "created_at":at,"flatten_at":at+chrono::Duration::hours(1),"max_unprotected_seconds":10})).unwrap();
        repository.append_document("protocol_binding",&context.intent_id,&json!({"context":context,"release_sha256":"c".repeat(64),"classification":"SYNTHETIC_DATABASE_TEST"})).unwrap();
        repository.release_executor_lease().unwrap();
        assert!(protocol_needs_execution(&mut repository).unwrap());
        let states = protocol_states(&mut repository).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0]["state"]["phase"], "FLAT");
        let records = repository.load(&context.intent_id).unwrap();
        assert!(records.iter().all(|r| !matches!(
            r.input,
            ProtocolInput::DispatchEntry
                | ProtocolInput::DispatchProtection
                | ProtocolInput::DispatchExit
        )));
        assert_eq!(
            protocol_states(&mut repository).unwrap(),
            states,
            "restart must not create a dispatch or duplicate journal records"
        );
        assert!(!protocol_needs_execution(&mut repository).unwrap());
        let generation = repository
            .acquire_executor_lease("crashed_worker", 180)
            .unwrap();
        let mut replacement = PgTargetStore::connect(
            &std::env::var("TRADY_EXECUTOR_DSN").unwrap(),
            "trady_executor",
            "month_test_owner".into(),
            context.account_id.clone(),
        )
        .unwrap();
        assert!(
            replacement
                .acquire_executor_lease("replacement", 180)
                .is_err(),
            "live backend must retain exclusive fence"
        );
        drop(repository); // Connection death without lease release, as after an abrupt process exit.
        let mut claimed = None;
        for _ in 0..100 {
            if let Ok(next) = replacement.acquire_executor_lease("replacement", 180) {
                claimed = Some(next);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            claimed,
            Some(generation + 1),
            "dead backend must not retain its180second lease"
        );
        assert!(
            replacement
                .assert_executor_lease("crashed_worker", generation)
                .is_err()
        );
        assert_eq!(
            recover(&replacement.load(&context.intent_id).unwrap())
                .unwrap()
                .phase,
            trady_application::eur_protocol::ProtocolPhase::Flat
        );
        replacement.release_executor_lease().unwrap();
    }

    #[test]
    fn month_policy_rejects_live_promotion_unbounded_time_and_risk_expansion() {
        let config: EurRuntimeConfig =
            serde_json::from_str(include_str!("../../../config/eur-shadow.example.json")).unwrap();
        let policy = json!({"schema_version":1,"run_id":"eur_demo_month","owner_id":config.owner_id,"account_id":config.account_id,"broker_account_id":52211463,"environment":"demo","live_enabled":false,
            "starts_at":"2026-11-06T00:00:00Z","ends_at":"2026-12-06T00:00:00Z","source_runtime_config":"/etc/trady/runtime/eur-shadow.json","artifact_directory":"/var/lib/trady-executor/eur-demo/month","poll_seconds":60,
            "automatic_live_activation":false,"automatic_model_promotion":false,"max_daily_entries":1,"max_open_positions":1,"no_overnight":true});
        serde_json::from_value::<DemoOperation>(policy.clone())
            .unwrap()
            .validate(&config)
            .unwrap();
        for (field, value) in [
            ("environment", json!("live")),
            ("automatic_live_activation", json!(true)),
            ("automatic_model_promotion", json!(true)),
            ("ends_at", json!("2027-01-06T00:00:00Z")),
            ("max_daily_entries", json!(2)),
            (
                "artifact_directory",
                json!("/var/lib/trady-executor/eur-demo/../other"),
            ),
        ] {
            let mut invalid = policy.clone();
            invalid[field] = value;
            assert!(
                serde_json::from_value::<DemoOperation>(invalid)
                    .unwrap()
                    .validate(&config)
                    .is_err(),
                "{field}"
            );
        }
        let mut updated_config = config.clone();
        updated_config.policy.revision = trady_policy::intraday::THIRTY_ENTRY_POLICY_VERSION.into();
        updated_config.policy.max_daily_entries = 30;
        updated_config.policy.max_daily_exits = None;
        let mut updated = policy;
        updated["max_daily_entries"] = json!(30);
        assert!(
            serde_json::from_value::<DemoOperation>(updated.clone())
                .unwrap()
                .validate(&updated_config)
                .is_err(),
            "new month cannot omit the capability audit role"
        );
        updated["capability_attribution_config"] =
            json!("/opt/trady/config/reviewed-attribution.json");
        updated["capability_evidence_config"] =
            json!("/opt/trady/config/retained-attribution-proof.json");
        serde_json::from_value::<DemoOperation>(updated.clone())
            .unwrap()
            .validate(&updated_config)
            .unwrap();
        updated["max_daily_entries"] = json!(31);
        assert!(
            serde_json::from_value::<DemoOperation>(updated)
                .unwrap()
                .validate(&updated_config)
                .is_err()
        );
    }
}

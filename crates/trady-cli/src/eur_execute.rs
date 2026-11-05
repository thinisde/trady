//! Explicit, evidence-gated DEMO driver. SHADOW services never call this module.
use chrono::Utc;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use trady_adapters::{
    credentials::CredentialStore,
    trading212::{
        Credentials, Environment, Trading212Config, Trading212Executor,
        eur_protocol::Trading212ProtocolBroker,
    },
};
use trady_application::{
    eur_protocol::{
        ProtocolContext, ProtocolJournal, ProtocolPhase, driver::ProtocolDriver, recover,
    },
    eur_runtime::{EurRuntimeConfig, OrderGateSnapshot, OrderReadiness, ReadinessBlocker},
    harness::{content_hash, new_id},
};
use trady_domain::{Error, ExecutionStage, Result, require};
use trady_storage::target::PgTargetStore;

pub(super) fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let meta =
        fs::symlink_metadata(path).map_err(|_| Error("EUR execution input unavailable".into()))?;
    require(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= 24 * 1024 * 1024,
        "invalid EUR execution input",
    )?;
    serde_json::from_slice(
        &fs::read(path).map_err(|_| Error("EUR execution input unavailable".into()))?,
    )
    .map_err(|_| Error("invalid EUR execution JSON".into()))
}
pub(super) fn file_hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = fs::File::open(path).map_err(|_| Error("release artifact unavailable".into()))?;
    let mut h = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = f
            .read(&mut buffer)
            .map_err(|_| Error("artifact hashing failed".into()))?;
        if n == 0 {
            break;
        }
        h.update(&buffer[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
pub(super) fn artifact(bundle: &Path, manifest: &Value, role: &str) -> Result<PathBuf> {
    let relative = manifest["artifacts"][role]["path"]
        .as_str()
        .ok_or_else(|| Error("release artifact missing".into()))?;
    let p = Path::new(relative);
    require(
        !p.is_absolute()
            && p.components().count() == 2
            && p.starts_with("artifacts")
            && !p
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
        "release path escape",
    )?;
    let path = bundle.join(p);
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| Error("release artifact unavailable".into()))?;
    require(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && !bundle.join("artifacts").is_symlink()
            && file_hash(&path)?
                == manifest["artifacts"][role]["sha256"]
                    .as_str()
                    .unwrap_or_default(),
        "release artifact changed",
    )?;
    Ok(path)
}
pub(super) fn approved(bundle: &Path, manifest: &Value, approval: &str) -> Result<bool> {
    let helper = artifact(bundle, manifest, "activation_helper")?;
    require(
        file_hash(&helper)? == content_hash(include_str!("../../../scripts/verify-eur-release.py")),
        "untrusted EUR activation helper",
    )?;
    let mut child = Command::new("/usr/bin/python3")
        .env_clear()
        .args(["-I"])
        .arg(helper)
        .args(["verify", "--bundle"])
        .arg(bundle)
        .args(["--owner-approval", approval])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Error("activation verifier unavailable".into()))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(30) => {
                thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error("activation verification timed out".into()));
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|_| Error("activation verification failed".into()))?;
    let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
    Ok(output.status.success() && value["activation_authorized"] == true)
}

pub fn run(args: &[String]) -> Result<Value> {
    let once = args.iter().any(|a| a == "--once");
    let args: Vec<_> = args
        .iter()
        .filter(|a| a.as_str() != "--once")
        .cloned()
        .collect();
    let values = super::eur::options(
        &args,
        &[
            "--config",
            "--bundle",
            "--owner-approval",
            "--snapshot",
            "--intent",
            "--exit-reason",
            "--recover-leg",
            "--retry-cancel-leg",
            "--broker-order-id",
            "--review-reference",
        ],
        &["--config", "--bundle", "--owner-approval", "--intent"],
    )?;
    let bundle = Path::new(&values["--bundle"]);
    let manifest: Value = read(&bundle.join("manifest.json"))?;
    let config_path = artifact(bundle, &manifest, "runtime")?;
    require(
        file_hash(Path::new(&values["--config"]))? == file_hash(&config_path)?,
        "runtime differs from release bundle",
    )?;
    require(
        file_hash(
            &std::env::current_exe().map_err(|_| Error("runtime binary unavailable".into()))?,
        )? == file_hash(&artifact(bundle, &manifest, "binary")?)?,
        "runtime binary differs from approved release",
    )?;
    let config: EurRuntimeConfig = read(&config_path)?;
    config.validate(Utc::now())?;
    require(
        config.enabled && config.execution_stage == ExecutionStage::Demo && !config.live_enabled,
        "EUR execution requires enabled DEMO and disabled LIVE",
    )?;
    let authorized = approved(bundle, &manifest, &values["--owner-approval"])?;
    let initial_approval_hash = if authorized {
        Some(file_hash(Path::new(&values["--owner-approval"]))?)
    } else {
        None
    };
    let authority_until = if authorized {
        let mut documents = vec![
            manifest.clone(),
            read::<Value>(Path::new(&values["--owner-approval"]))?,
        ];
        for role in [
            "instrument_rules",
            "quote_compatibility",
            "costs",
            "protocol_acceptance",
            "paper_day_acceptance",
            "independent_review",
            "forward_cost_model",
        ] {
            documents.push(read::<Value>(&artifact(bundle, &manifest, role)?)?);
        }
        let crosswalk: Value = read(&artifact(bundle, &manifest, "crosswalk")?)?;
        let mut until: chrono::DateTime<Utc> =
            serde_json::from_value(crosswalk["expires_at"].clone())
                .map_err(|_| Error("authority expiry missing".into()))?;
        for document in documents {
            let expiry: chrono::DateTime<Utc> =
                serde_json::from_value(document["valid_until"].clone())
                    .map_err(|_| Error("authority expiry missing".into()))?;
            until = until.min(expiry);
        }
        until
    } else {
        Utc::now()
    };
    // Verify evidence before acquiring credentials on a first entry attempt.
    let credentials = CredentialStore::load()?;
    let mut store = PgTargetStore::connect(
        &super::secret("TRADY_EXECUTOR_DSN")?,
        "trady_executor",
        config.owner_id.clone(),
        config.account_id.to_string(),
    )?;
    store.acquire_executor_lease(&new_id("eur_protocol_worker"), 180)?;
    let result = (|| {
        let records = store.load(&values["--intent"])?;
        let mut entry_snapshot = None;
        let mut entry_signal_valid_until = None;
        let mut retained_catalog = None;
        let (mut context, binding) = if records.is_empty() {
            require(
                authorized,
                "EUR release evidence or exact-hash owner approval is missing, expired or rejected",
            )?;
            let operation = super::eur_operation::entry_operation(
                &config,
                Path::new(&values["--config"]),
                bundle,
                &manifest,
            )?;
            let risk_path = config
                .shadow
                .as_ref()
                .and_then(|s| s.risk_config.as_ref())
                .ok_or_else(|| Error("accepted forward EUR costs missing".into()))?;
            require(
                file_hash(&super::eur_shadow::resolve(
                    Path::new(&values["--config"]),
                    risk_path,
                ))? == file_hash(&artifact(bundle, &manifest, "forward_cost_model")?)?,
                "risk cost inputs differ from accepted release model",
            )?;
            let path = values.get("--snapshot").ok_or_else(|| {
                Error("first dispatch requires a complete current order snapshot".into())
            })?;
            let mut snapshot: OrderGateSnapshot = read(Path::new(path))?;
            let cost_acceptance: Value = read(&artifact(bundle, &manifest, "costs")?)?;
            let cost_scope = cost_acceptance["forward_cost_scopes"]
                .as_array()
                .and_then(|scopes| {
                    scopes
                        .iter()
                        .find(|s| s["ticker"] == snapshot.binding.broker_instrument_id)
                })
                .ok_or_else(|| Error("forward cost instrument scope missing".into()))?;
            let minimum: rust_decimal::Decimal =
                serde_json::from_value(cost_scope["minimum_entry_notional_eur"].clone())
                    .map_err(|_| Error("invalid accepted cost scope".into()))?;
            let ratio: rust_decimal::Decimal =
                serde_json::from_value(cost_scope["maximum_exit_to_entry_notional_ratio"].clone())
                    .map_err(|_| Error("invalid accepted cost scope".into()))?;
            require(
                snapshot.plan.quantity * snapshot.plan.entry_price >= minimum
                    && snapshot.plan.target_price <= snapshot.plan.entry_price * ratio,
                "risk plan exceeds accepted forward cost scope",
            )?;
            config.validate_universe(&snapshot.universe, Utc::now())?;
            let research = store
                .read_document(
                    "research_result",
                    &format!("{}_research", snapshot.selection.session_id),
                )?
                .ok_or_else(|| {
                    Error(
                        "selection and risk plan were not produced by the durable EUR workflow"
                            .into(),
                    )
                })?;
            if let Some((_, source_hash)) = &operation {
                let source = store
                    .read_document("shadow_result", &snapshot.selection.session_id)?
                    .ok_or_else(|| Error("approved source result missing".into()))?;
                require(
                    source["configuration_hash"] == *source_hash
                        && source["selection"]
                            == serde_json::to_value(&snapshot.selection).unwrap_or_default()
                        && source["risk_plan"]
                            == serde_json::to_value(&snapshot.plan).unwrap_or_default(),
                    "entry research was not produced by the approved source configuration",
                )?;
            }
            let signal_at: chrono::DateTime<Utc> = serde_json::from_value(
                research["strategy_signals"][snapshot.binding.instrument_id.as_str()]["signal_at"]
                    .clone(),
            )
            .map_err(|_| Error("durable strategy signal time missing".into()))?;
            let interval = config
                .shadow
                .as_ref()
                .and_then(|s| s.bar_interval_seconds)
                .unwrap_or(300);
            require(
                signal_at <= Utc::now()
                    && (Utc::now() - signal_at).num_seconds() <= i64::from(interval) * 2 + 15,
                "durable strategy signal expired before entry",
            )?;
            entry_signal_valid_until =
                Some(signal_at + chrono::Duration::seconds(i64::from(interval) * 2 + 15));
            require(
                research["selection"]
                    == serde_json::to_value(&snapshot.selection)
                        .map_err(|_| Error("selection encoding failed".into()))?
                    && research["risk_plan"]
                        == serde_json::to_value(&snapshot.plan)
                            .map_err(|_| Error("risk plan encoding failed".into()))?,
                "execution snapshot rebinds the durable research or risk plan",
            )?;
            require(
                research["risk_cost_model"]
                    == read::<Value>(&artifact(bundle, &manifest, "forward_cost_model")?)?,
                "durable risk plan used different or unrecorded forward costs",
            )?;
            let costs = snapshot
                .plan
                .cost_model
                .as_ref()
                .ok_or_else(|| Error("entry plan lacks reproducible cost inputs".into()))?;
            require(
                research["risk_cost_model"]["entry"]
                    == serde_json::to_value(&costs.entry).unwrap_or_default()
                    && research["risk_cost_model"]["exit"]
                        == serde_json::to_value(&costs.exit).unwrap_or_default(),
                "entry plan economics differ from the accepted forward cost model",
            )?;
            let rules: Value = read(&artifact(bundle, &manifest, "instrument_rules")?)?;
            require(
                rules["instruments"].as_array().is_some_and(|rows| {
                    rows.iter().any(|r| {
                        r["binding"] == serde_json::to_value(&snapshot.binding).unwrap_or_default()
                    })
                }),
                "binding differs from accepted broker rules",
            )?;
            require(
                snapshot.quote.source == "ibkr_tws_api",
                "real-time quote must originate from IBKR SMART Live",
            )?;
            snapshot.daily_entry_count =
                u8::try_from(store.daily_entry_count(Utc::now().date_naive())?)
                    .map_err(|_| Error("daily entry count overflow".into()))?;
            if let OrderReadiness::Blocked { blockers } = snapshot.evaluate(&config, Utc::now())? {
                require(
                    blockers.iter().all(|b| {
                        matches!(
                            b,
                            ReadinessBlocker::ReleaseEvidenceMissing
                                | ReadinessBlocker::OwnerApprovalMissing
                                | ReadinessBlocker::ProtectionProtocolUnavailable
                                | ReadinessBlocker::ProtectionUnsupported
                        )
                    }),
                    "current EUR order readiness is blocked",
                )?;
            }
            let context = ProtocolContext {
                runtime_mode: ExecutionStage::Demo,
                policy_revision: config.policy.revision.clone(),
                intent_id: values["--intent"].clone(),
                account_id: config.account_id.to_string(),
                broker_account_id: 52211463,
                instrument_id: snapshot.binding.instrument_id.to_string(),
                broker_ticker: snapshot.binding.broker_instrument_id.clone(),
                isin: snapshot.binding.isin.clone(),
                policy_hash: content_hash(
                    &serde_json::to_string(&config.policy)
                        .map_err(|_| Error("policy encoding failed".into()))?,
                ),
                configuration_hash: file_hash(&config_path)?,
                evidence_ids: snapshot.plan.evidence_snapshot_ids.clone(),
                quantity: snapshot.plan.quantity,
                entry_limit: snapshot.plan.entry_price,
                initial_stop: snapshot.plan.stop_price,
                target: snapshot.plan.target_price,
                created_at: Utc::now(),
                flatten_at: snapshot.plan.flatten_deadline.min(authority_until).min(
                    operation
                        .as_ref()
                        .map_or(snapshot.plan.flatten_deadline, |(op, _)| op.ends_at),
                ),
                max_unprotected_seconds: 10,
                entry_valid_until: None,
            };
            context.validate()?;
            entry_snapshot = Some(snapshot.clone());
            (context, snapshot.binding)
        } else {
            let context = recover(&records)?.context;
            let document = store
                .read_document("protocol_binding", &context.intent_id)?
                .ok_or_else(|| Error("durable broker binding missing".into()))?;
            require(
                document["release_sha256"] == file_hash(&bundle.join("manifest.json"))?,
                "recovery release changed",
            )?;
            if let Ok(at) = serde_json::from_value::<chrono::DateTime<Utc>>(
                document["entry_catalog_observed_at"].clone(),
            ) {
                retained_catalog = Some((document["entry_catalog"].clone(), at));
            }
            (
                context,
                serde_json::from_value(document["binding"].clone())
                    .map_err(|_| Error("invalid durable broker binding".into()))?,
            )
        };
        let broker_path = config
            .trading212_config
            .as_ref()
            .ok_or_else(|| Error("broker configuration missing".into()))?;
        let broker: Trading212Config = read(&super::eur_shadow::resolve(
            Path::new(&values["--config"]),
            broker_path,
        ))?;
        require(
            file_hash(&super::eur_shadow::resolve(
                Path::new(&values["--config"]),
                broker_path,
            ))? == file_hash(&artifact(bundle, &manifest, "broker_config")?)?,
            "broker configuration differs from release bundle",
        )?;
        require(
            broker.environment == Environment::Demo
                && broker.account_id == config.account_id
                && broker.broker_account_id == 52211463,
            "broker account differs from release",
        )?;
        let broker_credentials = Credentials::from_store(Environment::Demo, &credentials)?;
        let mut executor = Trading212Executor::new(broker, broker_credentials)
            .map_err(|_| Error("broker transport unavailable".into()))?;
        // Metadata may have a fifty-second account-wide rate limit. Warm it
        // before the driver can cancel a working protective stop on restart.
        let (entry_catalog, entry_catalog_observed_at) = if let Some((raw, at)) = retained_catalog
            && executor
                .information()
                .seed_protocol_catalog(&raw, at)
                .is_ok()
        {
            (raw, at)
        } else {
            let raw = executor
                .information()
                .entry_catalog_snapshot()
                .map_err(|_| Error("broker catalog unavailable before protocol recovery".into()))?;
            let scoped: Vec<_> = raw
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["ticker"] == context.broker_ticker)
                .cloned()
                .collect();
            (json!(scoped), Utc::now())
        };
        let ibkr: trady_adapters::ibkr_smart::IbkrSmartConfig =
            read(&artifact(bundle, &manifest, "ibkr_config")?)?;
        let crosswalk_path = artifact(bundle, &manifest, "crosswalk")?;
        let crosswalk: trady_application::eur_shadow::ShadowCrosswalk = read(&crosswalk_path)?;
        let identity = crosswalk
            .members
            .iter()
            .find(|m| m.internal_instrument_id.as_str() == context.instrument_id)
            .ok_or_else(|| Error("protocol quote identity missing".into()))?;
        ibkr.validate_identity(identity)?;
        if entry_snapshot.is_some() {
            super::eur_operation::check_entry_catalog(identity, &entry_catalog)?;
        }
        let state_directory = Path::new(
            &config
                .shadow
                .as_ref()
                .ok_or_else(|| Error("protocol state directory unavailable".into()))?
                .state_directory,
        );
        fs::create_dir_all(state_directory)
            .map_err(|_| Error("protocol state directory unavailable".into()))?;
        if let Some(mut snapshot) = entry_snapshot {
            snapshot.account = executor
                .information()
                .entry_snapshot_paced(&config.account_id)
                .map_err(|_| Error("entry account refresh unavailable".into()))?;
            let raw_path = state_directory.join(format!("{}.json", new_id("entry_ibkr")));
            let raw = ibkr.capture(&crosswalk_path, &raw_path)?;
            let normalized = ibkr.normalize(
                identity,
                &raw,
                Utc::now(),
                config.eligibility.quote_max_age_seconds,
            )?;
            snapshot.quote = super::eur_operation::venue_quote(
                identity,
                &normalized,
                new_id("entry_live_quote"),
                &snapshot.session,
            )?;
            if let OrderReadiness::Blocked { blockers } = snapshot.evaluate(&config, Utc::now())? {
                require(
                    blockers.iter().all(|b| {
                        matches!(
                            b,
                            ReadinessBlocker::ReleaseEvidenceMissing
                                | ReadinessBlocker::OwnerApprovalMissing
                                | ReadinessBlocker::ProtectionProtocolUnavailable
                                | ReadinessBlocker::ProtectionUnsupported
                        )
                    }),
                    "refreshed entry readiness blocked",
                )?;
            }
            context.created_at = Utc::now();
            context.entry_valid_until = Some(
                (snapshot.quote.observed_at
                    + chrono::Duration::seconds(config.eligibility.quote_max_age_seconds))
                .min(context.flatten_at)
                .min(
                    entry_signal_valid_until
                        .ok_or_else(|| Error("entry signal expiry missing".into()))?,
                ),
            );
            context.validate()?;
            store.append_document("protocol_binding",&context.intent_id,&json!({"binding":binding,"context":context,"initial_order_gate":snapshot,"entry_raw_quote":raw,"entry_catalog":entry_catalog,"entry_catalog_observed_at":entry_catalog_observed_at,"release_sha256":file_hash(&bundle.join("manifest.json"))?}))?;
        }
        let mut port = Trading212ProtocolBroker::new(executor, binding);
        port.require_entry_authority(
            PathBuf::from(&values["--owner-approval"]),
            initial_approval_hash.clone().unwrap_or_default(),
            authority_until,
        );
        let mut driver = ProtocolDriver {
            journal: &mut store,
            broker: port,
        };
        require(
            !(values.contains_key("--recover-leg") && values.contains_key("--retry-cancel-leg")),
            "choose exact-ID recovery or reviewed cancellation retry",
        )?;
        if let Some(leg) = values
            .get("--recover-leg")
            .or_else(|| values.get("--retry-cancel-leg"))
        {
            let leg = match leg.as_str() {
                "ENTRY" => trady_application::eur_protocol::OrderLeg::Entry,
                "PROTECTION" => trady_application::eur_protocol::OrderLeg::Protection,
                "EXIT" => trady_application::eur_protocol::OrderLeg::Exit,
                _ => return Err(Error("invalid reviewed recovery leg".into())),
            };
            let id = values.get("--broker-order-id").ok_or_else(|| {
                Error("reviewed recovery requires an exact broker order ID".into())
            })?;
            let review = values
                .get("--review-reference")
                .ok_or_else(|| Error("reviewed recovery reference missing".into()))?;
            trady_application::eur_protocol::advance(
                &mut driver.journal,
                &context,
                new_id("reviewed_recovery"),
                if values.contains_key("--retry-cancel-leg") {
                    trady_application::eur_protocol::ProtocolInput::ReviewedCancellationRetry {
                        leg,
                        broker_id: id.clone(),
                        review_reference: review.clone(),
                    }
                } else {
                    trady_application::eur_protocol::ProtocolInput::ReviewedBrokerId {
                        leg,
                        broker_id: id.clone(),
                        review_reference: review.clone(),
                    }
                },
                Utc::now(),
            )?;
        } else {
            require(
                !values.contains_key("--broker-order-id")
                    && !values.contains_key("--review-reference"),
                "recovery options require --recover-leg or --retry-cancel-leg",
            )?;
        }
        (|| {
            loop {
                driver.journal.renew_executor_lease(180)?;
                let mut expired = !authorized
                    || Utc::now() >= authority_until
                    || file_hash(Path::new(&values["--owner-approval"])).ok()
                        != initial_approval_hash;
                let state = driver.state(&context)?;
                let mut target_reached = false;
                if state.phase == ProtocolPhase::Protected
                    && Utc::now() < context.flatten_at
                    && !expired
                {
                    let path = state_directory.join(format!("{}.json", new_id("holding_ibkr")));
                    if let Ok(raw) = ibkr.capture(&crosswalk_path, &path)
                        && let Ok(quote) = ibkr.normalize(
                            identity,
                            &raw,
                            Utc::now(),
                            config.eligibility.quote_max_age_seconds,
                        )
                    {
                        let bid: Option<rust_decimal::Decimal> =
                            serde_json::from_value(quote["bid"].clone()).ok();
                        target_reached =
                            quote["fresh"] == true && bid.is_some_and(|b| b >= context.target);
                        driver.journal.append_document("protocol_quote",&new_id("target_book"),&json!({"intent_id":context.intent_id,"raw":raw,"quote":quote,"target_reached":target_reached}))?;
                    }
                }
                expired |= Utc::now() >= authority_until;
                expired |=
                    file_hash(Path::new(&values["--owner-approval"])).ok() != initial_approval_hash;
                let reason = values
                    .get("--exit-reason")
                    .map(String::as_str)
                    .or(if expired {
                        Some("RELEASE_AUTHORITY_EXPIRED")
                    } else if target_reached {
                        Some("TARGET_REACHED")
                    } else {
                        None
                    });
                let state = driver.drive_once(&context, Utc::now(), reason)?;
                let recovery_action = driver.recovery_action(&state)?;
                if once
                    || matches!(state.phase, ProtocolPhase::Flat | ProtocolPhase::Rejected)
                    || matches!(
                        recovery_action,
                        trady_application::eur_protocol::ProtocolEffect::RequireOperator { .. }
                    )
                {
                    return Ok(
                        json!({"status":"EUR_PROTOCOL_CHECKPOINT","state":state,"recovery_action":recovery_action,"atomic_protection":false,"live_enabled":false}),
                    );
                }
                thread::sleep(Duration::from_millis(250));
            }
        })()
    })();
    // Handles validation failures before the driver takes ownership as well.
    let _ = store.release_executor_lease();
    result
}

//! Read-only real-data shadow cycles. No executable broker mutation client is constructed.
use super::*;
use std::{collections::BTreeMap, fs::OpenOptions, io::Write};
use trady_adapters::{
    shadow::{ShadowBroker, ShadowSource},
    trading212::Trading212Reader,
};
use trady_application::{
    broker::{BrokerOrderRequest, OrderKind, TimeInForce},
    optimization::digest,
};

fn record(directory: &Path, value: &Value) -> Result<String> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(directory)
        .map_err(|_| Error("cannot create shadow journal".into()))?;
    let metadata = fs::symlink_metadata(directory)
        .map_err(|_| Error("shadow journal is unavailable".into()))?;
    require(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "shadow journal must be a regular directory",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        require(
            metadata.permissions().mode() & 0o077 == 0,
            "shadow journal must be private to its owner",
        )?;
    }
    let hash = digest(value)?;
    let path = directory.join(format!("cycle_{hash}.json"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| Error("cannot reserve immutable shadow cycle".into()))?;
    let bytes =
        serde_json::to_vec(value).map_err(|_| Error("cannot encode shadow cycle".into()))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| Error("cannot persist shadow cycle".into()))?;
    fs::File::open(directory)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error("cannot persist shadow journal directory".into()))?;
    Ok(hash)
}

pub(super) fn run(config: RuntimeConfig, config_path: &str, once: bool) -> Result<Value> {
    require(
        config.trading.mode == Mode::Shadow,
        "shadow runner requires a shadow namespace",
    )?;
    let directory = Path::new(
        config
            .shadow_directory
            .as_deref()
            .ok_or_else(|| Error("shadow_directory is required".into()))?,
    )
    .to_path_buf();
    require(
        directory.is_absolute(),
        "shadow journal must use an absolute path",
    )?;
    let preflight = configuration_check(&config, config_path)?;
    if preflight["configuration_ready"] != true {
        let now = Utc::now();
        let audit = json!({
            "schema_version":1,"phase":"configuration","mode":"shadow",
            "account_id":config.trading.account_id,"started_at":now,"finished_at":now,
            "configuration_hash":digest(&config)?,"preflight":preflight,"order_mutations":0
        });
        let hash = record(&directory, &audit)?;
        return Ok(json!({
            "status":"shadow_blocked","cycles":0,"preflight":preflight,
            "shadow_record_hash":hash,"order_mutations":0
        }));
    }
    let credentials = CredentialStore::load()?;
    let mut broker = Trading212Reader::new(
        config.broker.clone(),
        Credentials::from_store(config.broker.environment, &credentials)?,
    )
    .map_err(|e| Error(e.to_string()))?;
    let token = credentials
        .optional("TRADY_TRADING_INPUTS_TOKEN")?
        .map(|s| s.into_inner());
    let stopped = Arc::new(AtomicBool::new(false));
    let signal = stopped.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
        .map_err(|_| Error("cannot install shadow shutdown handler".into()))?;
    let mut inputs = if config.analyst_socket.is_some() {
        worker::IsolatedInputs::new_external(config.clone(), config_path, token, stopped.clone())?
    } else {
        worker::IsolatedInputs::new(
            config.clone(),
            config_path,
            credentials
                .required("TRADY_ANALYST_DSN")?
                .expose()
                .to_owned(),
            token,
            stopped.clone(),
        )?
    };
    let mut cycles = 0_u64;
    while !stopped.load(Ordering::Relaxed) {
        let started = Utc::now();
        let result = (|| -> Result<Value> {
            let original =
                crate::broker_read::snapshot(&mut broker, &config.broker.account_id, &stopped)?;
            original.validate()?;
            require(
                trady_policy::fresh(original.observed_at, Utc::now(), 60),
                "shadow broker observation is stale",
            )?;
            let source_hash = digest(&original)?;
            let mut snapshot = original.clone();
            snapshot.account_id = config.trading.account_id.clone();
            snapshot.mode = Mode::Shadow;
            snapshot.snapshot_id = format!("shadow_{}", digest(&snapshot)?);
            if !snapshot.positions.is_empty()
                || snapshot.orders.iter().any(|o| !o.status.terminal())
            {
                return Ok(
                    json!({"status":"no_trade","reason":"real account has holdings or unresolved orders","source_snapshot_hash":source_hash,"snapshot":snapshot}),
                );
            }
            let session = inputs
                .session(&config.trading.account_id, Utc::now())?
                .ok_or_else(|| Error("verified shadow exchange session is unavailable".into()))?;
            let now = Utc::now();
            require(
                session.observed_at <= now
                    && trady_policy::fresh(session.observed_at, now, 60)
                    && session.opens_at <= now
                    && now < session.closes_at - Duration::from_secs(30 * 60)
                    && session.opening_equity.currency == config.trading.base_currency,
                "shadow entry session is stale or closed",
            )?;
            if config.controls.pause_entries || config.controls.close_position {
                return Ok(
                    json!({"status":"paused","source_snapshot_hash":source_hash,"session_id":session.session_id}),
                );
            }
            let Some(candidate) = inputs.candidate(&snapshot, Utc::now())? else {
                return Ok(
                    json!({"status":"no_trade","source_snapshot_hash":source_hash,"session_id":session.session_id}),
                );
            };
            let quote = inputs.quote(&candidate.instrument_id)?;
            candidate.validate(&config.trading, &snapshot, &quote, Utc::now())?;
            let costs = inputs
                .current_exit_costs(&candidate.instrument_id, Utc::now())?
                .ok_or_else(|| Error("shadow exit costs are unavailable".into()))?;
            candidate.validate_refreshed_exit_costs(&costs, Utc::now())?;
            let limits =
                candidate.authoritative_limits(&config.trading, &snapshot, &quote, Utc::now())?;
            let observation = ExecutionObservation {
                snapshot: snapshot.clone(),
                position_costs: BTreeMap::new(),
                available_quantities: BTreeMap::new(),
                total_equity: snapshot.spendable_cash.clone(),
            };
            let mut recording = ShadowBroker::new(
                &directory,
                observation.clone(),
                ShadowSource::Real {
                    market_data_hash: digest(&(&source_hash, &quote))?,
                },
            )?;
            recording.update(
                observation,
                ShadowSource::Real {
                    market_data_hash: digest(&(&source_hash, &quote))?,
                },
            )?;
            let slot = directory.join(format!(
                "session_{}.json",
                digest(&(&config.trading.account_id, &session.session_id))?
            ));
            let request = BrokerOrderRequest {
                intent_id: IntentId::new(format!(
                    "shadow_{}",
                    digest(&(&config.trading.account_id, &session.session_id))?
                ))?,
                account_id: config.trading.account_id.clone(),
                instrument_id: candidate.instrument_id.clone(),
                instrument_currency: candidate.currency.clone(),
                side: Side::Buy,
                quantity: limits.quantity,
                kind: OrderKind::Limit {
                    price: limits.buy_limit,
                    time_in_force: TimeInForce::Day,
                },
            };
            let mut slot_options = OpenOptions::new();
            slot_options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                slot_options.mode(0o600);
            }
            match slot_options.open(&slot) {
                Ok(mut file) => {
                    let bytes = serde_json::to_vec(&request)
                        .map_err(|_| Error("cannot encode shadow session reservation".into()))?;
                    file.write_all(&bytes)
                        .and_then(|()| file.sync_all())
                        .map_err(|_| Error("cannot persist shadow daily slot".into()))?;
                    fs::File::open(&directory)
                        .and_then(|f| f.sync_all())
                        .map_err(|_| Error("cannot persist shadow daily reservation".into()))?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Ok(
                        json!({"status":"daily_shadow_slot_consumed","session_id":session.session_id}),
                    );
                }
                Err(_) => return Err(Error("cannot reserve shadow daily slot".into())),
            }
            let intent_hash = recording.record_proposal(&request)?;
            Ok(
                json!({"status":"shadow_intent_recorded","source_snapshot_hash":source_hash,"session_id":session.session_id,
                "quote":quote,"candidate":candidate,"request":request,"intent_hash":intent_hash,"order_mutations":0}),
            )
        })();
        let value = match result {
            Ok(value) => value,
            Err(error) => json!({"status":"blocked","reason":error.to_string()}),
        };
        let cycle = json!({"schema_version":1,"account_id":config.trading.account_id,"mode":"shadow","started_at":started,
            "finished_at":Utc::now(),"configuration_hash":digest(&config)?,"result":value,"order_mutations":0});
        let hash = record(&directory, &cycle)?;
        super::super::output(json!({"shadow_cycle_hash":hash,"cycle":cycle}))?;
        cycles = cycles
            .checked_add(1)
            .ok_or_else(|| Error("shadow cycle counter exhausted".into()))?;
        if once {
            break;
        }
        for _ in 0..config.interval_seconds * 10 {
            if stopped.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(json!({"status":"shadow_stopped","cycles":cycles,"order_mutations":0}))
}

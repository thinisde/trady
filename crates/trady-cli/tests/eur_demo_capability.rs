//! Explicit online DEMO capability experiment. Never part of normal test runs
//! or release activation. One holding, bounded notional, exact identity and cleanup.
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
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
    broker::{
        BrokerFailure, BrokerFailureKind, BrokerOrderRequest, BrokerResult, CancellationReceipt,
    },
    eur_protocol::{
        ProtocolContext, ProtocolEffect, ProtocolInput, ProtocolJournal, ProtocolPhase,
        ProtocolState, advance,
        driver::{BrokerObservation, ProtectionConfirmation, ProtocolBroker, ProtocolDriver},
    },
    eur_runtime::EurRuntimeConfig,
    eur_shadow::{ShadowCrosswalk, session_from_catalog},
    harness::{content_hash, new_id},
};
use trady_domain::{
    AccountId, BrokerOrder, Currency, EurInstrumentBinding, ExecutionStage, InstrumentId, Side,
};
use trady_storage::target::PgTargetStore;

fn read<T: DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).expect("scoped probe input"))
        .expect("valid scoped probe JSON")
}
fn write(path: &Path, value: &Value) {
    let mut o = fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(path).expect("new private probe artifact");
    f.write_all(serde_json::to_string_pretty(value).unwrap().as_bytes())
        .unwrap();
    f.sync_all().unwrap();
}
fn append(path: &Path, value: &Value) {
    let mut o = fs::OpenOptions::new();
    o.write(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(path).unwrap();
    writeln!(f, "{value}").unwrap();
    f.sync_all().unwrap();
}
fn retry_read<T>(mut f: impl FnMut() -> BrokerResult<T>) -> T {
    let started = Instant::now();
    loop {
        match f() {
            Ok(value) => return value,
            Err(error)
                if error.kind == BrokerFailureKind::RateLimited
                    && started.elapsed() < Duration::from_secs(65) =>
            {
                thread::sleep(Duration::from_secs(
                    error.retry_after_seconds.unwrap_or(1).clamp(1, 5),
                ))
            }
            Err(error) => panic!("bounded broker read unavailable: {error}"),
        }
    }
}
struct ProbeBroker {
    inner: Trading212ProtocolBroker,
    unknown_once: bool,
    reservation_once: bool,
    reservation_id: Option<String>,
    stop_cancel_once: bool,
    stop_cancel_delay_millis: u64,
    hidden_id: Option<String>,
    directory: PathBuf,
}
impl ProtocolBroker for ProbeBroker {
    fn observe(&mut self, state: &ProtocolState) -> BrokerResult<BrokerObservation> {
        self.inner.observe(state)
    }
    fn confirm_entry(&mut self, state: &ProtocolState) -> BrokerResult<Option<BrokerObservation>> {
        self.inner.confirm_entry(state)
    }
    fn prepare_cancel(
        &mut self,
        state: &ProtocolState,
        leg: trady_application::eur_protocol::OrderLeg,
    ) -> BrokerResult<()> {
        self.inner.prepare_cancel(state, leg)
    }
    fn confirm_protection(
        &mut self,
        state: &ProtocolState,
    ) -> BrokerResult<Option<ProtectionConfirmation>> {
        self.inner.confirm_protection(state)
    }
    fn submit(
        &mut self,
        r: &BrokerOrderRequest,
        deadline: DateTime<Utc>,
    ) -> BrokerResult<BrokerOrder> {
        if self.reservation_once
            && matches!(r.kind, trady_application::broker::OrderKind::Stop { .. })
        {
            self.reservation_once = false;
            let mut external = r.clone();
            external.intent_id =
                trady_domain::IntentId::new("controlled_external_reservation_20261105").unwrap();
            let authority: Value = read(&self.directory.join("authority.json"));
            let entry: Decimal = authority["context"]["entry_limit"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            external.kind = trady_application::broker::OrderKind::Limit {
                price: entry * Decimal::from(2),
                time_in_force: trady_application::broker::TimeInForce::Day,
            };
            append(
                &self.directory.join("faults.jsonl"),
                &json!({"at":Utc::now(),"kind":"CONTROLLED_EXTERNAL_RESERVATION_BETWEEN_OBSERVATION_AND_STOP","phase":"BEFORE_SEND","method":"POST","path":"orders/limit","request":external}),
            );
            let result = self.inner.executor.submit_eur_bound_before(
                &external,
                &self.inner.binding,
                Some(deadline),
            );
            append(
                &self.directory.join("transport.jsonl"),
                &json!({"at":Utc::now(),"controlled_external_reservation":true,"request":external,"response":result.as_ref().ok(),"failure_http_status":result.as_ref().err().and_then(|e|e.http_status)}),
            );
            let order = result?;
            self.reservation_id = Some(order.broker_order_id.clone());
            append(
                &self.directory.join("faults.jsonl"),
                &json!({"at":Utc::now(),"phase":"RESPONSE","method":"POST","path":"orders/limit","response":order}),
            );
        }
        let response = self.inner.submit(r, deadline);
        append(
            &self.directory.join("transport.jsonl"),
            &json!({"at":Utc::now(),"request":r,"response":response.as_ref().ok(),"failure":response.as_ref().err().map(|e|json!({"kind":format!("{:?}",e.kind),"http_status":e.http_status,"outcome_unknown":e.outcome_unknown,"unverified_broker_order_id":e.broker_order_id}))}),
        );
        if self.stop_cancel_once
            && matches!(r.kind, trady_application::broker::OrderKind::Stop { .. })
        {
            self.stop_cancel_once = false;
            if let Ok(order) = &response {
                thread::sleep(Duration::from_millis(self.stop_cancel_delay_millis));
                append(
                    &self.directory.join("faults.jsonl"),
                    &json!({"at":Utc::now(),"kind":"CONTROLLED_NATIVE_STOP_CANCEL_RACE","delay_millis":self.stop_cancel_delay_millis,"phase":"BEFORE_SEND","method":"DELETE","path":format!("orders/{}",order.broker_order_id)}),
                );
                let cancel = self.cancel(&r.account_id, &order.broker_order_id);
                append(
                    &self.directory.join("faults.jsonl"),
                    &json!({"at":Utc::now(),"phase":if cancel.as_ref().err().is_some_and(|e|e.outcome_unknown){"UNKNOWN"}else{"RESPONSE"},"method":"DELETE","path":format!("orders/{}",order.broker_order_id),"accepted":cancel.is_ok(),"http_status":cancel.as_ref().err().and_then(|e|e.http_status)}),
                );
            }
        }
        if r.side == Side::Buy && self.unknown_once {
            self.unknown_once = false;
            if let Ok(order) = response {
                self.hidden_id = Some(order.broker_order_id.clone());
                append(
                    &self.directory.join("faults.jsonl"),
                    &json!({"at":Utc::now(),"kind":"CONTROLLED_APPLICATION_ACK_LOSS_REAL_BROKER_ORDER","exact_acknowledgement":order}),
                );
                let mut failure = BrokerFailure::new(BrokerFailureKind::Transport);
                failure.outcome_unknown = true;
                return Err(failure);
            }
        }
        response
    }
    fn cancel(&mut self, a: &AccountId, id: &str) -> BrokerResult<CancellationReceipt> {
        let result = self.inner.cancel(a, id);
        append(
            &self.directory.join("transport.jsonl"),
            &json!({"at":Utc::now(),"cancel_exact_id":id,"accepted":result.is_ok(),"failure_http_status":result.as_ref().err().and_then(|e|e.http_status)}),
        );
        result
    }
}

#[test]
#[ignore = "requires explicit bounded Trading 212 DEMO capability-probe authorization, live IBKR and exact-ID supervision"]
fn supervised_eur_demo_capability() {
    assert_eq!(
        env::var("TRADY_DEMO_EVIDENCE_AUTHORIZED").as_deref(),
        Ok("EUR_CAPABILITY_20261105")
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory =
        PathBuf::from(env::var("TRADY_PROBE_DIRECTORY").expect("private evidence directory"));
    assert!(directory.is_absolute() && directory.is_dir());
    let instrument = env::var("TRADY_PROBE_INSTRUMENT").expect("exact instrument");
    assert!(["AIR_PA_EQ", "ASML_AS_EQ"].contains(&instrument.as_str()));
    let case = env::var("TRADY_PROBE_CASE").unwrap_or_else(|_| "normal".into());
    assert!(
        [
            "normal",
            "retest",
            "unknown",
            "precision",
            "cancel_race",
            "timing",
            "paced",
            "reservation_race",
            "reservation_fast",
            "small_fast",
            "unknown_small",
            "depth_partial",
            "depth_partial_followup",
            "passive_partial",
            "stop_cancel_race",
            "stop_fill_race",
            "stop_fill_trigger"
        ]
        .contains(&case.as_str())
    );
    let phase = env::var("TRADY_PROBE_PHASE").unwrap_or_else(|_| "protect".into());
    assert!(["protect", "exit"].contains(&phase.as_str()));
    let config: EurRuntimeConfig = read(Path::new("/etc/trady/runtime/eur-shadow.json"));
    config.validate(Utc::now()).unwrap();
    assert!(!config.live_enabled && config.execution_stage == ExecutionStage::Shadow);
    let settings = config.shadow.as_ref().unwrap();
    let crosswalk: ShadowCrosswalk = read(Path::new(&settings.crosswalk_config));
    crosswalk.validate(Utc::now()).unwrap();
    let identity = crosswalk
        .members
        .iter()
        .find(|m| m.internal_instrument_id.as_str() == instrument)
        .unwrap();
    assert!(
        (identity.broker_ticker == "AIRp_EQ" && identity.isin == "NL0000235190")
            || (identity.broker_ticker == "ASMLa_EQ" && identity.isin == "NL0010273215")
    );
    let credentials = CredentialStore::load().unwrap();
    let dsn = credentials
        .required("TRADY_EXECUTOR_DSN")
        .unwrap()
        .expose()
        .to_owned();
    let account = AccountId::new(format!(
        "paper_cap_20261105_{}_{}",
        if instrument == "AIR_PA_EQ" {
            "air"
        } else {
            "asml"
        },
        case
    ))
    .unwrap();
    let intent = format!(
        "cap_20261105_{}_{}",
        if instrument == "AIR_PA_EQ" {
            "air"
        } else {
            "asml"
        },
        case
    );
    let open = || {
        PgTargetStore::connect(
            &dsn,
            "trady_executor",
            config.owner_id.clone(),
            account.to_string(),
        )
        .unwrap()
    };
    let mut store = open();
    store
        .acquire_executor_lease(&new_id("capability_probe"), 180)
        .unwrap();
    let existing = store.read_document("capability_probe", &intent).unwrap();
    let mut broker_config: Trading212Config =
        read(Path::new(config.trading212_config.as_ref().unwrap()));
    assert_eq!(broker_config.environment, Environment::Demo);
    assert_eq!(broker_config.broker_account_id, 52211463);
    assert_eq!(broker_config.base_currency.as_str(), "EUR");
    broker_config.account_id = account.clone();
    let mut executor = Trading212Executor::new(
        broker_config,
        Credentials::from_store(Environment::Demo, &credentials).unwrap(),
    )
    .unwrap();
    // Prewarm the catalog before touching a protected holding after restart.
    // Only metadata reads are retried; a dispatched mutation is never retried.
    let start = Instant::now();
    let instruments = loop {
        match executor.information().instruments() {
            Ok(v) => break v,
            Err(e)
                if e.kind == BrokerFailureKind::RateLimited
                    && start.elapsed() < Duration::from_secs(65) =>
            {
                thread::sleep(Duration::from_secs(
                    e.retry_after_seconds.unwrap_or(5).min(5),
                ))
            }
            Err(e) => panic!("capability catalog unavailable: {e}"),
        }
    };
    assert!(
        instruments
            .iter()
            .any(|r| r.instrument_id.as_str() == identity.broker_ticker
                && r.isin == identity.isin
                && r.currency.as_str() == "EUR")
    );
    let (context, binding): (ProtocolContext, EurInstrumentBinding) = if let Some(document) =
        existing
    {
        (
            serde_json::from_value(document["context"].clone()).unwrap(),
            serde_json::from_value(document["binding"].clone()).unwrap(),
        )
    } else {
        assert_eq!(
            phase, "protect",
            "an exit invocation cannot create an entry"
        );
        thread::sleep(Duration::from_secs(5));
        let snapshot = retry_read(|| executor.information().read_snapshot(&account));
        assert!(
            snapshot.positions.is_empty() && snapshot.orders.is_empty(),
            "one actual broker holding/order set at a time"
        );
        let calendar = retry_read(|| executor.information().exchanges_snapshot());
        let now = Utc::now();
        let session =
            session_from_catalog(identity, &config.session_deadlines, &calendar, now, now).unwrap();
        assert!(now >= session.opens_at && now < session.flatten_at - ChronoDuration::minutes(5));
        let stream_path =
            PathBuf::from(env::var("TRADY_IBKR_STREAM_DIRECTORY").unwrap()).join("latest.json");
        let armed = Instant::now();
        let mut last_lease_renewal = Instant::now();
        let stream: Value = loop {
            if last_lease_renewal.elapsed() >= Duration::from_secs(30) {
                store.renew_executor_lease(180).unwrap();
                last_lease_renewal = Instant::now();
            }
            let stream: Value = read(&stream_path);
            let book = &stream["instruments"][&instrument];
            let fresh = book["observed_at_seconds"].as_i64().is_some_and(|at| {
                Utc::now().timestamp() - at >= 0 && Utc::now().timestamp() - at <= 1
            });
            let ask: Decimal = book["ask"].as_str().unwrap_or("0").parse().unwrap();
            let size: Decimal = book["ask_size"].as_str().unwrap_or("0").parse().unwrap();
            if !case.starts_with("depth_partial")
                || (fresh
                    && ask > Decimal::ZERO
                    && size > Decimal::ZERO
                    && size * ask < Decimal::from(2500))
            {
                break stream;
            }
            assert!(
                armed.elapsed() < Duration::from_secs(300),
                "no bounded displayed-depth opportunity; no entry dispatched"
            );
            thread::sleep(Duration::from_millis(50));
        };
        let now = Utc::now();
        let book = &stream["instruments"][&instrument];
        assert_eq!(book["market_data_type"], 1);
        assert_eq!(book["broker_ticker"], identity.broker_ticker);
        assert_eq!(book["isin"], identity.isin);
        let observed =
            DateTime::from_timestamp(book["observed_at_seconds"].as_i64().unwrap(), 0).unwrap();
        assert!(now >= observed && now - observed < ChronoDuration::seconds(5));
        let bid: Decimal = book["bid"].as_str().unwrap().parse().unwrap();
        let ask: Decimal = book["ask"].as_str().unwrap().parse().unwrap();
        assert!(
            bid > Decimal::ZERO
                && ask >= bid
                && (ask - bid) / bid * Decimal::from(10000) <= Decimal::from(20)
        );
        let quantum = Decimal::new(1, 2); // conservative experiment rounding, not a claimed minimum broker tick
        let entry = trady_policy::ceil_to(
            (if case == "passive_partial" { bid } else { ask })
                * if case.starts_with("depth_partial") || case == "passive_partial" {
                    Decimal::ONE
                } else if case == "stop_fill_trigger" {
                    Decimal::new(1002, 3)
                } else {
                    Decimal::new(10005, 4)
                },
            quantum,
        )
        .unwrap();
        assert!(entry <= Decimal::from(2500) && entry < snapshot.spendable_cash.amount);
        let stop = if case == "stop_fill_trigger" {
            entry - quantum
        } else if case == "stop_fill_race" {
            // Capability-only immediate-trigger experiment. The native broker
            // LTP decides whether to fill; no synthetic fill is injected.
            trady_policy::ceil_to(ask.min(entry - quantum), quantum).unwrap()
        } else if case == "stop_cancel_race" {
            trady_policy::ceil_to((bid + quantum).min(entry - quantum), quantum).unwrap()
        } else {
            trady_policy::ceil_to(entry * Decimal::new(99, 2), quantum).unwrap()
        } + if case == "precision" {
            Decimal::new(1, 9)
        } else {
            Decimal::ZERO
        };
        let target = trady_policy::ceil_to(entry * Decimal::new(102, 2), quantum).unwrap();
        let until = (now + ChronoDuration::minutes(5)).min(session.flatten_at);
        let binding = EurInstrumentBinding {
            instrument_id: InstrumentId::new(instrument.clone()).unwrap(),
            broker_instrument_id: identity.broker_ticker.clone(),
            isin: identity.isin.clone(),
            legal_name: identity.legal_name.clone(),
            ticker: identity.yahoo_symbol.split('.').next().unwrap().into(),
            exchange_mic: identity.exchange_mic.clone(),
            country: "NL".into(),
            currency: Currency::new("EUR").unwrap(),
            session_timezone: identity.session_timezone.clone(),
            yahoo_symbol: identity.yahoo_symbol.clone(),
            minimum_quantity: if case.starts_with("reservation_")
                || matches!(
                    case.as_str(),
                    "small_fast"
                        | "unknown_small"
                        | "depth_partial"
                        | "depth_partial_followup"
                        | "passive_partial"
                        | "stop_cancel_race"
                        | "stop_fill_race"
                        | "stop_fill_trigger"
                ) {
                if instrument == "AIR_PA_EQ" {
                    Decimal::new(1, 4)
                } else {
                    Decimal::new(1, 5)
                }
            } else {
                Decimal::ONE
            },
            price_tick: if case == "precision" {
                Decimal::new(1, 9)
            } else {
                quantum
            },
            quantity_step: if instrument == "AIR_PA_EQ" {
                Decimal::new(1, 4)
            } else {
                Decimal::new(1, 5)
            },
            fractional_orders: true,
            value_rules: if case.starts_with("reservation_")
                || matches!(
                    case.as_str(),
                    "small_fast"
                        | "unknown_small"
                        | "depth_partial"
                        | "depth_partial_followup"
                        | "passive_partial"
                        | "stop_cancel_race"
                        | "stop_fill_race"
                        | "stop_fill_trigger"
                ) {
                Some(trady_domain::eur::EurOrderValueRules {
                    minimum_trade_value: Decimal::ONE,
                    minimum_remaining_position_value: Decimal::ONE,
                    price_floor: entry / Decimal::from(2),
                    price_ceiling: entry * Decimal::from(2),
                })
            } else {
                None
            },
            effective_from: now - ChronoDuration::seconds(1),
            expires_at: Some(until + ChronoDuration::minutes(5)),
            liquidity_threshold: trady_domain::eur::LiquidityThreshold {
                minimum_average_daily_notional: Decimal::ONE,
                maximum_order_participation_bps: Decimal::from(10),
                maximum_spread_bps: Decimal::from(20),
            },
            volatility_threshold: trady_domain::eur::VolatilityThreshold {
                minimum_fraction: Decimal::ZERO,
                maximum_fraction: Decimal::new(12, 2),
            },
            provenance_ids: vec!["owner_authorized_capability_probe_20261105".into()],
            review_owner: "capability_experiment_only_not_release_approval".into(),
        };
        binding.validate(now).unwrap();
        let context = ProtocolContext {
            runtime_mode: ExecutionStage::Demo,
            policy_revision: config.policy.revision.clone(),
            intent_id: intent.clone(),
            account_id: account.to_string(),
            broker_account_id: 52211463,
            instrument_id: instrument.clone(),
            broker_ticker: identity.broker_ticker.clone(),
            isin: identity.isin.clone(),
            policy_hash: content_hash(&serde_json::to_string(&config.policy).unwrap()),
            configuration_hash: content_hash(&format!(
                "BOUNDED_CAPABILITY_EXPERIMENT:{case}:{instrument}"
            )),
            evidence_ids: vec!["owner_authorized_capability_probe_20261105".into()],
            quantity: if case.starts_with("depth_partial") || case == "passive_partial" {
                let step = if instrument == "AIR_PA_EQ" {
                    Decimal::new(1, 4)
                } else {
                    Decimal::new(1, 5)
                };
                let q = trady_policy::floor_to(Decimal::from(2500) / entry, step).unwrap();
                assert!(
                    case == "passive_partial"
                        || q > book["ask_size"]
                            .as_str()
                            .unwrap()
                            .parse::<Decimal>()
                            .unwrap()
                );
                assert!(
                    q * entry <= Decimal::from(2500) && q * entry <= snapshot.spendable_cash.amount
                );
                q
            } else if case.starts_with("reservation_") {
                Decimal::new(2, 2)
            } else if matches!(
                case.as_str(),
                "small_fast"
                    | "unknown_small"
                    | "stop_cancel_race"
                    | "stop_fill_race"
                    | "stop_fill_trigger"
            ) {
                if instrument == "AIR_PA_EQ" {
                    Decimal::new(1, 2)
                } else {
                    Decimal::new(1, 3)
                }
            } else {
                Decimal::ONE
            },
            entry_limit: entry,
            initial_stop: stop,
            target,
            created_at: now,
            flatten_at: until,
            max_unprotected_seconds: 10,
            entry_valid_until: None,
        };
        context.validate().unwrap();
        let doc = json!({"classification":"BOUNDED_DEMO_CAPABILITY_EXPERIMENT_NOT_RELEASE","context":context,"binding":binding,"initial_broker_snapshot":snapshot,"initial_ibkr_book":book,"max_holding_count":1,"max_quantity":context.quantity,"max_entry_notional_eur":2500,"broker_account_id":52211463,"depth_opportunity_wait_millis":armed.elapsed().as_millis()});
        store
            .append_document("capability_probe", &intent, &doc)
            .unwrap();
        write(&directory.join("authority.json"), &doc);
        (context, binding)
    };
    let mut driver = ProtocolDriver {
        journal: store,
        broker: ProbeBroker {
            inner: Trading212ProtocolBroker::new(executor, binding),
            unknown_once: matches!(case.as_str(), "unknown" | "unknown_small")
                && phase == "protect",
            reservation_once: case.starts_with("reservation_") && phase == "protect",
            reservation_id: None,
            stop_cancel_once: matches!(
                case.as_str(),
                "stop_cancel_race" | "stop_fill_race" | "stop_fill_trigger"
            ) && phase == "protect",
            stop_cancel_delay_millis: if case == "stop_fill_trigger" {
                2500
            } else if case == "stop_fill_race" {
                1500
            } else {
                300
            },
            hidden_id: None,
            directory: directory.clone(),
        },
    };
    let run_started = Instant::now();
    let mut cancellation_probed = false;
    let final_state = loop {
        driver.journal.renew_executor_lease(180).unwrap();
        let reason = if phase == "exit" {
            Some("CAPABILITY_PROBE_COMPLETE")
        } else if run_started.elapsed() > Duration::from_secs(45) {
            Some("CAPABILITY_ENTRY_TIMEOUT")
        } else {
            None
        };
        let state = driver.drive_once(&context, Utc::now(), reason).unwrap();
        if state.protection.is_some()
            && let Some(id) = driver.broker.reservation_id.take()
        {
            // The independent fault actor removes only its own acknowledged
            // reservation. Normal protocol history must still prove it gone.
            driver
                .broker
                .cancel(&account, &id)
                .expect("exact external reservation cleanup");
        }
        append(
            &directory.join("checkpoints.jsonl"),
            &json!({"at":Utc::now(),"phase":state.phase,"revision":state.revision,"holding_quantity":state.holding_quantity,"reason":state.reason}),
        );
        if case == "cancel_race"
            && !cancellation_probed
            && state.phase == ProtocolPhase::EntryReconciling
        {
            cancellation_probed = true;
            let id = state.entry.as_ref().unwrap().broker_id.as_ref().unwrap();
            advance(
                &mut driver.journal,
                &context,
                new_id("entry_cancel_race"),
                ProtocolInput::Cancel {
                    leg: trady_application::eur_protocol::OrderLeg::Entry,
                },
                Utc::now(),
            )
            .unwrap();
            if driver.broker.cancel(&account, id).is_ok() {
                advance(
                    &mut driver.journal,
                    &context,
                    new_id("entry_cancel_ack"),
                    ProtocolInput::CancellationAcknowledged {
                        leg: trady_application::eur_protocol::OrderLeg::Entry,
                    },
                    Utc::now(),
                )
                .unwrap();
            }
            continue;
        }
        if matches!(
            state.recovery_effect(),
            ProtocolEffect::RequireOperator { .. }
        ) {
            if let Some(id) = driver.broker.hidden_id.take() {
                driver.journal.release_executor_lease().unwrap();
                driver.journal = open();
                driver
                    .journal
                    .acquire_executor_lease(&new_id("capability_recovery"), 180)
                    .unwrap();
                advance(
                    &mut driver.journal,
                    &context,
                    new_id("reviewed_exact_ack"),
                    ProtocolInput::ReviewedBrokerId {
                        leg: trady_application::eur_protocol::OrderLeg::Entry,
                        broker_id: id,
                        review_reference: "controlled_ack_loss_forensic_receipt_20261105".into(),
                    },
                    Utc::now(),
                )
                .unwrap();
                continue;
            }
            break state;
        }
        if matches!(state.phase, ProtocolPhase::Flat | ProtocolPhase::Rejected)
            || (phase == "protect" && state.phase == ProtocolPhase::Protected)
        {
            break state;
        }
        assert!(
            run_started.elapsed() < Duration::from_secs(180),
            "probe timeout: inspect exact durable IDs and clean up; no mutation retry"
        );
        thread::sleep(Duration::from_millis(100));
    };
    let records = driver.journal.load(&intent).unwrap();
    let observations = driver
        .journal
        .list_documents(
            "protocol_broker_observation",
            context.created_at - ChronoDuration::seconds(1),
            100,
        )
        .unwrap();
    let confirmations = driver
        .journal
        .list_documents(
            "protocol_protection_confirmation",
            context.created_at - ChronoDuration::seconds(1),
            100,
        )
        .unwrap();
    let result = json!({"classification":"REAL_TRADING212_DEMO_CAPABILITY_EXPERIMENT_NOT_RELEASE","phase_requested":phase,"state":final_state,"records":records,"broker_observations":observations,"protection_confirmations":confirmations,"controlled_response_loss":matches!(case.as_str(),"unknown"|"unknown_small"),"paper_day_passed":false,"release_activation_authorized":false,"live_enabled":false});
    write(&directory.join(format!("result_{phase}.json")), &result);
    driver.journal.release_executor_lease().unwrap();
    assert!(
        if phase == "protect" {
            final_state.phase == ProtocolPhase::Protected
                || (case == "precision"
                    && final_state.phase == ProtocolPhase::Flat
                    && final_state
                        .protection
                        .as_ref()
                        .is_some_and(|p| p.status == trady_domain::OrderStatus::Rejected))
                || (case.starts_with("reservation_")
                    && final_state.phase == ProtocolPhase::Flat
                    && final_state
                        .protection
                        .as_ref()
                        .is_some_and(|p| p.status == trady_domain::OrderStatus::Rejected))
                || (matches!(
                    case.as_str(),
                    "stop_cancel_race" | "stop_fill_race" | "stop_fill_trigger"
                ) && final_state.phase == ProtocolPhase::Flat)
                || (case == "cancel_race"
                    && matches!(
                        final_state.phase,
                        ProtocolPhase::Rejected | ProtocolPhase::Flat
                    )
                    && final_state
                        .entry
                        .as_ref()
                        .is_some_and(|p| p.filled == Decimal::ZERO))
        } else {
            final_state.phase == ProtocolPhase::Flat
        },
        "capability checkpoint did not reach its requested verified state"
    );
    println!(
        "{}",
        json!({"status":"DEMO_CAPABILITY_CHECKPOINT_VERIFIED","phase":final_state.phase,"artifact":directory.join(format!("result_{phase}.json")),"release_activation_authorized":false})
    );
    let _ = root;
}

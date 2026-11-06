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
    ibkr_smart::{IbkrBinding, IbkrSmartConfig},
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
use trady_domain::eur::EurUniverseSnapshot;
use trady_domain::{AccountId, BrokerOrder, EurInstrumentBinding, ExecutionStage, Side};
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
fn probe_ids(experiment: &str, instrument: &str, case: &str) -> (AccountId, String) {
    assert!(
        !experiment.is_empty()
            && experiment.len() <= 32
            && experiment
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    );
    let stem = format!(
        "{experiment}_{}_{case}",
        if instrument == "AIR_PA_EQ" {
            "air"
        } else {
            "asml"
        }
    );
    (
        AccountId::new(format!("paper_cap_{stem}")).unwrap(),
        format!("cap_{stem}"),
    )
}
fn entry_budget(cash: Decimal, fraction: Option<Decimal>) -> Decimal {
    match fraction {
        Some(value) => {
            assert_eq!(
                value,
                Decimal::new(98, 2),
                "only the authorized 98% cash cap"
            );
            trady_policy::floor_to(cash * value, Decimal::new(1, 2)).unwrap()
        }
        None => Decimal::from(2500),
    }
}

#[test]
fn balance_probe_uses_fresh_cash_with_fractional_shares_below_the_98_percent_cap() {
    let cash = Decimal::new(499342, 2);
    let budget = entry_budget(cash, Some(Decimal::new(98, 2)));
    assert_eq!(budget, Decimal::new(489355, 2));
    for (price, step) in [
        (Decimal::from(1296), Decimal::new(1, 5)),
        (Decimal::new(19610, 2), Decimal::new(1, 4)),
    ] {
        let quantity = trady_policy::floor_to(budget / price, step).unwrap();
        assert!(quantity * price <= budget);
        assert!((quantity + step) * price > budget);
        assert!(quantity.fract() > Decimal::ZERO);
    }
    assert_eq!(entry_budget(cash, None), Decimal::from(2500));
    assert_eq!(
        entry_budget(Decimal::from(1000), Some(Decimal::new(98, 2))),
        Decimal::from(980)
    );
}

fn eligible_book(
    book: &Value,
    contract: &IbkrBinding,
    binding: &EurInstrumentBinding,
    now: DateTime<Utc>,
    thin: bool,
) -> bool {
    let decimal = |key| book[key].as_str().and_then(|v| v.parse::<Decimal>().ok());
    let Some(observed) = book["observed_at_seconds"]
        .as_i64()
        .and_then(|v| DateTime::from_timestamp(v, 0))
    else {
        return false;
    };
    let Some(received) = book["received_at"]
        .as_str()
        .and_then(|v| v.parse::<DateTime<Utc>>().ok())
    else {
        return false;
    };
    let (Some(bid), Some(ask), Some(bid_size), Some(ask_size)) = (
        decimal("bid"),
        decimal("ask"),
        decimal("bid_size"),
        decimal("ask_size"),
    ) else {
        return false;
    };
    let Some(rules) = binding.value_rules.as_ref() else {
        return false;
    };
    let Ok(entry) = trady_policy::ceil_to(ask, binding.price_tick) else {
        return false;
    };
    let Ok(stop) = trady_policy::ceil_to(entry * Decimal::new(99, 2), binding.price_tick) else {
        return false;
    };
    book["instrument_id"] == binding.instrument_id.as_str()
        && book["market_data_type"] == 1
        && book["source_venue"] == "SMART"
        && book["currency"] == "EUR"
        && book["broker_ticker"] == contract.broker_ticker
        && book["isin"] == contract.isin
        && book["con_id"] == contract.con_id
        && book["primary_exchange"] == contract.primary_exchange
        && book["kind"] == "bid_ask"
        && observed <= received
        && received <= now
        && now - observed <= ChronoDuration::seconds(1)
        && bid > Decimal::ZERO
        && ask >= bid
        && bid_size > Decimal::ZERO
        && ask_size > Decimal::ZERO
        && (ask - bid) / bid * Decimal::from(10000)
            <= binding.liquidity_threshold.maximum_spread_bps
        && [entry, stop]
            .iter()
            .all(|p| *p >= rules.price_floor && *p <= rules.price_ceiling)
        && (!thin || ask_size * entry < Decimal::from(2500))
}

#[test]
fn probe_requires_exact_fresh_smart_book_and_reviewed_entry_and_stop_band() {
    let universe: EurUniverseSnapshot = serde_json::from_str(include_str!(
        "../../../config/eur_equity_universe.example.json"
    ))
    .unwrap();
    let mut binding = universe.members[0].clone();
    binding.broker_instrument_id = "AIRp_EQ".into();
    binding.value_rules = Some(trady_domain::eur::EurOrderValueRules {
        minimum_trade_value: Decimal::ONE,
        minimum_remaining_position_value: Decimal::ONE,
        price_floor: Decimal::from(190),
        price_ceiling: Decimal::from(210),
    });
    let contract = IbkrBinding {
        con_id: 29612256,
        isin: binding.isin.clone(),
        broker_ticker: "AIRp_EQ".into(),
        primary_exchange: "SBF".into(),
        exchange_mic: "XPAR".into(),
    };
    let now: DateTime<Utc> = "2026-11-06T08:40:00Z".parse().unwrap();
    let book = json!({"instrument_id":"AIR_PA_EQ","broker_ticker":"AIRp_EQ","isin":"NL0000235190","con_id":29612256,"primary_exchange":"SBF","currency":"EUR","source_venue":"SMART","kind":"bid_ask","market_data_type":1,"observed_at_seconds":now.timestamp(),"received_at":now,"bid":"197.60","ask":"197.64","bid_size":"100","ask_size":"1"});
    assert!(eligible_book(&book, &contract, &binding, now, true));
    for (key, bad) in [
        ("con_id", json!(1)),
        ("primary_exchange", json!("AEB")),
        ("source_venue", json!("LSE")),
        ("currency", json!("USD")),
        ("market_data_type", json!(3)),
        ("bid_size", json!("0")),
        ("ask_size", json!("0")),
        ("ask_size", json!("100")),
        ("observed_at_seconds", json!(now.timestamp() - 11)),
        ("observed_at_seconds", json!(now.timestamp() + 1)),
        ("isin", json!("WRONG")),
        ("broker_ticker", json!("OTHER")),
        ("bid", json!("0")),
        ("ask", json!("211")),
        ("received_at", json!(now - ChronoDuration::seconds(1))),
        ("received_at", json!(now + ChronoDuration::seconds(1))),
    ] {
        let mut invalid = book.clone();
        invalid[key] = bad;
        assert!(
            !eligible_book(&invalid, &contract, &binding, now, true),
            "{key}"
        );
    }
    let mut stop_outside = book;
    stop_outside["bid"] = json!("190");
    stop_outside["ask"] = json!("190.01");
    assert!(!eligible_book(
        &stop_outside,
        &contract,
        &binding,
        now,
        true
    ));
    assert_ne!(
        probe_ids("nov6_a", "AIR_PA_EQ", "depth_partial"),
        probe_ids("nov6_b", "AIR_PA_EQ", "depth_partial")
    );
}
fn retry_read<T>(mut f: impl FnMut() -> BrokerResult<T>) -> T {
    let started = Instant::now();
    loop {
        match f() {
            Ok(value) => return value,
            Err(error) => {
                retain_read_failure("bounded_preflight_read", &error);
                if error.kind == BrokerFailureKind::RateLimited
                    && started.elapsed() < Duration::from_secs(65)
                {
                    thread::sleep(Duration::from_secs(
                        error.retry_after_seconds.unwrap_or(1).clamp(1, 5),
                    ));
                } else {
                    panic!("bounded broker read unavailable: {error}");
                }
            }
        }
    }
}
fn retain_read_failure(operation: &str, error: &BrokerFailure) {
    let directory =
        PathBuf::from(env::var("TRADY_PROBE_DIRECTORY").expect("probe evidence directory"));
    append(
        &directory.join("read-failures.jsonl"),
        &json!({"at":Utc::now(),"operation":operation,"kind":format!("{:?}",error.kind),"http_status":error.http_status,"retry_after_seconds":error.retry_after_seconds,"outcome_unknown":error.outcome_unknown}),
    );
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
        let result = self.inner.observe(state);
        if let Err(error) = &result {
            retain_read_failure("observe", error);
        }
        result
    }
    fn confirm_entry(&mut self, state: &ProtocolState) -> BrokerResult<Option<BrokerObservation>> {
        let result = self.inner.confirm_entry(state);
        if let Err(error) = &result {
            retain_read_failure("confirm_entry", error);
        }
        result
    }
    fn prepare_cancel(
        &mut self,
        state: &ProtocolState,
        leg: trady_application::eur_protocol::OrderLeg,
    ) -> BrokerResult<()> {
        let result = self.inner.prepare_cancel(state, leg);
        if let Err(error) = &result {
            retain_read_failure("prepare_cancel", error);
        }
        result
    }
    fn confirm_protection(
        &mut self,
        state: &ProtocolState,
    ) -> BrokerResult<Option<ProtectionConfirmation>> {
        let result = self.inner.confirm_protection(state);
        if let Err(error) = &result {
            retain_read_failure("confirm_protection", error);
        }
        result
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
        let path = format!(
            "orders/{}",
            match r.kind {
                trady_application::broker::OrderKind::Market { .. } => "market",
                trady_application::broker::OrderKind::Limit { .. } => "limit",
                trady_application::broker::OrderKind::Stop { .. } => "stop",
                trady_application::broker::OrderKind::StopLimit { .. } => "stop_limit",
            }
        );
        append(
            &self.directory.join("transport.jsonl"),
            &json!({"at":Utc::now(),"phase":"BEFORE_SEND","method":"POST","path":path,"request":r,"deadline":deadline}),
        );
        let response = self.inner.submit(r, deadline);
        append(
            &self.directory.join("transport.jsonl"),
            &json!({"at":Utc::now(),"phase":if response.as_ref().err().is_some_and(|e|e.outcome_unknown){"UNKNOWN"}else{"RESPONSE"},"method":"POST","path":path,"request":r,"response":response.as_ref().ok(),"http_status":response.as_ref().err().and_then(|e|e.http_status),"definitive_no_effect":response.as_ref().err().is_some_and(|e| !e.outcome_unknown && matches!(e.kind, BrokerFailureKind::InvalidRequest | BrokerFailureKind::Unsupported | BrokerFailureKind::AccountMismatch | BrokerFailureKind::Rejected)),"failure":response.as_ref().err().map(|e|json!({"kind":format!("{:?}",e.kind),"http_status":e.http_status,"outcome_unknown":e.outcome_unknown,"unverified_broker_order_id":e.broker_order_id}))}),
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
        let path = format!("orders/{id}");
        append(
            &self.directory.join("transport.jsonl"),
            &json!({"at":Utc::now(),"phase":"BEFORE_SEND","method":"DELETE","path":path,"cancel_exact_id":id}),
        );
        let result = self.inner.cancel(a, id);
        append(
            &self.directory.join("transport.jsonl"),
            &json!({"at":Utc::now(),"phase":if result.as_ref().err().is_some_and(|e|e.outcome_unknown){"UNKNOWN"}else{"RESPONSE"},"method":"DELETE","path":path,"cancel_exact_id":id,"accepted":result.is_ok(),"http_status":result.as_ref().err().and_then(|e|e.http_status),"failure_http_status":result.as_ref().err().and_then(|e|e.http_status),"outcome_unknown":result.as_ref().err().is_some_and(|e|e.outcome_unknown)}),
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
    assert!(["protect", "exit", "cleanup"].contains(&phase.as_str()));
    assert!(
        !directory.join(format!("result_{phase}.json")).exists(),
        "completed phases cannot be replayed"
    );
    let experiment =
        env::var("TRADY_PROBE_EXPERIMENT_ID").expect("fresh experiment namespace required");
    let entry_cutoff: DateTime<Utc> = env::var("TRADY_PROBE_ENTRY_CUTOFF")
        .expect("explicit entry cutoff")
        .parse()
        .unwrap();
    if phase == "protect" {
        assert!(Utc::now() < entry_cutoff, "entry observation cutoff passed");
        write(
            &directory.join("protect-start.json"),
            &json!({"experiment_id":experiment,"at":Utc::now(),"entry_cutoff":entry_cutoff}),
        );
    }
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
    let universe: EurUniverseSnapshot = read(Path::new(config.universe_config.as_ref().unwrap()));
    config.validate_universe(&universe, Utc::now()).unwrap();
    let accepted_binding = universe
        .member(&identity.internal_instrument_id)
        .unwrap()
        .clone();
    assert_eq!(
        accepted_binding.broker_instrument_id,
        identity.broker_ticker
    );
    assert_eq!(accepted_binding.isin, identity.isin);
    let ibkr: IbkrSmartConfig = read(Path::new(settings.ibkr_config.as_ref().unwrap()));
    let contract = ibkr.validate_identity(identity).unwrap();
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
    let (account, intent) = probe_ids(&experiment, &instrument, &case);
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
        .acquire_demo_broker_lease(&new_id("capability_probe"), 180)
        .unwrap();
    let existing = store.read_document("capability_probe", &intent).unwrap();
    assert_eq!(
        existing.is_some(),
        phase != "protect",
        "new protect must use an unused durable scope; exit requires its original scope"
    );
    if phase != "protect" {
        assert!(
            !store.protocol_is_externally_cleaned(&intent).unwrap(),
            "externally cleaned intents are permanently read-only"
        );
        let records = store.load(&intent).unwrap();
        assert!(
            !records.is_empty(),
            "cleanup cannot create a protocol or entry"
        );
        let state = trady_application::eur_protocol::recover(&records).unwrap();
        assert!(
            !matches!(state.phase, ProtocolPhase::Flat | ProtocolPhase::Rejected),
            "terminal original scope cannot own a later holding"
        );
        assert!(
            state.entry.is_some()
                && !matches!(
                    state.recovery_effect(),
                    ProtocolEffect::RequireOperator { .. }
                ),
            "unknown submissions require original exact-ID review"
        );
        let original: Value = read(&directory.join("authority.json"));
        assert_eq!(
            serde_json::to_value(&state.context).unwrap(),
            original["context"]
        );
        assert_eq!(existing.as_ref().unwrap()["context"], original["context"]);
        let unresolved = store.physical_unresolved_protocols().unwrap();
        let heads = unresolved.as_array().unwrap();
        assert!(
            heads.len() == 1
                && heads[0]["owner_id"] == config.owner_id
                && heads[0]["account_id"] == account.as_str()
                && heads[0]["intent_id"] == intent,
            "another unresolved physical scope requires review"
        );
    }
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
                retain_read_failure("catalog_warming", &e);
                thread::sleep(Duration::from_secs(
                    e.retry_after_seconds.unwrap_or(5).min(5),
                ))
            }
            Err(e) => {
                retain_read_failure("catalog_warming", &e);
                panic!("capability catalog unavailable: {e}");
            }
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
        let cash_fraction = env::var("TRADY_PROBE_ENTRY_CASH_FRACTION")
            .ok()
            .map(|value| value.parse::<Decimal>().expect("authorized cash fraction"));
        let budget = entry_budget(snapshot.spendable_cash.amount, cash_fraction);
        let calendar = retry_read(|| executor.information().exchanges_snapshot());
        let now = Utc::now();
        let session =
            session_from_catalog(identity, &config.session_deadlines, &calendar, now, now).unwrap();
        assert!(now >= session.opens_at && now < session.flatten_at - ChronoDuration::minutes(5));
        let stream_path =
            PathBuf::from(env::var("TRADY_IBKR_STREAM_DIRECTORY").unwrap()).join("latest.json");
        let armed = Instant::now();
        let armed_at = Utc::now();
        let mut last_lease_renewal = Instant::now();
        let mut last_book = Value::Null;
        let stream: Value = loop {
            assert!(
                Utc::now() < entry_cutoff,
                "entry observation cutoff passed; no dispatch"
            );
            if last_lease_renewal.elapsed() >= Duration::from_secs(30) {
                store.renew_executor_lease(180).unwrap();
                last_lease_renewal = Instant::now();
            }
            let stream: Value = read(&stream_path);
            let book = &stream["instruments"][&instrument];
            let eligible = eligible_book(
                book,
                contract,
                &accepted_binding,
                Utc::now(),
                case.starts_with("depth_partial"),
            ) && (case != "depth_partial_followup"
                || book["ask_size"]
                    .as_str()
                    .and_then(|v| v.parse::<Decimal>().ok())
                    .is_some_and(|size| size <= Decimal::ONE));
            if book != &last_book {
                append(
                    &directory.join("opportunity-watch.jsonl"),
                    &json!({"at":Utc::now(),"book":book,"eligible":eligible}),
                );
                last_book = book.clone();
            }
            if eligible
                && book["observed_at_seconds"]
                    .as_i64()
                    .is_some_and(|at| at > armed_at.timestamp())
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
        let quantum = accepted_binding.price_tick; // reviewed application grid, not a claimed native minimum tick
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
        let binding = accepted_binding.clone();
        let rules = binding.value_rules.as_ref().expect("reviewed price band");
        assert!(
            [entry, stop]
                .iter()
                .all(|p| *p >= rules.price_floor && *p <= rules.price_ceiling),
            "entry and protection must stay inside accepted bands"
        );
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
                "BOUNDED_CAPABILITY_EXPERIMENT:{experiment}:{case}:{instrument}"
            )),
            evidence_ids: vec!["owner_authorized_capability_probe_20261105".into()],
            quantity: if case.starts_with("depth_partial") || case == "passive_partial" {
                let q = trady_policy::floor_to(budget / entry, binding.quantity_step).unwrap();
                assert!(
                    case == "passive_partial"
                        || q > book["ask_size"]
                            .as_str()
                            .unwrap()
                            .parse::<Decimal>()
                            .unwrap()
                );
                assert!(q * entry <= budget && q * entry <= snapshot.spendable_cash.amount);
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
            research_cycle_id: None,
            entry_valid_until: Some(
                (observed + ChronoDuration::seconds(10))
                    .min(entry_cutoff)
                    .min(session.flatten_at)
                    .min(binding.expires_at.unwrap()),
            ),
        };
        context.validate().unwrap();
        let doc = json!({"classification":"BOUNDED_DEMO_CAPABILITY_EXPERIMENT_NOT_RELEASE","experiment_id":experiment,"context":context,"binding":binding,"initial_broker_snapshot":snapshot,"initial_ibkr_book":book,"max_holding_count":1,"max_quantity":context.quantity,"max_entry_notional_eur":budget,"maximum_entry_cash_fraction":cash_fraction,"broker_account_id":52211463,"depth_opportunity_wait_millis":armed.elapsed().as_millis()});
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
        let reason = if phase != "protect" {
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
            driver.recovery_action(&state).unwrap(),
            ProtocolEffect::RequireOperator { .. }
        ) {
            if let Some(id) = driver.broker.hidden_id.take() {
                driver.journal.release_executor_lease().unwrap();
                driver.journal = open();
                driver
                    .journal
                    .acquire_demo_broker_lease(&new_id("capability_recovery"), 180)
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

use super::*;
use crate::FakeClock;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex, atomic::AtomicBool},
    thread,
};
use trady_application::broker::{BrokerFailureKind, TimeInForce};
use trady_domain::{EurInstrumentBinding, OrderStatus, Side};

#[derive(Clone)]
enum Reply {
    Json(u16, String, Vec<(String, String)>),
    Disconnect,
}
fn ok(value: Value) -> Reply {
    Reply::Json(200, value.to_string(), vec![])
}

#[derive(Clone)]
struct Request {
    line: String,
    headers: String,
    body: String,
}
struct Server {
    base: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = stop.clone();
        let thread_requests = requests.clone();
        let join = thread::spawn(move || {
            let mut replies = VecDeque::from(replies);
            while !thread_stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let end = loop {
                    let mut chunk = [0u8; 2048];
                    let n = stream.read(&mut chunk).unwrap();
                    if n == 0 {
                        break None;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(end + 4);
                    }
                    assert!(bytes.len() < 32768);
                };
                let Some(end) = end else {
                    continue;
                };
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let size = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while bytes.len() < end + size {
                    let mut chunk = [0; 2048];
                    let n = stream.read(&mut chunk).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                }
                thread_requests.lock().unwrap().push(Request {
                    line: headers.lines().next().unwrap().into(),
                    headers,
                    body: String::from_utf8(bytes[end..].to_vec()).unwrap(),
                });
                match replies.pop_front().unwrap_or(Reply::Json(
                    500,
                    "unexpected fixture request".into(),
                    vec![],
                )) {
                    Reply::Disconnect => {}
                    Reply::Json(status, body, headers) => {
                        let mut response = format!(
                            "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
                            body.len()
                        );
                        for (name, value) in headers {
                            response.push_str(&format!("{name}: {value}\r\n"));
                        }
                        let _ = stream.write_all(format!("{response}\r\n{body}").as_bytes());
                    }
                }
            }
        });
        Self {
            base,
            requests,
            stop,
            join: Some(join),
        }
    }
    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
    fn reader(&self) -> Trading212Reader<FakeClock> {
        Trading212Reader {
            config: config(),
            http: Http::for_test(self.base.clone(), 1),
            clock: FakeClock::new(now()),
            bound_at: None,
            metadata: None,
        }
    }
    fn executor(&self) -> Trading212Executor<FakeClock> {
        Trading212Executor {
            reader: self.reader(),
        }
    }
    fn euro_executor(&self) -> Trading212Executor<FakeClock> {
        let mut executor = self.executor();
        executor.reader.config.environment = Environment::Demo;
        executor.reader.config.account_id = AccountId::new("paper_t212_test").unwrap();
        executor
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.join.take().unwrap().join().unwrap();
    }
}
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
        .unwrap()
        .to_utc()
}
fn config() -> Trading212Config {
    Trading212Config {
        config_version: 1,
        environment: Environment::Live,
        account_id: AccountId::new("live_t212_test").unwrap(),
        broker_account_id: 17,
        base_currency: Currency::new("EUR").unwrap(),
        timeout_seconds: 10,
    }
}
fn summary() -> Reply {
    ok(
        json!({"id":17,"currency":"EUR","cash":{"availableToTrade":50,"reservedForOrders":2,"inPies":3}}),
    )
}
fn metadata() -> Reply {
    ok(json!([{"ticker":"AAPL_US_EQ","currencyCode":"USD","isin":"US0378331005","type":"STOCK"}]))
}
fn euro_metadata() -> Reply {
    ok(json!([{"ticker":"demo_air_pa","currencyCode":"EUR","isin":"FR0000120404","type":"STOCK"}]))
}
fn request(kind: OrderKind, side: Side) -> BrokerOrderRequest {
    BrokerOrderRequest {
        intent_id: IntentId::new("durable_local_intent").unwrap(),
        account_id: config().account_id,
        instrument_id: InstrumentId::new("AAPL_US_EQ").unwrap(),
        instrument_currency: Currency::new("USD").unwrap(),
        side,
        quantity: Decimal::from_str_exact("0.1234567890123456789012345678").unwrap(),
        kind,
    }
}
fn euro_request() -> BrokerOrderRequest {
    BrokerOrderRequest {
        intent_id: IntentId::new("eur_local_intent").unwrap(),
        account_id: AccountId::new("paper_t212_test").unwrap(),
        instrument_id: InstrumentId::new("demo_air_pa").unwrap(),
        instrument_currency: Currency::new("EUR").unwrap(),
        side: Side::Buy,
        quantity: Decimal::ONE,
        kind: limit(),
    }
}
fn euro_binding() -> EurInstrumentBinding {
    EurInstrumentBinding {
        instrument_id: InstrumentId::new("AIR_PA_EQ").unwrap(),
        broker_instrument_id: "demo_air_pa".into(),
        isin: "FR0000120404".into(),
        legal_name: "Airbus SE".into(),
        ticker: "AIR".into(),
        exchange_mic: "XPAR".into(),
        country: "FR".into(),
        currency: Currency::new("EUR").unwrap(),
        session_timezone: "Europe/Paris".into(),
        yahoo_symbol: "AIR.PA".into(),
        minimum_quantity: Decimal::ONE,
        price_tick: Decimal::new(1, 2),
        quantity_step: Decimal::ONE,
        fractional_orders: false,
        value_rules: None,
        effective_from: now() - chrono::Duration::days(1),
        expires_at: Some(now() + chrono::Duration::days(1)),
        liquidity_threshold: trady_domain::eur::LiquidityThreshold {
            minimum_average_daily_notional: Decimal::from(1_000_000u32),
            maximum_order_participation_bps: Decimal::from(10u32),
            maximum_spread_bps: Decimal::from(20u32),
        },
        volatility_threshold: trady_domain::eur::VolatilityThreshold {
            minimum_fraction: Decimal::new(1, 3),
            maximum_fraction: Decimal::new(12, 2),
        },
        provenance_ids: vec!["review_1".into()],
        review_owner: "owner".into(),
    }
}
fn order() -> Value {
    json!({"id":42,"ticker":"AAPL_US_EQ","instrument":{"ticker":"AAPL_US_EQ","currency":"USD"},"side":"BUY","quantity":1,"filledQuantity":0,"status":"NEW","strategy":"QUANTITY","type":"LIMIT","limitPrice":123.45,"timeInForce":"DAY","extendedHours":false})
}
fn response_for(request: &BrokerOrderRequest) -> String {
    let mut response = order();
    response["side"] = json!(request.side);
    let body: Value = serde_json::from_str(&dto::order_body(request).unwrap()).unwrap();
    for (key, value) in body.as_object().unwrap() {
        response[key] = value.clone();
    }
    let kind = match request.kind {
        OrderKind::Market { .. } => "MARKET",
        OrderKind::Limit { .. } => "LIMIT",
        OrderKind::Stop { .. } => "STOP",
        OrderKind::StopLimit { .. } => "STOP_LIMIT",
    };
    response["type"] = json!(kind);
    if let Some(tif) = body.get("timeValidity") {
        response["timeInForce"] = tif.clone();
    }
    // Replace the quantity token after Value processing, which would otherwise use f64.
    let text = response.to_string();
    let mut fields: BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(&text).unwrap();
    let wire: BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(&dto::order_body(request).unwrap()).unwrap();
    fields.insert("quantity".into(), wire["quantity"].clone());
    serde_json::to_string(&fields).unwrap()
}

fn euro_response_for(request: &BrokerOrderRequest) -> String {
    let mut response: Value = serde_json::from_str(&response_for(request)).unwrap();
    response["ticker"] = json!("demo_air_pa");
    response["instrument"] = json!({"ticker":"demo_air_pa","currency":"EUR"});
    response.to_string()
}
fn limit() -> OrderKind {
    OrderKind::Limit {
        price: Decimal::new(12345, 2),
        time_in_force: TimeInForce::Day,
    }
}

#[test]
fn account_wide_month_history_requires_complete_scoped_pagination() {
    for continuation in [
        json!(null),
        json!("/api/v0/equity/history/orders?limit=50&cursor=10"),
        json!("https://live.trading212.com/api/v0/equity/history/orders?limit=50&cursor=10"),
    ] {
        let safe = continuation.is_null() || continuation.as_str().unwrap().starts_with('/');
        let mut replies = vec![
            ok(json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}})),
            ok(json!({"items":[],"nextPagePath":continuation})),
        ];
        if safe && !continuation.is_null() {
            replies.push(ok(json!({"items":[],"nextPagePath":null})));
        }
        let server = Server::new(replies);
        let mut reader = server.euro_executor().reader;
        reader.config.broker_account_id = 52211463;
        let result = reader.eur_month_order_history(|| Ok(()));
        assert_eq!(result.is_ok(), safe);
        if let Ok(value) = result {
            assert_eq!(value["complete"], true);
        }
        assert!(server.requests().iter().all(|r| r.line.starts_with("GET ")));
    }
    let server = Server::new(vec![
        ok(json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}})),
        ok(json!({"items":[]})),
    ]);
    let mut reader = server.euro_executor().reader;
    reader.config.broker_account_id = 52211463;
    assert!(
        reader.eur_month_order_history(|| Ok(())).is_err(),
        "omitted continuation cannot prove full fill coverage"
    );
}

#[test]
fn native_cash_collector_pages_with_a_fence_and_rejects_unsafe_continuations() {
    for next in [
        "/api/v0/equity/history/transactions?limit=50&cursor=2",
        "https://other.example/api/v0/equity/history/transactions?cursor=2",
        "/api/v0/equity/history/dividends?cursor=2",
    ] {
        let safe = next.starts_with("/api/v0/equity/history/transactions?");
        let mut replies = vec![
            ok(json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}})),
            ok(json!({"items":[],"nextPagePath":next})),
        ];
        if safe {
            replies.extend([
                ok(json!({"items":[],"nextPagePath":null})),
                ok(json!({"items":[],"nextPagePath":null})),
            ]);
        }
        let server = Server::new(replies);
        let mut reader = server.euro_executor().reader;
        reader.config.broker_account_id = 52211463;
        let mut fence_calls = 0;
        let result = reader.eur_cash_events(now(), || {
            fence_calls += 1;
            Ok(())
        });
        assert_eq!(result.is_ok(), safe);
        assert!(
            server.requests()[1]
                .line
                .starts_with("GET /api/v0/equity/history/transactions?limit=50 ")
        );
        assert_eq!(
            server
                .requests()
                .iter()
                .filter(|r| r.line.starts_with("POST "))
                .count(),
            0
        );
        assert!(fence_calls >= 2);
        if let Ok(value) = result {
            assert_eq!(
                value["sources"]["transactions"]["pages"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(value["sources"]["dividends"]["complete"], true);
        }
    }
    let server = Server::new(vec![ok(
        json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}}),
    )]);
    let mut reader = server.euro_executor().reader;
    reader.config.broker_account_id = 52211463;
    assert!(reader.eur_cash_events(now(), || Err(invalid())).is_err());
    assert_eq!(
        server.requests().len(),
        1,
        "lost fence must prevent history reads"
    );
}

#[test]
fn short_restart_reuses_exact_entry_catalog_without_metadata_or_unsafe_post() {
    use trady_application::eur_protocol::driver::ProtocolBroker;
    let mut request = euro_request();
    request.side = Side::Sell;
    request.kind = OrderKind::Stop {
        price: Decimal::from(98),
        time_in_force: TimeInForce::Day,
    };
    let server = Server::new(vec![
        ok(json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}})),
        ok(
            json!([{"instrument":{"ticker":"demo_air_pa","currency":"EUR"},"quantity":1,"quantityAvailableForTrading":1}]),
        ),
        ok(serde_json::from_str(&euro_response_for(&request)).unwrap()),
    ]);
    let mut executor = server.euro_executor();
    executor.reader.config.broker_account_id = 52211463;
    let Reply::Json(_, text, _) = euro_metadata() else {
        panic!("fixture")
    };
    let raw: Value = serde_json::from_str(&text).unwrap();
    executor
        .information()
        .seed_protocol_catalog(&raw, now() - chrono::Duration::seconds(1))
        .unwrap();
    let mut broker = eur_protocol::Trading212ProtocolBroker::new(executor, euro_binding());
    broker
        .submit(&request, now() + chrono::Duration::seconds(5))
        .unwrap();
    assert!(
        server
            .requests()
            .iter()
            .all(|r| !r.line.contains("metadata/instruments"))
    );
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.line.starts_with("POST "))
            .count(),
        1
    );
    assert!(
        broker
            .executor
            .information()
            .seed_protocol_catalog(&raw, now() - chrono::Duration::seconds(540))
            .is_err()
    );
    assert!(
        broker
            .executor
            .information()
            .seed_protocol_catalog(&raw, now() + chrono::Duration::seconds(1))
            .is_err()
    );
}

#[test]
fn revoked_entry_authority_after_preparation_prevents_the_actual_post() {
    let server = Server::new(vec![summary(), euro_metadata()]);
    let mut executor = server.euro_executor();
    let mut checked = false;
    let result = executor.submit_eur_bound_guarded(
        &euro_request(),
        &euro_binding(),
        Some(now() + chrono::Duration::seconds(10)),
        || {
            checked = true;
            Err(invalid())
        },
    );
    assert!(checked && result.is_err());
    assert_eq!(server.requests().len(), 2);
    assert!(
        server
            .requests()
            .iter()
            .all(|r| !r.line.starts_with("POST "))
    );
}

#[test]
fn expired_entry_authority_after_information_reads_never_sends_a_post() {
    use trady_application::autonomous::ExecutionBroker;
    let server = Server::new(vec![summary(), metadata()]);
    let request = request(limit(), Side::Buy);
    let error = server
        .executor()
        .submit_order_before(&request, now())
        .unwrap_err();
    assert_eq!(error.kind, BrokerFailureKind::InvalidRequest);
    assert!(!error.outcome_unknown);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| request.line.starts_with("GET "))
    );
}

#[test]
fn explicit_environments_and_redacted_credentials() {
    assert_eq!(Environment::Live.base_url(), "https://live.trading212.com");
    assert_eq!(Environment::Demo.base_url(), "https://demo.trading212.com");
    assert!(config().validate().is_ok());
    let mut bad = config();
    bad.environment = Environment::Demo;
    assert!(bad.validate().is_err());
    bad.account_id = AccountId::new("paper_t212_test").unwrap();
    assert!(bad.validate().is_ok());
    for timeout in [0, 31, u64::MAX] {
        bad.timeout_seconds = timeout;
        assert!(bad.validate().is_err());
    }
    let mut json = serde_json::to_value(config()).unwrap();
    json["base_url"] = json!("http://elsewhere");
    assert!(serde_json::from_value::<Trading212Config>(json).is_err());
    let credentials = Credentials::new("private-key".into(), "private-secret".into()).unwrap();
    let debug = format!("{credentials:?}");
    assert!(!debug.contains("private"));
    assert!(Credentials::new("bad:key".into(), "secret".into()).is_err());
    assert!(Credentials::new("key".into(), "secret\n".into()).is_err());
}

#[test]
fn exact_eur_crosswalk_can_submit_without_iex_ticker_inference() {
    let server = Server::new(vec![
        summary(),
        euro_metadata(),
        ok(serde_json::from_str(&euro_response_for(&euro_request())).unwrap()),
    ]);
    let request = euro_request();
    let binding = euro_binding();
    let result = server.euro_executor().submit_eur_bound_before(
        &request,
        &binding,
        Some(now() + chrono::Duration::minutes(5)),
    );
    let order = result.expect("EUR crosswalk order");
    assert_eq!(order.instrument_id.as_str(), "demo_air_pa");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].line.starts_with("GET "));
    assert!(requests[1].line.starts_with("GET "));
    assert!(requests[2].line.starts_with("POST "));
    assert!(requests[2].body.contains("demo_air_pa"));
}

#[test]
fn eur_crosswalk_rejects_internal_or_currency_identity_mismatch_before_post() {
    let server = Server::new(vec![]);
    let mut request = euro_request();
    request.instrument_id = InstrumentId::new("AIR_PA_EQ").unwrap();
    let error = server
        .euro_executor()
        .submit_eur_bound_before(
            &request,
            &euro_binding(),
            Some(now() + chrono::Duration::minutes(5)),
        )
        .expect_err("mismatched EUR identity");
    assert_eq!(error.kind, BrokerFailureKind::Unsupported);
    assert!(server.requests().is_empty());
}

#[test]
fn eur_catalog_isin_fractional_and_tick_changes_never_reach_post() {
    for variant in 0..3 {
        let server = Server::new(vec![summary(), euro_metadata()]);
        let mut binding = euro_binding();
        let mut request = euro_request();
        match variant {
            0 => binding.isin = "NL0010273215".into(),
            1 => request.quantity = Decimal::new(15, 1),
            _ => {
                request.kind = OrderKind::Limit {
                    price: Decimal::new(123451, 3),
                    time_in_force: TimeInForce::Day,
                }
            }
        }
        assert!(
            server
                .euro_executor()
                .submit_eur_bound_before(
                    &request,
                    &binding,
                    Some(now() + chrono::Duration::minutes(1))
                )
                .is_err()
        );
        assert!(
            server
                .requests()
                .iter()
                .all(|request| request.line.starts_with("GET "))
        );
    }
}

#[test]
fn snapshot_uses_available_cash_total_holdings_and_exact_decimals() {
    let mut partial = order();
    partial["side"] = json!("SELL");
    partial["quantity"] = json!(-2);
    partial["filledQuantity"] = json!(-1);
    partial["status"] = json!("PARTIALLY_FILLED");
    let server = Server::new(vec![Reply::Json(200, r#"{"id":17,"currency":"EUR","cash":{"availableToTrade":49.000000000000000001,"inPies":700,"reservedForOrders":100}}"#.into(), vec![]), ok(json!([{"instrument":{"ticker":"AAPL_US_EQ","currency":"USD"},"quantity":2.25,"quantityAvailableForTrading":1,"quantityInPies":1.25}])), ok(json!([partial]))]);
    let mut reader = server.reader();
    assert!(
        reader
            .read_snapshot(&AccountId::new("live_other").unwrap())
            .is_err()
    );
    assert!(server.requests().is_empty());
    let snapshot = reader.read_snapshot(&config().account_id).unwrap();
    assert_eq!(snapshot.mode, Mode::Live);
    assert_eq!(snapshot.observed_at, now());
    assert_eq!(
        snapshot.spendable_cash.amount.to_string(),
        "49.000000000000000001"
    );
    assert_eq!(snapshot.positions[0].quantity, Decimal::new(225, 2));
    assert_eq!(snapshot.orders[0].quantity, Decimal::TWO);
    assert_eq!(snapshot.orders[0].filled_quantity, Decimal::ONE);
    assert_eq!(snapshot.orders[0].status, OrderStatus::PartiallyFilled);
    assert_eq!(snapshot.orders[0].client_intent_id, None);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for r in requests {
        assert!(r.line.starts_with("GET /api/v0/equity/"));
        assert!(
            r.headers
                .contains("Basic Zml4dHVyZS1rZXk6Zml4dHVyZS1zZWNyZXQ=")
        );
    }
    assert_eq!(reader.capabilities(), Capabilities::default());
    assert!(
        reader
            .find_order(&IntentId::new("local_intent").unwrap())
            .is_err()
    );
}

#[test]
fn mismatched_account_or_currency_blocks_before_positions_or_mutations() {
    for response in [
        json!({"id":18,"currency":"EUR","cash":{"availableToTrade":50}}),
        json!({"id":17,"currency":"USD","cash":{"availableToTrade":50}}),
    ] {
        let server = Server::new(vec![ok(response)]);
        let mut executor = server.executor();
        let failure = executor
            .submit_order(&request(limit(), Side::Buy))
            .unwrap_err();
        assert_eq!(failure.kind, BrokerFailureKind::AccountMismatch);
        assert!(!failure.outcome_unknown);
        assert_eq!(server.requests().len(), 1);
    }
}

#[test]
fn direct_information_reads_verify_binding_before_returning_private_data() {
    for operation in 0..4 {
        let server = Server::new(vec![ok(
            json!({"id":18,"currency":"EUR","cash":{"availableToTrade":50}}),
        )]);
        let mut reader = server.reader();
        let failure = match operation {
            0 => reader.pending_orders().map(|_| ()).unwrap_err(),
            1 => reader.pending_order("42").map(|_| ()).unwrap_err(),
            2 => reader
                .historical_orders(&HistoryQuery {
                    instrument_id: None,
                    cursor: None,
                    limit: 50,
                })
                .map(|_| ())
                .unwrap_err(),
            _ => reader.instruments().map(|_| ()).unwrap_err(),
        };
        assert_eq!(failure.kind, BrokerFailureKind::AccountMismatch);
        assert_eq!(server.requests().len(), 1);
        assert!(server.requests()[0].line.contains("/account/summary"));
    }
    let server = Server::new(vec![
        summary(),
        ok(json!([])),
        ok(json!({"items":[],"nextPagePath":null})),
    ]);
    let mut reader = server.reader();
    reader.pending_orders().unwrap();
    reader
        .historical_orders(&HistoryQuery {
            instrument_id: None,
            cursor: None,
            limit: 50,
        })
        .unwrap();
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.line.contains("/account/summary"))
            .count(),
        1
    );
}

#[test]
fn snapshot_rejects_clock_reversal_and_overlong_collection() {
    struct MovingClock {
        called: std::cell::Cell<bool>,
        delta: TimeDelta,
    }
    impl Clock for MovingClock {
        fn now(&self) -> DateTime<Utc> {
            if self.called.replace(true) {
                now() + self.delta
            } else {
                now()
            }
        }
    }
    for delta in [TimeDelta::seconds(-1), TimeDelta::seconds(61)] {
        let server = Server::new(vec![summary(), ok(json!([])), ok(json!([]))]);
        let mut reader = Trading212Reader {
            config: config(),
            http: Http::for_test(server.base.clone(), 1),
            clock: MovingClock {
                called: std::cell::Cell::new(false),
                delta,
            },
            bound_at: None,
            metadata: None,
        };
        assert_eq!(
            reader.read_snapshot(&config().account_id).unwrap_err().kind,
            BrokerFailureKind::InvalidResponse
        );
    }
}

#[test]
fn numeric_tokens_are_exact_and_out_of_range_values_fail() {
    for (input, expected) in [
        ("9007199254740993.0000000001", "9007199254740993.0000000001"),
        ("1e-8", "0.00000001"),
        ("-1.23E+2", "-123"),
        ("1e-28", "0.0000000000000000000000000001"),
    ] {
        let number: dto::Number = serde_json::from_str(input).unwrap();
        assert_eq!(number.0.to_string(), expected);
        assert!(!serde_json::to_string(&number).unwrap().starts_with('"'));
    }
    for input in [
        "1e-29",
        "1e100",
        "0.12345678901234567890123456789e0",
        "0.12345678901234567890123456789",
        "\"1.5\"",
        "null",
        "true",
    ] {
        assert!(
            serde_json::from_str::<dto::Number>(input).is_err(),
            "{input}"
        );
    }
}

#[test]
fn all_order_types_send_exact_signed_quantity_and_validate_acknowledgment() {
    for kind in [
        limit(),
        OrderKind::Market {
            extended_hours: false,
        },
        OrderKind::Stop {
            price: Decimal::new(1205, 1),
            time_in_force: TimeInForce::GoodTillCancel,
        },
        OrderKind::StopLimit {
            stop: Decimal::new(1205, 1),
            limit: Decimal::new(120, 0),
            time_in_force: TimeInForce::Day,
        },
    ] {
        for side in [Side::Buy, Side::Sell] {
            let request = request(kind.clone(), side);
            let server = Server::new(vec![
                summary(),
                metadata(),
                Reply::Json(200, response_for(&request), vec![]),
            ]);
            let acknowledged = server.executor().submit_order(&request).unwrap();
            assert_eq!(acknowledged.quantity, request.quantity);
            assert_eq!(acknowledged.side, side);
            assert_eq!(acknowledged.broker_order_id, "42");
            let sent = server.requests();
            assert_eq!(sent.len(), 3);
            assert!(sent[2].line.starts_with("POST /api/v0/equity/orders/"));
            assert!(sent[2].body.contains(if side == Side::Sell {
                "\"quantity\":-0.1234567890123456789012345678"
            } else {
                "\"quantity\":0.1234567890123456789012345678"
            }));
            assert!(!sent[2].body.contains("intent"));
            assert!(!sent[2].body.contains("timeInForce"));
            if !matches!(kind, OrderKind::Market { .. }) {
                assert!(sent[2].body.contains("timeValidity"));
            }
        }
    }
}

#[test]
fn uncertain_submissions_are_never_retried_or_reported_as_rejected() {
    let mut mismatch = order();
    mismatch["ticker"] = json!("MSFT_US_EQ");
    let replies = [
        Reply::Disconnect,
        Reply::Json(500, "fixture-secret reflected error".into(), vec![]),
        Reply::Json(409, "ambiguous conflict".into(), vec![]),
        Reply::Json(
            302,
            "redirect".into(),
            vec![("Location".into(), "http://127.0.0.1:1/steal".into())],
        ),
        Reply::Json(200, "{invalid".into(), vec![]),
        ok(mismatch),
    ];
    for reply in replies {
        let server = Server::new(vec![summary(), metadata(), reply]);
        let failure = server
            .executor()
            .submit_order(&request(limit(), Side::Buy))
            .unwrap_err();
        assert!(failure.outcome_unknown);
        assert_ne!(failure.kind, BrokerFailureKind::Rejected);
        assert!(!failure.to_string().contains("fixture-secret"));
        assert_eq!(
            server
                .requests()
                .iter()
                .filter(|r| r.line.starts_with("POST"))
                .count(),
            1
        );
        assert_eq!(server.requests().len(), 3);
    }
}

#[test]
fn explicit_rejections_and_rate_limits_do_not_retry() {
    for status in [400, 401, 403, 404, 422, 429] {
        let server = Server::new(vec![
            summary(),
            metadata(),
            Reply::Json(
                status,
                "secret response".into(),
                vec![("Retry-After".into(), "45".into())],
            ),
        ]);
        let failure = server
            .executor()
            .submit_order(&request(limit(), Side::Buy))
            .unwrap_err();
        assert!(!failure.outcome_unknown);
        assert_eq!(failure.http_status, Some(status));
        assert_eq!(failure.retry_after_seconds, Some(45));
        assert!(!format!("{failure:?}").contains("secret response"));
        assert_eq!(server.requests().len(), 3);
    }
    let server = Server::new(vec![Reply::Json(
        429,
        "limited".into(),
        vec![
            ("x-ratelimit-remaining".into(), "0".into()),
            (
                "x-ratelimit-reset".into(),
                (Utc::now().timestamp() + 45).to_string(),
            ),
        ],
    )]);
    let mut reader = server.reader();
    assert_eq!(
        reader.spendable_cash().unwrap_err().kind,
        BrokerFailureKind::RateLimited
    );
    let blocked = reader.spendable_cash().unwrap_err();
    assert!(blocked.retry_after_seconds.unwrap() >= 44);
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn invalid_requests_and_path_injection_never_reach_network() {
    let server = Server::new(vec![]);
    let mut executor = server.executor();
    let mut r = request(limit(), Side::Buy);
    r.quantity = Decimal::ZERO;
    assert!(executor.submit_order(&r).is_err());
    r.quantity = Decimal::ONE;
    r.account_id = AccountId::new("live_other").unwrap();
    assert!(executor.submit_order(&r).is_err());
    for id in [
        "../positions",
        "0",
        "-2",
        "+42",
        "042",
        "42?x=y",
        "9223372036854775808",
    ] {
        assert!(executor.cancel_order(&config().account_id, id).is_err());
    }
    assert!(server.requests().is_empty());
}

#[test]
fn metadata_currency_and_instrument_type_are_checked_before_submission() {
    for (currency, kind) in [("EUR", "STOCK"), ("USD", "ETF")] {
        let server = Server::new(vec![
            summary(),
            ok(json!([{"ticker":"AAPL_US_EQ","currencyCode":currency,"isin":"test","type":kind}])),
        ]);
        assert_eq!(
            server
                .executor()
                .submit_order(&request(limit(), Side::Buy))
                .unwrap_err()
                .kind,
            BrokerFailureKind::Unsupported
        );
        assert_eq!(server.requests().len(), 2);
    }
}

#[test]
fn metadata_rows_with_unrepresentable_tickers_are_counted_and_skipped() {
    let server = Server::new(vec![
        summary(),
        ok(json!([
            {"ticker":"AAPL_US_EQ","currencyCode":"USD","isin":"US0378331005","type":"STOCK"},
            {"ticker":"BRK.B_US_EQ","currencyCode":"USD","isin":"US0846707026","type":"STOCK"}
        ])),
    ]);
    let catalog = server.reader().instruments_with_rejections().unwrap();
    assert_eq!(catalog.instruments.len(), 1);
    assert_eq!(catalog.instruments[0].instrument_id.as_str(), "AAPL_US_EQ");
    assert_eq!(catalog.rejected_count, 1);
    assert_eq!(server.requests().len(), 2);
}

#[test]
fn euro_stock_catalog_filters_metadata_without_promoting_capability() {
    let server = Server::new(vec![
        summary(),
        ok(json!([
            {"ticker":"SAP_DE_EQ","currencyCode":"EUR","isin":"DE0007164600","type":"STOCK"},
            {"ticker":"AAPL_US_EQ","currencyCode":"USD","isin":"US0378331005","type":"STOCK"},
            {"ticker":"EURO_ETF","currencyCode":"EUR","isin":"IE00B4L5Y983","type":"ETF"}
        ])),
    ]);
    let stocks = server.reader().euro_stocks().unwrap();
    assert_eq!(stocks.len(), 1);
    assert_eq!(stocks[0].instrument_id.as_str(), "SAP_DE_EQ");
    assert_eq!(server.requests().len(), 2);
}

#[test]
fn cancellation_receipt_is_not_terminal_state_and_pending_404_is_only_absence() {
    let server = Server::new(vec![
        summary(),
        Reply::Json(200, String::new(), vec![]),
        Reply::Json(404, "not pending".into(), vec![]),
    ]);
    let mut executor = server.executor();
    assert_eq!(
        executor
            .cancel_order(&config().account_id, "42")
            .unwrap()
            .broker_order_id,
        "42"
    );
    assert_eq!(executor.information().pending_order("42").unwrap(), None);
    assert_eq!(
        server.requests()[1].line,
        "DELETE /api/v0/equity/orders/42 HTTP/1.1"
    );
    let server = Server::new(vec![summary(), Reply::Disconnect]);
    let failure = server
        .executor()
        .cancel_order(&config().account_id, "42")
        .unwrap_err();
    assert!(failure.outcome_unknown);
    assert_eq!(failure.broker_order_id.as_deref(), Some("42"));
    assert_eq!(server.requests().len(), 2);
}

#[test]
fn history_deduplicates_order_totals_and_returns_a_validated_cursor() {
    let mut filled = order();
    filled["status"] = json!("FILLED");
    filled["filledQuantity"] = json!(1);
    let server = Server::new(vec![
        summary(),
        ok(json!({"items":[
                {"order":filled,"fill":{"id":7,"filledAt":"2026-10-30T14:00:01Z","price":123.4,"quantity":0.3,"tradingMethod":"TOTV","type":"TRADE","walletImpact":{"currency":"EUR","fxRate":1.1,"netValue":-37.02,"realisedProfitLoss":0,"taxes":[{"chargedAt":"2026-10-30T14:00:01Z","currency":"EUR","name":"TRANSACTION_FEE","quantity":0.2}]} }},
                {"order":filled,"fill":{"id":8,"filledAt":"2026-10-30T14:00:02Z","price":123.5,"quantity":0.7,"tradingMethod":"OTC","type":"TRADE","walletImpact":{"currency":"EUR","fxRate":1.1,"netValue":-86.45,"realisedProfitLoss":0,"taxes":[]}}}
            ],"nextPagePath":"/api/v0/equity/history/orders?limit=50&cursor=123&ticker=AAPL_US_EQ"})),
    ]);
    let page = server
        .reader()
        .historical_orders(&HistoryQuery {
            instrument_id: Some(InstrumentId::new("AAPL_US_EQ").unwrap()),
            cursor: None,
            limit: 50,
        })
        .unwrap();
    assert_eq!(page.orders.len(), 1);
    assert_eq!(page.orders[0].filled_quantity, Decimal::ONE);
    assert_eq!(page.fills.len(), 2);
    assert_eq!(page.fills[0].quantity, Some(Decimal::new(3, 1)));
    assert_eq!(page.fills[0].price, Some(Decimal::new(1234, 1)));
    assert_eq!(page.fills[0].fill_type.as_deref(), Some("TRADE"));
    assert_eq!(
        page.fills[0].wallet_impact.as_ref().unwrap().fx_rate,
        Some(Decimal::new(11, 1))
    );
    assert_eq!(page.fills[0].wallet_impact.as_ref().unwrap().taxes.len(), 1);
    assert_eq!(page.next_cursor, Some(123));
    assert!(
        server.requests()[1]
            .line
            .contains("limit=50&ticker=AAPL_US_EQ")
    );
}

#[test]
fn history_rejects_conflicting_duplicate_fill_ids_and_overfilled_totals() {
    let mut filled = order();
    filled["status"] = json!("FILLED");
    filled["filledQuantity"] = json!(1);
    let server = Server::new(vec![
        summary(),
        ok(json!({
            "items":[
                {"order":filled,"fill":{"id":7,"quantity":0.5,"price":10}},
                {"order":filled,"fill":{"id":7,"quantity":0.5,"price":11}}
            ],
            "nextPagePath":null
        })),
    ]);
    assert!(
        server
            .reader()
            .historical_orders(&HistoryQuery {
                instrument_id: None,
                cursor: None,
                limit: 50,
            })
            .is_err()
    );

    let mut filled = order();
    filled["status"] = json!("FILLED");
    filled["filledQuantity"] = json!(1);
    let server = Server::new(vec![
        summary(),
        ok(json!({
            "items":[
                {"order":filled,"fill":{"id":7,"quantity":0.7,"price":10}},
                {"order":filled,"fill":{"id":8,"quantity":0.7,"price":10}}
            ],
            "nextPagePath":null
        })),
    ]);
    assert!(
        server
            .reader()
            .historical_orders(&HistoryQuery {
                instrument_id: None,
                cursor: None,
                limit: 50,
            })
            .is_err()
    );
}

#[test]
fn history_rejects_untrusted_urls_filters_and_repeating_cursors() {
    for path in [
        "https://evil.invalid/steal",
        "//evil.invalid/api/v0/equity/history/orders?limit=50&cursor=2",
        "/api/v0/equity/history/orders?limit=50&cursor=1",
        "/api/v0/equity/history/orders?limit=50&cursor=2&cursor=3",
        "/api/v0/equity/history/orders?limit=50&cursor=2&ticker=MSFT_US_EQ",
        "/api/v0/equity/history/orders?limit=50&cursor=2#bad",
        "/api/v0/equity/history/orders?limit=51&cursor=2",
    ] {
        let server = Server::new(vec![summary(), ok(json!({"items":[],"nextPagePath":path}))]);
        assert!(
            server
                .reader()
                .historical_orders(&HistoryQuery {
                    instrument_id: None,
                    cursor: Some(1),
                    limit: 50
                })
                .is_err()
        );
        assert_eq!(server.requests().len(), 2);
    }
}

#[test]
fn missing_history_continuation_is_not_treated_as_complete_history() {
    let server = Server::new(vec![summary(), ok(json!({"items":[]}))]);
    assert_eq!(
        server
            .reader()
            .historical_orders(&HistoryQuery {
                instrument_id: None,
                cursor: None,
                limit: 50
            })
            .unwrap_err()
            .kind,
        BrokerFailureKind::InvalidResponse
    );
}

#[test]
fn malformed_or_value_orders_fail_and_unknown_statuses_stay_unknown() {
    for field in ["strategy", "quantity", "filledQuantity", "id"] {
        let mut value = order();
        value.as_object_mut().unwrap().remove(field);
        let server = Server::new(vec![summary(), ok(json!([value]))]);
        assert!(server.reader().pending_orders().is_err());
    }
    for status in [
        "CANCELLING",
        "REPLACING",
        "REPLACED",
        "LOCAL",
        "UNCONFIRMED",
        "NEW_PROVIDER_STATUS",
    ] {
        let mut value = order();
        value["status"] = json!(status);
        let server = Server::new(vec![summary(), ok(json!([value]))]);
        assert_eq!(
            server.reader().pending_orders().unwrap()[0].status,
            OrderStatus::Unknown
        );
    }
    let mut value = order();
    value["strategy"] = json!("VALUE");
    let server = Server::new(vec![summary(), ok(json!([value]))]);
    assert!(server.reader().pending_orders().is_err());
    let mut value = order();
    value["ticker"] = json!("BRK.B_US_EQ");
    value["instrument"]["ticker"] = json!("BRK.B_US_EQ");
    let server = Server::new(vec![summary(), ok(json!([value]))]);
    assert!(server.reader().pending_orders().is_err());
}

#[test]
fn eur_protocol_observer_requires_exact_stop_shape_and_terminal_history() {
    use trady_application::eur_protocol::{
        OrderLeg, ProtocolContext, ProtocolInput, ProtocolState, driver::ProtocolBroker,
    };
    let context = ProtocolContext {
        runtime_mode: trady_domain::ExecutionStage::Demo,
        policy_revision: "eur_intraday_v1".into(),
        intent_id: "eur_protocol_http_test".into(),
        account_id: "paper_t212_test".into(),
        broker_account_id: 52211463,
        instrument_id: "AIR_PA_EQ".into(),
        broker_ticker: "AIRp_EQ".into(),
        isin: "NL0000235190".into(),
        policy_hash: "a".repeat(64),
        configuration_hash: "b".repeat(64),
        evidence_ids: vec!["fixture_only".into()],
        quantity: Decimal::from(2),
        entry_limit: Decimal::from(100),
        initial_stop: Decimal::from(98),
        target: Decimal::from(103),
        created_at: now(),
        flatten_at: now() + chrono::Duration::hours(1),
        max_unprotected_seconds: 10,
        entry_valid_until: None,
    };
    let mut state = ProtocolState::new(context).unwrap();
    for (i, input) in [
        ProtocolInput::Reserve,
        ProtocolInput::DispatchEntry,
        ProtocolInput::Acknowledged {
            leg: OrderLeg::Entry,
            broker_id: "1001".into(),
        },
        ProtocolInput::Observed {
            leg: OrderLeg::Entry,
            broker_id: "1001".into(),
            broker_ticker: "AIRp_EQ".into(),
            quantity: Decimal::from(2),
            filled: Decimal::from(2),
            status: OrderStatus::Filled,
            holding_quantity: Decimal::from(2),
            pending_present: false,
            history_complete: true,
            stop_price: None,
        },
        ProtocolInput::DispatchProtection,
        ProtocolInput::Acknowledged {
            leg: OrderLeg::Protection,
            broker_id: "1002".into(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        state = state
            .transition(format!("fixture_{i}"), input, now())
            .unwrap()
            .state;
    }
    let entry = json!({"id":1001,"ticker":"AIRp_EQ","strategy":"QUANTITY","type":"LIMIT","quantity":2,"filledQuantity":2,"side":"BUY","status":"FILLED","limitPrice":100,"timeInForce":"DAY","extendedHours":false});
    let stop = json!({"id":1002,"ticker":"AIRp_EQ","strategy":"QUANTITY","type":"STOP","quantity":2,"filledQuantity":0,"side":"SELL","status":"NEW","stopPrice":98,"timeInForce":"DAY","extendedHours":false});
    for case in [
        "working",
        "missing_history",
        "wrong_stop",
        "terminal_tif_omitted",
        "pending_tif_omitted",
        "missing_fill_time",
        "wrong_confirmation",
        "wrong_confirmation_tif",
        "wrong_confirmation_holding",
    ] {
        let mut observed_stop = stop.clone();
        let mut observed_entry = entry.clone();
        if case == "terminal_tif_omitted" {
            observed_entry
                .as_object_mut()
                .unwrap()
                .remove("timeInForce");
        }
        if case == "pending_tif_omitted" {
            observed_stop.as_object_mut().unwrap().remove("timeInForce");
        }
        if case == "wrong_stop" {
            observed_stop["stopPrice"] = json!(97);
        }
        let pending = if case == "missing_history" {
            json!([])
        } else {
            json!([observed_stop])
        };
        let mut confirmed_stop = stop.clone();
        if case == "wrong_confirmation" {
            confirmed_stop["stopPrice"] = json!(97);
        }
        if case == "wrong_confirmation_tif" {
            confirmed_stop
                .as_object_mut()
                .unwrap()
                .remove("timeInForce");
        }
        let server = Server::new(vec![
            ok(json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}})),
            ok(
                json!([{"instrument":{"ticker":"AIRp_EQ","currency":"EUR"},"quantity":2,"quantityAvailableForTrading":0}]),
            ),
            ok(pending),
            ok(
                json!({"items":[{"order":observed_entry,"fill":{"quantity":2,"filledAt":if case == "missing_fill_time" { serde_json::Value::Null } else { json!(now()) }}}],"nextPagePath":null}),
            ),
            ok(confirmed_stop),
            ok(
                json!([{"instrument":{"ticker":"AIRp_EQ","currency":"EUR"},"quantity":if case == "wrong_confirmation_holding" {1} else {2},"quantityAvailableForTrading":0}]),
            ),
        ]);
        let mut executor = server.euro_executor();
        executor.reader.config.broker_account_id = 52211463;
        let mut binding = euro_binding();
        binding.broker_instrument_id = "AIRp_EQ".into();
        binding.isin = "NL0000235190".into();
        let mut port = eur_protocol::Trading212ProtocolBroker::new(executor, binding);
        let result = port.observe(&state);
        match case {
            "working"
            | "terminal_tif_omitted"
            | "wrong_confirmation"
            | "wrong_confirmation_tif"
            | "wrong_confirmation_holding" => {
                let v = result.unwrap();
                assert!(v.history_complete);
                assert_eq!(v.exact_orders.len(), 2);
                assert_eq!(v.pending_order_ids, vec!["1002"]);
                let confirmation = port.confirm_protection(&state);
                if case.starts_with("wrong_confirmation") {
                    assert!(confirmation.is_err());
                } else {
                    let confirmation = confirmation.unwrap().unwrap();
                    assert_eq!(confirmation.order.broker_order_id, "1002");
                    assert_eq!(confirmation.stop_price, Decimal::from(98));
                }
            }
            "missing_history" => assert!(!result.unwrap().history_complete),
            _ => assert!(result.is_err()),
        }
    }
}

#[test]
fn eur_protocol_cancel_reuses_recent_broker_identity_without_summary_rate_limit() {
    use trady_application::eur_protocol::driver::ProtocolBroker;
    let server = Server::new(vec![
        ok(json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}})),
        ok(json!({})),
    ]);
    let mut executor = server.euro_executor();
    executor.reader.config.broker_account_id = 52211463;
    executor.information().spendable_cash().unwrap();
    let mut port = eur_protocol::Trading212ProtocolBroker::new(executor, euro_binding());
    let receipt = port
        .cancel(&AccountId::new("paper_t212_test").unwrap(), "1002")
        .unwrap();
    assert_eq!(receipt.broker_order_id, "1002");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1]
            .line
            .starts_with("DELETE /api/v0/equity/orders/1002 ")
    );
}

#[test]
fn protection_cancel_refreshes_near_expiry_catalog_before_any_delete() {
    use trady_application::eur_protocol::{OrderLeg, ProtocolState, driver::ProtocolBroker};
    for available in [true, false] {
        let mut responses = vec![ok(
            json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}}),
        )];
        responses.push(if available {
            euro_metadata()
        } else {
            Reply::Json(503, "{}".into(), vec![])
        });
        if available {
            responses.push(ok(json!({})));
        }
        let server = Server::new(responses);
        let mut executor = server.euro_executor();
        executor.reader.config.broker_account_id = 52211463;
        executor.reader.metadata = Some((
            std::time::Instant::now() - std::time::Duration::from_secs(599),
            Default::default(),
            vec![],
        ));
        let mut broker = eur_protocol::Trading212ProtocolBroker::new(executor, euro_binding());
        let context = serde_json::from_value(json!({"runtime_mode":"DEMO","policy_revision":"eur_intraday_v1","intent_id":"catalog_headroom_fixture","account_id":"paper_t212_test","broker_account_id":52211463,
            "instrument_id":"AIR_PA_EQ","broker_ticker":"demo_air_pa","isin":"FR0000120404","policy_hash":"a".repeat(64),"configuration_hash":"b".repeat(64),"evidence_ids":["synthetic_fixture_only"],
            "quantity":"1","entry_limit":"100","initial_stop":"98","target":"103","created_at":now(),"flatten_at":now()+chrono::Duration::hours(1),"max_unprotected_seconds":10})).unwrap();
        let state = ProtocolState::new(context).unwrap();
        assert_eq!(
            broker.prepare_cancel(&state, OrderLeg::Protection).is_ok(),
            available
        );
        if available {
            broker
                .cancel(&AccountId::new("paper_t212_test").unwrap(), "1002")
                .unwrap();
        }
        let requests = server.requests();
        assert!(requests[1].line.contains("metadata/instruments"));
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.line.starts_with("DELETE "))
                .count(),
            usize::from(available)
        );
    }
}

#[test]
fn eur_protocol_stop_uses_fresh_binding_and_rechecks_available_quantity_once() {
    use trady_application::eur_protocol::driver::ProtocolBroker;
    let mut request = euro_request();
    request.side = Side::Sell;
    request.kind = OrderKind::Stop {
        price: Decimal::from(98),
        time_in_force: TimeInForce::Day,
    };
    for age_seconds in [None, Some(0), Some(2)] {
        let fresh = age_seconds == Some(0);
        let mut responses = vec![ok(
            json!({"id":52211463,"currency":"EUR","cash":{"availableToTrade":4000}}),
        )];
        if !fresh {
            responses.push(ok(json!([{"instrument":{"ticker":"demo_air_pa","currency":"EUR"},"quantity":1,"quantityAvailableForTrading":1}])));
        }
        responses.push(euro_metadata());
        responses.push(ok(
            serde_json::from_str(&euro_response_for(&request)).unwrap()
        ));
        let server = Server::new(responses);
        let mut executor = server.euro_executor();
        executor.reader.config.broker_account_id = 52211463;
        executor.information().spendable_cash().unwrap();
        let mut port = eur_protocol::Trading212ProtocolBroker::new(executor, euro_binding());
        port.observed_sellable = age_seconds.map(|seconds| {
            (
                "demo_air_pa".into(),
                Decimal::ONE,
                now() - chrono::Duration::seconds(seconds),
            )
        });
        port.submit(&request, now() + chrono::Duration::seconds(10))
            .unwrap();
        assert!(
            port.observed_sellable.is_none(),
            "one observation may authorize only one dispatch"
        );
        let requests = server.requests();
        assert_eq!(requests.len(), if fresh { 3 } else { 4 });
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.line.contains("account/summary"))
                .count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.line.contains("GET /api/v0/equity/positions"))
                .count(),
            if fresh { 0 } else { 1 }
        );
        assert!(
            requests
                .last()
                .unwrap()
                .line
                .starts_with("POST /api/v0/equity/orders/stop ")
        );
    }
}

#[test]
fn expired_eur_binding_allows_only_recovery_market_sell() {
    for (side, kind, accepted) in [
        (
            Side::Sell,
            OrderKind::Market {
                extended_hours: false,
            },
            true,
        ),
        (Side::Buy, limit(), false),
        (
            Side::Sell,
            OrderKind::Stop {
                price: Decimal::from(98),
                time_in_force: TimeInForce::Day,
            },
            false,
        ),
    ] {
        let mut binding = euro_binding();
        binding.expires_at = Some(now() - chrono::Duration::seconds(1));
        let mut request = euro_request();
        request.side = side;
        request.kind = kind;
        let mut responses = vec![];
        if accepted {
            responses.extend([
                summary(),
                euro_metadata(),
                ok(json!([{"instrument":{"ticker":"demo_air_pa","currency":"EUR"},"quantity":1,"quantityAvailableForTrading":1}])),
                ok(serde_json::from_str(&euro_response_for(&request)).unwrap()),
            ]);
        }
        let server = Server::new(responses);
        let result = server.euro_executor().submit_eur_bound_before(
            &request,
            &binding,
            Some(now() + chrono::Duration::seconds(10)),
        );
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(
            server
                .requests()
                .iter()
                .filter(|r| r.line.starts_with("POST "))
                .count(),
            usize::from(accepted)
        );
    }
}

#[test]
fn eur_order_value_guard_keeps_the_quantity_step_separate() {
    for (quantity, accepted) in [(Decimal::new(50, 4), false), (Decimal::new(51, 4), true)] {
        let mut binding = euro_binding();
        binding.quantity_step = Decimal::new(1, 4);
        binding.minimum_quantity = binding.quantity_step;
        binding.fractional_orders = true;
        binding.value_rules = Some(trady_domain::eur::EurOrderValueRules {
            minimum_trade_value: Decimal::ONE,
            minimum_remaining_position_value: Decimal::ONE,
            price_floor: Decimal::from(190),
            price_ceiling: Decimal::from(210),
        });
        let mut request = euro_request();
        request.quantity = quantity;
        request.kind = OrderKind::Limit {
            price: Decimal::new(19840, 2),
            time_in_force: TimeInForce::Day,
        };
        let mut responses = vec![summary(), euro_metadata()];
        if accepted {
            responses.push(ok(
                serde_json::from_str(&euro_response_for(&request)).unwrap()
            ));
        }
        let server = Server::new(responses);
        let result = server.euro_executor().submit_eur_bound_before(
            &request,
            &binding,
            Some(now() + chrono::Duration::seconds(10)),
        );
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(
            server
                .requests()
                .iter()
                .filter(|r| r.line.starts_with("POST "))
                .count(),
            usize::from(accepted)
        );
    }
}

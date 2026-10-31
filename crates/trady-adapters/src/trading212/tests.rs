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
use trady_domain::{OrderStatus, Side};

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
fn limit() -> OrderKind {
    OrderKind::Limit {
        price: Decimal::new(12345, 2),
        time_in_force: TimeInForce::Day,
    }
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
        ok(
            json!({"items":[{"order":filled,"fill":{"quantity":0.3}},{"order":filled,"fill":{"quantity":0.7}}],"nextPagePath":"/api/v0/equity/history/orders?limit=50&cursor=123&ticker=AAPL_US_EQ"}),
        ),
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
    assert_eq!(page.next_cursor, Some(123));
    assert!(
        server.requests()[1]
            .line
            .contains("limit=50&ticker=AAPL_US_EQ")
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
}

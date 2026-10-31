//! Root registers this new file with `#[cfg(test)] mod autonomous_tests;` in
//! trading212/mod.rs. Loopback HTTP fixtures exercise the real execution reader.

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
use trady_application::{
    autonomous::{ExecutionBroker, OrderRole, TrackedOrder},
    broker::TimeInForce,
};
use trady_domain::{OrderStatus, Side};

struct Server {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
    stopped: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn new(replies: Vec<Value>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_requests = requests.clone();
        let worker_stopped = stopped.clone();
        let join = thread::spawn(move || {
            let mut replies = VecDeque::from(replies);
            while !worker_stopped.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("fixture accept failed: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = vec![];
                loop {
                    let mut buffer = [0; 2048];
                    let length = stream.read(&mut buffer).unwrap();
                    if length == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..length]);
                    if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                        break;
                    }
                    assert!(request.len() <= 32768);
                }
                let request = String::from_utf8(request).unwrap();
                if let Some(line) = request.lines().next() {
                    worker_requests.lock().unwrap().push(line.into());
                }
                // Fixtures serve GETs only. An accidental mutation fails the test.
                assert!(
                    request.starts_with("GET "),
                    "execution observation must stay read-only"
                );
                let body = replies
                    .pop_front()
                    .expect("unexpected extra broker read")
                    .to_string();
                let response = format!(
                    "HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            base,
            requests,
            stopped,
            join: Some(join),
        }
    }

    fn executor(&self) -> Trading212Executor<FakeClock> {
        Trading212Executor {
            reader: Trading212Reader {
                config: config(),
                http: Http::for_test(self.base.clone(), 1),
                clock: FakeClock::new(now()),
                bound_at: None,
                metadata: None,
            },
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
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
        account_id: AccountId::new("live_execution_fixture").unwrap(),
        broker_account_id: 17,
        base_currency: Currency::new("EUR").unwrap(),
        timeout_seconds: 1,
    }
}

fn summary() -> Value {
    json!({"id":17,"currency":"EUR","totalValue":1020,"cash":{"availableToTrade":20}})
}

fn positions() -> Value {
    json!([{"instrument":{"ticker":"AAPL_US_EQ","currency":"EUR"},"quantity":10,
        "quantityAvailableForTrading":0,"walletImpact":{"currency":"EUR","totalCost":1000}}])
}

fn order(id: i64, status: &str, filled: i64) -> Value {
    json!({"id":id,"ticker":"AAPL_US_EQ","instrument":{"ticker":"AAPL_US_EQ","currency":"EUR"},
        "quantity":10,"filledQuantity":filled,"side":"BUY","status":status,"strategy":"QUANTITY",
        "type":"LIMIT","limitPrice":100,"timeInForce":"DAY","extendedHours":false})
}

fn tracked(status: OrderStatus) -> TrackedOrder {
    TrackedOrder {
        request: BrokerOrderRequest {
            intent_id: IntentId::new("persisted_entry_intent").unwrap(),
            account_id: config().account_id,
            instrument_id: InstrumentId::new("AAPL_US_EQ").unwrap(),
            instrument_currency: Currency::new("EUR").unwrap(),
            side: Side::Buy,
            quantity: Decimal::from(10),
            kind: OrderKind::Limit {
                price: Decimal::from(100),
                time_in_force: TimeInForce::Day,
            },
        },
        role: OrderRole::Entry,
        attempted: true,
        observation: Some(BrokerOrder {
            broker_order_id: "42".into(),
            client_intent_id: None,
            instrument_id: InstrumentId::new("AAPL_US_EQ").unwrap(),
            side: Side::Buy,
            quantity: Decimal::from(10),
            filled_quantity: if status == OrderStatus::Filled {
                Decimal::from(10)
            } else {
                Decimal::ZERO
            },
            status,
        }),
        cancellation_requested: false,
        failure: None,
    }
}

#[test]
fn absent_pending_order_resolves_only_exact_terminal_history_id_and_actual_wallet_costs() {
    let wanted = order(42, "FILLED", 10);
    let server = Server::new(vec![
        summary(),
        positions(),
        json!([]),
        json!({"items":[{"order":order(43,"FILLED",10)},
            {"order":wanted,"fill":{"quantity":4}},
            {"order":wanted,"fill":{"quantity":6}}],"nextPagePath":null}),
    ]);
    let observed = server
        .executor()
        .observe(&config().account_id, &[tracked(OrderStatus::Open)])
        .unwrap();
    assert_eq!(observed.snapshot.orders.len(), 1);
    let found = &observed.snapshot.orders[0];
    assert_eq!(found.broker_order_id, "42");
    assert_eq!(found.status, OrderStatus::Filled);
    assert_eq!(
        found.filled_quantity,
        Decimal::from(10),
        "history rows contain total fills, not independent fill events"
    );
    assert_eq!(observed.snapshot.positions[0].quantity, Decimal::from(10));
    assert_eq!(
        observed.position_costs[&InstrumentId::new("AAPL_US_EQ").unwrap()].amount,
        Decimal::from(1000)
    );
    assert_eq!(
        observed.available_quantities[&InstrumentId::new("AAPL_US_EQ").unwrap()],
        Decimal::ZERO
    );
    assert_eq!(observed.total_equity.amount, Decimal::from(1020));
    assert_eq!(observed.snapshot.observed_at, now());
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert!(requests[3].starts_with("GET /api/v0/equity/history/orders?limit=50 "));
}

#[test]
fn lookalike_terminal_order_never_satisfies_an_absent_exact_broker_id() {
    let server = Server::new(vec![
        summary(),
        positions(),
        json!([]),
        json!({"items":[{"order":order(43,"FILLED",10)}],"nextPagePath":null}),
    ]);
    let observed = server
        .executor()
        .observe(&config().account_id, &[tracked(OrderStatus::Open)])
        .unwrap();
    assert!(
        observed.snapshot.orders.is_empty(),
        "missing 42 must stay missing for application recovery"
    );
    assert_eq!(server.requests().len(), 4);
}

#[test]
fn history_pagination_finds_exact_id_without_returning_unrelated_orders() {
    let server = Server::new(vec![
        summary(),
        positions(),
        json!([]),
        json!({"items":[{"order":order(43,"FILLED",10)}],
            "nextPagePath":"/api/v0/equity/history/orders?limit=50&cursor=700"}),
        json!({"items":[{"order":order(42,"FILLED",10)}],"nextPagePath":null}),
    ]);
    let observed = server
        .executor()
        .observe(&config().account_id, &[tracked(OrderStatus::Open)])
        .unwrap();
    assert_eq!(observed.snapshot.orders.len(), 1);
    assert_eq!(observed.snapshot.orders[0].broker_order_id, "42");
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert!(requests[4].contains("limit=50&cursor=700"));
}

#[test]
fn repeated_history_cursor_fails_closed_instead_of_looping_or_guessing() {
    let server = Server::new(vec![
        summary(),
        positions(),
        json!([]),
        json!({"items":[],"nextPagePath":"/api/v0/equity/history/orders?limit=50&cursor=700"}),
        json!({"items":[],"nextPagePath":"/api/v0/equity/history/orders?limit=50&cursor=700"}),
    ]);
    assert!(
        server
            .executor()
            .observe(&config().account_id, &[tracked(OrderStatus::Open)])
            .is_err()
    );
    assert_eq!(server.requests().len(), 5);
}

#[test]
fn unknown_pending_status_is_preserved_for_application_recovery_without_history_guessing() {
    let server = Server::new(vec![
        summary(),
        positions(),
        json!([order(42, "CANCEL_PENDING", 0)]),
    ]);
    let observed = server
        .executor()
        .observe(&config().account_id, &[tracked(OrderStatus::Open)])
        .unwrap();
    assert_eq!(observed.snapshot.orders[0].status, OrderStatus::Unknown);
    assert_eq!(server.requests().len(), 3);
}

#[test]
fn known_id_with_unknown_submission_outcome_can_resolve_from_terminal_history() {
    let server = Server::new(vec![
        summary(),
        positions(),
        json!([]),
        json!({"items":[{"order":order(42,"FILLED",10)}],"nextPagePath":null}),
    ]);
    let observed = server
        .executor()
        .observe(&config().account_id, &[tracked(OrderStatus::Unknown)])
        .unwrap();
    assert_eq!(observed.snapshot.orders[0].status, OrderStatus::Filled);
    assert_eq!(observed.snapshot.orders[0].broker_order_id, "42");
    assert_eq!(server.requests().len(), 4);
}

#[test]
fn durable_terminal_orders_do_not_trigger_history_reads_when_no_pending_row_remains() {
    let server = Server::new(vec![summary(), positions(), json!([])]);
    let observed = server
        .executor()
        .observe(&config().account_id, &[tracked(OrderStatus::Filled)])
        .unwrap();
    assert!(
        observed.snapshot.orders.is_empty(),
        "the application retains its committed terminal observation"
    );
    assert_eq!(server.requests().len(), 3);
}

#[test]
fn missing_or_invalid_execution_accounting_never_produces_an_execution_observation() {
    for case in [
        "equity_missing",
        "equity_negative",
        "available_missing",
        "available_negative",
        "available_exceeds_holding",
        "basis_missing",
        "basis_wrong_currency",
        "basis_nonpositive",
    ] {
        let mut account = summary();
        let mut holding = positions();
        let requests = match case {
            "equity_missing" => {
                account.as_object_mut().unwrap().remove("totalValue");
                1
            }
            "equity_negative" => {
                account["totalValue"] = json!(-1);
                1
            }
            "available_missing" => {
                holding[0]
                    .as_object_mut()
                    .unwrap()
                    .remove("quantityAvailableForTrading");
                2
            }
            "available_negative" => {
                holding[0]["quantityAvailableForTrading"] = json!(-1);
                2
            }
            "available_exceeds_holding" => {
                holding[0]["quantityAvailableForTrading"] = json!(11);
                2
            }
            "basis_missing" => {
                holding[0].as_object_mut().unwrap().remove("walletImpact");
                2
            }
            "basis_wrong_currency" => {
                holding[0]["walletImpact"]["currency"] = json!("USD");
                2
            }
            "basis_nonpositive" => {
                holding[0]["walletImpact"]["totalCost"] = json!(0);
                2
            }
            _ => unreachable!(),
        };
        let server = Server::new(vec![account, holding]);
        assert!(
            server
                .executor()
                .observe(&config().account_id, &[])
                .is_err(),
            "case {case}"
        );
        assert_eq!(server.requests().len(), requests, "case {case}");
    }
}

#[test]
fn mismatched_broker_account_identity_stops_before_position_order_or_history_reads() {
    let mut account = summary();
    account["id"] = json!(99);
    let server = Server::new(vec![account]);
    assert!(
        server
            .executor()
            .observe(&config().account_id, &[tracked(OrderStatus::Open)])
            .is_err()
    );
    assert_eq!(server.requests().len(), 1);
}

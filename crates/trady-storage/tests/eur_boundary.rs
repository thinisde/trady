//! Disposable PostgreSQL only: EUR fences, recovery reads, and delivery claims.
use chrono::{Duration, Utc};
use postgres::{Client, NoTls};
use rust_decimal::Decimal;
use serde_json::json;
use std::env;
use trady_application::{
    broker::{BrokerOrderRequest, OrderKind},
    eur_execution::EurExecutionJournal,
    harness::content_hash,
    telegram::{LifecycleEvent, LifecycleEventKind},
};
use trady_domain::*;
use trady_storage::target::{PgTargetOutbox, PgTargetStore};

fn dsn(name: &str) -> String {
    env::var(name).expect("disposable runner DSN")
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn eur_fences_restart_acknowledgements_and_outbox_are_durable() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    trady_storage::migrate(&dsn("TRADY_MIGRATOR_DSN")).unwrap();
    let account = format!("paper_eur_boundary_{}", Utc::now().timestamp_micros());
    let executor = dsn("TRADY_EXECUTOR_DSN");
    let analyst = dsn("TRADY_ANALYST_DSN");
    let gateway = dsn("TRADY_GATEWAY_DSN");
    let open = || {
        PgTargetStore::connect(&executor, "trady_executor", "owner".into(), account.clone())
            .unwrap()
    };
    let mut store = open();
    assert!(
        store
            .append_document("coordinator_event", "test", &json!({"phase":"IDLE"}))
            .is_err()
    );
    let first = store.acquire_executor_lease("worker_one", 30).unwrap();
    let mut rival = open();
    assert!(rival.acquire_executor_lease("worker_one", 30).is_err());
    store.renew_executor_lease(30).unwrap();
    assert!(
        store
            .append_document("coordinator_event", "test", &json!({"phase":"IDLE"}))
            .unwrap()
    );
    assert!(
        !store
            .append_document("coordinator_event", "test", &json!({"phase":"IDLE"}))
            .unwrap()
    );
    assert!(
        store
            .append_document("coordinator_event", "test", &json!({"phase":"OPEN"}))
            .is_err()
    );
    let mut raw = Client::connect(&executor, NoTls).unwrap();
    for sql in [
        "DELETE FROM target.records",
        "DELETE FROM target.telegram_outbox",
        "UPDATE target.executor_leases SET generation=999",
        "SELECT target.acquire_executor_lease(NULL,NULL,NULL)",
        "SELECT target.renew_executor_lease(NULL,NULL,NULL,NULL)",
    ] {
        assert!(raw.batch_execute(sql).is_err(), "{sql}");
    }
    let value = json!({"value":"x"});
    assert!(raw.query_one(
        "SELECT target.append_scoped_record('quote','forged','owner',$1,$2,$3,$4,'worker_one',$5)",
        &[&account,&value,&value.to_string(),&"a".repeat(64),&first]).is_err());
    let mut research =
        PgTargetStore::connect(&analyst, "trady_analyst", "owner".into(), account.clone()).unwrap();
    assert!(
        research
            .append_document("order_intent", "forbidden", &json!({}))
            .is_err()
    );
    assert!(
        research
            .append_document("provider_snapshot", "observed", &json!({"status":"ERROR"}))
            .unwrap()
    );
    let mut other = PgTargetStore::connect(
        &analyst,
        "trady_analyst",
        "someone_else".into(),
        account.clone(),
    )
    .unwrap();
    assert!(
        other
            .read_document("provider_snapshot", "observed")
            .unwrap()
            .is_none()
    );
    assert!(
        other
            .append_document("provider_snapshot", "observed", &json!({"status":"ERROR"}))
            .is_err()
    );

    let at = Utc::now();
    let intent = BrokerOrderRequest {
        intent_id: IntentId::new(format!("intent_{}", at.timestamp_micros())).unwrap(),
        account_id: AccountId::new(&account).unwrap(),
        instrument_id: InstrumentId::new("CATALOG_EQ").unwrap(),
        instrument_currency: Currency::new("EUR").unwrap(),
        side: Side::Buy,
        quantity: Decimal::from(2),
        kind: OrderKind::Market {
            extended_hours: false,
        },
    };
    store.order_intent(&intent, at).unwrap();
    store.release_executor_lease().unwrap();
    drop(store);
    let mut restored = open();
    let next = restored.acquire_executor_lease("worker_two", 30).unwrap();
    assert!(next > first);
    assert!(rival.assert_executor_lease("worker_one", first).is_err());
    let mut order = BrokerOrder {
        broker_order_id: "broker42".into(),
        client_intent_id: Some(intent.intent_id.clone()),
        instrument_id: intent.instrument_id.clone(),
        side: Side::Buy,
        quantity: Decimal::from(2),
        filled_quantity: Decimal::ONE,
        status: OrderStatus::PartiallyFilled,
    };
    restored.broker_order(&order, at).unwrap();
    order.filled_quantity = Decimal::from(2);
    order.status = OrderStatus::Filled;
    restored
        .broker_order(&order, at + Duration::microseconds(1))
        .unwrap();
    assert!(
        restored
            .read_document("order_intent", intent.intent_id.as_str())
            .unwrap()
            .is_some()
    );
    order.quantity = Decimal::from(3);
    assert!(restored.broker_order(&order, at).is_err());

    let event = LifecycleEvent {
        event_id: format!("event_{}", at.timestamp_micros()),
        kind: LifecycleEventKind::RiskHalt,
        account_id: AccountId::new(&account).unwrap(),
        session_id: "fixture_eur_day".into(),
        instrument_id: None,
        evidence_ids: vec![],
        text: "No order was submitted.".into(),
        occurred_at: at,
    };
    restored
        .publish_lifecycle("deployment", &event, &[123, 456], at)
        .unwrap();
    restored
        .publish_lifecycle("deployment", &event, &[123, 456], at)
        .unwrap();
    let mut outbox = PgTargetOutbox::connect(&gateway, "deployment".into(), vec![123]).unwrap();
    let delivery = outbox.claim().unwrap().expect("first chat");
    assert_eq!(
        delivery.payload_hash,
        content_hash(&delivery.payload.to_string())
    );
    outbox.finish(&delivery, true).unwrap();
    assert!(outbox.claim().unwrap().is_none());
    drop(outbox);
    let mut restarted = PgTargetOutbox::connect(&gateway, "deployment".into(), vec![123]).unwrap();
    assert!(
        restarted.claim().unwrap().is_none(),
        "acknowledged send must not repeat"
    );
    let mut second_chat =
        PgTargetOutbox::connect(&gateway, "deployment".into(), vec![456]).unwrap();
    let ambiguous = second_chat.claim().unwrap().expect("second chat");
    drop(second_chat);
    let mut restarted = PgTargetOutbox::connect(&gateway, "deployment".into(), vec![456]).unwrap();
    assert!(
        restarted.claim().unwrap().is_none(),
        "interrupted send must not be retried blindly"
    );
    restarted.finish(&ambiguous, false).unwrap();
    assert!(restarted.claim().unwrap().is_none());
    let mut changed = event.clone();
    changed.text = "changed".into();
    assert!(
        restored
            .publish_lifecycle("deployment", &changed, &[123], at)
            .is_err()
    );
    let mut raw_gateway = Client::connect(&gateway, NoTls).unwrap();
    assert!(
        raw_gateway
            .batch_execute("SELECT target.acquire_executor_lease('paper_forged','w',30)")
            .is_err()
    );
    assert!(
        raw_gateway
            .batch_execute("DELETE FROM target.telegram_outbox")
            .is_err()
    );
    restored.release_executor_lease().unwrap();
}

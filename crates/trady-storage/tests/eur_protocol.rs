use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use std::env;
use trady_application::eur_protocol::*;
use trady_storage::target::PgTargetStore;

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn durable_protocol_reserves_daily_slot_and_survives_unknown_dispatch_restart() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    trady_storage::migrate(&env::var("TRADY_MIGRATOR_DSN").unwrap()).unwrap();
    let at = Utc::now();
    let account = format!("replay_protocol_{}", at.timestamp_micros());
    let c = ProtocolContext {
        runtime_mode: trady_domain::ExecutionStage::Replay,
        policy_revision: "eur_intraday_v1".into(),
        intent_id: "one_entry".into(),
        account_id: account.clone(),
        broker_account_id: 52211463,
        instrument_id: "AIR_PA_EQ".into(),
        broker_ticker: "AIRp_EQ".into(),
        isin: "NL0000235190".into(),
        policy_hash: "a".repeat(64),
        configuration_hash: "b".repeat(64),
        evidence_ids: vec!["synthetic_fixture".into()],
        quantity: Decimal::ONE,
        entry_limit: Decimal::new(100, 0),
        initial_stop: Decimal::new(98, 0),
        target: Decimal::new(103, 0),
        created_at: at,
        flatten_at: at + Duration::minutes(1),
        max_unprotected_seconds: 5,
    };
    let dsn = env::var("TRADY_EXECUTOR_DSN").unwrap();
    let open =
        || PgTargetStore::connect(&dsn, "trady_executor", "owner".into(), account.clone()).unwrap();
    let mut store = open();
    store.acquire_executor_lease("first_worker", 120).unwrap();
    assert_eq!(
        advance(&mut store, &c, "reserve".into(), ProtocolInput::Reserve, at).unwrap(),
        ProtocolEffect::None
    );
    assert!(matches!(
        advance(
            &mut store,
            &c,
            "dispatch".into(),
            ProtocolInput::DispatchEntry,
            at
        )
        .unwrap(),
        ProtocolEffect::SubmitEntry { .. }
    ));
    assert_eq!(store.daily_entry_count(at.date_naive()).unwrap(), 1);
    drop(store);
    let mut restarted = open();
    restarted
        .acquire_executor_lease("restart_worker", 120)
        .unwrap();
    assert_eq!(
        recover(&restarted.load(&c.intent_id).unwrap())
            .unwrap()
            .phase,
        ProtocolPhase::EntryUnknown
    );
    assert!(matches!(
        advance(
            &mut restarted,
            &c,
            "dispatch".into(),
            ProtocolInput::DispatchEntry,
            at
        )
        .unwrap(),
        ProtocolEffect::RequireOperator { .. }
    ));
    let mut second = c.clone();
    second.intent_id = "second_entry".into();
    assert!(
        advance(
            &mut restarted,
            &second,
            "reserve_second".into(),
            ProtocolInput::Reserve,
            at
        )
        .is_err()
    );
    let mut raw = postgres::Client::connect(&dsn, postgres::NoTls).unwrap();
    for table in ["protocol_heads", "protocol_events", "daily_entry_slots"] {
        assert!(
            raw.batch_execute(&format!("DELETE FROM target.{table}"))
                .is_err()
        );
    }
    let analyst_dsn = env::var("TRADY_ANALYST_DSN").unwrap();
    let mut analyst =
        PgTargetStore::connect(&analyst_dsn, "trady_analyst", "owner".into(), account).unwrap();
    assert_eq!(analyst.load(&c.intent_id).unwrap().len(), 2);
    let record = recover(&analyst.load(&c.intent_id).unwrap())
        .unwrap()
        .transition("forbidden".into(), ProtocolInput::Tick, at)
        .unwrap();
    assert!(analyst.append(2, &record).is_err());
    restarted.release_executor_lease().unwrap();
}

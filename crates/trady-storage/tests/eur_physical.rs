//! Synthetic disposable DB checks, never native broker evidence.
use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use std::{env, thread, time::Duration as Wait};
use trady_application::eur_protocol::*;
use trady_storage::target::PgTargetStore;

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn physical_demo_fence_and_thirty_entries_span_aliases_owners_and_research_cycles() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    let migrator = env::var("TRADY_MIGRATOR_DSN").unwrap();
    trady_storage::migrate(&migrator).unwrap();
    let dsn = env::var("TRADY_EXECUTOR_DSN").unwrap();
    let tag = Utc::now().timestamp_micros();
    let owner = format!("physical_fixture_{tag}");
    let open = |index: usize| {
        PgTargetStore::connect(
            &dsn,
            "trady_executor",
            format!("{owner}_{index}"),
            format!("paper_physical_{tag}_{index}"),
        )
        .unwrap()
    };
    let mut first = open(0);
    first.acquire_demo_broker_lease("physical_one", 1).unwrap();
    let mut rival = open(1);
    assert!(rival.acquire_demo_broker_lease("rival", 60).is_err());
    thread::sleep(Wait::from_millis(1100));
    assert!(
        first
            .acquire_demo_broker_lease("expired_reacquire", 60)
            .is_err(),
        "expired same-session reacquire must require reconnect without stacking locks"
    );
    assert!(
        rival.acquire_demo_broker_lease("rival", 60).is_err(),
        "live physical session excludes an alias after TTL"
    );
    drop(first);
    rival.acquire_demo_broker_lease("rival", 60).unwrap();
    rival.release_executor_lease().unwrap();
    drop(rival);
    let at = Utc::now();
    let context = |index: usize| ProtocolContext {
        runtime_mode: trady_domain::ExecutionStage::Demo,
        policy_revision: "eur_intraday_30_entries_v2".into(),
        intent_id: format!("intent_{index}"),
        account_id: format!("paper_physical_{tag}_{index}"),
        broker_account_id: 52211463,
        instrument_id: "AIR_PA_EQ".into(),
        broker_ticker: "AIRp_EQ".into(),
        isin: "NL0000235190".into(),
        policy_hash: "a".repeat(64),
        configuration_hash: "b".repeat(64),
        evidence_ids: vec!["synthetic_db_only".into()],
        quantity: Decimal::ONE,
        entry_limit: Decimal::from(100),
        initial_stop: Decimal::from(99),
        target: Decimal::from(102),
        created_at: at,
        flatten_at: at + Duration::minutes(10),
        max_unprotected_seconds: 10,
        research_cycle_id: Some(format!("synthetic_cycle_{tag}_{index}")),
        entry_valid_until: None,
    };
    // A generic logical alias lease is insufficient to append a DEMO Reserve.
    let mut ordinary = open(99);
    ordinary.acquire_executor_lease("unfenced", 60).unwrap();
    assert!(
        advance(
            &mut ordinary,
            &context(99),
            "reserve".into(),
            ProtocolInput::Reserve,
            at
        )
        .is_err()
    );
    ordinary.release_executor_lease().unwrap();
    drop(ordinary);
    // An UNKNOWN intent remains a physical blocker after its backend dies.
    let mut uncertain = open(900);
    uncertain
        .acquire_demo_broker_lease("uncertain", 60)
        .unwrap();
    advance(
        &mut uncertain,
        &context(900),
        "reserve_unknown".into(),
        ProtocolInput::Reserve,
        at,
    )
    .unwrap();
    advance(
        &mut uncertain,
        &context(900),
        "dispatch_unknown".into(),
        ProtocolInput::DispatchEntry,
        at,
    )
    .unwrap();
    drop(uncertain);
    let mut other = open(901);
    other.acquire_demo_broker_lease("new_alias", 60).unwrap();
    let error = advance(
        &mut other,
        &context(901),
        "wrong_new_entry".into(),
        ProtocolInput::Reserve,
        at,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("DEMO_PHYSICAL_UNRESOLVED_INTENT")
    );
    other.release_executor_lease().unwrap();
    // Synthetic privileged review exercises exact-head metadata only; this is
    // not evidence that an UNKNOWN broker submission can be inferred absent.
    let mut admin = postgres::Client::connect(&migrator, postgres::NoTls).unwrap();
    admin.batch_execute("SET ROLE trady_owner").unwrap();
    let original_owner = format!("{owner}_900");
    admin.execute("INSERT INTO target.reviewed_external_cleanups SELECT owner_id,account_id,intent_id,revision,state_sha256,repeat('a',64),repeat('b',64),clock_timestamp(),clock_timestamp()+interval '1 day' FROM target.protocol_heads WHERE owner_id=$1",&[&original_owner]).unwrap();
    let mut frozen = open(900);
    frozen
        .acquire_demo_broker_lease("old_scope_frozen", 60)
        .unwrap();
    assert!(
        frozen
            .physical_unresolved_protocols()
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    let error = advance(
        &mut frozen,
        &context(900),
        "forbidden_old_mutation".into(),
        ProtocolInput::Tick,
        at,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("DEMO_EXTERNALLY_CLEANED_INTENT_READ_ONLY")
    );
    assert!(frozen.protocol_is_externally_cleaned("intent_900").unwrap());
    admin.execute("UPDATE target.reviewed_external_cleanups SET valid_until=clock_timestamp()-interval '1 second' WHERE owner_id=$1", &[&original_owner]).unwrap();
    assert!(frozen.protocol_is_externally_cleaned("intent_900").unwrap());
    assert!(
        !frozen
            .physical_unresolved_protocols()
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty(),
        "expired review blocks new entry again"
    );
    assert!(
        frozen
            .physical_unresolved_protocols_for_audit()
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty(),
        "read-only historical facts retain exact-head closure after review expiry"
    );
    let error = advance(
        &mut frozen,
        &context(900),
        "forbidden_expired_old_mutation".into(),
        ProtocolInput::Tick,
        at,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("DEMO_EXTERNALLY_CLEANED_INTENT_READ_ONLY"),
        "factual tombstone never re-enables old recovery after review expiry"
    );
    frozen.release_executor_lease().unwrap();
    admin
        .execute(
            "DELETE FROM target.reviewed_external_cleanups WHERE owner_id=$1",
            &[&original_owner],
        )
        .unwrap();
    let mut original = open(900);
    original
        .acquire_demo_broker_lease("exact_recovery", 60)
        .unwrap();
    advance(
        &mut original,
        &context(900),
        "review_exact_id".into(),
        ProtocolInput::ReviewedBrokerId {
            leg: OrderLeg::Entry,
            broker_id: "100900".into(),
            review_reference: "SYNTHETIC_EXACT_ID_REVIEW".into(),
        },
        at,
    )
    .unwrap();
    advance(
        &mut original,
        &context(900),
        "observe_cancelled".into(),
        ProtocolInput::Observed {
            leg: OrderLeg::Entry,
            broker_id: "100900".into(),
            broker_ticker: "AIRp_EQ".into(),
            quantity: Decimal::ONE,
            filled: Decimal::ZERO,
            status: trady_domain::OrderStatus::Cancelled,
            holding_quantity: Decimal::ZERO,
            pending_present: false,
            history_complete: true,
            stop_price: None,
        },
        at,
    )
    .unwrap();
    assert_eq!(
        recover(&original.load("intent_900").unwrap())
            .unwrap()
            .phase,
        ProtocolPhase::Rejected
    );
    original.release_executor_lease().unwrap();
    for index in 0..29 {
        let mut store = open(index);
        store.acquire_demo_broker_lease("stock_entry", 60).unwrap();
        advance(
            &mut store,
            &context(index),
            "reserve".into(),
            ProtocolInput::Reserve,
            at,
        )
        .unwrap();
        assert_eq!(
            store.daily_entry_count(at.date_naive()).unwrap(),
            (index + 2) as i64
        );
        advance(
            &mut store,
            &context(index),
            "close_unsubmitted".into(),
            ProtocolInput::RequestExit {
                reason: "SYNTHETIC_UNSUBMITTED_TEST".into(),
            },
            at,
        )
        .unwrap();
        if index == 0 {
            let mut duplicate = context(77);
            duplicate.research_cycle_id = context(0).research_cycle_id;
            // A different alias cannot acquire the physical fence concurrently.
            let mut alias = open(77);
            assert!(alias.acquire_demo_broker_lease("duplicate", 60).is_err());
            store.release_executor_lease().unwrap();
            alias.acquire_demo_broker_lease("duplicate", 60).unwrap();
            assert!(
                advance(
                    &mut alias,
                    &duplicate,
                    "reserve".into(),
                    ProtocolInput::Reserve,
                    at
                )
                .is_err(),
                "same research cycle must not be consumed twice"
            );
            alias.release_executor_lease().unwrap();
        } else {
            store.release_executor_lease().unwrap();
        }
    }
    let mut blocked = open(31);
    blocked.acquire_demo_broker_lease("blocked", 60).unwrap();
    assert_eq!(blocked.daily_entry_count(at.date_naive()).unwrap(), 30);
    assert!(
        advance(
            &mut blocked,
            &context(31),
            "reserve".into(),
            ProtocolInput::Reserve,
            at
        )
        .is_err()
    );
    blocked.release_executor_lease().unwrap();
    // The original intent can still recover and exit after all30slots are spent.
    let mut recovery = open(0);
    recovery.acquire_demo_broker_lease("recover", 60).unwrap();
    advance(
        &mut recovery,
        &context(0),
        "recover_exit".into(),
        ProtocolInput::RequestExit {
            reason: "OWNER_EXIT".into(),
        },
        at,
    )
    .unwrap();
    assert_eq!(
        recover(&recovery.load("intent_0").unwrap()).unwrap().phase,
        ProtocolPhase::Flat
    );
    assert_eq!(recovery.daily_entry_count(at.date_naive()).unwrap(), 30);
    recovery.release_executor_lease().unwrap();
    // Keep other isolated tests independent; no production database is used.
    let mut cleanup = postgres::Client::connect(&migrator, postgres::NoTls).unwrap();
    cleanup.batch_execute("SET ROLE trady_owner").unwrap();
    cleanup
        .execute(
            "DELETE FROM target.physical_entry_reservations WHERE owner_id LIKE $1",
            &[&format!("{owner}_%")],
        )
        .unwrap();
}

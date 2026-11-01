//! Invoked only by scripts/test-postgres.py against its disposable container.
use chrono::Utc;
use postgres::{Client, NoTls};
use rust_decimal::Decimal;
use std::{
    env,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};
use trady_application::{ExecutionRepository, reconcile};
use trady_domain::*;
use trady_storage::{PgExecutionRepository, migrate};

fn dsn(role: &str) -> String {
    if let Ok(value) = env::var(format!("{}_DSN", role.to_ascii_uppercase())) {
        return value;
    }
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        env::var("TRADY_TEST_PORT").unwrap()
    )
}
fn snapshot(account: &AccountId, id: &str) -> AccountSnapshot {
    AccountSnapshot {
        snapshot_id: id.into(),
        account_id: account.clone(),
        mode: Mode::Paper,
        spendable_cash: Money {
            amount: Decimal::new(50, 0),
            currency: Currency::new("EUR").unwrap(),
        },
        positions: vec![],
        orders: vec![],
        observed_at: Utc::now(),
    }
}
fn intent(account: &AccountId, id: &str) -> OrderIntent {
    OrderIntent {
        intent_id: IntentId::new(id).unwrap(),
        account_id: account.clone(),
        instrument_id: InstrumentId::new("fixture_stock").unwrap(),
        side: Side::Buy,
        quantity: Decimal::new(49, 1),
        limit_price: Decimal::TEN,
        session_id: "xnys_fixture_session".into(),
        created_at: Utc::now(),
        state: IntentState::Prepared,
        broker_order_id: None,
    }
}
fn save_flat(repo: &mut PgExecutionRepository, fence: &Fence, id: &str) {
    let r = reconcile(snapshot(&fence.account_id, id), &[], Utc::now()).unwrap();
    repo.save_reconciliation(fence, &r).unwrap();
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn durable_recovery_fencing_races_queues_and_role_boundaries() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    migrate(&dsn("trady_migrator")).unwrap();
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    let mut gateway = Client::connect(&dsn("trady_gateway"), NoTls).unwrap();
    for role in ["trady_analyst", "trady_executor", "trady_gateway"] {
        let mut c = Client::connect(&dsn(role), NoTls).unwrap();
        assert!(
            c.batch_execute("CREATE TABLE public.forbidden(id integer)")
                .is_err()
        );
        assert!(c.batch_execute("SET ROLE trady_owner").is_err());
        assert!(
            c.batch_execute(
                "INSERT INTO execution.accounts(account_id,currency) VALUES('paper_forged','EUR')"
            )
            .is_err()
        );
        assert!(
            c.batch_execute("DELETE FROM execution.order_intents")
                .is_err()
        );
        assert!(
            c.batch_execute("UPDATE execution.account_leases SET generation=100")
                .is_err()
        );
        for database in ["postgres", "paperless_fixture"] {
            assert!(
                Client::connect(
                    &dsn(role).replace("dbname=trady", &format!("dbname={database}")),
                    NoTls
                )
                .is_err()
            );
        }
    }
    // A separate application's authentication and data remain usable in the fixture server.
    let mut other = Client::connect(
        &dsn("paperless_fixture_owner").replace("dbname=trady", "dbname=paperless_fixture"),
        NoTls,
    )
    .unwrap();
    other
        .batch_execute(
            "CREATE TABLE retained(value text); INSERT INTO retained VALUES('unchanged')",
        )
        .unwrap();
    assert_eq!(
        other
            .query_one("SELECT value FROM retained", &[])
            .unwrap()
            .get::<_, String>(0),
        "unchanged"
    );
    assert!(PgExecutionRepository::connect(&dsn("postgres")).is_err());
    assert!(migrate(&dsn("trady_executor")).is_err());
    assert!(
        analyst
            .batch_execute("SELECT execution.register_paper_account('paper_forged','EUR')")
            .is_err()
    );
    assert!(analyst.batch_execute("INSERT INTO control.owner_commands(command_id,command_kind,owner_user_id,account_id,payload) VALUES('forged','CLOSE_REQUEST','owner','paper_test','{}')").is_err());
    assert!(
        gateway
            .batch_execute("INSERT INTO policy.versions(policy_version,body) VALUES(1,'{}')")
            .is_err()
    );
    assert!(analyst.batch_execute("INSERT INTO control.research_jobs(job_id,payload,producer) VALUES('forged','{}','trady_gateway')").is_err());
    analyst.batch_execute("INSERT INTO control.research_jobs(job_id,payload) VALUES('research_1','{}'); INSERT INTO research.runs(run_id,document) VALUES('run_1','{}')").unwrap();
    gateway.batch_execute("INSERT INTO control.owner_commands(command_id,command_kind,owner_user_id,account_id,payload) VALUES('owner_1','STATUS','fixture_owner','paper_test','{}')").unwrap();
    assert_eq!(
        admin
            .query_one(
                "SELECT producer FROM control.research_jobs WHERE job_id='research_1'",
                &[]
            )
            .unwrap()
            .get::<_, String>(0),
        "trady_analyst"
    );
    assert_eq!(
        admin
            .query_one(
                "SELECT producer FROM control.owner_commands WHERE command_id='owner_1'",
                &[]
            )
            .unwrap()
            .get::<_, String>(0),
        "trady_gateway"
    );
    let invalid_owners:i64=admin.query_one("SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname IN ('policy','research','execution','control','audit') AND pg_get_userbyid(c.relowner)<>'trady_owner'",&[]).unwrap().get(0);
    assert_eq!(invalid_owners, 0);

    let mut unchecked = Client::connect(&dsn("trady_executor"), NoTls).unwrap();
    let validation_account = AccountId::new("paper_sql_validation").unwrap();
    let mut validation_repo = PgExecutionRepository::connect(&dsn("trady_executor")).unwrap();
    validation_repo
        .register_paper_account(&validation_account, &Currency::new("EUR").unwrap())
        .unwrap();
    let validation_fence = validation_repo
        .acquire(
            &validation_account,
            &WorkerId::new("sql_validation").unwrap(),
            30,
        )
        .unwrap();
    save_flat(
        &mut validation_repo,
        &validation_fence,
        "sql_validation_flat",
    );
    assert!(
        unchecked
            .query_one(
                "SELECT execution.renew_lease($1,NULL,NULL,30)",
                &[&validation_account.as_str()]
            )
            .is_err()
    );
    assert!(
        unchecked
            .query_one(
                "SELECT execution.renew_lease($1,$2,NULL,30)",
                &[
                    &validation_account.as_str(),
                    &validation_fence.worker_id.as_str()
                ]
            )
            .is_err()
    );
    let original = serde_json::to_value(intent(&validation_account, "validation_intent")).unwrap();
    for key in ["quantity", "limit_price"] {
        for null_instead_of_missing in [false, true] {
            let mut malformed = original.clone();
            if null_instead_of_missing {
                malformed[key] = serde_json::Value::Null;
            } else {
                malformed.as_object_mut().unwrap().remove(key);
            }
            assert!(
                unchecked
                    .query_one(
                        "SELECT execution.reserve_paper_intent($1,$2,$3,$4)",
                        &[
                            &validation_account.as_str(),
                            &validation_fence.worker_id.as_str(),
                            &validation_fence.generation,
                            &malformed
                        ]
                    )
                    .is_err()
            );
        }
    }
    let mut extra = original.clone();
    extra["model_authority"] = serde_json::json!(true);
    assert!(
        unchecked
            .query_one(
                "SELECT execution.reserve_paper_intent($1,$2,$3,$4)",
                &[
                    &validation_account.as_str(),
                    &validation_fence.worker_id.as_str(),
                    &validation_fence.generation,
                    &extra
                ]
            )
            .is_err()
    );
    assert!(
        unchecked
            .query_one(
                "SELECT execution.reserve_paper_intent($1,NULL,NULL,$2)",
                &[&validation_account.as_str(), &original]
            )
            .is_err()
    );
    assert!(
        validation_repo
            .unresolved_intents(&validation_account)
            .unwrap()
            .is_empty()
    );

    let account = AccountId::new("paper_restart").unwrap();
    let mut repo = PgExecutionRepository::connect(&dsn("trady_executor")).unwrap();
    repo.register_paper_account(&account, &Currency::new("EUR").unwrap())
        .unwrap();
    let first = repo
        .acquire(&account, &WorkerId::new("first").unwrap(), 1)
        .unwrap();
    assert!(
        repo.acquire(&account, &WorkerId::new("competitor").unwrap(), 30)
            .is_err()
    );
    let i = intent(&account, "durable_intent");
    assert!(repo.reserve_paper_intent(&first, &i).is_err());
    save_flat(&mut repo, &first, "initial_flat");
    assert!(repo.reserve_paper_intent(&first, &i).unwrap());
    assert!(!repo.reserve_paper_intent(&first, &i).unwrap());
    let mut changed = i.clone();
    changed.quantity = Decimal::ONE;
    assert!(repo.reserve_paper_intent(&first, &changed).is_err());
    drop(repo);
    thread::sleep(Duration::from_millis(1100));
    let mut restarted = PgExecutionRepository::connect(&dsn("trady_executor")).unwrap();
    let second = restarted
        .acquire(&account, &WorkerId::new("restart").unwrap(), 30)
        .unwrap();
    assert!(second.generation > first.generation);
    assert!(restarted.renew(&first, 30).is_err());
    assert!(
        restarted
            .reserve_paper_intent(&second, &intent(&account, "premature"))
            .is_err()
    );
    let unresolved = restarted.unresolved_intents(&account).unwrap();
    assert_eq!(unresolved.len(), 1);
    let missing = reconcile(
        snapshot(&account, "missing_after_restart"),
        &unresolved,
        Utc::now(),
    )
    .unwrap();
    assert_eq!(missing.state, PositionState::RecoveryRequired);
    assert!(restarted.save_reconciliation(&first, &missing).is_err());
    restarted.save_reconciliation(&second, &missing).unwrap();
    let mut partial = snapshot(&account, "partial_fill");
    partial.spendable_cash.amount = Decimal::new(255, 1);
    partial.positions.push(Position {
        instrument_id: i.instrument_id.clone(),
        quantity: Decimal::new(245, 2),
    });
    partial.orders.push(BrokerOrder {
        broker_order_id: "broker_accepted".into(),
        client_intent_id: Some(i.intent_id.clone()),
        instrument_id: i.instrument_id.clone(),
        side: Side::Buy,
        quantity: i.quantity,
        filled_quantity: Decimal::new(245, 2),
        status: OrderStatus::PartiallyFilled,
    });
    let r = reconcile(
        partial,
        &restarted.unresolved_intents(&account).unwrap(),
        Utc::now(),
    )
    .unwrap();
    restarted.save_reconciliation(&second, &r).unwrap();
    assert_eq!(r.state, PositionState::OpenUnprotected);
    let mut filled = snapshot(&account, "full_fill");
    filled.spendable_cash.amount = Decimal::ONE;
    filled.positions.push(Position {
        instrument_id: i.instrument_id.clone(),
        quantity: i.quantity,
    });
    filled.orders.push(BrokerOrder {
        broker_order_id: "broker_accepted".into(),
        client_intent_id: Some(i.intent_id.clone()),
        instrument_id: i.instrument_id.clone(),
        side: Side::Buy,
        quantity: i.quantity,
        filled_quantity: i.quantity,
        status: OrderStatus::Filled,
    });
    let r = reconcile(
        filled,
        &restarted.unresolved_intents(&account).unwrap(),
        Utc::now(),
    )
    .unwrap();
    restarted.save_reconciliation(&second, &r).unwrap();
    restarted.save_reconciliation(&second, &r).unwrap();
    assert!(restarted.unresolved_intents(&account).unwrap().is_empty());
    assert_eq!(
        restarted.latest_reconciliation(&account).unwrap().unwrap(),
        r
    );
    let mut forged = r.clone();
    forged.snapshot.spendable_cash.amount = Decimal::new(999, 0);
    assert!(restarted.save_reconciliation(&second, &forged).is_err());
    assert!(
        restarted
            .reserve_paper_intent(&second, &intent(&account, "holding_addition"))
            .is_err()
    );
    assert_eq!(
        admin
            .query_one(
                "SELECT count(*) FROM execution.order_intents WHERE account_id=$1",
                &[&account.as_str()]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    assert_eq!(admin.query_one("SELECT count(*) FROM control.notification_outbox WHERE event_key='snapshot:paper_restart:full_fill'",&[]).unwrap().get::<_,i64>(0),1);

    // Two independent connections race for the same account's session slot.
    let race = AccountId::new("paper_race").unwrap();
    restarted
        .register_paper_account(&race, &Currency::new("EUR").unwrap())
        .unwrap();
    let race_fence = restarted
        .acquire(&race, &WorkerId::new("race_worker").unwrap(), 30)
        .unwrap();
    save_flat(&mut restarted, &race_fence, "race_flat");
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = vec![];
    for id in ["morning", "midday"] {
        let barrier = barrier.clone();
        let f = race_fence.clone();
        let i = intent(&race, id);
        let connection = dsn("trady_executor");
        threads.push(thread::spawn(move || {
            let mut r = PgExecutionRepository::connect(&connection).unwrap();
            barrier.wait();
            r.reserve_paper_intent(&f, &i)
        }));
    }
    let winners = threads
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(|r| matches!(r, Ok(true)))
        .count();
    assert_eq!(winners, 1);
    let mut executor = Client::connect(&dsn("trady_executor"), NoTls).unwrap();
    assert!(
        executor
            .batch_execute("UPDATE research.runs SET document='{}'")
            .is_err()
    );

    // Notification delivery is at least once; expired acknowledgements cannot consume a newer claim.
    let row = gateway
        .query_one(
            "SELECT * FROM control.claim_notification('gateway_first',1)",
            &[],
        )
        .unwrap();
    let key: String = row.get(0);
    let generation: i64 = row.get(2);
    assert!(
        !gateway
            .query_one(
                "SELECT control.ack_notification('wrong_worker',$1,$2)",
                &[&key, &generation]
            )
            .unwrap()
            .get::<_, bool>(0)
    );
    thread::sleep(Duration::from_millis(1100));
    assert!(
        !gateway
            .query_one(
                "SELECT control.ack_notification('gateway_first',$1,$2)",
                &[&key, &generation]
            )
            .unwrap()
            .get::<_, bool>(0)
    );
    let row = gateway
        .query_one(
            "SELECT * FROM control.claim_notification('gateway_second',30)",
            &[],
        )
        .unwrap();
    assert_eq!(row.get::<_, String>(0), key);
    let next_generation: i64 = row.get(2);
    assert!(next_generation > generation);
    assert!(
        gateway
            .query_one(
                "SELECT control.ack_notification('gateway_second',$1,$2)",
                &[&key, &next_generation]
            )
            .unwrap()
            .get::<_, bool>(0)
    );
    println!(
        "verified isolated DB privileges/authentication, immutable snapshots, partial fills, durable restart, lease fencing, racing entry slots, and leased outbox replay"
    );
}

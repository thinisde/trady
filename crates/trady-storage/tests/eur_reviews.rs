use chrono::Utc;
use serde_json::json;
use std::env;
use trady_storage::target::PgTargetStore;

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn review_handoffs_are_immutable_scoped_and_cannot_approve_or_activate() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    trady_storage::migrate(&env::var("TRADY_MIGRATOR_DSN").unwrap()).unwrap();
    let now = Utc::now();
    let account = format!("replay_reviews_{}", now.timestamp_micros());
    let analyst_dsn = env::var("TRADY_ANALYST_DSN").unwrap();
    let mut analyst = PgTargetStore::connect(
        &analyst_dsn,
        "trady_analyst",
        "review_owner".into(),
        account.clone(),
    )
    .unwrap();
    let bundle = json!({
        "schema_version":1,"bundle_id":"review_fixture","status":"PENDING_REVIEW",
        "report_ids":["daily_fixture"],"evidence_ids":["quote_fixture"],
        "hashes":{"report_0":"a".repeat(64)},
        "metrics":{"observed_trading_days":1,"performance_metrics_available":false},
        "review_questions":["Which input failures need investigation?"],
        "policy_revision":"eur_intraday_v1","configuration_hash":"b".repeat(64),
        "code_hash":"c".repeat(64),"period_start":now,"period_end":now,"generated_at":now
    });
    assert!(
        analyst
            .append_review_artifact("review_fixture", &bundle)
            .unwrap()
    );
    drop(analyst);
    let mut analyst = PgTargetStore::connect(
        &analyst_dsn,
        "trady_analyst",
        "review_owner".into(),
        account.clone(),
    )
    .unwrap();
    assert!(
        !analyst
            .append_review_artifact("review_fixture", &bundle)
            .unwrap()
    );
    assert_eq!(
        analyst.read_review_artifact("review_fixture").unwrap(),
        Some(bundle.clone())
    );
    for (key, value) in [
        ("status", json!("APPROVED")),
        ("credentials", json!({"password":"synthetic_never_real"})),
        ("mutation_tools", json!(["broker.submit"])),
        ("metrics", json!({"token":"synthetic_never_real"})),
        ("configuration_hash", json!("missing")),
        ("report_ids", json!([])),
    ] {
        let mut invalid = bundle.clone();
        invalid[key] = value;
        assert!(
            analyst
                .append_review_artifact("review_fixture", &invalid)
                .is_err(),
            "{key}"
        );
    }
    let mut rebound = bundle.clone();
    rebound["metrics"]["observed_trading_days"] = json!(2);
    assert!(
        analyst
            .append_review_artifact("review_fixture", &rebound)
            .is_err()
    );
    let mut other = PgTargetStore::connect(
        &analyst_dsn,
        "trady_analyst",
        "another_owner".into(),
        account.clone(),
    )
    .unwrap();
    assert_eq!(other.read_review_artifact("review_fixture").unwrap(), None);
    let executor_dsn = env::var("TRADY_EXECUTOR_DSN").unwrap();
    let mut executor = PgTargetStore::connect(
        &executor_dsn,
        "trady_executor",
        "review_owner".into(),
        account,
    )
    .unwrap();
    assert_eq!(
        executor.read_review_artifact("review_fixture").unwrap(),
        Some(bundle.clone())
    );
    assert!(
        executor
            .append_review_artifact("review_fixture", &bundle)
            .is_err()
    );
    for dsn in [&analyst_dsn, &executor_dsn] {
        let mut raw = postgres::Client::connect(dsn, postgres::NoTls).unwrap();
        assert!(
            raw.batch_execute("UPDATE target.review_artifacts SET document='{}'")
                .is_err()
        );
        assert!(
            raw.batch_execute("DELETE FROM target.review_artifacts")
                .is_err()
        );
    }
}

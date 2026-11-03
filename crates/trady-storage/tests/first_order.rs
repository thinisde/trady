//! PostgreSQL contract tests for the supervised first-order journal.
use chrono::{Duration, Utc};
use postgres::{Client, NoTls};
use serde_json::json;
use std::env;
use trady_domain::{AccountId, IntentId};
use trady_storage::{
    autonomous::{
        FirstOrderApprovalRecord, FirstOrderDispatchRecord, FirstOrderIntentRecord,
        FirstOrderReconciliationRecord, PgTradingJournal,
    },
    migrate,
};

fn dsn(role: &str) -> String {
    env::var(format!("{}_DSN", role.to_ascii_uppercase())).unwrap_or_else(|_| {
        format!(
            "host=127.0.0.1 port={} user={role} dbname=trady",
            env::var("TRADY_TEST_PORT").unwrap()
        )
    })
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn supervised_first_order_journal_is_hash_bound_and_role_restricted() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    migrate(&dsn("trady_migrator")).unwrap();

    let suffix = std::process::id();
    let account = AccountId::new(format!("paper_first_order_{suffix}")).unwrap();
    let intent_id = IntentId::new(format!("first_order_{suffix}")).unwrap();
    let intent_hash = "a".repeat(64);
    let approval_hash = "b".repeat(64);
    let request_hash = "c".repeat(64);
    let intent = json!({
        "schema_version": 1,
        "kind": "supervised_first_order_intent",
        "account_id": account,
        "intent_id": intent_id,
        "order_mutations": 0
    });
    let environment = "paper";
    let account_day = Utc::now().date_naive();
    let owner = "owner_first_order";
    let mut journal = PgTradingJournal::connect(&dsn("trady_executor")).unwrap();
    let prepared = journal
        .prepare_first_order(&FirstOrderIntentRecord {
            account_id: &account,
            environment,
            account_day,
            intent_id: &intent_id,
            intent_sha256: &intent_hash,
            owner_id: owner,
            intent_document: &intent,
            audit_id: "prepare_first_order_test",
        })
        .unwrap();
    assert_eq!(prepared["status"], "PREPARED");

    journal
        .record_first_order_approval(&FirstOrderApprovalRecord {
            account_id: &account,
            environment,
            account_day,
            intent_id: &intent_id,
            intent_sha256: &intent_hash,
            owner_id: owner,
            approval_sha256: &approval_hash,
            expires_at: Utc::now() + Duration::minutes(30),
            audit_id: "approve_first_order_test",
        })
        .unwrap();
    let stored = journal
        .read_first_order_approval(&account, environment, account_day, &intent_hash)
        .unwrap()
        .unwrap();
    assert_eq!(stored["status"], "APPROVED");
    assert_eq!(stored["approval_sha256"], approval_hash);

    let reserved = journal
        .reserve_first_order(
            &account,
            environment,
            account_day,
            &intent_hash,
            &approval_hash,
        )
        .unwrap();
    assert_eq!(reserved["status"], "RESERVED_FOR_DISPATCH");
    assert!(
        journal
            .reserve_first_order(
                &account,
                environment,
                account_day,
                &intent_hash,
                &approval_hash,
            )
            .is_err()
    );
    assert!(
        journal
            .reconcile_first_order(&FirstOrderReconciliationRecord {
                account_id: &account,
                environment,
                account_day,
                intent_sha256: &intent_hash,
                broker_order_id: Some("order-before-dispatch"),
                status: "RECONCILED_COMPLETE",
                document: &json!({"premature": true}),
            })
            .is_err()
    );

    journal
        .mark_first_order_dispatch(&FirstOrderDispatchRecord {
            account_id: &account,
            environment,
            account_day,
            intent_sha256: &intent_hash,
            request_sha256: &request_hash,
            status: "UNKNOWN_SUBMISSION_NO_ID",
            broker_order_id: None,
        })
        .unwrap();
    journal
        .reconcile_first_order(&FirstOrderReconciliationRecord {
            account_id: &account,
            environment,
            account_day,
            intent_sha256: &intent_hash,
            broker_order_id: None,
            status: "RECOVERY_REQUIRED",
            document: &json!({"unknown": true}),
        })
        .unwrap();
    let recovered = journal
        .read_first_order_approval(&account, environment, account_day, &intent_hash)
        .unwrap()
        .unwrap();
    assert_eq!(recovered["status"], "RECOVERY_REQUIRED");
    assert_eq!(recovered["unknown_outcome"], true);

    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    assert!(
        analyst
            .query(
                "SELECT account_id FROM execution.first_order_approvals WHERE account_id=$1",
                &[&account.as_str()],
            )
            .is_err()
    );
}

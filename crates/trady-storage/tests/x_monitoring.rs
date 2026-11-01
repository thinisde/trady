//! This test runs only on the disposable database created by scripts/test-postgres.py.
use chrono::{DateTime, Duration, Utc};
use postgres::{Client, NoTls};
use std::sync::{Arc, Barrier};
use trady_adapters::news::digest;
use trady_application::{
    research::{ResearchConfig, ResearchRepository, prepare_state},
    x_monitoring::*,
};
use trady_domain::research::*;
use trady_storage::{
    migrate,
    research::PgResearchRepository,
    x_monitoring::{PgXCostRepository, XReconciliation},
};

fn dsn(role: &str) -> String {
    if let Ok(value) = std::env::var(format!("{}_DSN", role.to_ascii_uppercase())) {
        return value;
    }
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        std::env::var("TRADY_TEST_PORT").unwrap()
    )
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn local_x_export_uses_ingestion_time_and_current_rights_without_backdating_late_edits() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let cfg: ResearchConfig =
        serde_json::from_slice(include_bytes!("../../../config/x-pilot.example.json")).unwrap();
    let mut source = cfg.sources[0].clone();
    source.id = "x_export_fixture".into();
    source.rights.store_content = true;
    source.rights.llm_processing = true;
    source.rights.terms_reference = "fixture inference permission".into();
    let mut repository = PgResearchRepository::connect(&dsn("trady_analyst")).unwrap();
    let state = prepare_state(&source, repository.state(&source.id).unwrap()).unwrap();
    let before = Utc::now();
    let mut item:SourceItem=serde_json::from_value(serde_json::json!({
        "item_id":digest("x_export_fixture:900"),"source_id":source.id,"external_id":"900","canonical_url":"https://x.com/i/status/900",
        "lineage_id":digest("x-lineage"),"content_hash":digest("original"),"parser_version":"fixture","publisher":"X user 42",
        "feed_title":null,"title":null,"summary":null,"text":"Permitted fixture evidence",
        "published_at":before-Duration::hours(1),"event_at":null,"first_seen_at":before-Duration::minutes(20),
        "fetched_at":before-Duration::minutes(1),"expires_at":before+Duration::hours(24),"language":"en",
        "rights":source.rights,"content_status":"AVAILABLE","related_urls":[],"gaps":[],
        "social":{"action":"ORIGINAL","actor_user_id":"42","original_author_id":"42","conversation_id":"900",
            "reply_to_id":null,"quoted_id":null,"reposted_id":null,"edit_history_ids":["900"],"missing_context":false,"context":[]}
    })).unwrap();
    repository
        .commit_page(
            &source.id,
            state.revision,
            &state,
            &[item.clone()],
            Utc::now(),
        )
        .unwrap();
    // Publication/fetch timestamps are earlier, but the database had not ingested it at this cutoff.
    assert!(
        repository
            .export_x_evidence(Some(&source.id), before, Utc::now(), 10)
            .unwrap()
            .is_empty()
    );
    let cutoff = Utc::now();
    let exported = repository
        .export_x_evidence(Some(&source.id), cutoff, Utc::now(), 10)
        .unwrap();
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0]["training_permitted"], false);
    assert!(exported[0]["verified_at"].is_null());
    assert!(exported[0]["feature_ready_at"].is_null());
    assert_eq!(exported[0]["coverage"]["transport"], "X_RECENT_SEARCH");
    item.text = Some("Later correction".into());
    item.content_hash = digest("corrected");
    let current = repository.state(&source.id).unwrap();
    repository
        .commit_page(&source.id, current.revision, &current, &[item], Utc::now())
        .unwrap();
    assert!(
        repository
            .export_x_evidence(Some(&source.id), cutoff, Utc::now(), 10)
            .unwrap()
            .is_empty()
    );
    // A transaction spanning a cutoff cannot expose its pre-commit row-write time.
    let mut raw = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    let mut tx = raw.transaction().unwrap();
    tx.execute("UPDATE research.source_items SET document=jsonb_set(document,'{text}','\"Transaction correction\"'::jsonb) WHERE source_id=$1",&[&source.id]).unwrap();
    let during = Utc::now();
    tx.commit().unwrap();
    assert!(
        repository
            .export_x_evidence(Some(&source.id), Utc::now(), Utc::now(), 10)
            .unwrap()
            .is_empty()
    );
    raw.execute(
        "UPDATE research.source_items SET ingested_at=clock_timestamp() WHERE source_id=$1",
        &[&source.id],
    )
    .unwrap();
    assert!(
        repository
            .export_x_evidence(Some(&source.id), during, Utc::now(), 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repository
            .export_x_evidence(Some(&source.id), Utc::now(), Utc::now(), 10)
            .unwrap()
            .len(),
        1
    );
    source.rights.llm_processing = false;
    let state = prepare_state(&source, repository.state(&source.id).unwrap()).unwrap();
    repository
        .commit_page(&source.id, state.revision, &state, &[], Utc::now())
        .unwrap();
    assert!(
        repository
            .export_x_evidence(Some(&source.id), Utc::now(), Utc::now(), 10)
            .unwrap()
            .is_empty()
    );
}
fn policy(scope: &str) -> XBudgetPolicy {
    let cfg: ResearchConfig =
        serde_json::from_slice(include_bytes!("../../../config/x-pilot.example.json")).unwrap();
    let mut p = cfg.x_budget.unwrap();
    p.enabled = true;
    p.billing_scope = scope.into();
    let now = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    p.cycle_start = now - Duration::hours(1);
    p.cycle_end = now + Duration::days(29);
    p.approval_reference = "fixture owner".into();
    p.entitlement_reference = "fixture access".into();
    p.provider_limit_reference = "fixture provider cap".into();
    p
}
fn scope(source: &str) -> XCostScope {
    XCostScope {
        source_id: source.into(),
        registry_hash: "a".repeat(64),
        rule_hash: Some("b".repeat(64)),
    }
}
fn posts(count: u32) -> XCostPlan {
    XCostPlan {
        ordinary: XResources {
            posts: count,
            ..Default::default()
        },
        support: XResources::default(),
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn shared_budget_survives_restarts_concurrency_revisions_failures_and_provider_reconciliation() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    migrate(&dsn("trady_migrator")).unwrap();
    assert!(PgXCostRepository::connect(&dsn("trady_executor")).is_err());
    assert!(PgXCostRepository::connect(&dsn("trady_gateway")).is_err());
    let mut p = policy("x_shared_fixture");
    p.ceiling_microusd = 100_000;
    p.ordinary_microusd = 80_000;
    p.support_microusd = 20_000;
    let mut ledger = PgXCostRepository::connect(&dsn("trady_analyst")).unwrap();
    let first = ledger
        .reserve(&p, &scope("first_account"), posts(10), Utc::now())
        .unwrap()
        .unwrap();
    // The initial pessimistic reservation persists even if the process disappears.
    drop(ledger);
    let mut ledger = PgXCostRepository::connect(&dsn("trady_analyst")).unwrap();
    assert!(
        ledger
            .reserve(&p, &scope("other_account"), posts(10), Utc::now())
            .unwrap()
            .is_none()
    );
    ledger.settle(first, Some(posts(2)), "OBSERVED").unwrap();
    ledger.settle(first, Some(posts(2)), "OBSERVED").unwrap();
    assert!(ledger.settle(first, Some(posts(1)), "OBSERVED").is_err());
    let barrier = Arc::new(Barrier::new(3));
    let jobs: Vec<_> = (0..2)
        .map(|n| {
            let p = p.clone();
            let barrier = barrier.clone();
            let dsn = dsn("trady_analyst");
            std::thread::spawn(move || {
                let mut r = PgXCostRepository::connect(&dsn).unwrap();
                barrier.wait();
                r.reserve(
                    &p,
                    &scope(&format!("concurrent_{n}")),
                    posts(10),
                    Utc::now(),
                )
                .unwrap()
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<_> = jobs.into_iter().map(|j| j.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|id| id.is_some()).count(), 1);
    let pending = results.into_iter().flatten().next().unwrap();
    ledger.settle(pending, None, "UNKNOWN_CONSUMPTION").unwrap();
    let status = ledger.status(&p).unwrap();
    assert_eq!(status["accounting"]["ordinary_microusd"], 60_000);
    let mut changed = p.clone();
    changed.version += 1;
    assert!(
        ledger
            .reserve(&changed, &scope("changed_rule"), posts(1), Utc::now())
            .is_err()
    );
    let mut overlapping = p.clone();
    overlapping.cycle_start += Duration::minutes(1);
    assert!(
        ledger
            .reserve(&overlapping, &scope("changed_cycle"), posts(1), Utc::now())
            .is_err()
    );
    // A provider total plus uncertain/in-flight work must not free locally held resources.
    ledger
        .reconcile(
            &p,
            &XReconciliation {
                provider_total_microusd: 75_000,
                as_of: Utc::now(),
                evidence_reference: "fixture console report".into(),
            },
        )
        .unwrap();
    assert!(
        ledger
            .reserve(&p, &scope("after_reconciliation"), posts(1), Utc::now())
            .unwrap()
            .is_none()
    );
    assert!(
        ledger
            .reconcile(
                &p,
                &XReconciliation {
                    provider_total_microusd: 1,
                    as_of: Utc::now(),
                    evidence_reference: "regression".into()
                }
            )
            .is_err()
    );
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    assert_eq!(analyst.query_one("SELECT count(*)::bigint FROM research.x_cost_reconciliations WHERE billing_scope=$1",&[&p.billing_scope]).unwrap().get::<_,i64>(0),1);
    let mut gateway = Client::connect(&dsn("trady_gateway"), NoTls).unwrap();
    assert!(
        gateway
            .query("SELECT * FROM research.x_cost_reservations", &[])
            .is_err()
    );

    let policy = policy("x_settlement_invariants_fixture");
    let id = ledger
        .reserve(&policy, &scope("settlement_boundary"), posts(1), Utc::now())
        .unwrap()
        .unwrap();
    assert!(
        ledger
            .settle(id, Some(posts(0)), "UNKNOWN_CONSUMPTION")
            .is_err()
    );
    assert_eq!(
        ledger.status(&policy).unwrap()["accounting"]["ordinary_microusd"],
        5_000
    );
    // Storage derives the overrun from the saved bound, even if a caller labels it OBSERVED.
    ledger.settle(id, Some(posts(2)), "OBSERVED").unwrap();
    ledger.settle(id, Some(posts(2)), "OBSERVED").unwrap();
    assert_eq!(
        ledger.status(&policy).unwrap()["accounting"]["blocked"],
        true
    );
    assert!(
        ledger
            .reserve(&policy, &scope("after_overrun"), posts(1), Utc::now())
            .unwrap()
            .is_none()
    );
}

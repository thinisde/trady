//! Disposable PostgreSQL + one-shot HTTP fixtures only. No model or external send.
use chrono::{DateTime, Duration, Utc};
use postgres::{Client, NoTls};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::TcpListener,
    thread,
};
use trady_adapters::finbert::{FinBertClient, FinBertConfig};
use trady_application::{
    harness::{Scope, content_hash, new_id},
    research::*,
    research_jobs::*,
    telegram::gateway::*,
};
use trady_domain::{Mode, research::*, sentiment::FINBERT_REVISION};
use trady_storage::{migrate, research::PgResearchRepository, research_jobs::PgResearchJobs};

fn dsn(role: &str) -> String {
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        std::env::var("TRADY_TEST_PORT").unwrap()
    )
}
fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap()
}
fn isolated() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
}
fn policy() -> JobPolicy {
    JobPolicy {
        scope: Scope {
            owner_id: new_id("owner"),
            account_id: new_id("paper_jobs"),
            policy_version: 1,
        },
        mode: Mode::Paper,
        telegram_user_id: 12345,
        bot_id: 98765,
        source_ids: BTreeSet::from([
            "fixture_jobs_primary".into(),
            "fixture_jobs_independent".into(),
        ]),
        instrument_ids: BTreeSet::from(["stock_fixture".into()]),
        registry_hash: content_hash("offline registry fixture"),
        sentiment_revision: Some(FINBERT_REVISION.into()),
        daily_job_limit: 30,
        pending_job_limit: 10,
        enabled: true,
    }
}
fn request(p: &JobPolicy, update_id: i64, command: GatewayCommand) -> GatewayRequest {
    GatewayRequest {
        bot_id: p.bot_id,
        update_id,
        sender_id: p.telegram_user_id,
        chat_id: p.telegram_user_id,
        scope: p.scope.clone(),
        command,
    }
}
fn source(id: &str) -> SourceConfig {
    SourceConfig {
        id: id.into(),
        kind: SourceKind::PublisherRss,
        enabled: true,
        url: "https://example.com/feed".into(),
        allowed_hosts: vec!["example.com".into()],
        credential_env: None,
        interval_seconds: 60,
        daily_request_limit: 100,
        query: None,
        x_account: None,
        x_rule: None,
        rights: SourceRights {
            store_content: true,
            llm_processing: true,
            retention_hours: 24,
            terms_reference: "scoped offline fixture license".into(),
        },
    }
}
fn persist_item(
    repo: &mut PgResearchRepository,
    source: &SourceConfig,
    at: DateTime<Utc>,
) -> SourceItem {
    let old = repo.state(&source.id).unwrap();
    let state = prepare_state(source, old.clone()).unwrap();
    let text = format!("Offline {} issuer fixture", source.id);
    let item = SourceItem {
        item_id: content_hash(&format!("{}:{}", source.id, at)),
        source_id: source.id.clone(),
        external_id: new_id("fixture"),
        canonical_url: format!(
            "https://example.com/{}/{}",
            source.id,
            at.timestamp_micros()
        ),
        lineage_id: content_hash(&source.id),
        content_hash: content_hash(&text),
        parser_version: "fixture-1".into(),
        publisher: source.id.clone(),
        feed_title: None,
        title: Some(text.clone()),
        summary: None,
        text: Some(text),
        published_at: Some(at - Duration::minutes(1)),
        event_at: Some(at - Duration::minutes(1)),
        first_seen_at: at,
        fetched_at: at,
        expires_at: at + Duration::hours(2),
        language: Some("en".into()),
        rights: source.rights.clone(),
        content_status: ContentStatus::Available,
        social: None,
        related_urls: vec![],
        gaps: vec![],
    };
    repo.commit_page(
        &source.id,
        old.revision,
        &state,
        std::slice::from_ref(&item),
        at,
    )
    .unwrap();
    item
}
fn sidecar_fixture() -> (FinBertClient, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let (offset, length) = loop {
            let mut buf = [0; 4096];
            let read = stream.read(&mut buf).unwrap();
            assert!(read > 0);
            bytes.extend_from_slice(&buf[..read]);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
                assert!(head.starts_with("POST /v1/sentiment "));
                let length = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|s| s.parse::<usize>().unwrap())
                    })
                    .unwrap();
                break (end + 4, length);
            }
        };
        while bytes.len() < offset + length {
            let mut buf = [0; 4096];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
        }
        let input: Value = serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
        let documents = input["documents"].as_array().unwrap();
        let predictions:Vec<_>=documents.iter().map(|d|json!({"item_id":d["item_id"],"text_sha256":d["text_sha256"],"probabilities":{"positive":"0.7","negative":"0.1","neutral":"0.2"},"truncated":false})).collect();
        let body=json!({"schema_version":1,"model_id":"ProsusAI/finbert","model_revision":FINBERT_REVISION,"predictions":predictions}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        documents.len()
    });
    (
        FinBertClient::new(FinBertConfig {
            config_version: 1,
            enabled: true,
            endpoint: format!("http://{address}/"),
            model_revision: FINBERT_REVISION.into(),
            timeout_seconds: 3,
        })
        .unwrap(),
        handle,
    )
}

#[test]
#[ignore = "requires isolated PostgreSQL; run through root disposable PostgreSQL runner"]
fn durable_jobs_replay_budget_fencing_scope_and_real_finbert_client_fixture() {
    isolated();
    let p = policy();
    let at = now();
    let mut analyst = PgResearchJobs::connect_analyst(&dsn("trady_analyst")).unwrap();
    analyst.install_policy(&p).unwrap();
    let mut gateway = PgResearchJobs::connect_gateway(&dsn("trady_gateway")).unwrap();
    assert!(PgResearchJobs::connect_analyst(&dsn("trady_gateway")).is_err());
    let work = Work::Research {
        instrument_id: Some("stock_fixture".into()),
    };
    let a = gateway.enqueue(&p.scope, "fixture_replay", &work).unwrap();
    assert_eq!(
        a.id,
        gateway
            .enqueue(&p.scope, "fixture_replay", &work)
            .unwrap()
            .id
    );
    assert!(
        gateway
            .enqueue(
                &p.scope,
                "fixture_replay",
                &Work::Research {
                    instrument_id: None
                }
            )
            .is_err()
    );
    let first = analyst
        .claim(&p.scope, "worker_fixture_a", 30)
        .unwrap()
        .unwrap();
    assert!(
        analyst
            .claim(&p.scope, "worker_fixture_b", 30)
            .unwrap()
            .is_none()
    );
    let mut raw = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    raw.execute("UPDATE control.research_work SET lease_until=clock_timestamp()-interval '1 second' WHERE job_id=$1",&[&a.id]).unwrap();
    let second = analyst
        .claim(&p.scope, "worker_fixture_b", 30)
        .unwrap()
        .unwrap();
    assert!(second.generation > first.generation);
    assert!(analyst.complete(&first, &json!({"stale":true})).is_err());
    analyst.complete(&second, &json!({"fixture":true})).unwrap();
    let mut other = p.clone();
    other.scope.account_id = new_id("live_jobs");
    other.mode = Mode::Live;
    analyst.install_policy(&other).unwrap();
    assert!(gateway.get_job(&other.scope, &a.id).unwrap().is_none());
    let mut observations = PgResearchRepository::connect(&dsn("trady_analyst")).unwrap();
    let primary = persist_item(&mut observations, &source("fixture_jobs_primary"), at);
    let secondary = persist_item(&mut observations, &source("fixture_jobs_independent"), at);
    let verifications = vec![&primary, &secondary]
        .into_iter()
        .map(|i| Verification {
            item_id: i.item_id.clone(),
            instrument_id: "stock_fixture".into(),
            verification_reference: "explicit offline trusted-verifier fixture".into(),
            issuer_mapping_verified: true,
            primary_verified: i.source_id == "fixture_jobs_primary",
            reporting_origin: Some(i.source_id.clone()),
            origin_verified: true,
            ambiguities: vec![],
        })
        .collect();
    let packet = analyst
        .publish_verified_packet(
            &p.scope,
            "stock_fixture",
            Some("profile_fixture".into()),
            verifications,
            vec![ClaimReview {
                claim_id: "claim_fixture".into(),
                supporting_item_ids: vec![primary.item_id, secondary.item_id],
                contradicting_item_ids: vec![],
                ambiguities: vec![],
            }],
            at + Duration::hours(1),
            at,
        )
        .unwrap();
    assert_eq!(analyst.queue_pending_sentiment(&p.scope, at).unwrap(), 1);
    let lease = analyst
        .claim(&p.scope, "fixture_sentiment_worker", 30)
        .unwrap()
        .unwrap();
    let (client, server) = sidecar_fixture();
    let result = score_sentiment_job(&mut analyst, &client, &lease, now()).unwrap();
    assert_eq!(server.join().unwrap(), 2);
    let id = result["assessment_id"].as_str().unwrap();
    let reference = analyst
        .checked_readiness_reference(&p.scope, id, now())
        .unwrap();
    let mut executor = PgResearchJobs::connect_executor(&dsn("trady_executor")).unwrap();
    assert!(executor.reference_current(&reference).unwrap());
    let mut revoked = source("fixture_jobs_primary");
    revoked.rights.llm_processing = false;
    let old = observations.state(&revoked.id).unwrap();
    let state = prepare_state(&revoked, old.clone()).unwrap();
    observations
        .commit_page(&revoked.id, old.revision, &state, &[], now())
        .unwrap();
    assert!(!executor.reference_current(&reference).unwrap());
    assert!(analyst.packet(&p.scope, &packet.id, now()).is_err());
    let mut low = p.clone();
    low.scope.account_id = new_id("paper_budget");
    low.daily_job_limit = 1;
    analyst.install_policy(&low).unwrap();
    let job = gateway.enqueue(&low.scope, "one", &work).unwrap();
    let lease = analyst
        .claim(&low.scope, "budget_fixture", 30)
        .unwrap()
        .unwrap();
    assert_eq!(job.id, lease.job.id);
    analyst.complete(&lease, &json!({})).unwrap();
    assert!(
        gateway
            .enqueue(
                &low.scope,
                "two",
                &Work::Research {
                    instrument_id: None
                }
            )
            .is_err()
    );
    let mut raw = Client::connect(&dsn("trady_gateway"), NoTls).unwrap();
    assert!(
        raw.batch_execute("UPDATE research.model_sessions SET revision=revision+1")
            .is_err()
    );
    assert!(
        raw.batch_execute("UPDATE execution.accounts SET recovery_required=false")
            .is_err()
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL; run through root disposable PostgreSQL runner"]
fn gateway_confirmation_is_owner_bound_replay_safe_and_control_results_are_durable_notifications() {
    isolated();
    let p = policy();
    let mut analyst = PgResearchJobs::connect_analyst(&dsn("trady_analyst")).unwrap();
    analyst.install_policy(&p).unwrap();
    let mut gateway = PgResearchJobs::connect_gateway(&dsn("trady_gateway")).unwrap();
    let mut executor = PgResearchJobs::connect_executor(&dsn("trady_executor")).unwrap();
    // Distinct bot per fixture avoids colliding with other isolated tests.
    let mut p = p;
    p.bot_id = 123456;
    analyst.install_policy(&p).unwrap();
    let context = OwnerControlContext {
        scope: p.scope.clone(),
        mode: p.mode,
        plan_revision: Some(content_hash("plan_fixture")),
        holding_revision: Some(content_hash("holding_fixture")),
        holding_id: Some("position_fixture".into()),
        instrument_id: Some("stock_fixture".into()),
        quantity: Some("2.5".into()),
        observed_at: now(),
    };
    executor.publish_control_context(&context).unwrap();
    let req = request(&p, 1, GatewayCommand::ClosePosition);
    let preview = gateway.accept_gateway(&req, None, "", now()).unwrap();
    let token = preview.split("/confirm ").last().unwrap().to_owned();
    assert!(token.starts_with("confirmation_"));
    assert!(
        !gateway
            .accept_gateway(&req, None, "", now())
            .unwrap()
            .contains(&token)
    );
    let confirm = request(&p, 2, GatewayCommand::Confirm(token.clone()));
    let result = gateway.accept_gateway(&confirm, None, "", now()).unwrap();
    assert!(result.contains("REQUESTED"));
    assert_eq!(
        result,
        gateway.accept_gateway(&confirm, None, "", now()).unwrap()
    );
    let controls = executor
        .pending_owner_controls(&p.scope.account_id, 10)
        .unwrap();
    assert_eq!(controls.len(), 1);
    assert_eq!(controls[0].context_hash, context.binding_hash().unwrap());
    executor
        .record_control_result(
            &controls[0].id,
            "ACCEPTED",
            &json!({"reason":"offline executor acceptance fixture"}),
        )
        .unwrap();
    let pending = gateway.pending_deliveries(p.bot_id, 20).unwrap();
    assert!(pending.iter().any(|d| d.text.contains("ACCEPTED")));
    let replay = request(&p, 3, GatewayCommand::Confirm(token));
    assert!(
        gateway
            .accept_gateway(&replay, None, "", now())
            .unwrap()
            .contains("already ACCEPTED")
    );
    let stale = request(&p, 4, GatewayCommand::ResumeEntries);
    let preview = gateway.accept_gateway(&stale, None, "", now()).unwrap();
    let token = preview.split("/confirm ").last().unwrap().to_owned();
    let mut changed = context.clone();
    changed.quantity = Some("2.6".into());
    changed.observed_at = now();
    executor.publish_control_context(&changed).unwrap();
    let stale_confirm = request(&p, 5, GatewayCommand::Confirm(token));
    assert!(
        !gateway
            .accept_gateway(&stale_confirm, None, "", now())
            .unwrap()
            .contains(" REQUESTED")
    );
    let pause = request(&p, 6, GatewayCommand::PauseEntries);
    assert!(
        gateway
            .accept_gateway(&pause, None, "", now())
            .unwrap()
            .contains("REQUESTED")
    );
    let pending = executor
        .pending_owner_controls(&p.scope.account_id, 10)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, OwnerControlKind::PauseEntries);
    assert!(pending[0].context.holding_id.is_none());
}

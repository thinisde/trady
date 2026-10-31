//! Run only through the disposable PostgreSQL runner, never an existing server.
use chrono::{DateTime, Duration, Utc};
use postgres::{Client, NoTls};
use trady_adapters::news::{digest, parse_feed};
use trady_application::research::*;
use trady_domain::research::*;
use trady_storage::{migrate, research::PgResearchRepository};

fn dsn(role: &str) -> String {
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        std::env::var("TRADY_TEST_PORT").unwrap()
    )
}
fn source() -> SourceConfig {
    SourceConfig {
        x_rule: None,
        id: "fixture_news_storage".into(),
        kind: SourceKind::PublisherRss,
        enabled: true,
        url: "https://example.com/feed".into(),
        allowed_hosts: vec!["example.com".into()],
        credential_env: None,
        interval_seconds: 60,
        daily_request_limit: 100,
        query: None,
        x_account: None,
        rights: SourceRights {
            store_content: true,
            llm_processing: false,
            retention_hours: 24,
            terms_reference: "fixture license".into(),
        },
    }
}
fn feed(title: &str) -> String {
    format!(
        "<rss><channel><title>Issuer</title><item><guid>release-1</guid><title>{title}</title><link>https://example.com/release</link><description>Permitted detail</description><pubDate>Fri, 30 Oct 2026 12:00:00 GMT</pubDate></item></channel></rss>"
    )
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn durable_collection_replay_atomicity_corrections_retention_and_privileges() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    migrate(&dsn("trady_migrator")).unwrap();
    assert!(PgResearchRepository::connect(&dsn("trady_executor")).is_err());
    assert!(PgResearchRepository::connect(&dsn("trady_gateway")).is_err());
    let mut source = source();
    let now = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let mut repository = PgResearchRepository::connect(&dsn("trady_analyst")).unwrap();
    let mut state = prepare_state(&source, repository.state(&source.id).unwrap()).unwrap();
    state.window_start = Some(now - Duration::hours(1));
    state.window_end = Some(now);
    state.next_token = Some("page-two".into());
    state.last_status = Some(CollectionStatus::Partial);
    let page = parse_feed(&source, feed("Original claim").as_bytes(), 5, now).unwrap();
    let item_id = page.items[0].item_id.clone();
    assert_eq!(
        repository
            .commit_page(&source.id, 0, &state, &page.items, now)
            .unwrap(),
        1
    );
    drop(repository);
    let mut repository = PgResearchRepository::connect(&dsn("trady_analyst")).unwrap();
    let recovered = repository.state(&source.id).unwrap();
    assert_eq!(recovered.next_token.as_deref(), Some("page-two"));
    assert!(recovered.watermark.is_none());
    assert!(
        repository
            .commit_page(&source.id, 0, &state, &[], now)
            .is_err()
    );
    assert_eq!(
        repository
            .commit_page(&source.id, recovered.revision, &recovered, &page.items, now)
            .unwrap(),
        0
    );
    let items = repository
        .read_items(Some(&source.id), 10, now, false)
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title.as_deref(), Some("Original claim"));
    let projected = repository
        .read_items(Some(&source.id), 10, now, true)
        .unwrap();
    assert!(projected[0].title.is_none());
    assert!(projected[0].summary.is_none());

    let corrected = parse_feed(
        &source,
        feed("Corrected claim").as_bytes(),
        5,
        now + Duration::seconds(1),
    )
    .unwrap();
    let state = repository.state(&source.id).unwrap();
    assert_eq!(
        repository
            .commit_page(
                &source.id,
                state.revision,
                &state,
                &corrected.items,
                now + Duration::seconds(1)
            )
            .unwrap(),
        1
    );
    let current = repository
        .read_items(Some(&source.id), 10, now + Duration::seconds(1), false)
        .unwrap();
    assert_eq!(current[0].title.as_deref(), Some("Corrected claim"));
    assert_eq!(current[0].first_seen_at, page.items[0].first_seen_at);
    assert_eq!(current[0].expires_at, page.items[0].expires_at); // Replay cannot extend retention.
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    assert_eq!(
        admin
            .query_one(
                "SELECT count(*) FROM research.source_item_revisions WHERE item_id=$1",
                &[&item_id]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    assert_eq!(
        admin
            .query_one(
                "SELECT count(*) FROM research.source_invalidations WHERE item_id=$1",
                &[&item_id]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );

    // An invalid item after a valid candidate cannot partially commit or move its cursor.
    let mut invalid = corrected.items.clone();
    invalid[0].source_id = "wrong_source".into();
    let state = repository.state(&source.id).unwrap();
    assert!(
        repository
            .commit_page(
                &source.id,
                state.revision,
                &state,
                &invalid,
                now + Duration::seconds(2)
            )
            .is_err()
    );
    assert_eq!(repository.state(&source.id).unwrap(), state);

    // Rights revocation removes current and historical text within the state transaction.
    source.rights.store_content = false;
    let state = prepare_state(&source, repository.state(&source.id).unwrap()).unwrap();
    repository
        .commit_page(
            &source.id,
            state.revision,
            &state,
            &[],
            now + Duration::seconds(2),
        )
        .unwrap();
    for table in ["source_items", "source_item_revisions"] {
        assert_eq!(admin.query_one(&format!("SELECT count(*) FROM research.{table} WHERE item_id=$1 AND (document->>'title' IS NOT NULL OR document->>'summary' IS NOT NULL)"),&[&item_id]).unwrap().get::<_,i64>(0),0);
    }
    for role in ["trady_executor", "trady_gateway"] {
        let mut client = Client::connect(&dsn(role), NoTls).unwrap();
        assert!(
            client
                .query("SELECT * FROM research.source_items", &[])
                .is_err()
        );
        assert!(
            client
                .execute("UPDATE research.source_states SET revision=999", &[])
                .is_err()
        );
    }
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    assert!(
        analyst
            .execute("DELETE FROM research.source_items", &[])
            .is_err()
    );
    assert!(
        analyst
            .execute("UPDATE execution.accounts SET recovery_required=false", &[])
            .is_err()
    );

    // Shortened retention applies to already stored content, including its JSON deadline.
    let mut short_source = source.clone();
    short_source.id = "fixture_short_retention".into();
    short_source.rights.store_content = true;
    let earlier = now - Duration::hours(2);
    let short_page = parse_feed(
        &short_source,
        feed("Short retention claim").as_bytes(),
        5,
        earlier,
    )
    .unwrap();
    let state = prepare_state(&short_source, repository.state(&short_source.id).unwrap()).unwrap();
    repository
        .commit_page(
            &short_source.id,
            state.revision,
            &state,
            &short_page.items,
            now,
        )
        .unwrap();
    short_source.rights.retention_hours = 1;
    let state = prepare_state(&short_source, repository.state(&short_source.id).unwrap()).unwrap();
    repository
        .commit_page(&short_source.id, state.revision, &state, &[], now)
        .unwrap();
    let items = repository
        .read_items(Some(&short_source.id), 5, now, false)
        .unwrap();
    assert!(items[0].title.is_none());
    assert_eq!(items[0].expires_at, earlier + Duration::hours(1));
    // A fresh provider response cannot restore content after its original deadline.
    let refetched = parse_feed(
        &short_source,
        feed("Short retention claim").as_bytes(),
        5,
        now,
    )
    .unwrap();
    let state = repository.state(&short_source.id).unwrap();
    repository
        .commit_page(
            &short_source.id,
            state.revision,
            &state,
            &refetched.items,
            now,
        )
        .unwrap();
    let stored = repository
        .read_item(&refetched.items[0].item_id, now, false)
        .unwrap()
        .unwrap();
    assert!(stored.title.is_none());
    assert_eq!(stored.expires_at, earlier + Duration::hours(1));

    // New X edit IDs invalidate older versions and remove expanded context quotations.
    let mut x_source = source.clone();
    x_source.id = "fixture_x_storage".into();
    x_source.kind = SourceKind::XLookup;
    x_source.rights.store_content = true;
    let mut old = page.items[0].clone();
    old.source_id = x_source.id.clone();
    old.external_id = "101".into();
    old.item_id = digest("fixture_x_storage:101");
    old.text = Some("Old X statement".into());
    old.canonical_url = "https://x.com/i/status/101".into();
    old.content_hash = digest("old X fixture");
    old.content_status = ContentStatus::Available;
    old.rights = x_source.rights.clone();
    old.social = Some(SocialObservation {
        action: SocialAction::Quote,
        actor_user_id: "42".into(),
        original_author_id: Some("84".into()),
        conversation_id: None,
        reply_to_id: None,
        quoted_id: Some("100".into()),
        reposted_id: None,
        edit_history_ids: vec!["101".into()],
        missing_context: false,
        context: vec![ReferencedPost {
            post_id: "100".into(),
            author_id: "84".into(),
            text: Some("Context quotation".into()),
            published_at: now,
        }],
    });
    let state = prepare_state(&x_source, repository.state(&x_source.id).unwrap()).unwrap();
    repository
        .commit_page(&x_source.id, state.revision, &state, &[old.clone()], now)
        .unwrap();
    let mut new = old.clone();
    new.external_id = "102".into();
    new.item_id = digest("fixture_x_storage:102");
    new.content_hash = digest("new X fixture");
    new.canonical_url = "https://x.com/i/status/102".into();
    new.text = Some("Corrected X statement".into());
    new.social.as_mut().unwrap().edit_history_ids = vec!["101".into(), "102".into()];
    let new_id = new.item_id.clone();
    let state = repository.state(&x_source.id).unwrap();
    repository
        .commit_page(&x_source.id, state.revision, &state, &[new], now)
        .unwrap();
    let items = repository
        .read_items(Some(&x_source.id), 10, now, false)
        .unwrap();
    let old = items.iter().find(|i| i.external_id == "101").unwrap();
    assert_eq!(old.content_status, ContentStatus::Superseded);
    assert!(old.text.is_none());
    assert!(old.social.as_ref().unwrap().context[0].text.is_none());
    // An older version discovered by another collector cannot resurrect the edited text.
    let mut other_source = x_source.clone();
    other_source.id = "fixture_x_other_collector".into();
    let mut replay = page.items[0].clone();
    replay.source_id = other_source.id.clone();
    replay.external_id = "101".into();
    replay.item_id = digest("fixture_x_other_collector:101");
    replay.social = old.social.clone();
    replay.content_status = ContentStatus::Available;
    replay.text = Some("Late old text".into());
    replay.rights = other_source.rights.clone();
    let state = prepare_state(&other_source, repository.state(&other_source.id).unwrap()).unwrap();
    repository
        .commit_page(&other_source.id, state.revision, &state, &[replay], now)
        .unwrap();
    let items = repository
        .read_items(Some(&other_source.id), 5, now, false)
        .unwrap();
    assert!(items[0].text.is_none());
    assert_eq!(items[0].content_status, ContentStatus::Superseded);

    // Explicit unavailability removes expanded copies from current and historical records.
    let mut parent = repository.read_item(&new_id, now, false).unwrap().unwrap();
    parent.item_id = digest("fixture_x_storage:100");
    parent.external_id = "100".into();
    parent.content_hash = digest("parent fixture");
    parent.social.as_mut().unwrap().edit_history_ids = vec!["100".into()];
    parent.social.as_mut().unwrap().context.clear();
    let state = repository.state(&x_source.id).unwrap();
    repository
        .commit_page(&x_source.id, state.revision, &state, &[parent.clone()], now)
        .unwrap();
    let mut amended = repository.read_item(&new_id, now, false).unwrap().unwrap();
    amended.content_hash = digest("amended quote fixture");
    amended.text = Some("Updated quote introduction".into());
    let state = repository.state(&x_source.id).unwrap();
    repository
        .commit_page(&x_source.id, state.revision, &state, &[amended], now)
        .unwrap();
    assert_eq!(repository.mark_x_unavailable("100", now).unwrap(), 1);
    assert_eq!(repository.mark_x_unavailable("999", now).unwrap(), 0);
    let parent = repository
        .read_item(&parent.item_id, now, false)
        .unwrap()
        .unwrap();
    assert_eq!(parent.content_status, ContentStatus::Unavailable);
    assert!(parent.text.is_none());
    let mut unavailable = repository.read_item(&new_id, now, false).unwrap().unwrap();
    assert!(unavailable.text.is_some());
    assert!(unavailable.social.as_ref().unwrap().missing_context);
    assert!(
        unavailable.social.as_ref().unwrap().context[0]
            .text
            .is_none()
    );
    assert_eq!(admin.query_one("SELECT count(*) FROM research.source_item_revisions WHERE item_id=$1 AND document->'social'->'context' @? '$[*].text ? (@ != null)'", &[&new_id]).unwrap().get::<_,i64>(0),0);

    // Availability changes invalidate even when the provider content hash is unchanged.
    unavailable.content_status = ContentStatus::Unavailable;
    let state = repository.state(&x_source.id).unwrap();
    assert_eq!(
        repository
            .commit_page(&x_source.id, state.revision, &state, &[unavailable], now)
            .unwrap(),
        1
    );
    assert_eq!(admin.query_one("SELECT count(*) FROM research.source_invalidations WHERE item_id=$1 AND reason='UNAVAILABLE'", &[&new_id]).unwrap().get::<_,i64>(0),1);
    assert!(
        repository
            .read_item(&new_id, now, false)
            .unwrap()
            .unwrap()
            .text
            .is_none()
    );
    repository.purge_content(now + Duration::hours(25)).unwrap();
    let items = repository
        .read_items(Some(&x_source.id), 10, now + Duration::hours(25), false)
        .unwrap();
    assert!(items.iter().all(|i| i.text.is_none() && i.title.is_none()));
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn simultaneous_collectors_serialize_a_shared_x_edit_chain() {
    use std::sync::{Arc, Barrier};
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let now = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let mut old_source = source();
    old_source.id = "fixture_concurrent_old_edit".into();
    let mut old = parse_feed(&old_source, feed("Original post").as_bytes(), 5, now)
        .unwrap()
        .items
        .remove(0);
    old_source.kind = SourceKind::XLookup;
    old.external_id = "201".into();
    old.item_id = digest("fixture_concurrent_old_edit:201");
    old.text = Some("Original post".into());
    old.content_status = ContentStatus::Available;
    old.social = Some(SocialObservation {
        action: SocialAction::Original,
        actor_user_id: "42".into(),
        original_author_id: Some("42".into()),
        conversation_id: None,
        reply_to_id: None,
        quoted_id: None,
        reposted_id: None,
        edit_history_ids: vec!["201".into()],
        missing_context: false,
        context: vec![],
    });
    let mut new_source = old_source.clone();
    new_source.id = "fixture_concurrent_new_edit".into();
    let mut new = old.clone();
    new.source_id = new_source.id.clone();
    new.external_id = "202".into();
    new.item_id = digest("fixture_concurrent_new_edit:202");
    new.text = Some("Edited post".into());
    new.content_hash = digest("concurrent edited post");
    new.social
        .as_mut()
        .unwrap()
        .edit_history_ids
        .push("202".into());
    let barrier = Arc::new(Barrier::new(2));
    let workers: Vec<_> = [(old_source.clone(), old.clone()), (new_source, new.clone())]
        .into_iter()
        .map(|(source, item)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut repository = PgResearchRepository::connect(&dsn("trady_analyst")).unwrap();
                let state = prepare_state(&source, repository.state(&source.id).unwrap()).unwrap();
                barrier.wait();
                repository
                    .commit_page(&source.id, state.revision, &state, &[item], now)
                    .unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let mut repository = PgResearchRepository::connect(&dsn("trady_analyst")).unwrap();
    let old = repository
        .read_item(&old.item_id, now, false)
        .unwrap()
        .unwrap();
    let new = repository
        .read_item(&new.item_id, now, false)
        .unwrap()
        .unwrap();
    assert_eq!(old.content_status, ContentStatus::Superseded);
    assert!(old.text.is_none());
    assert_eq!(new.content_status, ContentStatus::Available);
    assert!(new.text.is_some());
}

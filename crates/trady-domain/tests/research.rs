use chrono::{DateTime, Utc};
use serde_json::json;
use trady_domain::research::{ContentStatus, SourceItem};

#[test]
fn rights_expiry_and_retraction_redact_all_content_but_preserve_audit_identity() {
    let now: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .unwrap()
        .to_utc();
    let item: SourceItem = serde_json::from_value(json!({
        "item_id":"audit-id","source_id":"fixture","external_id":"101","canonical_url":"https://x.com/i/status/101",
        "lineage_id":"lineage","content_hash":"hash","parser_version":"fixture-1","publisher":"Fixture actor",
        "feed_title":"Fixture feed","title":"Title","summary":"Summary","text":"Post",
        "published_at":"2026-10-31T11:00:00Z","event_at":null,"first_seen_at":"2026-10-31T11:59:00Z",
        "fetched_at":"2026-10-31T11:59:00Z","expires_at":"2026-11-01T11:59:00Z","language":"en",
        "rights":{"store_content":true,"llm_processing":true,"retention_hours":24,"terms_reference":"fixture permission"},
        "content_status":"AVAILABLE","related_urls":[],"gaps":[],
        "social":{"action":"QUOTE","actor_user_id":"42","original_author_id":"84","conversation_id":null,
            "reply_to_id":null,"quoted_id":"100","reposted_id":null,"edit_history_ids":["101"],"missing_context":false,
            "context":[{"post_id":"100","author_id":"84","text":"Parent text","published_at":"2026-10-31T10:00:00Z"}]}
    })).unwrap();
    for (store, model, for_model, expired, status, redacted) in [
        (true, true, false, false, ContentStatus::Available, false),
        (true, false, false, false, ContentStatus::Available, false),
        (true, false, true, false, ContentStatus::Available, true),
        (false, true, false, false, ContentStatus::Available, true),
        (true, true, false, true, ContentStatus::Available, true),
        (true, true, false, false, ContentStatus::Superseded, true),
        (true, true, false, false, ContentStatus::Unavailable, true),
    ] {
        let mut observed = item.clone();
        observed.rights.store_content = store;
        observed.rights.llm_processing = model;
        observed.content_status = status;
        if expired {
            observed.expires_at = now;
        }
        observed.redact(now, for_model);
        assert_eq!(observed.title.is_none(), redacted);
        assert_eq!(observed.feed_title.is_none(), redacted);
        assert_eq!(observed.text.is_none(), redacted);
        assert_eq!(observed.summary.is_none(), redacted);
        assert_eq!(
            observed.social.as_ref().unwrap().context[0].text.is_none(),
            redacted
        );
        assert_eq!(observed.item_id, item.item_id);
        assert_eq!(observed.content_hash, item.content_hash);
        assert_eq!(observed.published_at, item.published_at);
        assert_eq!(observed.first_seen_at, item.first_seen_at);
        if matches!(
            status,
            ContentStatus::Superseded | ContentStatus::Unavailable
        ) {
            assert_eq!(observed.content_status, status);
        }
    }
}

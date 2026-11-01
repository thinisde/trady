use super::*;
use serde_json::json;
use std::collections::VecDeque;

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .unwrap()
        .to_utc()
}
fn source(kind: SourceKind) -> SourceConfig {
    SourceConfig {
        id: "fixture_source".into(),
        kind,
        enabled: true,
        url: match kind {
            SourceKind::XTimeline | SourceKind::XLookup => "https://api.x.com/2/",
            SourceKind::AlphaVantage => "https://www.alphavantage.co/query",
            _ => "https://example.com/feed",
        }
        .into(),
        allowed_hosts: vec![
            "example.com".into(),
            "api.x.com".into(),
            "www.alphavantage.co".into(),
        ],
        credential_env: Some("TRADY_TEST_NEWS_TOKEN".into()),
        interval_seconds: 60,
        daily_request_limit: 100,
        query: None,
        x_rule: None,
        x_account: if kind == SourceKind::XTimeline {
            Some(XAccount {
                person: "Fixture actor".into(),
                handle: "fixture_actor".into(),
                user_id: Some("42".into()),
                verified_at: Some(now()),
                ownership_reference: "fixture ownership".into(),
                priority: 0,
                scope: vec!["fixture".into()],
            })
        } else {
            None
        },
        rights: SourceRights {
            store_content: true,
            llm_processing: false,
            retention_hours: 24,
            terms_reference: "test fixture permission".into(),
        },
    }
}
fn window() -> CollectionRequest {
    CollectionRequest {
        start_at: now() - Duration::hours(1),
        end_at: now(),
        next_token: Some("cursor-one".into()),
        etag: None,
        last_modified: None,
        limit: 5,
    }
}
#[derive(Default)]
struct FakeHttp {
    responses: VecDeque<HttpResponse>,
    requests: Vec<HttpRequest>,
}
impl HttpTransport for FakeHttp {
    fn x_budget_enforced(&self) -> bool {
        true
    }
    fn get(&mut self, request: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure> {
        self.requests.push(request);
        self.responses
            .pop_front()
            .ok_or_else(|| SourceFailure::unavailable("fixture_response_missing"))
    }
}
fn response(value: serde_json::Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: serde_json::to_vec(&value).unwrap(),
        ..Default::default()
    }
}
fn client(responses: Vec<HttpResponse>) -> SourceClient<FakeHttp> {
    SourceClient::with_transport(
        FakeHttp {
            responses: responses.into(),
            requests: vec![],
        },
        |_| Ok(Some("fixture-token".into())),
    )
}

fn search_source() -> SourceConfig {
    let mut source = source(SourceKind::XTimeline);
    source.kind = SourceKind::XRecentSearch;
    source.x_rule = Some(trady_application::x_monitoring::XRule {
        version: 1,
        terms: vec!["Tesla".into(), "data center".into()],
        exclude_reposts: false,
        approved_at: Some(now()),
        approval_reference: "fixture owner coverage decision".into(),
    });
    source
}

fn search_window() -> CollectionRequest {
    CollectionRequest {
        limit: 10,
        end_at: now() - Duration::seconds(30),
        ..window()
    }
}

#[test]
fn recent_search_sends_one_author_rule_and_keeps_replies_without_routine_context_reads() {
    let source = search_source();
    let mut client = client(vec![
        user("42"),
        response(
            json!({"data":[post("500",json!([{"type":"replied_to","id":"400"}]))],"meta":{"next_token":"next-page","result_count":1}}),
        ),
    ]);
    let page = client.fetch(&source, &search_window(), now()).unwrap();
    assert_eq!(page.next_token.as_deref(), Some("next-page"));
    assert!(page.complete);
    assert_eq!(
        page.items[0].social.as_ref().unwrap().action,
        SocialAction::Reply
    );
    assert!(page.items[0].social.as_ref().unwrap().missing_context);
    assert!(
        page.gaps
            .iter()
            .any(|g| g == "query_selected_coverage_not_full_timeline")
    );
    let request = &client.transport.requests[1];
    assert_eq!(request.url.path(), "/2/tweets/search/recent");
    let pairs: std::collections::BTreeMap<_, _> = request.url.query_pairs().collect();
    assert_eq!(
        pairs["query"],
        "from:fixture_actor (Tesla OR \"data center\")"
    );
    assert_eq!(pairs["expansions"], "author_id");
    assert!(pairs["tweet.fields"].contains("referenced_tweets"));
    assert!(pairs["tweet.fields"].contains("author_id"));
    assert!(!pairs.contains_key("post.fields"));
    assert_eq!(pairs["next_token"], "cursor-one");
    assert!(!request.url.as_str().contains("is%3Areply"));
}

#[test]
fn recent_search_blocks_identity_reuse_incoming_reactions_and_unsupported_history() {
    let source = search_source();
    let mut reused = client(vec![user("84")]);
    assert_eq!(
        reused
            .fetch(&source, &search_window(), now())
            .unwrap_err()
            .reason,
        "x_stable_identity_mismatch"
    );
    assert_eq!(reused.transport.requests.len(), 1);
    let mut incoming = post("500", json!([]));
    incoming["author_id"] = json!("84");
    let mut incoming = client(vec![
        user("42"),
        response(json!({"data":[incoming],"meta":{"result_count":1}})),
    ]);
    assert!(incoming.fetch(&source, &search_window(), now()).is_err());
    let mut old = search_window();
    old.start_at = now() - Duration::days(8);
    let mut no_network = client(vec![]);
    assert_eq!(
        no_network.fetch(&source, &old, now()).unwrap_err().reason,
        "x_recent_history_or_request_bounds_unsupported"
    );
    assert!(no_network.transport.requests.is_empty());
}

#[test]
fn null_action_metadata_stays_unknown_and_repost_exclusion_requires_evidence() {
    let no_references = post("500", json!(null));
    let mut source = search_source();
    let mut unknown = client(vec![
        user("42"),
        response(json!({"data":[no_references.clone()],"meta":{"result_count":1}})),
    ]);
    let page = unknown.fetch(&source, &search_window(), now()).unwrap();
    assert_eq!(
        page.items[0].social.as_ref().unwrap().action,
        SocialAction::Unknown
    );
    assert!(
        page.items[0]
            .social
            .as_ref()
            .unwrap()
            .original_author_id
            .is_none()
    );
    source.x_rule.as_mut().unwrap().exclude_reposts = true;
    for candidate in [
        no_references,
        post("500", json!([{"type":"retweeted","id":"400"}])),
    ] {
        let mut client = client(vec![
            user("42"),
            response(json!({"data":[candidate],"meta":{"result_count":1}})),
        ]);
        assert_eq!(
            client
                .fetch(&source, &search_window(), now())
                .unwrap_err()
                .reason,
            "x_repost_exclusion_not_verified"
        );
    }
}

#[test]
fn recent_search_accepts_originals_with_omitted_optional_references() {
    let mut original = post("500", json!([]));
    original.as_object_mut().unwrap().remove("referenced_posts");
    let mut source = search_source();
    source.x_rule.as_mut().unwrap().exclude_reposts = true;
    let mut client = client(vec![
        user("42"),
        response(json!({"data":[original],"meta":{"result_count":1}})),
    ]);
    let page = client.fetch(&source, &search_window(), now()).unwrap();
    let social = page.items[0].social.as_ref().unwrap();
    assert_eq!(social.action, SocialAction::Original);
    assert!(!social.missing_context);
    assert_eq!(social.original_author_id.as_deref(), Some("42"));
    assert!(
        client.transport.requests[1]
            .url
            .query_pairs()
            .any(|(key, value)| { key == "query" && value.ends_with(" -is:retweet") })
    );
}

#[test]
fn counts_require_contiguous_complete_buckets_and_accept_legacy_names() {
    let source = search_source();
    let mut window = search_window();
    window.next_token = None;
    let start = window.start_at;
    let end = window.end_at;
    let mut valid = client(vec![
        user("42"),
        response(
            json!({"data":[{"start":start,"end":end,"tweet_count":4}],"meta":{"total_tweet_count":4}}),
        ),
    ]);
    let count = valid.recent_x_counts(&source, &window, now()).unwrap();
    assert!(count.complete);
    assert_eq!(count.total_posts, 4);
    assert_eq!(
        valid.transport.requests[1].url.path(),
        "/2/tweets/counts/recent"
    );
    for value in [
        json!({"data":[{"start":start,"end":end,"post_count":4}],"meta":{"total_post_count":4,"next_token":"pending"}}),
        json!({"data":[{"start":start+Duration::minutes(1),"end":end,"post_count":4}],"meta":{"total_post_count":4}}),
        json!({"data":[{"start":start,"end":end,"post_count":3}],"meta":{"total_post_count":4}}),
    ] {
        let mut partial = client(vec![user("42"), response(value)]);
        assert!(
            !partial
                .recent_x_counts(&source, &window, now())
                .unwrap()
                .complete
        );
    }
    let mut missing = client(vec![
        user("42"),
        response(json!({"meta":{"total_post_count":0}})),
    ]);
    assert!(missing.recent_x_counts(&source, &window, now()).is_err());
}

#[test]
fn default_unmetered_transport_is_not_authorized_to_perform_x_reads() {
    struct Unmetered;
    impl HttpTransport for Unmetered {
        fn get(&mut self, _: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure> {
            panic!("network must remain blocked")
        }
    }
    let mut client = SourceClient::with_transport(Unmetered, |_| Ok(Some("fixture-token".into())));
    assert_eq!(
        client
            .resolve_x(&source(SourceKind::XLookup), "fixture_actor", now())
            .unwrap_err()
            .reason,
        "x_durable_cost_gate_required"
    );
}

#[test]
fn credential_store_failures_stop_requests_without_exposing_details() {
    let mut client = SourceClient::with_transport(FakeHttp::default(), |_| {
        Err(Error("private-credential-detail".into()))
    });
    let failure = client
        .resolve_x(&source(SourceKind::XLookup), "fixture_actor", now())
        .unwrap_err();
    assert_eq!(failure.reason, "source_credential_unavailable");
    assert!(!format!("{failure:?}").contains("private-credential-detail"));
    assert!(client.transport.requests.is_empty());
}

fn user(id: &str) -> HttpResponse {
    response(json!({"data":{"id":id,"username":"fixture_actor","name":"Fixture"}}))
}
fn post(id: &str, reference: serde_json::Value) -> serde_json::Value {
    json!({"id":id,"text":"Untrusted fixture text","author_id":"42","created_at":"2026-10-31T11:59:00Z",
        "edit_history_post_ids":[id],"referenced_posts":reference,"conversation_id":id})
}

#[test]
fn x_preserves_outgoing_actions_actor_context_and_current_api_fields() {
    let mut context = post("400", json!([]));
    context["author_id"] = json!("84");
    let posts = vec![
        post("501", json!([])),
        post("502", json!([{"type":"replied_to","id":"400"}])),
        post("503", json!([{"type":"quoted","id":"400"}])),
        post("504", json!([{"type":"retweeted","id":"400"}])),
    ];
    let mut c = client(vec![
        user("42"),
        response(
            json!({"data":posts,"includes":{"posts":[context]},"meta":{"next_token":"cursor-two"}}),
        ),
    ]);
    let page = c
        .fetch(&source(SourceKind::XTimeline), &window(), now())
        .unwrap();
    let actions: Vec<_> = page
        .items
        .iter()
        .map(|i| i.social.as_ref().unwrap().action)
        .collect();
    assert_eq!(
        actions,
        [
            SocialAction::Original,
            SocialAction::Reply,
            SocialAction::Quote,
            SocialAction::Repost
        ]
    );
    assert_eq!(page.items[3].social.as_ref().unwrap().actor_user_id, "42");
    assert_eq!(
        page.items[3]
            .social
            .as_ref()
            .unwrap()
            .original_author_id
            .as_deref(),
        Some("84")
    );
    assert_eq!(
        page.items[3].social.as_ref().unwrap().context[0]
            .text
            .as_deref(),
        Some("Untrusted fixture text")
    );
    assert_eq!(page.next_token.as_deref(), Some("cursor-two"));
    let pairs: std::collections::BTreeMap<_, _> =
        c.transport.requests[1].url.query_pairs().collect();
    assert_eq!(
        pairs["tweet.fields"],
        "id,text,author_id,created_at,conversation_id,entities,lang,referenced_tweets,edit_history_tweet_ids"
    );
    assert_eq!(
        pairs["expansions"],
        "author_id,referenced_tweets.id,edit_history_tweet_ids"
    );
    assert!(!pairs.contains_key("exclude"));
    assert_eq!(pairs["pagination_token"], "cursor-one");
    assert_eq!(pairs["end_time"], "2026-10-31T12:00:00Z");
}

#[test]
fn x_identity_mismatch_and_incoming_reactions_fail_closed() {
    let mut c = client(vec![user("43")]);
    assert_eq!(
        c.fetch(&source(SourceKind::XTimeline), &window(), now())
            .unwrap_err()
            .reason,
        "x_stable_identity_mismatch"
    );
    assert_eq!(c.transport.requests.len(), 1);
    let mut incoming = post("501", json!([]));
    incoming["author_id"] = json!("84");
    let mut c = client(vec![user("42"), response(json!({"data":[incoming]}))]);
    assert_eq!(
        c.fetch(&source(SourceKind::XTimeline), &window(), now())
            .unwrap_err()
            .reason,
        "x_incoming_reaction_is_not_actor_speech"
    );
}

#[test]
fn legacy_x_payloads_missing_context_edits_and_partial_errors_are_explicit() {
    let mut p = post("502", json!([]));
    p.as_object_mut().unwrap().remove("referenced_posts");
    p["referenced_tweets"] = json!([{"type":"quoted","id":"400"}]);
    p.as_object_mut().unwrap().remove("edit_history_post_ids");
    p["edit_history_tweet_ids"] = json!(["501", "502", "503"]);
    let mut c = client(vec![
        user("42"),
        response(json!({"data":[p],"errors":[{"detail":"unavailable context"}]})),
    ]);
    let page = c
        .fetch(&source(SourceKind::XTimeline), &window(), now())
        .unwrap();
    assert!(!page.complete);
    assert!(page.items[0].social.as_ref().unwrap().missing_context);
    assert_eq!(page.items[0].content_status, ContentStatus::Superseded);
    assert!(page.items[0].text.is_none());
    assert!(page.items[0].event_at.is_none());
}

#[test]
fn x_reply_with_an_available_quote_still_reports_a_missing_parent() {
    let p = post(
        "502",
        json!([{"type":"replied_to","id":"399"},{"type":"quoted","id":"400"}]),
    );
    let mut c = client(vec![
        user("42"),
        response(json!({"data":[p],"includes":{"posts":[post("400",json!([]))]}})),
    ]);
    let page = c
        .fetch(&source(SourceKind::XTimeline), &window(), now())
        .unwrap();
    let social = page.items[0].social.as_ref().unwrap();
    assert_eq!(social.context.len(), 1);
    assert!(social.missing_context);
    assert_eq!(social.reply_to_id.as_deref(), Some("399"));
    assert_eq!(social.quoted_id.as_deref(), Some("400"));
}

#[test]
fn x_lookup_rejects_host_spoofing_and_reports_unavailable_without_content() {
    for url in [
        "https://x.com.evil.test/actor/status/12",
        "http://x.com/actor/status/12",
        "https://x.com@127.0.0.1/actor/status/12",
        "https://x.com/actor/status/12/other",
    ] {
        assert!(x_post_id(url).is_err());
    }
    assert_eq!(
        x_post_id("https://twitter.com/actor/status/12?s=20#fragment").unwrap(),
        "12"
    );
    let mut c = client(vec![response(
        json!({"errors":[{"detail":"deleted or protected"}]}),
    )]);
    assert_eq!(
        c.lookup_x(
            &source(SourceKind::XLookup),
            "https://x.com/actor/status/12",
            now()
        )
        .unwrap_err()
        .reason,
        "post_deleted_private_or_inaccessible"
    );
}

#[test]
fn rate_limits_and_api_error_notices_do_not_look_like_empty_news() {
    let mut c = client(vec![HttpResponse {
        status: 429,
        retry_after: Some("121".into()),
        ..Default::default()
    }]);
    let failure = c
        .fetch(&source(SourceKind::PublisherRss), &window(), now())
        .unwrap_err();
    assert_eq!(failure.status, CollectionStatus::RateLimited);
    assert_eq!(failure.retry_at, Some(now() + Duration::seconds(121)));
    let mut c = client(vec![response(
        json!({"Information":"secret query or provider notice"}),
    )]);
    let mut alpha_window = window();
    alpha_window.next_token = None;
    let failure = c
        .fetch(&source(SourceKind::AlphaVantage), &alpha_window, now())
        .unwrap_err();
    assert_eq!(failure.status, CollectionStatus::RateLimited);
    assert!(!failure.reason.contains("secret"));
}

const RSS: &str = "<rss version=\"2.0\"><channel><title>Fixture feed</title><item><guid>story-1</guid><title>Result &amp; outlook</title><link>https://example.com/story?utm_source=copy</link><source>Original publisher</source><pubDate>Fri, 30 Oct 2026 12:00:00 GMT</pubDate><description><![CDATA[<p>Permitted snippet.</p>]]></description></item></channel></rss>";

#[test]
fn finnhub_and_gdelt_share_news_lineage_and_preserve_incomplete_coverage() {
    let mut s = source(SourceKind::Finnhub);
    s.url = "https://api.finnhub.io/api/v1/company-news".into();
    s.allowed_hosts = vec!["api.finnhub.io".into()];
    s.credential_env = Some("TRADY_FINNHUB_API_KEY".into());
    s.query = Some("AAPL".into());
    let mut w = window();
    w.next_token = None;
    w.limit = 1;
    let mut c = client(vec![response(
        json!([{"id":42,"url":"https://example.com/story?utm_source=vendor","source":"Publisher","headline":"Result","summary":"Permitted snippet","datetime":now().timestamp()-10}]),
    )]);
    let page = c.fetch(&s, &w, now()).unwrap();
    assert!(!page.complete);
    assert_eq!(page.items.len(), 1);
    let finnhub_lineage = page.items[0].lineage_id.clone();
    assert_eq!(page.items[0].canonical_url, "https://example.com/story");
    assert!(page.items[0].text.is_none());
    let mut s = source(SourceKind::Gdelt);
    s.url = "https://api.gdeltproject.org/api/v2/doc/doc".into();
    s.allowed_hosts = vec!["api.gdeltproject.org".into()];
    s.credential_env = None;
    s.query = Some("\"Apple Inc\"".into());
    let mut c = client(vec![response(
        json!({"articles":[{"url":"https://example.com/story","domain":"example.com","title":"Result","seendate":now().format("%Y%m%dT%H%M%SZ").to_string(),"language":"English"}]}),
    )]);
    let page = c.fetch(&s, &w, now()).unwrap();
    assert!(!page.complete);
    assert_eq!(page.items[0].lineage_id, finnhub_lineage);
    assert_eq!(page.items[0].published_at, None);
    assert!(
        page.items[0]
            .gaps
            .iter()
            .any(|g| g == "discovery_time_is_not_publication_time")
    );
    let mut c = client(vec![response(json!({"error":"private provider message"}))]);
    assert!(c.fetch(&s, &w, now()).is_err());
}

#[test]
fn rss_keeps_attribution_publication_time_and_conditional_requests() {
    let mut c = client(vec![
        HttpResponse {
            status: 200,
            body: RSS.as_bytes().into(),
            etag: Some("v1".into()),
            ..Default::default()
        },
        HttpResponse {
            status: 304,
            ..Default::default()
        },
    ]);
    let source = source(SourceKind::PublisherRss);
    let first = c.fetch(&source, &window(), now()).unwrap();
    assert_eq!(first.items[0].title.as_deref(), Some("Result & outlook"));
    assert_eq!(
        first.items[0].summary.as_deref(),
        Some("Permitted snippet.")
    );
    assert_eq!(first.items[0].publisher, "Original publisher");
    assert_eq!(first.items[0].canonical_url, "https://example.com/story");
    assert!(first.items[0].event_at.is_none());
    assert_ne!(first.items[0].published_at, Some(now()));
    let mut request = window();
    request.etag = first.etag;
    assert!(c.fetch(&source, &request, now()).unwrap().items.is_empty());
    assert_eq!(c.transport.requests[1].etag.as_deref(), Some("v1"));
}

#[test]
fn rss_google_metadata_rights_and_model_projection_stay_separate() {
    let mut source = source(SourceKind::GoogleNewsRss);
    source.url = "https://news.google.com/rss".into();
    let mut page = parse_feed(&source, RSS.as_bytes(), 5, now()).unwrap();
    assert_eq!(page.items[0].content_status, ContentStatus::MetadataOnly);
    assert!(
        page.items[0]
            .gaps
            .iter()
            .any(|g| g.contains("not_resolved"))
    );
    page.items[0].redact(now(), true);
    assert!(page.items[0].summary.is_none());
    source.rights.store_content = false;
    let page = parse_feed(&source, RSS.as_bytes(), 5, now()).unwrap();
    assert!(page.items[0].title.is_none());
    assert!(page.items[0].summary.is_none());
}

#[test]
fn rss_rejects_entities_invalid_dates_changed_documents_and_bounds() {
    let source = source(SourceKind::PublisherRss);
    for xml in [
        "<!DOCTYPE rss [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><rss>&x;</rss>",
        "<html><body>login</body></html>",
        "<rss><channel><title>x</title>",
        &RSS.replace("Fri, 30 Oct 2026 12:00:00 GMT", "not-a-date"),
        &RSS.replace("&amp;", "&external;"),
    ] {
        assert!(parse_feed(&source, xml.as_bytes(), 5, now()).is_err());
    }
    assert!(parse_feed(&source, &vec![b' '; MAX_BODY_BYTES + 1], 5, now()).is_err());
    let double = RSS.replace(
        "</channel>",
        &format!(
            "{}</channel>",
            RSS.split("<item>")
                .nth(1)
                .unwrap()
                .split("</item>")
                .next()
                .map(|v| format!("<item>{v}</item>"))
                .unwrap()
        ),
    );
    let capped = parse_feed(&source, double.as_bytes(), 1, now()).unwrap();
    assert!(!capped.complete);
    assert_eq!(capped.items.len(), 1);
}

#[test]
fn atom_alternate_links_and_utc_dates_parse() {
    let xml = "<feed xmlns=\"http://www.w3.org/2005/Atom\"><title>Issuer</title><entry><id>release-1</id><title>Release</title><link rel=\"self\" href=\"https://example.com/self\"/><link rel=\"alternate\" href=\"https://example.com/release\"/><published>2026-10-30T12:00:00Z</published><author><name>Issuer IR</name></author></entry></feed>";
    let page = parse_feed(&source(SourceKind::PublisherRss), xml.as_bytes(), 5, now()).unwrap();
    assert_eq!(page.items[0].canonical_url, "https://example.com/release");
    assert_eq!(page.items[0].publisher, "Issuer IR");
}

#[test]
fn rss_language_metadata_is_preserved_even_after_items() {
    let source = source(SourceKind::PublisherRss);
    for tag in ["en", "en-US", "de"] {
        let xml = RSS.replace(
            "</channel>",
            &format!("<language>{tag}</language></channel>"),
        );
        let page = parse_feed(&source, xml.as_bytes(), 5, now()).unwrap();
        assert_eq!(page.items[0].language, Some(tag.to_ascii_lowercase()));
    }
    let page = parse_feed(&source, RSS.as_bytes(), 5, now()).unwrap();
    assert!(page.items[0].language.is_none());
    let invalid = RSS.replace("</channel>", "<language>English text</language></channel>");
    assert!(parse_feed(&source, invalid.as_bytes(), 5, now()).is_err());
    let english = RSS.replace("</channel>", "<language>en</language></channel>");
    let german = RSS.replace("</channel>", "<language>de</language></channel>");
    assert_ne!(
        parse_feed(&source, english.as_bytes(), 5, now())
            .unwrap()
            .items[0]
            .content_hash,
        parse_feed(&source, german.as_bytes(), 5, now())
            .unwrap()
            .items[0]
            .content_hash,
    );
}

#[test]
fn gdelt_plaintext_throttling_is_a_provider_backoff() {
    let mut source = source(SourceKind::Gdelt);
    source.url = "https://api.gdeltproject.org/api/v2/doc/doc".into();
    source.allowed_hosts = vec!["api.gdeltproject.org".into()];
    source.credential_env = None;
    source.query = Some("\"Apple Inc\"".into());
    let mut window = window();
    window.next_token = None;
    let mut client = client(vec![HttpResponse {
        status: 200,
        body: b"Please limit requests to one every 5 seconds or contact the provider.".to_vec(),
        ..Default::default()
    }]);
    let failure = client.fetch(&source, &window, now()).unwrap_err();
    assert_eq!(failure.status, CollectionStatus::RateLimited);
    assert_eq!(failure.reason, "gdelt_rate_limited");
    assert_eq!(failure.retry_at, Some(now() + Duration::seconds(60)));
}

#[test]
fn gdelt_missing_articles_back_off_without_claiming_complete_empty_news() {
    let mut s = source(SourceKind::Gdelt);
    s.url = "https://api.gdeltproject.org/api/v2/doc/doc".into();
    s.allowed_hosts = vec!["api.gdeltproject.org".into()];
    s.credential_env = None;
    s.query = Some("Microsoft".into());
    let mut w = window();
    w.next_token = None;
    for payload in [
        json!({}),
        json!({"articles":null}),
        json!({"articles":"invalid"}),
    ] {
        let failure = client(vec![response(payload)])
            .fetch(&s, &w, now())
            .unwrap_err();
        assert_eq!(failure.status, CollectionStatus::Unavailable);
        assert_eq!(failure.reason, "gdelt_articles_missing");
        assert_eq!(failure.retry_at, Some(now() + Duration::seconds(60)));
    }
    let page = client(vec![response(json!({"articles":[]}))])
        .fetch(&s, &w, now())
        .unwrap();
    assert!(page.items.is_empty());
    assert!(page.complete);
}

#[test]
fn atom_language_inheritance_overrides_and_resets_are_scoped() {
    let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="en-GB">
        <title>Issuer</title>
        <entry><id>1</id><title>English</title><link href="https://example.com/1"/></entry>
        <entry xml:lang="de"><id>2</id><title>Deutsch</title><link href="https://example.com/2"/></entry>
        <entry xml:lang=""><id>3</id><title>Unknown</title><link href="https://example.com/3"/></entry>
        <entry><id>4</id><title>English again</title><link href="https://example.com/4"/></entry>
    </feed>"#;
    let page = parse_feed(&source(SourceKind::PublisherRss), xml.as_bytes(), 5, now()).unwrap();
    let languages: Vec<_> = page
        .items
        .iter()
        .map(|item| item.language.as_deref())
        .collect();
    assert_eq!(languages, [Some("en-gb"), Some("de"), None, Some("en-gb")]);
}

#[test]
fn atom_sentiment_language_tracks_the_selected_text_construct() {
    let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="en">
        <title>Issuer</title>
        <entry><id>1</id><title>English</title><summary xml:lang="de">Deutsch</summary><link href="https://example.com/1"/></entry>
        <entry><id>2</id><title xml:lang="fr">Français</title><link href="https://example.com/2"/></entry>
        <entry><id>3</id><title>English</title><summary type="xhtml"><div>English <span xml:lang="de">Deutsch</span></div></summary><link href="https://example.com/3"/></entry>
    </feed>"#;
    let page = parse_feed(&source(SourceKind::PublisherRss), xml.as_bytes(), 5, now()).unwrap();
    let languages: Vec<_> = page
        .items
        .iter()
        .map(|item| item.language.as_deref())
        .collect();
    assert_eq!(languages, [Some("de"), Some("fr"), None]);
}

#[test]
fn atom_xhtml_inherited_bases_and_redirect_destinations_preserve_content() {
    let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom" xml:base="../releases/">
        <entry xml:base="2026/"><id>release-1</id>
        <title type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml">Quarterly <b>results</b></div></title>
        <summary type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml">Revenue <em>increased</em>.</div></summary>
        <link rel="alternate" xml:base="october/" href="release?utm_source=feed"/>
        <published>2026-10-30T12:00:00Z</published></entry><title>Late feed title</title></feed>"#;
    let mut c = client(vec![HttpResponse {
        status: 200,
        body: xml.as_bytes().to_vec(),
        final_url: Some(Url::parse("https://example.com/news/feed").unwrap()),
        ..Default::default()
    }]);
    let page = c
        .fetch(&source(SourceKind::PublisherRss), &window(), now())
        .unwrap();
    let item = &page.items[0];
    assert_eq!(item.title.as_deref(), Some("Quarterly results"));
    assert_eq!(item.summary.as_deref(), Some("Revenue increased."));
    assert_eq!(item.feed_title.as_deref(), Some("Late feed title"));
    assert_eq!(
        item.canonical_url,
        "https://example.com/releases/2026/october/release"
    );
}

#[test]
fn alpha_capped_windows_resume_and_saturated_minute_is_incomplete() {
    let articles: Vec<_> = (0..5).map(|i|json!({"title":format!("Result {i}"),"url":format!("https://example.com/{i}"),
        "time_published":"20261031T113000","summary":"Fixture summary","source":"Original publisher"})).collect();
    let mut c = client(vec![
        response(json!({"feed":articles})),
        response(json!({"feed":[]})),
        response(json!({"feed":[]})),
    ]);
    let mut request = window();
    request.next_token = None;
    let source = source(SourceKind::AlphaVantage);
    let first = c.fetch(&source, &request, now()).unwrap();
    assert_eq!(first.items.len(), 5);
    assert!(first.complete);
    assert!(first.next_token.is_some());
    request.next_token = first.next_token;
    let second = c.fetch(&source, &request, now()).unwrap();
    assert!(second.next_token.is_some());
    request.next_token = second.next_token;
    assert!(
        c.fetch(&source, &request, now())
            .unwrap()
            .next_token
            .is_none()
    );
    assert!(
        c.transport.requests[1]
            .url
            .query_pairs()
            .any(|(k, v)| k == "time_to" && v == "20261031T1130")
    );
    let mut c = client(vec![response(json!({"feed":articles}))]);
    request.next_token = None;
    request.start_at = now() - Duration::seconds(30);
    let page = c.fetch(&source, &request, now()).unwrap();
    assert!(!page.complete);
    assert!(page.gaps.iter().any(|g| g.contains("saturated")));
}

#[test]
fn alpha_saturated_minute_keeps_recoverable_sibling_windows() {
    let articles: Vec<_> = (0..5)
        .map(|i| {
            json!({"title":format!("Result {i}"),"url":format!("https://example.com/{i}"),
        "time_published":"20261031T115930","summary":"Fixture summary","source":"Publisher"})
        })
        .collect();
    let mut request = window();
    request.start_at = now() - Duration::minutes(2);
    request.next_token = Some(json!({"windows":[[now()-Duration::minutes(2),now()-Duration::minutes(1)],[now()-Duration::minutes(1),now()]]}).to_string());
    let mut c = client(vec![
        response(json!({"feed":articles})),
        response(json!({"feed":[]})),
    ]);
    let source = source(SourceKind::AlphaVantage);
    let page = c.fetch(&source, &request, now()).unwrap();
    assert!(page.gaps.iter().any(|gap| gap.contains("saturated")));
    let cursor: serde_json::Value =
        serde_json::from_str(page.next_token.as_ref().unwrap()).unwrap();
    assert_eq!(cursor["windows"].as_array().unwrap().len(), 2);
    request.next_token = page.next_token;
    let sibling = c.fetch(&source, &request, now()).unwrap();
    assert!(
        c.transport.requests[1]
            .url
            .query_pairs()
            .any(|(key, value)| key == "time_from" && value == "20261031T1159")
    );
    let cursor: serde_json::Value =
        serde_json::from_str(sibling.next_token.as_ref().unwrap()).unwrap();
    assert_eq!(cursor["windows"].as_array().unwrap().len(), 1);
}

#[test]
fn nested_atom_source_titles_and_markup_do_not_corrupt_the_headline() {
    let xml = "<feed><title>Feed</title><entry><id>one</id><title>Original headline</title><link href=\"https://example.com/story\"/><source><title>Original publisher</title></source><summary><div>Nested summary</div></summary></entry></feed>";
    let page = parse_feed(&source(SourceKind::PublisherRss), xml.as_bytes(), 5, now()).unwrap();
    assert_eq!(page.items[0].title.as_deref(), Some("Original headline"));
    assert_eq!(page.items[0].publisher, "Original publisher");
    assert_eq!(page.items[0].summary.as_deref(), Some("Nested summary"));
    assert!(
        parse_feed(
            &source(SourceKind::PublisherRss),
            b"<rss><channel><title>Empty or changed feed</title></channel></rss>",
            5,
            now()
        )
        .is_err()
    );
}

#[test]
fn network_boundary_rejects_private_addresses_unapproved_hosts_and_ports() {
    for ip in [
        "127.0.0.1",
        "10.0.0.1",
        "169.254.169.254",
        "100.64.0.1",
        "192.168.1.2",
        "198.18.0.1",
        "224.0.0.1",
        "::1",
        "::ffff:127.0.0.1",
        "fd00::1",
        "2001:db8::1",
        "2002:7f00:1::",
    ] {
        assert!(!http::public_address(ip.parse().unwrap()), "{ip}");
    }
    assert!(http::public_address("8.8.8.8".parse().unwrap()));
    for url in [
        "https://127.0.0.1/feed",
        "http://example.com/feed",
        "https://example.com:8443/feed",
        "https://evil.test/feed",
    ] {
        let mut http = SafeHttp;
        assert!(
            http.get(HttpRequest {
                url: Url::parse(url).unwrap(),
                allowed_hosts: vec!["example.com".into(), "127.0.0.1".into()],
                bearer: None,
                etag: None,
                last_modified: None,
                redirects: 3
            })
            .is_err()
        );
    }
}

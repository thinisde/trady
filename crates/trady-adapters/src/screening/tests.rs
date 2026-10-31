use super::*;
use crate::news::HttpResponse;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use trady_application::research::SourceFailure;
use trady_domain::{Currency, screening::*};

type Requests = Rc<RefCell<Vec<HttpRequest>>>;

fn now() -> DateTime<Utc> {
    "2026-10-30T14:00:00Z".parse().unwrap()
}
fn config() -> ApprovedUniverseConfig {
    ApprovedUniverseConfig {
        config_version: 1,
        enabled: true,
        source_id: "fixture_universe".into(),
        url: "https://example.com/sp500.json".into(),
        allowed_hosts: vec!["example.com".into()],
        expected_provider: "Fixture provider".into(),
        expected_taxonomy_edition: "GICS_2026".into(),
        rights: SourceRights {
            store_content: true,
            llm_processing: false,
            retention_hours: 24,
            terms_reference: "test_fixture_only".into(),
        },
        minimum_interval_seconds: 60,
        daily_request_limit: 20,
    }
}
fn document() -> UniverseDocument {
    UniverseDocument {
        schema_version: 1,
        index_id: "SP500".into(),
        complete: true,
        snapshot_id: "provider_membership_1".into(),
        classification_snapshot_id: "provider_classification_1".into(),
        provider: "Fixture provider".into(),
        taxonomy_edition: "GICS_2026".into(),
        as_of: now() - Duration::minutes(1),
        available_at: now() - Duration::seconds(30),
        expires_at: now() + Duration::hours(1),
        members: vec![UniverseMember {
            issuer_id: "cik_0000320193".into(),
            share_class_isin: "US0378331005".into(),
            symbol: "AAPL".into(),
            listing_mic: "XNAS".into(),
            currency: Currency::new("USD").unwrap(),
            membership: EffectiveInterval {
                from: now() - Duration::days(1),
                until: None,
            },
            classification: Classification {
                sector_code: "45".into(),
                industry_group_code: "4520".into(),
                effective: EffectiveInterval {
                    from: now() - Duration::days(1),
                    until: None,
                },
                observed_at: now() - Duration::minutes(1),
            },
        }],
    }
}
fn response(document: &UniverseDocument) -> HttpResponse {
    HttpResponse {
        status: 200,
        final_url: Some(Url::parse(&config().url).unwrap()),
        body: serde_json::to_vec(document).unwrap(),
        ..HttpResponse::default()
    }
}
struct Transport {
    responses: VecDeque<HttpResponse>,
    requests: Requests,
}
impl HttpTransport for Transport {
    fn get(&mut self, request: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure> {
        self.requests.borrow_mut().push(request);
        self.responses
            .pop_front()
            .ok_or_else(|| SourceFailure::unavailable("fixture response missing"))
    }
}
fn transport(responses: Vec<HttpResponse>) -> (Transport, Requests) {
    let requests = Rc::new(RefCell::new(vec![]));
    (
        Transport {
            responses: responses.into(),
            requests: requests.clone(),
        },
        requests,
    )
}

#[test]
fn reviewed_universe_token_is_sent_only_as_a_header_and_rejects_control_bytes() {
    let (transport, requests) = transport(vec![response(&document())]);
    let mut source = HttpUniverseSource::with_transport(config(), transport)
        .unwrap()
        .with_bearer(Some("fixture-token".into()))
        .unwrap();
    source.fetch_universe(now()).unwrap();
    let requests = requests.borrow();
    assert_eq!(requests[0].bearer.as_deref(), Some("fixture-token"));
    assert!(!requests[0].url.as_str().contains("fixture-token"));
    assert!(
        source
            .with_bearer(Some("invalid\r\nheader".into()))
            .is_err()
    );
}

#[test]
fn screening_http_fetches_actual_response_rows_and_binds_configured_lineage() {
    let document = document();
    let body = serde_json::to_vec(&document).unwrap();
    let (transport, requests) = transport(vec![response(&document)]);
    let mut source = HttpUniverseSource::with_transport(config(), transport).unwrap();
    let universe = source.fetch_universe(now()).unwrap();
    assert_eq!(universe.members, document.members);
    assert_eq!(universe.provenance.content_sha256, hex_digest(&body));
    assert_eq!(universe.provenance.source_id, config().source_id);
    assert_eq!(universe.provenance.rights, config().rights);
    assert_eq!(universe.provenance.fetched_at, now());
    assert_eq!(universe.provenance.parser_version, PARSER_VERSION);
    assert_eq!(requests.borrow().len(), 1);
    let requests = requests.borrow();
    let request = &requests[0];
    assert_eq!(request.allowed_hosts, config().allowed_hosts);
    assert!(request.bearer.is_none() && request.etag.is_none() && request.last_modified.is_none());
    assert_eq!(request.redirects, 0);
}

#[test]
fn screening_http_never_fetches_disabled_or_unlicensed_sources() {
    let mut disabled = config();
    disabled.enabled = false;
    let (transport, requests) = transport(vec![]);
    let mut source = HttpUniverseSource::with_transport(disabled, transport).unwrap();
    assert!(source.fetch_universe(now()).is_err());
    assert!(requests.borrow().is_empty());
    let mut unlicensed = config();
    unlicensed.rights.store_content = false;
    assert!(unlicensed.validate().is_err());
    let mut unreviewed = config();
    unreviewed.rights.terms_reference.clear();
    assert!(unreviewed.validate().is_err());
}

#[test]
fn screening_production_config_example_is_disabled_without_invented_rights() {
    let mut example: ApprovedUniverseConfig =
        serde_json::from_str(include_str!("universe.example.json")).unwrap();
    example.validate().unwrap();
    assert!(!example.enabled && !example.rights.store_content && !example.rights.llm_processing);
    assert!(example.rights.terms_reference.is_empty());
    example.enabled = true;
    assert!(example.validate().is_err());
}

#[test]
fn screening_http_reuses_destination_protections_without_credentials_or_query_tokens() {
    for url in [
        "http://example.com/sp500.json",
        "https://127.0.0.1/sp500.json",
        "https://localhost/sp500.json",
        "https://example.com.evil.invalid/sp500.json",
        "https://user:password@example.com/sp500.json",
        "https://example.com:444/sp500.json",
        "https://example.com/sp500.json?apikey=forbidden",
        "https://example.com/sp500.json#fragment",
    ] {
        let mut c = config();
        c.url = url.into();
        assert!(c.validate().is_err(), "accepted {url}");
    }
    let mut private = config();
    private.url = "https://127.0.0.1/sp500.json".into();
    private.allowed_hosts = vec!["127.0.0.1".into()];
    assert!(private.validate().is_err());
}

#[test]
fn screening_http_rejects_partial_future_expired_wrong_provider_and_wrong_taxonomy_documents() {
    let cases: &[fn(&mut UniverseDocument)] = &[
        |d| d.complete = false,
        |d| d.index_id = "NASDAQ100".into(),
        |d| d.schema_version = 2,
        |d| d.provider = "unapproved provider".into(),
        |d| d.taxonomy_edition = "consultation_draft".into(),
        |d| d.as_of = now() + Duration::seconds(1),
        |d| d.available_at = now() + Duration::seconds(1),
        |d| d.expires_at = now(),
        |d| d.snapshot_id.clear(),
        |d| {
            d.members
                .resize(MAX_UNIVERSE_ROWS + 1, d.members[0].clone())
        },
    ];
    for change in cases {
        let mut doc = document();
        change(&mut doc);
        let (transport, _) = transport(vec![response(&doc)]);
        let mut source = HttpUniverseSource::with_transport(config(), transport).unwrap();
        assert!(source.fetch_universe(now()).is_err());
    }
}

#[test]
fn screening_http_rejects_unknown_fields_duplicates_oversized_bodies_and_redirects() {
    let mut unknown = response(&document());
    let mut value = serde_json::to_value(document()).unwrap();
    value["entry_authorized"] = true.into();
    unknown.body = serde_json::to_vec(&value).unwrap();
    let mut duplicate = response(&document());
    duplicate.body = String::from_utf8(duplicate.body)
        .unwrap()
        .replacen('{', "{\"complete\":true,", 1)
        .into_bytes();
    let mut oversized = response(&document());
    oversized.body = vec![b' '; MAX_BODY_BYTES + 1];
    let mut redirected = response(&document());
    redirected.final_url = Some(Url::parse("https://example.com/other.json").unwrap());
    let mut redirect_status = response(&document());
    redirect_status.status = 302;
    let mut not_modified = response(&document());
    not_modified.status = 304;
    let mut missing_url = response(&document());
    missing_url.final_url = None;
    for response in [
        unknown,
        duplicate,
        oversized,
        redirected,
        redirect_status,
        not_modified,
        missing_url,
    ] {
        let (transport, _) = transport(vec![response]);
        let mut source = HttpUniverseSource::with_transport(config(), transport).unwrap();
        assert!(source.fetch_universe(now()).is_err());
    }
}

#[test]
fn screening_http_enforces_request_interval_daily_cap_and_provider_backoff() {
    let mut capped = config();
    capped.daily_request_limit = 1;
    let (transport, requests) = transport(vec![response(&document())]);
    let mut source = HttpUniverseSource::with_transport(capped, transport).unwrap();
    source.fetch_universe(now()).unwrap();
    assert!(source.fetch_universe(now() + Duration::seconds(1)).is_err());
    assert!(
        source
            .fetch_universe(now() + Duration::seconds(61))
            .is_err()
    );
    assert_eq!(requests.borrow().len(), 1);
    let mut limited = response(&document());
    limited.status = 429;
    limited.retry_after = Some("600".into());
    let (transport, requests) = self::transport(vec![limited, response(&document())]);
    let mut source = HttpUniverseSource::with_transport(config(), transport).unwrap();
    assert!(source.fetch_universe(now()).is_err());
    assert_eq!(source.retry_at(), Some(now() + Duration::seconds(600)));
    assert!(
        source
            .fetch_universe(now() + Duration::seconds(61))
            .is_err()
    );
    source
        .fetch_universe(now() + Duration::seconds(600))
        .unwrap();
    assert_eq!(requests.borrow().len(), 2);
}

#[test]
fn screening_http_empty_universe_is_preserved_for_explicit_domain_blocking() {
    let mut doc = document();
    doc.members.clear();
    let (transport, _) = transport(vec![response(&doc)]);
    let mut source = HttpUniverseSource::with_transport(config(), transport).unwrap();
    assert!(source.fetch_universe(now()).unwrap().members.is_empty());
}

use chrono::{DateTime, Duration, Utc};
use std::collections::VecDeque;
use trady_application::research::*;
use trady_domain::{Result, require, research::*};

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .unwrap()
        .to_utc()
}
fn config() -> ResearchConfig {
    ResearchConfig {
        config_version: 1,
        max_pages: 1,
        page_size: 5,
        lookback_seconds: 3600,
        overlap_seconds: 120,
        sources: vec![SourceConfig {
            id: "fixture_news".into(),
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
                terms_reference: "fixture permission".into(),
            },
        }],
    }
}
#[derive(Default)]
struct Memory {
    state: SourceState,
    commits: usize,
}
impl ResearchRepository for Memory {
    fn state(&mut self, _: &str) -> Result<SourceState> {
        Ok(self.state.clone())
    }
    fn commit_page(
        &mut self,
        _: &str,
        expected: i64,
        state: &SourceState,
        _: &[SourceItem],
        _: DateTime<Utc>,
    ) -> Result<usize> {
        require(self.state.revision == expected, "concurrent change")?;
        self.state = state.clone();
        self.state.revision += 1;
        self.commits += 1;
        Ok(0)
    }
    fn read_items(
        &mut self,
        _: Option<&str>,
        _: usize,
        _: DateTime<Utc>,
        _: bool,
    ) -> Result<Vec<SourceItem>> {
        Ok(vec![])
    }
}
#[derive(Default)]
struct Provider {
    pages: VecDeque<std::result::Result<SourcePage, SourceFailure>>,
    requests: Vec<CollectionRequest>,
}
impl NewsSource for Provider {
    fn fetch(
        &mut self,
        _: &SourceConfig,
        request: &CollectionRequest,
        _: DateTime<Utc>,
    ) -> std::result::Result<SourcePage, SourceFailure> {
        self.requests.push(request.clone());
        self.pages.pop_front().expect("unexpected provider request")
    }
}
fn page(next: Option<&str>) -> std::result::Result<SourcePage, SourceFailure> {
    Ok(SourcePage {
        items: vec![],
        next_token: next.map(String::from),
        etag: Some("etag-1".into()),
        last_modified: None,
        complete: true,
        gaps: vec![],
    })
}

#[test]
fn capped_windows_resume_after_restart_without_advancing_on_failure() {
    let cfg = config();
    let source = &cfg.sources[0];
    let mut repo = Memory::default();
    let mut provider = Provider {
        pages: vec![
            page(Some("page-2")),
            Err(SourceFailure::unavailable("outage")),
            page(None),
        ]
        .into(),
        ..Default::default()
    };
    let first = collect_source(&cfg, source, &mut provider, &mut repo, now(), true).unwrap();
    assert_eq!(first.status, CollectionStatus::Partial);
    assert!(first.watermark.is_none());
    assert_eq!(repo.state.next_token.as_deref(), Some("page-2"));
    assert!(repo.state.etag.is_none());
    let pending = repo.state.clone();
    let second = collect_source(
        &cfg,
        source,
        &mut provider,
        &mut repo,
        now() + Duration::minutes(1),
        true,
    )
    .unwrap();
    assert_eq!(second.status, CollectionStatus::Unavailable);
    assert_eq!(repo.state.next_token, pending.next_token);
    assert!(repo.state.watermark.is_none());
    assert_eq!(repo.state.window_end, Some(now()));
    let third = collect_source(
        &cfg,
        source,
        &mut provider,
        &mut repo,
        now() + Duration::minutes(2),
        true,
    )
    .unwrap();
    assert_eq!(third.status, CollectionStatus::Complete);
    assert_eq!(third.watermark, Some(now()));
    assert!(repo.state.next_token.is_none());
    assert!(repo.state.window_end.is_none());
    assert_eq!(repo.state.etag.as_deref(), Some("etag-1"));
    assert!(provider.requests.iter().all(|r| r.end_at == now()));
    assert_eq!(repo.state.requests_today, 12); // Four possible HTTP hops reserved per RSS fetch.
}

#[test]
fn force_cannot_bypass_provider_retry_or_durable_daily_request_caps() {
    let mut cfg = config();
    cfg.sources[0].daily_request_limit = 4;
    let mut repo = Memory::default();
    let mut provider = Provider {
        pages: vec![Err(SourceFailure {
            status: CollectionStatus::RateLimited,
            reason: "quota".into(),
            retry_at: Some(now() + Duration::hours(1)),
        })]
        .into(),
        ..Default::default()
    };
    collect_source(&cfg, &cfg.sources[0], &mut provider, &mut repo, now(), true).unwrap();
    let before = repo.commits;
    collect_source(
        &cfg,
        &cfg.sources[0],
        &mut provider,
        &mut repo,
        now() + Duration::minutes(1),
        true,
    )
    .unwrap();
    assert_eq!(repo.commits, before);
    assert_eq!(provider.requests.len(), 1);
    // An ownership/terms refresh must not clear an outstanding provider backoff.
    cfg.sources[0].rights.terms_reference.push_str(" reviewed");
    collect_source(
        &cfg,
        &cfg.sources[0],
        &mut provider,
        &mut repo,
        now() + Duration::minutes(2),
        true,
    )
    .unwrap();
    assert_eq!(provider.requests.len(), 1);
    let report = collect_source(
        &cfg,
        &cfg.sources[0],
        &mut provider,
        &mut repo,
        now() + Duration::hours(2),
        true,
    )
    .unwrap();
    assert_eq!(report.status, CollectionStatus::RateLimited);
    assert_eq!(provider.requests.len(), 1);
    assert!(report.gaps.iter().any(|g| g.contains("daily_source")));
}

#[test]
fn partial_api_results_retain_cursor_and_unverified_accounts_never_fetch() {
    let mut cfg = config();
    let mut repo = Memory::default();
    let mut provider = Provider {
        pages: vec![Ok(SourcePage {
            complete: false,
            ..page(Some("untrusted-next")).unwrap()
        })]
        .into(),
        ..Default::default()
    };
    collect_source(&cfg, &cfg.sources[0], &mut provider, &mut repo, now(), true).unwrap();
    assert!(repo.state.watermark.is_none());
    assert!(repo.state.next_token.is_none());
    cfg.sources[0].kind = SourceKind::XTimeline;
    cfg.sources[0].x_account = Some(XAccount {
        person: "Fixture".into(),
        handle: "fixture".into(),
        user_id: None,
        verified_at: None,
        ownership_reference: String::new(),
        priority: 0,
        scope: vec![],
    });
    let report = collect_source(
        &cfg,
        &cfg.sources[0],
        &mut provider,
        &mut repo,
        now() + Duration::minutes(2),
        true,
    )
    .unwrap();
    assert_eq!(report.status, CollectionStatus::Disabled);
    assert_eq!(provider.requests.len(), 1);
}

#[test]
fn repeated_cursor_cannot_spin_and_source_changes_preserve_spent_budget() {
    let cfg = config();
    let source = &cfg.sources[0];
    let mut repo = Memory::default();
    let mut provider = Provider {
        pages: vec![page(Some("same")), page(Some("same"))].into(),
        ..Default::default()
    };
    collect_source(&cfg, source, &mut provider, &mut repo, now(), true).unwrap();
    let report = collect_source(
        &cfg,
        source,
        &mut provider,
        &mut repo,
        now() + Duration::minutes(1),
        true,
    )
    .unwrap();
    assert_eq!(report.status, CollectionStatus::Partial);
    assert!(report.gaps.contains(&"repeated_cursor".into()));
    let mut changed = source.clone();
    changed.query = Some("new approved scope".into());
    let next = prepare_state(&changed, repo.state.clone()).unwrap();
    assert!(next.window_end.is_none());
    assert!(next.next_token.is_none());
    assert_eq!(next.requests_today, 8);
    assert_eq!(next.revision, repo.state.revision);
}

use chrono::{DateTime, Duration, Utc};
use std::collections::VecDeque;
use trady_application::{research::*, x_monitoring::*};
use trady_domain::research::*;
use trady_domain::{Result, require};

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .unwrap()
        .to_utc()
}

#[test]
fn separate_pilot_is_disabled_and_does_not_replace_owner_registry() {
    let pilot: ResearchConfig =
        serde_json::from_slice(include_bytes!("../../../config/x-pilot.example.json")).unwrap();
    pilot.validate().unwrap();
    assert!(!pilot.x_budget.as_ref().unwrap().enabled);
    assert_eq!(
        pilot
            .sources
            .iter()
            .map(|s| s.x_account.as_ref().unwrap().handle.as_str())
            .collect::<Vec<_>>(),
        vec![
            "BillAckman",
            "elonmusk",
            "SecScottBessent",
            "michaeljburry",
            "sama"
        ]
    );
    assert!(pilot.sources.iter().all(|s| !s.enabled
        && !s.rights.store_content
        && !s.rights.llm_processing
        && s.x_account.as_ref().unwrap().user_id.is_none()
        && !s.x_rule.as_ref().unwrap().exclude_reposts));
    let current: ResearchConfig =
        serde_json::from_slice(include_bytes!("../../../config/research.json")).unwrap();
    // Absent new fields must preserve legacy source fingerprints and pending cursors on upgrade.
    assert!(!serde_json::to_string(&current).unwrap().contains("x_rule"));
    assert!(
        !serde_json::to_string(&current)
            .unwrap()
            .contains("x_budget")
    );
    assert_eq!(
        current
            .sources
            .iter()
            .filter_map(|s| s.x_account.as_ref())
            .map(|a| a.handle.as_str())
            .collect::<Vec<_>>(),
        vec![
            "realDonaldTrump",
            "elonmusk",
            "TheRoaringKitty",
            "BillAckman",
            "Carl_C_Icahn"
        ]
    );
}

#[test]
fn query_grammar_rejects_scope_injection_and_requires_explicit_revision_changes() {
    let mut rule = XRule {
        version: 1,
        terms: vec!["Tesla".into(), "data center".into()],
        exclude_reposts: false,
        approved_at: Some(now()),
        approval_reference: "owner fixture".into(),
    };
    assert_eq!(
        rule.query("fixture").unwrap(),
        "from:fixture (Tesla OR \"data center\")"
    );
    for term in [
        "OR",
        "from:other",
        "Tesla) OR from:other",
        "-is:reply",
        "two  spaces",
        "\n",
    ] {
        rule.terms = vec![term.into()];
        assert!(rule.query("fixture").is_err());
    }
    let cfg: ResearchConfig =
        serde_json::from_slice(include_bytes!("../../../config/x-pilot.example.json")).unwrap();
    let mut source = cfg.sources[0].clone();
    source.x_rule.as_mut().unwrap().approved_at = Some(now());
    let state = prepare_state(&source, SourceState::default()).unwrap();
    source.x_rule.as_mut().unwrap().terms.push("newterm".into());
    assert!(prepare_state(&source, state.clone()).is_err());
    source.x_rule.as_mut().unwrap().version += 1;
    let revised = prepare_state(&source, state).unwrap();
    assert_eq!(revised.coverage_revision, 1);
    assert!(revised.x_counts.is_none());
    assert!(revised.watermark.is_none());
}

#[test]
fn counts_projection_is_checked_and_incomplete_counts_are_unknown() {
    let cfg: ResearchConfig =
        serde_json::from_slice(include_bytes!("../../../config/x-pilot.example.json")).unwrap();
    let budget = cfg.x_budget.unwrap();
    let mut count = XCountsMeasurement {
        rule_hash: "fixture".into(),
        query: "from:fixture".into(),
        start_at: now() - Duration::days(7),
        end_at: now(),
        measured_at: now(),
        complete: true,
        total_posts: 70,
        buckets: vec![],
        next_token: None,
        gaps: vec![],
    };
    // Ten daily posts, 30 days, 150% margin, $0.005 each.
    assert_eq!(planned_post_cost(&count, &budget).unwrap(), 2_250_000);
    count.complete = false;
    assert!(planned_post_cost(&count, &budget).is_err());
    count.complete = true;
    count.start_at = count.end_at;
    assert!(planned_post_cost(&count, &budget).is_err());
}

#[test]
fn rules_and_billing_require_owner_review_without_granting_execution_authority() {
    let mut cfg: ResearchConfig =
        serde_json::from_slice(include_bytes!("../../../config/x-pilot.example.json")).unwrap();
    let source = &mut cfg.sources[0];
    source.enabled = true;
    source.rights.terms_reference = "fixture terms".into();
    assert_eq!(
        source.activation_gap(now()),
        Some("x_watchlist_and_coverage_not_approved")
    );
    source.x_rule.as_mut().unwrap().approval_reference = "owner replacement and coverage".into();
    source.x_rule.as_mut().unwrap().approved_at = Some(now());
    assert_eq!(
        source.activation_gap(now()),
        Some("x_identity_not_verified")
    );
    let mut budget = cfg.x_budget.unwrap();
    budget.enabled = true;
    assert!(budget.validate().is_err());
    budget.approval_reference = "owner".into();
    budget.entitlement_reference = "provider".into();
    budget.provider_limit_reference = "console limit".into();
    budget.cycle_start = now() - Duration::days(1);
    budget.cycle_end = now() + Duration::days(29);
    budget.active(now()).unwrap();
    assert!(budget.active(budget.cycle_end).is_err());
}

#[derive(Default)]
struct Memory {
    state: SourceState,
    fail_commit: bool,
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
        require(
            !self.fail_commit && expected == self.state.revision,
            "fixture commit failure",
        )?;
        self.state = state.clone();
        self.state.revision += 1;
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
struct Counts {
    modes: VecDeque<u8>,
    requests: Vec<CollectionRequest>,
}
impl XCountsSource for Counts {
    fn counts(
        &mut self,
        source: &SourceConfig,
        window: &CollectionRequest,
        now: DateTime<Utc>,
    ) -> std::result::Result<XCountsMeasurement, SourceFailure> {
        self.requests.push(window.clone());
        let mode = self.modes.pop_front().unwrap();
        if mode == 0 {
            return Err(SourceFailure::unavailable("fixture outage"));
        }
        let mid = window.start_at + Duration::days(3);
        let (start, end, next, total) = if mode == 1 {
            (window.start_at, mid, Some("page-2".into()), 5)
        } else {
            (mid, window.end_at, None, 7)
        };
        let account = source.x_account.as_ref().unwrap();
        let rule = source.x_rule.as_ref().unwrap();
        Ok(XCountsMeasurement {
            rule_hash: rule
                .hash(&account.handle, account.user_id.as_deref().unwrap())
                .unwrap(),
            query: rule.query(&account.handle).unwrap(),
            start_at: window.start_at,
            end_at: window.end_at,
            measured_at: now,
            complete: false,
            total_posts: total,
            buckets: vec![XCountBucket {
                start,
                end,
                post_count: total,
            }],
            next_token: next,
            gaps: vec![],
        })
    }
}
fn enabled_config() -> ResearchConfig {
    let mut cfg: ResearchConfig =
        serde_json::from_slice(include_bytes!("../../../config/x-pilot.example.json")).unwrap();
    cfg.sources.truncate(1);
    cfg.max_pages = 1;
    let s = &mut cfg.sources[0];
    s.enabled = true;
    s.interval_seconds = 60;
    s.rights.terms_reference = "fixture rights".into();
    let account = s.x_account.as_mut().unwrap();
    account.user_id = Some("42".into());
    account.verified_at = Some(now());
    account.ownership_reference = "fixture ownership".into();
    let rule = s.x_rule.as_mut().unwrap();
    rule.approved_at = Some(now());
    rule.approval_reference = "fixture owner replacement and coverage".into();
    let b = cfg.x_budget.as_mut().unwrap();
    b.enabled = true;
    b.cycle_start = now() - Duration::days(1);
    b.cycle_end = now() + Duration::days(29);
    b.approval_reference = "owner".into();
    b.entitlement_reference = "provider".into();
    b.provider_limit_reference = "console limit".into();
    cfg
}

#[test]
fn preflight_counts_resume_the_same_interval_after_restart_and_never_move_the_item_watermark() {
    let cfg = enabled_config();
    let source = &cfg.sources[0];
    let mut repo = Memory::default();
    let mut counts = Counts {
        modes: vec![1, 2].into(),
        requests: vec![],
    };
    let first = measure_x_counts(
        &cfg,
        source,
        &mut counts,
        &mut repo,
        now() + Duration::nanoseconds(123),
    )
    .unwrap();
    assert_eq!(first.status, CollectionStatus::Partial);
    assert!(first.planned_cycle_post_microusd.is_none());
    let checkpoint = repo.state.clone();
    assert_eq!(
        checkpoint.x_counts.as_ref().unwrap().next_token.as_deref(),
        Some("page-2")
    );
    let mut restarted = Memory {
        state: checkpoint,
        ..Default::default()
    };
    let second = measure_x_counts(
        &cfg,
        source,
        &mut counts,
        &mut restarted,
        now() + Duration::seconds(1),
    )
    .unwrap();
    assert_eq!(second.status, CollectionStatus::Complete);
    assert!(second.planned_cycle_post_microusd.is_some());
    let measurement = second.measurement.unwrap();
    assert!(measurement.complete);
    assert_eq!(measurement.total_posts, 12);
    assert_eq!(counts.requests[0].start_at, counts.requests[1].start_at);
    assert_eq!(counts.requests[0].end_at, counts.requests[1].end_at);
    assert_eq!(counts.requests[0].start_at.timestamp_subsec_nanos(), 0);
    assert!(restarted.state.watermark.is_none());
}

struct NoPosts;
impl NewsSource for NoPosts {
    fn fetch(
        &mut self,
        _: &SourceConfig,
        _: &CollectionRequest,
        _: DateTime<Utc>,
    ) -> std::result::Result<SourcePage, SourceFailure> {
        panic!("missing evidence must block new reads")
    }
}
#[test]
fn incomplete_preflight_and_failed_reservations_stop_collection_before_any_provider_call() {
    let cfg = enabled_config();
    let source = &cfg.sources[0];
    let mut repo = Memory::default();
    let report = collect_source(&cfg, source, &mut NoPosts, &mut repo, now(), true).unwrap();
    assert_eq!(report.status, CollectionStatus::Disabled);
    assert!(report.gaps.iter().any(|g| g.contains("preflight")));
    let mut repo = Memory {
        fail_commit: true,
        ..Default::default()
    };
    let mut counts = Counts {
        modes: VecDeque::new(),
        requests: vec![],
    };
    assert!(measure_x_counts(&cfg, source, &mut counts, &mut repo, now()).is_err());
    assert!(counts.requests.is_empty());
}

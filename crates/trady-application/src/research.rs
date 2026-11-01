//! Bounded collection orchestration. Providers cannot grant policy or order authority.

use crate::x_monitoring::{XBudgetPolicy, XRule, planned_post_cost};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use trady_domain::{Error, Result, require, research::*};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XAccount {
    pub person: String,
    pub handle: String,
    pub user_id: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub ownership_reference: String,
    pub priority: u8,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub id: String,
    pub kind: SourceKind,
    pub enabled: bool,
    pub url: String,
    pub allowed_hosts: Vec<String>,
    pub credential_env: Option<String>,
    pub interval_seconds: u32,
    pub daily_request_limit: u32,
    pub query: Option<String>,
    pub x_account: Option<XAccount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_rule: Option<XRule>,
    pub rights: SourceRights,
}

impl SourceConfig {
    pub fn validate(&self) -> Result<()> {
        require(valid_key(&self.id), "invalid research source ID")?;
        require(
            (60..=86400).contains(&self.interval_seconds)
                && (1..=10000).contains(&self.daily_request_limit)
                && (1..=8760).contains(&self.rights.retention_hours)
                && self.allowed_hosts.len() <= 16
                && !self.allowed_hosts.is_empty()
                && self.allowed_hosts.iter().all(|h| {
                    !h.is_empty()
                        && h.len() <= 253
                        && h.bytes().all(|c| {
                            c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'-'
                        })
                }),
            "invalid source interval, retention or host allowlist",
        )?;
        require(self.url.len() <= 2048, "source URL is too long")?;
        require(
            self.query.as_ref().is_none_or(|q| {
                !q.trim().is_empty() && q.len() <= 256 && !q.chars().any(char::is_control)
            }),
            "invalid source query",
        )?;
        require(
            self.credential_env.as_ref().is_none_or(|s| {
                s.starts_with("TRADY_")
                    && s.len() <= 96
                    && s.bytes()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
                    && !s.contains("BROKER")
                    && !s.contains("212")
                    && !s.ends_with("_DSN")
            }),
            "invalid source credential reference",
        )?;
        if self.enabled {
            require(
                !self.rights.terms_reference.trim().is_empty()
                    && self.rights.terms_reference.len() <= 512,
                "enabled sources require a reviewed terms reference",
            )?;
        }
        if matches!(self.kind, SourceKind::XTimeline | SourceKind::XRecentSearch) {
            let a = self
                .x_account
                .as_ref()
                .ok_or_else(|| Error("X collection requires an account".into()))?;
            require(
                !a.handle.is_empty()
                    && a.handle.len() <= 15
                    && a.handle
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    && a.user_id.as_ref().is_none_or(|id| valid_x_id(id))
                    && a.priority <= 1
                    && a.scope.len() <= 16,
                "invalid X account configuration",
            )?;
        } else {
            require(
                self.x_account.is_none(),
                "account belongs only to an X collector",
            )?;
        }
        if self.kind == SourceKind::XRecentSearch {
            let rule = self
                .x_rule
                .as_ref()
                .ok_or_else(|| Error("X search requires a versioned rule".into()))?;
            rule.query(&self.x_account.as_ref().expect("validated account").handle)?;
            require(
                self.query.is_none(),
                "X search uses the versioned rule, not a separate query",
            )?;
        } else {
            require(
                self.x_rule.is_none(),
                "X rule belongs only to recent search",
            )?;
        }
        Ok(())
    }

    pub fn activation_gap(&self, now: DateTime<Utc>) -> Option<&'static str> {
        if !self.enabled {
            return Some("source_disabled");
        }
        if let Some(rule) = &self.x_rule
            && (rule.approval_reference.trim().is_empty()
                || rule.approved_at.is_none_or(|at| at > now))
        {
            return Some("x_watchlist_and_coverage_not_approved");
        }
        if let Some(a) = &self.x_account {
            if a.user_id.is_none() || a.ownership_reference.trim().is_empty() {
                return Some("x_identity_not_verified");
            }
            if !a
                .verified_at
                .is_some_and(|at| at <= now && now - at <= Duration::days(7))
            {
                return Some("x_identity_review_expired");
            }
        }
        None
    }
}

pub fn valid_key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

pub fn valid_x_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 19 && s.bytes().all(|c| c.is_ascii_digit()) && s != "0"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchConfig {
    pub config_version: u32,
    pub max_pages: usize,
    pub page_size: usize,
    pub lookback_seconds: i64,
    pub overlap_seconds: i64,
    pub sources: Vec<SourceConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_budget: Option<XBudgetPolicy>,
}

impl ResearchConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1
                && (1..=10).contains(&self.max_pages)
                && (5..=100).contains(&self.page_size)
                && (60..=604800).contains(&self.lookback_seconds)
                && (1..=3600).contains(&self.overlap_seconds)
                && self.overlap_seconds < self.lookback_seconds
                && !self.sources.is_empty()
                && self.sources.len() <= 32,
            "invalid research configuration or collection bounds",
        )?;
        let mut ids = BTreeSet::new();
        let mut authors = BTreeSet::new();
        let mut user_ids = BTreeSet::new();
        if let Some(budget) = &self.x_budget {
            budget.validate()?;
        }
        for source in &self.sources {
            source.validate()?;
            require(ids.insert(&source.id), "duplicate research source ID")?;
            if let Some(account) = &source.x_account {
                require(
                    authors.insert(account.handle.to_ascii_lowercase()),
                    "overlapping primary X collectors",
                )?;
                if let Some(id) = &account.user_id {
                    require(user_ids.insert(id), "overlapping primary X stable IDs")?;
                }
            }
            if source.kind == SourceKind::XRecentSearch {
                require(
                    self.page_size >= 10,
                    "X recent search requires page_size at least 10",
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CollectionRequest {
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub next_token: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct SourcePage {
    pub items: Vec<SourceItem>,
    pub next_token: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// False for a provider partial error, parser cap or unsupported history recovery.
    pub complete: bool,
    pub gaps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SourceFailure {
    pub status: CollectionStatus,
    pub reason: String,
    pub retry_at: Option<DateTime<Utc>>,
}
impl SourceFailure {
    pub fn unavailable(reason: &str) -> Self {
        Self {
            status: CollectionStatus::Unavailable,
            reason: reason.into(),
            retry_at: None,
        }
    }
}

pub trait NewsSource {
    fn fetch(
        &mut self,
        source: &SourceConfig,
        request: &CollectionRequest,
        now: DateTime<Utc>,
    ) -> std::result::Result<SourcePage, SourceFailure>;
}

pub trait ResearchRepository {
    /// Production repositories reserve a shared provider/credential allowance before I/O.
    /// The default supports offline in-memory fixtures without a database.
    fn reserve_shared_requests(
        &mut self,
        _source: &SourceConfig,
        _at: DateTime<Utc>,
        _requests: u32,
    ) -> Result<bool> {
        Ok(true)
    }
    fn state(&mut self, source_id: &str) -> Result<SourceState>;
    /// Items, status, cursor, and audit event are committed in one compare-and-swap.
    fn commit_page(
        &mut self,
        source_id: &str,
        expected_revision: i64,
        state: &SourceState,
        items: &[SourceItem],
        at: DateTime<Utc>,
    ) -> Result<usize>;
    fn read_items(
        &mut self,
        source_id: Option<&str>,
        limit: usize,
        now: DateTime<Utc>,
        for_model: bool,
    ) -> Result<Vec<SourceItem>>;
}

pub fn prepare_state(source: &SourceConfig, mut state: SourceState) -> Result<SourceState> {
    if let (Some(old), Some(rule), Some(account)) =
        (&state.coverage, &source.x_rule, &source.x_account)
    {
        let hash = rule.hash(
            &account.handle,
            account.user_id.as_deref().unwrap_or_default(),
        )?;
        if old.approved_at.is_some() && old.rule_hash.as_ref().is_some_and(|prior| prior != &hash) {
            require(
                old.rule_version
                    .is_none_or(|version| rule.version > version),
                "changed X rule requires a new approved version",
            )?;
        }
    }
    let hash: String = Sha256::digest(
        serde_json::to_vec(source)
            .map_err(|_| Error("source registry serialization failed".into()))?,
    )
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect();
    if state.registry_hash.as_ref().is_some_and(|old| old != &hash) {
        state.watermark = None;
        state.window_start = None;
        state.window_end = None;
        state.next_token = None;
        state.etag = None;
        state.last_modified = None;
        state.last_attempt_at = None;
        state.x_counts = None;
        state.coverage_revision = state
            .coverage_revision
            .checked_add(1)
            .ok_or_else(|| Error("source coverage revision overflow".into()))?;
    }
    state.registry_hash = Some(hash);
    state.rights = Some(source.rights.clone());
    let rule = source.x_rule.as_ref();
    state.coverage = Some(SourceCoverage {
        transport: source.kind,
        revision: state.coverage_revision,
        rule_version: rule.map(|r| r.version),
        rule_hash: match (rule, source.x_account.as_ref()) {
            (Some(rule), Some(account)) => Some(rule.hash(
                &account.handle,
                account.user_id.as_deref().unwrap_or_default(),
            )?),
            _ => None,
        },
        query: match (rule, source.x_account.as_ref()) {
            (Some(rule), Some(account)) => Some(rule.query(&account.handle)?),
            _ => None,
        },
        approved_at: rule.and_then(|r| r.approved_at),
        approval_reference: rule.map(|r| r.approval_reference.clone()),
    });
    Ok(state)
}

/// Reserve an upper bound before network I/O; a process crash cannot refund API use.
pub fn reserve_source_requests(
    source: &SourceConfig,
    repository: &mut impl ResearchRepository,
    mut state: SourceState,
    at: DateTime<Utc>,
    requests: u32,
) -> Result<(SourceState, bool)> {
    require(
        requests > 0 && requests <= 4,
        "invalid source request reservation",
    )?;
    if state.request_day != Some(at.date_naive()) {
        state.request_day = Some(at.date_naive());
        state.requests_today = 0;
    }
    let allowed = state.requests_today.saturating_add(requests) <= source.daily_request_limit
        && repository.reserve_shared_requests(source, at, requests)?;
    state.last_attempt_at = Some(at);
    if allowed {
        state.requests_today += requests;
        state.last_status = Some(CollectionStatus::Partial);
        state.gaps = vec!["request_budget_reserved_before_fetch".into()];
    } else {
        state.last_status = Some(CollectionStatus::RateLimited);
        state.gaps = vec!["daily_source_request_budget_exhausted".into()];
        state.retry_at = at
            .date_naive()
            .succ_opt()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|t| t.and_utc());
    }
    repository.commit_page(&source.id, state.revision, &state, &[], at)?;
    state.revision = state
        .revision
        .checked_add(1)
        .ok_or_else(|| Error("source revision overflow".into()))?;
    Ok((state, allowed))
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionReport {
    pub source_id: String,
    pub status: CollectionStatus,
    pub pages: usize,
    pub changed_items: usize,
    pub watermark: Option<DateTime<Utc>>,
    pub retry_at: Option<DateTime<Utc>>,
    pub gaps: Vec<String>,
}

pub fn collect_source(
    config: &ResearchConfig,
    source: &SourceConfig,
    provider: &mut impl NewsSource,
    repository: &mut impl ResearchRepository,
    now: DateTime<Utc>,
    force: bool,
) -> Result<CollectionReport> {
    config.validate()?;
    require(
        config.sources.iter().any(|s| s == source),
        "source is not in the approved registry",
    )?;
    let mut state = prepare_state(source, repository.state(&source.id)?)?;
    let mut report = CollectionReport {
        source_id: source.id.clone(),
        status: CollectionStatus::Partial,
        pages: 0,
        changed_items: 0,
        watermark: state.watermark,
        retry_at: state.retry_at,
        gaps: vec![],
    };
    if let Some(gap) = source.activation_gap(now) {
        state.last_status = Some(CollectionStatus::Disabled);
        state.gaps = vec![gap.into()];
        state.last_attempt_at = Some(now);
        repository.commit_page(&source.id, state.revision, &state, &[], now)?;
        report.status = CollectionStatus::Disabled;
        report.gaps = state.gaps;
        return Ok(report);
    }
    if matches!(
        source.kind,
        SourceKind::XTimeline | SourceKind::XLookup | SourceKind::XRecentSearch
    ) && config
        .x_budget
        .as_ref()
        .is_none_or(|budget| budget.active(now).is_err())
    {
        state.last_status = Some(CollectionStatus::Disabled);
        state.gaps = vec!["x_active_reviewed_billing_policy_required".into()];
        state.last_attempt_at = Some(now);
        repository.commit_page(&source.id, state.revision, &state, &[], now)?;
        report.status = CollectionStatus::Disabled;
        report.gaps = state.gaps;
        return Ok(report);
    }
    if source.kind == SourceKind::XRecentSearch {
        let account = source.x_account.as_ref().expect("validated account");
        let hash = source.x_rule.as_ref().expect("validated rule").hash(
            &account.handle,
            account.user_id.as_deref().unwrap_or_default(),
        )?;
        let ready = state.x_counts.as_ref().is_some_and(|count| {
            count.complete
                && count.rule_hash == hash
                && count.measured_at <= now
                && now - count.measured_at <= Duration::days(7)
                && config.x_budget.as_ref().is_some_and(|budget| {
                    planned_post_cost(count, budget)
                        .is_ok_and(|cost| cost <= budget.ordinary_microusd)
                })
        });
        if !ready {
            state.last_status = Some(CollectionStatus::Disabled);
            state.last_attempt_at = Some(now);
            state.gaps = vec!["x_complete_affordable_preflight_counts_required".into()];
            repository.commit_page(&source.id, state.revision, &state, &[], now)?;
            report.status = CollectionStatus::Disabled;
            report.gaps = state.gaps;
            return Ok(report);
        }
    }
    // Older versions persisted cadence as a retry deadline after this local
    // parser failure. A forced repair can clear that exact legacy condition;
    // provider deadlines and request/cost reservations remain authoritative.
    let repaired_parser_cadence = force
        && state.last_status == Some(CollectionStatus::Unavailable)
        && state.gaps == ["x_repost_exclusion_not_verified"]
        && state.last_attempt_at.is_some_and(|attempt| {
            state.retry_at == Some(attempt + Duration::seconds(i64::from(source.interval_seconds)))
        });
    if (state.retry_at.is_some_and(|at| at > now) && !repaired_parser_cadence)
        || (!force
            && state
                .last_attempt_at
                .is_some_and(|at| now - at < Duration::seconds(i64::from(source.interval_seconds))))
    {
        report.status = state.last_status.unwrap_or(CollectionStatus::Partial);
        report.gaps = vec!["collection_not_due".into()];
        return Ok(report);
    }
    let end = state
        .window_end
        .unwrap_or(if source.kind == SourceKind::XRecentSearch {
            DateTime::from_timestamp((now - Duration::seconds(30)).timestamp(), 0)
                .ok_or_else(|| Error("invalid X search timestamp".into()))?
        } else {
            now
        });
    let mut start = match state.window_start {
        Some(at) => at,
        None => {
            state
                .watermark
                .unwrap_or(now - Duration::seconds(config.lookback_seconds))
                - Duration::seconds(config.overlap_seconds)
        }
    };
    if source.kind == SourceKind::XRecentSearch {
        start = DateTime::from_timestamp(start.timestamp(), 0)
            .ok_or_else(|| Error("invalid X search timestamp".into()))?;
    }
    require(
        start < end && end <= now,
        "invalid stored collection interval",
    )?;
    state.window_start = Some(start);
    state.window_end = Some(end);
    state.retry_at = None;
    for page_number in 0..config.max_pages {
        let request = CollectionRequest {
            start_at: start,
            end_at: end,
            next_token: state.next_token.clone(),
            etag: state.etag.clone(),
            last_modified: state.last_modified.clone(),
            limit: config.page_size,
        };
        let requests = match source.kind {
            SourceKind::XTimeline | SourceKind::XRecentSearch => 2,
            SourceKind::GoogleNewsRss | SourceKind::PublisherRss => 4,
            _ => 1,
        };
        let (reserved, allowed) =
            reserve_source_requests(source, repository, state, now, requests)?;
        state = reserved;
        if !allowed {
            report.status = CollectionStatus::RateLimited;
            break;
        }
        let old_revision = state.revision;
        match provider.fetch(source, &request, now) {
            Err(failure) => {
                state.last_status = Some(failure.status);
                // Cadence is already enforced through last_attempt_at. Only an
                // explicit provider deadline must also block a forced retry.
                state.retry_at = failure.retry_at;
                state.gaps = vec![failure.reason];
                repository.commit_page(&source.id, old_revision, &state, &[], now)?;
                report.status = failure.status;
                break;
            }
            Ok(page) => {
                require(
                    page.items.len() <= config.page_size,
                    "provider exceeded item budget",
                )?;
                require(
                    page.items
                        .iter()
                        .all(|i| i.source_id == source.id && i.rights == source.rights),
                    "provider returned items outside source scope or rights",
                )?;
                if let Some(token) = &page.next_token {
                    require(
                        !token.is_empty() && token.len() <= 2048,
                        "invalid continuation token",
                    )?;
                }
                state.gaps = page.gaps;
                let repeated_cursor =
                    page.next_token.is_some() && page.next_token == state.next_token;
                let complete = page.complete && page.next_token.is_none() && !repeated_cursor;
                if page.complete && !repeated_cursor {
                    state.next_token = page.next_token;
                }
                state.last_status = Some(if complete {
                    CollectionStatus::Complete
                } else {
                    CollectionStatus::Partial
                });
                if repeated_cursor {
                    state.gaps.push("repeated_cursor".into());
                }
                if !complete && page_number + 1 == config.max_pages {
                    state.gaps.push("page_budget_exhausted".into());
                }
                if complete {
                    state.watermark = Some(end);
                    state.window_start = None;
                    state.window_end = None;
                    state.etag = page.etag.or(state.etag);
                    state.last_modified = page.last_modified.or(state.last_modified);
                }
                report.changed_items +=
                    repository.commit_page(&source.id, old_revision, &state, &page.items, now)?;
                state.revision = old_revision
                    .checked_add(1)
                    .ok_or_else(|| Error("source revision overflow".into()))?;
                report.pages += 1;
                report.status = state.last_status.unwrap_or(CollectionStatus::Partial);
                if complete || !page.complete || repeated_cursor {
                    break;
                }
            }
        }
    }
    report.watermark = state.watermark;
    report.retry_at = state.retry_at;
    report.gaps = state.gaps;
    Ok(report)
}

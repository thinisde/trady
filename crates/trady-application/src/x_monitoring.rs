//! Collector-owned X billing and query contracts. Amounts are integer USD microdollars.

use crate::research::{
    CollectionRequest, ResearchConfig, ResearchRepository, SourceConfig, SourceFailure,
    prepare_state, reserve_source_requests,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use trady_domain::{Error, Result, require, research::*};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XRule {
    pub version: u32,
    pub terms: Vec<String>,
    pub exclude_reposts: bool,
    pub approved_at: Option<DateTime<Utc>>,
    pub approval_reference: String,
}

impl XRule {
    pub fn validate(&self) -> Result<()> {
        require(
            self.version > 0
                && self.terms.len() <= 32
                && self.approval_reference.len() <= 512
                && self.terms.iter().all(|term| {
                    !term.is_empty()
                        && term.len() <= 48
                        && term.trim() == term
                        && !term.contains("  ")
                        && !matches!(term.to_ascii_uppercase().as_str(), "OR" | "AND" | "NOT")
                        && term.bytes().all(|c| c.is_ascii_alphanumeric() || c == b' ')
                }),
            "invalid versioned X rule",
        )
    }

    /// Restrict the grammar to one author, literal terms and an explicit repost choice.
    pub fn query(&self, handle: &str) -> Result<String> {
        self.validate()?;
        require(
            !handle.is_empty()
                && handle.len() <= 15
                && handle
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_'),
            "invalid X rule author",
        )?;
        let mut query = format!("from:{handle}");
        if !self.terms.is_empty() {
            let terms: Vec<_> = self
                .terms
                .iter()
                .map(|term| {
                    if term.contains(' ') {
                        format!("\"{term}\"")
                    } else {
                        term.clone()
                    }
                })
                .collect();
            query.push_str(&format!(" ({})", terms.join(" OR ")));
        }
        if self.exclude_reposts {
            query.push_str(" -is:retweet");
        }
        require(query.len() <= 512, "X pilot query exceeds reviewed bound")?;
        Ok(query)
    }

    pub fn hash(&self, handle: &str, user_id: &str) -> Result<String> {
        fingerprint(&(
            self.version,
            self.query(handle)?,
            user_id,
            &self.approval_reference,
            self.approved_at,
        ))
    }
}

pub fn fingerprint(value: &impl Serialize) -> Result<String> {
    let bytes =
        serde_json::to_vec(value).map_err(|_| Error("X policy serialization failed".into()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XBudgetPolicy {
    pub enabled: bool,
    pub billing_scope: String,
    pub version: u32,
    pub cycle_start: DateTime<Utc>,
    pub cycle_end: DateTime<Utc>,
    pub ceiling_microusd: u64,
    pub ordinary_microusd: u64,
    pub support_microusd: u64,
    pub post_read_microusd: u64,
    pub user_read_microusd: u64,
    pub recent_counts_microusd: u64,
    pub burst_margin_percent: u32,
    pub approval_reference: String,
    pub entitlement_reference: String,
    pub provider_limit_reference: String,
}

impl XBudgetPolicy {
    pub fn validate(&self) -> Result<()> {
        require(
            crate::research::valid_key(&self.billing_scope)
                && self.version > 0
                && self.cycle_start < self.cycle_end
                && self.cycle_end - self.cycle_start <= Duration::days(32)
                && self.ceiling_microusd > 0
                && self.ceiling_microusd <= 1_000_000_000
                && self.ordinary_microusd > 0
                && self.support_microusd > 0
                && self.ordinary_microusd.checked_add(self.support_microusd)
                    == Some(self.ceiling_microusd)
                && [
                    self.post_read_microusd,
                    self.user_read_microusd,
                    self.recent_counts_microusd,
                ]
                .iter()
                .all(|rate| (1..=1_000_000).contains(rate))
                && (100..=1000).contains(&self.burst_margin_percent)
                && [
                    &self.approval_reference,
                    &self.entitlement_reference,
                    &self.provider_limit_reference,
                ]
                .iter()
                .all(|s| s.len() <= 512 && (!self.enabled || !s.trim().is_empty())),
            "invalid X billing policy or missing owner/provider review",
        )
    }

    pub fn active(&self, at: DateTime<Utc>) -> Result<()> {
        self.validate()?;
        require(
            self.enabled && self.cycle_start <= at && at < self.cycle_end,
            "X billing policy disabled or outside approved cycle",
        )
    }

    pub fn estimate(&self, resources: XResources) -> Result<u64> {
        u64::from(resources.posts)
            .checked_mul(self.post_read_microusd)
            .and_then(|n| {
                u64::from(resources.users)
                    .checked_mul(self.user_read_microusd)
                    .and_then(|u| n.checked_add(u))
            })
            .and_then(|n| {
                u64::from(resources.counts)
                    .checked_mul(self.recent_counts_microusd)
                    .and_then(|c| n.checked_add(c))
            })
            .ok_or_else(|| Error("X resource estimate overflow".into()))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XResources {
    pub posts: u32,
    pub users: u32,
    pub counts: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XCostPlan {
    pub ordinary: XResources,
    pub support: XResources,
}

impl XCostPlan {
    pub fn costs(&self, policy: &XBudgetPolicy) -> Result<(u64, u64)> {
        Ok((
            policy.estimate(self.ordinary)?,
            policy.estimate(self.support)?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XCostScope {
    pub source_id: String,
    pub registry_hash: String,
    pub rule_hash: Option<String>,
}

/// Serialize reservations across every consumer sharing the provider billing scope.
pub trait XCostRepository {
    fn reserve(
        &mut self,
        policy: &XBudgetPolicy,
        scope: &XCostScope,
        plan: XCostPlan,
        at: DateTime<Utc>,
    ) -> Result<Option<i64>>;
    /// None retains the conservative bound, including after uncertain transport failures.
    fn settle(&mut self, reservation: i64, actual: Option<XCostPlan>, outcome: &str) -> Result<()>;
}

/// Planning estimate only. Missing/incomplete counts never become a zero-volume estimate.
pub fn planned_post_cost(measurement: &XCountsMeasurement, policy: &XBudgetPolicy) -> Result<u64> {
    require(
        measurement.complete && measurement.start_at < measurement.end_at,
        "complete X counts required for planning",
    )?;
    let seconds = u128::try_from((measurement.end_at - measurement.start_at).num_seconds())
        .map_err(|_| Error("invalid X counts interval".into()))?;
    let cycle = u128::try_from((policy.cycle_end - policy.cycle_start).num_seconds())
        .map_err(|_| Error("invalid X budget interval".into()))?;
    let numerator = u128::from(measurement.total_posts)
        .checked_mul(cycle)
        .and_then(|n| n.checked_mul(u128::from(policy.burst_margin_percent)))
        .and_then(|n| n.checked_mul(u128::from(policy.post_read_microusd)))
        .ok_or_else(|| Error("X count projection overflow".into()))?;
    let divisor = seconds
        .checked_mul(100)
        .filter(|n| *n > 0)
        .ok_or_else(|| Error("invalid X counts interval".into()))?;
    u64::try_from(numerator.div_ceil(divisor))
        .map_err(|_| Error("X projection exceeds amount bound".into()))
}

pub trait XCountsSource {
    fn counts(
        &mut self,
        source: &SourceConfig,
        window: &CollectionRequest,
        now: DateTime<Utc>,
    ) -> std::result::Result<XCountsMeasurement, SourceFailure>;
}

#[derive(Debug, Serialize)]
pub struct XCountsReport {
    pub source_id: String,
    pub status: CollectionStatus,
    pub measurement: Option<XCountsMeasurement>,
    pub planned_cycle_post_microusd: Option<u64>,
    pub gaps: Vec<String>,
}

/// Counts have their own durable pagination and never advance the observation watermark.
pub fn measure_x_counts(
    config: &ResearchConfig,
    source: &SourceConfig,
    provider: &mut impl XCountsSource,
    repository: &mut impl ResearchRepository,
    now: DateTime<Utc>,
) -> Result<XCountsReport> {
    config.validate()?;
    require(
        source.kind == SourceKind::XRecentSearch && config.sources.iter().any(|s| s == source),
        "counts require a registered X search source",
    )?;
    let mut state = prepare_state(source, repository.state(&source.id)?)?;
    let blocked = source.activation_gap(now).or_else(|| {
        (config
            .x_budget
            .as_ref()
            .is_none_or(|p| p.active(now).is_err()))
        .then_some("x_active_reviewed_billing_policy_required")
    });
    if let Some(gap) = blocked {
        state.last_status = Some(CollectionStatus::Disabled);
        state.gaps = vec![gap.into()];
        repository.commit_page(&source.id, state.revision, &state, &[], now)?;
    } else if state.retry_at.is_some_and(|at| at > now)
        || (state
            .x_counts
            .as_ref()
            .is_none_or(|m| m.complete || m.next_token.is_none())
            && state
                .last_attempt_at
                .is_some_and(|at| now - at < Duration::seconds(i64::from(source.interval_seconds))))
    {
        return Ok(XCountsReport {
            source_id: source.id.clone(),
            status: state.last_status.unwrap_or(CollectionStatus::Partial),
            measurement: state.x_counts,
            planned_cycle_post_microusd: None,
            gaps: vec!["collection_not_due".into()],
        });
    } else {
        let account = source.x_account.as_ref().expect("validated account");
        let rule = source.x_rule.as_ref().expect("validated rule");
        let rule_hash = rule.hash(
            &account.handle,
            account.user_id.as_deref().unwrap_or_default(),
        )?;
        let end = DateTime::from_timestamp((now - Duration::seconds(30)).timestamp(), 0)
            .ok_or_else(|| Error("invalid X counts timestamp".into()))?;
        let mut pending = state
            .x_counts
            .clone()
            .filter(|m| !m.complete && m.rule_hash == rule_hash)
            .unwrap_or(XCountsMeasurement {
                rule_hash,
                query: rule.query(&account.handle)?,
                start_at: end - Duration::days(7) + Duration::seconds(60),
                end_at: end,
                measured_at: now,
                complete: false,
                total_posts: 0,
                buckets: vec![],
                next_token: None,
                gaps: vec![],
            });
        for page_number in 0..config.max_pages {
            let window = CollectionRequest {
                start_at: pending.start_at,
                end_at: pending.end_at,
                next_token: pending.next_token.clone(),
                etag: None,
                last_modified: None,
                limit: config.page_size,
            };
            let (reserved, allowed) = reserve_source_requests(source, repository, state, now, 2)?;
            state = reserved;
            if !allowed {
                break;
            }
            match provider.counts(source, &window, now) {
                Err(failure) => {
                    state.last_status = Some(failure.status);
                    state.gaps = vec![failure.reason];
                    state.retry_at = failure.retry_at.or(Some(
                        now + Duration::seconds(i64::from(source.interval_seconds)),
                    ));
                    state.x_counts = Some(pending.clone());
                    repository.commit_page(&source.id, state.revision, &state, &[], now)?;
                    break;
                }
                Ok(page) => {
                    let mut cursor = pending
                        .buckets
                        .last()
                        .map(|b| b.end)
                        .unwrap_or(pending.start_at);
                    let mut sum = Some(0_u64);
                    let mut valid = page.rule_hash == pending.rule_hash
                        && page.start_at == pending.start_at
                        && page.end_at == pending.end_at
                        && !page.buckets.is_empty()
                        && page.buckets.len() <= 8
                        && !page
                            .gaps
                            .iter()
                            .any(|g| g == "x_counts_provider_partial_error")
                        && !page.next_token.as_ref().is_some_and(|t| {
                            t.is_empty() || t.len() > 2048 || Some(t) == pending.next_token.as_ref()
                        });
                    for b in &page.buckets {
                        valid &= b.start == cursor && b.end > b.start && b.end <= pending.end_at;
                        cursor = b.end;
                        sum = sum.and_then(|n| n.checked_add(b.post_count));
                    }
                    valid &= sum == Some(page.total_posts)
                        && (page.next_token.is_some() || cursor == pending.end_at);
                    if valid {
                        pending.total_posts = pending
                            .total_posts
                            .checked_add(page.total_posts)
                            .ok_or_else(|| Error("X count total overflow".into()))?;
                        pending.buckets.extend(page.buckets);
                        pending.next_token = page.next_token;
                        pending.complete = pending.next_token.is_none() && cursor == pending.end_at;
                        pending.measured_at = now;
                        pending.gaps = if pending.complete {
                            vec![
                                "counts_are_estimates_not_future_volume_or_completeness_guarantees"
                                    .into(),
                            ]
                        } else {
                            vec!["x_counts_incomplete_volume_unknown".into()]
                        };
                    }
                    state.last_status = Some(if pending.complete {
                        CollectionStatus::Complete
                    } else {
                        CollectionStatus::Partial
                    });
                    state.gaps = if valid {
                        pending.gaps.clone()
                    } else {
                        vec!["x_counts_invalid_or_repeated_page_volume_unknown".into()]
                    };
                    if !pending.complete && page_number + 1 == config.max_pages {
                        state.gaps.push("x_counts_page_budget_exhausted".into());
                    }
                    state.x_counts = Some(pending.clone());
                    state.retry_at = None;
                    repository.commit_page(&source.id, state.revision, &state, &[], now)?;
                    state.revision = state
                        .revision
                        .checked_add(1)
                        .ok_or_else(|| Error("source revision overflow".into()))?;
                    if !valid || pending.complete {
                        break;
                    }
                }
            }
        }
    }
    let estimate = state
        .x_counts
        .as_ref()
        .filter(|m| m.complete)
        .and_then(|m| config.x_budget.as_ref().map(|p| (m, p)))
        .map(|(m, p)| planned_post_cost(m, p))
        .transpose()?;
    Ok(XCountsReport {
        source_id: source.id.clone(),
        status: state.last_status.unwrap_or(CollectionStatus::Partial),
        measurement: state.x_counts,
        planned_cycle_post_microusd: estimate,
        gaps: state.gaps,
    })
}

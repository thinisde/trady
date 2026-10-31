//! Durable research work and immutable, rights-aware sentiment references.
//! These records grant no broker or plan activation authority.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use trady_domain::{Error, Result, require, research::SourceItem, sentiment::SentimentAssessment};

use crate::harness::{Scope, content_hash, valid_id};

pub fn digest<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value)
        .map(|s| content_hash(&s))
        .map_err(|_| Error("research job serialization failed".into()))
}

pub fn hash_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobPolicy {
    pub scope: Scope,
    pub mode: trady_domain::Mode,
    pub telegram_user_id: i64,
    pub bot_id: i64,
    pub source_ids: BTreeSet<String>,
    pub instrument_ids: BTreeSet<String>,
    pub registry_hash: String,
    /// An explicitly approved FinBERT revision, independent of the model's response.
    pub sentiment_revision: Option<String>,
    pub daily_job_limit: u32,
    pub pending_job_limit: u32,
    pub enabled: bool,
}
impl JobPolicy {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        require(
            self.telegram_user_id > 0
                && self.bot_id > 0
                && self.scope.account_id.starts_with(match self.mode {
                    trady_domain::Mode::Paper => "paper_",
                    trady_domain::Mode::Shadow => "shadow_",
                    trady_domain::Mode::Live => "live_",
                })
                && !self.source_ids.is_empty()
                && self.source_ids.len() <= 32
                && self.source_ids.iter().all(|s| valid_id(s))
                && self.instrument_ids.len() <= 1000
                && self.instrument_ids.iter().all(|s| valid_id(s))
                && hash_id(&self.registry_hash)
                && self
                    .sentiment_revision
                    .as_ref()
                    .is_none_or(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                && (1..=1000).contains(&self.daily_job_limit)
                && (1..=100).contains(&self.pending_job_limit),
            "invalid research job policy",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum Work {
    Research {
        instrument_id: Option<String>,
    },
    SourceLookup {
        source_id: String,
        external_id: String,
    },
    CollectSource {
        source_id: String,
        holding: Option<HoldingWatch>,
    },
    Sentiment {
        packet_id: String,
        model_revision: String,
    },
    Dialogue {
        text: String,
        session_id: Option<String>,
    },
    Session {
        action: String,
        session_id: Option<String>,
    },
    Daily {
        session_date: String,
    },
    HoldingWatchdog {
        holding_id: String,
        instrument_id: String,
        holding_revision: String,
    },
}
impl Work {
    pub fn validate(&self, policy: &JobPolicy) -> Result<()> {
        policy.validate()?;
        require(policy.enabled, "research scope is disabled")?;
        let instrument = |id: &str| valid_id(id) && policy.instrument_ids.contains(id);
        require(
            match self {
                Self::Research { instrument_id } => instrument_id.as_deref().is_none_or(instrument),
                Self::SourceLookup {
                    source_id,
                    external_id,
                } => {
                    policy.source_ids.contains(source_id)
                        && !external_id.is_empty()
                        && external_id.len() <= 128
                        && external_id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                }
                Self::CollectSource { source_id, holding } => {
                    policy.source_ids.contains(source_id)
                        && holding.as_ref().is_none_or(|h| {
                            h.active
                                && valid_id(&h.holding_id)
                                && instrument(&h.instrument_id)
                                && hash_id(&h.revision_hash)
                        })
                }
                Self::Sentiment {
                    packet_id,
                    model_revision,
                } => {
                    valid_id(packet_id)
                        && policy.sentiment_revision.as_ref() == Some(model_revision)
                }
                Self::Dialogue { text, session_id } => {
                    !text.trim().is_empty()
                        && text.len() <= 4096
                        && session_id
                            .as_ref()
                            .is_none_or(|s| valid_id(s) && s.starts_with("session_"))
                }
                Self::Session { action, session_id } => {
                    matches!(action.as_str(), "list" | "new" | "show" | "use" | "close")
                        && (matches!(action.as_str(), "list" | "new") && session_id.is_none()
                            || session_id
                                .as_ref()
                                .is_some_and(|s| valid_id(s) && s.starts_with("session_")))
                }
                Self::Daily { session_date } => {
                    chrono::NaiveDate::parse_from_str(session_date, "%Y-%m-%d").is_ok()
                }
                Self::HoldingWatchdog {
                    holding_id,
                    instrument_id,
                    holding_revision,
                } => valid_id(holding_id) && instrument(instrument_id) && hash_id(holding_revision),
            },
            "research request exceeds approved scope or revision",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Queued,
    Running,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchJob {
    pub id: String,
    pub scope: Scope,
    pub work: Work,
    pub policy_hash: String,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub attempts: u32,
    pub generation: i64,
    pub result: Option<Value>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobLease {
    pub job: ResearchJob,
    pub worker_id: String,
    pub generation: i64,
}

/// Supplied by the trusted source verifier, never by an LLM tool. An unknown
/// reporting origin or issuer mapping remains an explicit blocker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    pub item_id: String,
    pub instrument_id: String,
    pub verification_reference: String,
    pub issuer_mapping_verified: bool,
    pub primary_verified: bool,
    pub reporting_origin: Option<String>,
    pub origin_verified: bool,
    pub ambiguities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDependency {
    pub item_id: String,
    pub source_id: String,
    pub content_hash: String,
    pub lineage_id: String,
    pub registry_hash: String,
    pub source_registry_hash: String,
    pub source_rights: trady_domain::research::SourceRights,
    pub item_rights: trady_domain::research::SourceRights,
    pub rights_hash: String,
    pub expires_at: DateTime<Utc>,
    pub verification: Verification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimReview {
    pub claim_id: String,
    pub supporting_item_ids: Vec<String>,
    pub contradicting_item_ids: Vec<String>,
    pub ambiguities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedResearchPacket {
    pub id: String,
    pub scope: Scope,
    pub instrument_id: String,
    pub profile_revision_id: Option<String>,
    pub registry_hash: String,
    pub sources: Vec<SourceDependency>,
    pub claims: Vec<ClaimReview>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
impl VerifiedResearchPacket {
    pub fn validate(&self, policy: &JobPolicy, now: DateTime<Utc>) -> Result<()> {
        policy.validate()?;
        require(
            policy.enabled
                && self.scope == policy.scope
                && valid_id(&self.id)
                && policy.instrument_ids.contains(&self.instrument_id)
                && self.registry_hash == policy.registry_hash
                && self.created_at <= now
                && self.expires_at > now
                && self.expires_at <= self.created_at + Duration::hours(24)
                && (1..=16).contains(&self.sources.len())
                && self.claims.len() <= 64
                && self
                    .profile_revision_id
                    .as_ref()
                    .is_none_or(|s| valid_id(s)),
            "packet scope, registry, lifetime or size mismatch",
        )?;
        let mut ids = BTreeSet::new();
        for dep in &self.sources {
            let v = &dep.verification;
            require(
                ids.insert(dep.item_id.clone())
                    && hash_id(&dep.item_id)
                    && hash_id(&dep.content_hash)
                    && hash_id(&dep.lineage_id)
                    && hash_id(&dep.rights_hash)
                    && hash_id(&dep.source_registry_hash)
                    && dep.registry_hash == policy.registry_hash
                    && policy.source_ids.contains(&dep.source_id)
                    && dep.expires_at >= self.expires_at
                    && v.item_id == dep.item_id
                    && v.instrument_id == self.instrument_id
                    && !v.verification_reference.trim().is_empty()
                    && v.verification_reference.len() <= 512
                    && v.reporting_origin.as_ref().is_none_or(|s| valid_id(s))
                    && (!v.origin_verified || v.reporting_origin.is_some())
                    && v.ambiguities.len() <= 16
                    && v.ambiguities.iter().all(|s| s.len() <= 256),
                "invalid verified source dependency",
            )?;
        }
        let mut claims = BTreeSet::new();
        for claim in &self.claims {
            require(
                valid_id(&claim.claim_id)
                    && claims.insert(&claim.claim_id)
                    && !claim.supporting_item_ids.is_empty()
                    && claim.supporting_item_ids.len() <= 16
                    && claim.contradicting_item_ids.len() <= 16
                    && claim
                        .supporting_item_ids
                        .iter()
                        .chain(&claim.contradicting_item_ids)
                        .all(|id| ids.contains(id))
                    && claim
                        .supporting_item_ids
                        .iter()
                        .all(|id| !claim.contradicting_item_ids.contains(id))
                    && claim.ambiguities.len() <= 16
                    && claim.ambiguities.iter().all(|s| s.len() <= 256),
                "claim references unknown, conflicting or oversized evidence",
            )?;
        }
        Ok(())
    }
}

/// Exact content, URLs, source chains and verifier-confirmed origins merge
/// syndication clusters. Similar headlines alone never establish a relationship.
pub fn syndication_clusters(
    items: &[SourceItem],
    dependencies: &[SourceDependency],
) -> Vec<Vec<String>> {
    let mut groups: Vec<BTreeSet<usize>> = (0..items.len()).map(|i| BTreeSet::from([i])).collect();
    for a in 0..items.len() {
        for b in a + 1..items.len() {
            let x = &items[a];
            let y = &items[b];
            let origin = |id: &str| {
                dependencies
                    .iter()
                    .find(|d| d.item_id == id)
                    .filter(|d| d.verification.origin_verified)
                    .and_then(|d| d.verification.reporting_origin.as_deref())
            };
            let same = x.lineage_id == y.lineage_id
                || x.content_hash == y.content_hash
                || x.canonical_url == y.canonical_url
                || x.related_urls
                    .iter()
                    .any(|u| u == &y.canonical_url || y.related_urls.contains(u))
                || y.related_urls.contains(&x.canonical_url)
                || origin(&x.item_id)
                    .zip(origin(&y.item_id))
                    .is_some_and(|(x, y)| x == y);
            if same {
                let ai = groups.iter().position(|g| g.contains(&a)).unwrap_or(0);
                let bi = groups.iter().position(|g| g.contains(&b)).unwrap_or(0);
                if ai != bi {
                    let other = groups.remove(bi);
                    let ai = groups.iter().position(|g| g.contains(&a)).unwrap_or(0);
                    groups[ai].extend(other);
                }
            }
        }
    }
    groups
        .into_iter()
        .map(|g| g.into_iter().map(|i| items[i].item_id.clone()).collect())
        .collect()
}

/// Checks rights and immutable content/registry dependencies before every use.
pub fn check_dependencies(
    packet: &VerifiedResearchPacket,
    items: &[SourceItem],
    policy: &JobPolicy,
    now: DateTime<Utc>,
) -> Result<()> {
    packet.validate(policy, now)?;
    require(
        items.len() == packet.sources.len(),
        "missing packet source dependencies",
    )?;
    let by_id: BTreeMap<_, _> = items.iter().map(|s| (s.item_id.as_str(), s)).collect();
    require(by_id.len() == items.len(), "duplicate packet source")?;
    for dep in &packet.sources {
        let item = by_id
            .get(dep.item_id.as_str())
            .ok_or_else(|| Error("packet source unavailable".into()))?;
        require(
            item.source_id == dep.source_id
                && item.content_hash == dep.content_hash
                && item.lineage_id == dep.lineage_id
                && item.rights == dep.item_rights
                && digest(&item.rights)? == dep.rights_hash
                && item.expires_at >= packet.expires_at
                && item.expires_at > now
                && item.rights.store_content
                && item.rights.llm_processing
                && matches!(
                    item.content_status,
                    trady_domain::research::ContentStatus::Available
                        | trady_domain::research::ContentStatus::Snippet
                )
                && item.fetched_at <= now
                && item
                    .event_at
                    .or(item.published_at)
                    .is_some_and(|at| at <= now && now - at <= Duration::hours(24))
                && item.social.as_ref().is_none_or(|s| !s.missing_context)
                && dep.verification.issuer_mapping_verified,
            "packet dependency changed, expired, ambiguous or revoked",
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentimentReadinessRef {
    pub assessment_id: String,
    pub assessment_hash: String,
    pub packet_id: String,
    pub packet_hash: String,
    pub scope: Scope,
    pub instrument_id: String,
    pub model_revision: String,
    pub scored_at: DateTime<Utc>,
    pub maximum_negative_probability: rust_decimal::Decimal,
    pub expires_at: DateTime<Utc>,
    pub dependency_hash: String,
}

pub fn readiness_reference(
    assessment_id: &str,
    assessment: &SentimentAssessment,
    packet: &VerifiedResearchPacket,
    items: &[SourceItem],
    policy: &JobPolicy,
    now: DateTime<Utc>,
) -> Result<SentimentReadinessRef> {
    check_dependencies(packet, items, policy, now)?;
    let clusters = syndication_clusters(items, &packet.sources);
    let evidence_ids: BTreeSet<_> = assessment
        .evidence
        .iter()
        .map(|e| e.item_id.as_str())
        .collect();
    require(
        valid_id(assessment_id)
            && assessment.schema_version == trady_domain::sentiment::SENTIMENT_VERSION
            && policy.sentiment_revision.as_deref() == Some(&assessment.model_revision)
            && assessment.model_id == trady_domain::sentiment::FINBERT_MODEL
            && assessment.scored_at >= packet.created_at
            && assessment.scored_at <= now
            && now - assessment.scored_at <= Duration::minutes(15)
            && assessment.evidence.len() == clusters.len()
            && evidence_ids.len() == assessment.evidence.len()
            && clusters.iter().all(|c| {
                c.iter()
                    .filter(|id| evidence_ids.contains(id.as_str()))
                    .count()
                    == 1
            })
            && !packet.claims.is_empty(),
        "sentiment assessment revision, coverage or claim verification mismatch",
    )?;
    for evidence in &assessment.evidence {
        let item = items
            .iter()
            .find(|i| i.item_id == evidence.item_id)
            .ok_or_else(|| Error("assessment evidence outside packet".into()))?;
        let text = item
            .text
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .or(item.summary.as_deref().filter(|s| !s.trim().is_empty()))
            .or(item.title.as_deref().filter(|s| !s.trim().is_empty()));
        require(
            evidence.lineage_id == item.lineage_id
                && text.is_some_and(|t| content_hash(t) == evidence.text_sha256)
                && evidence.expires_at >= packet.expires_at
                && evidence.observed_at == item.event_at.or(item.published_at).unwrap_or(now),
            "assessment evidence dependency mismatch",
        )?;
        crate::sentiment::validate_probabilities(&evidence.probabilities)?;
    }
    for claim in &packet.claims {
        let supporting: BTreeSet<_> = claim.supporting_item_ids.iter().collect();
        let independent = clusters
            .iter()
            .filter(|g| g.iter().any(|id| supporting.contains(id)))
            .count();
        let primary = packet
            .sources
            .iter()
            .any(|d| supporting.contains(&d.item_id) && d.verification.primary_verified);
        require(
            primary
                && independent >= 2
                && claim.contradicting_item_ids.is_empty()
                && claim.ambiguities.is_empty()
                && packet
                    .sources
                    .iter()
                    .filter(|d| supporting.contains(&d.item_id))
                    .all(|d| {
                        d.verification.origin_verified && d.verification.ambiguities.is_empty()
                    }),
            "claim requires primary verification, independent origins and resolved contradictions",
        )?;
    }
    let mut expires_at = packet.expires_at.min(
        assessment
            .scored_at
            .checked_add_signed(Duration::minutes(15))
            .ok_or_else(|| Error("sentiment expiry overflow".into()))?,
    );
    for evidence in &assessment.evidence {
        expires_at = expires_at.min(
            evidence
                .observed_at
                .checked_add_signed(Duration::hours(24))
                .ok_or_else(|| Error("sentiment source expiry overflow".into()))?,
        );
    }
    let maximum_negative_probability = assessment
        .evidence
        .iter()
        .map(|e| e.probabilities.negative)
        .max()
        .ok_or_else(|| Error("sentiment readiness requires source evidence".into()))?;
    require(expires_at > now, "sentiment readiness expired")?;
    Ok(SentimentReadinessRef {
        assessment_id: assessment_id.into(),
        assessment_hash: digest(assessment)?,
        packet_id: packet.id.clone(),
        packet_hash: digest(packet)?,
        scope: packet.scope.clone(),
        instrument_id: packet.instrument_id.clone(),
        model_revision: assessment.model_revision.clone(),
        scored_at: assessment.scored_at,
        maximum_negative_probability,
        expires_at,
        dependency_hash: digest(&packet.sources)?,
    })
}

/// Implementations persist queue state and use fencing generations/server time.
pub trait ResearchJobRepository {
    fn policy(&mut self, scope: &Scope) -> Result<JobPolicy>;
    fn enqueue(&mut self, scope: &Scope, key: &str, work: &Work) -> Result<ResearchJob>;
    fn claim(&mut self, scope: &Scope, worker: &str, ttl_seconds: u32) -> Result<Option<JobLease>>;
    fn complete(&mut self, lease: &JobLease, result: &Value) -> Result<()>;
    fn fail(&mut self, lease: &JobLease, code: &str) -> Result<()>;
    fn packet(
        &mut self,
        scope: &Scope,
        packet_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(VerifiedResearchPacket, Vec<SourceItem>)>;
    fn store_sentiment(
        &mut self,
        lease: &JobLease,
        packet: &VerifiedResearchPacket,
        assessment: &SentimentAssessment,
        now: DateTime<Utc>,
    ) -> Result<Value>;
}

/// Exact schemas are registered by the adapter. Implementations select only
/// scoped persisted IDs; they never accept arbitrary SQL, URLs or executables.
pub trait ResearchInformation {
    fn information(
        &mut self,
        scope: &Scope,
        name: &str,
        arguments: &Value,
        request_key: &str,
        now: DateTime<Utc>,
    ) -> Result<Value>;
}

pub fn valid_tool_input(schema: &Value, arguments: &Value) -> bool {
    jsonschema::validator_for(schema).is_ok_and(|validator| validator.is_valid(arguments))
}

/// A failed provider call is a durable failed job, never a neutral prediction.
pub fn score_sentiment_job(
    repository: &mut impl ResearchJobRepository,
    provider: &impl crate::sentiment::SentimentProvider,
    lease: &JobLease,
    now: DateTime<Utc>,
) -> Result<Value> {
    let Work::Sentiment {
        packet_id,
        model_revision,
    } = &lease.job.work
    else {
        return Err(Error("not a sentiment job".into()));
    };
    let policy = repository.policy(&lease.job.scope)?;
    lease.job.work.validate(&policy)?;
    require(
        provider.model_revision() == model_revision && lease.job.policy_hash == digest(&policy)?,
        "sentiment provider or job policy revision mismatch",
    )?;
    let (packet, items) = repository.packet(&lease.job.scope, packet_id, now)?;
    check_dependencies(&packet, &items, &policy, now)?;
    let selected: Vec<_> = syndication_clusters(&items, &packet.sources)
        .iter()
        .filter_map(|cluster| {
            let representative = cluster
                .iter()
                .find(|id| {
                    packet
                        .sources
                        .iter()
                        .any(|d| &d.item_id == *id && d.verification.primary_verified)
                })
                .or_else(|| cluster.first())?;
            items
                .iter()
                .find(|item| &item.item_id == representative)
                .cloned()
        })
        .collect();
    let assessment = crate::sentiment::assess_sentiment(provider, &selected, now)?;
    // Repository repeats dependency checks in the same transaction as insert.
    repository.store_sentiment(lease, &packet, &assessment, now)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingWatch {
    pub holding_id: String,
    pub instrument_id: String,
    pub revision_hash: String,
    pub active: bool,
}

/// Scheduler persists its cursor in the repository. Keys coalesce redelivery;
/// closed holdings are explicitly cancelled rather than silently kept alive.
pub fn daily_key(scope: &Scope, date: chrono::NaiveDate) -> Result<String> {
    Ok(format!("daily_{}", digest(&(scope, date))?))
}
pub fn watchdog_key(
    scope: &Scope,
    holding: &HoldingWatch,
    now: DateTime<Utc>,
    interval_seconds: u32,
) -> Result<String> {
    require(
        (60..=3600).contains(&interval_seconds)
            && holding.active
            && valid_id(&holding.holding_id)
            && valid_id(&holding.instrument_id)
            && hash_id(&holding.revision_hash),
        "invalid holding watchdog schedule",
    )?;
    Ok(format!(
        "watch_{}",
        digest(&(
            scope,
            holding,
            now.timestamp().div_euclid(i64::from(interval_seconds))
        ))?
    ))
}

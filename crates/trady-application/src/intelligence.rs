//! Rights-aware optional research, immutable vintages, and read-only projections.

use crate::research_jobs::digest;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::{
    Error, Mode, Result,
    financial::{Capability, Consumer, DataRights},
    intelligence::*,
    require,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureSource {
    pub source_id: String,
    pub provider: String,
    pub enabled: bool,
    pub rights_revision: String,
    pub mapping_reference: String,
    pub entity_ids: BTreeSet<String>,
    pub features: BTreeSet<FeatureKind>,
    pub capability: Capability,
    pub rights: DataRights,
    /// A separate entitlement is required to retain derived features.
    pub derived_storage: bool,
    /// Reviewed permission to retain hashes/tombstones after content erasure.
    #[serde(default)]
    pub audit_markers: bool,
}
impl FeatureSource {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_key(&self.source_id)
                && !self.provider.is_empty()
                && self.provider.len() <= 96
                && !self.rights_revision.is_empty()
                && self.rights_revision.len() <= 128
                && self.capability.revision == self.rights_revision
                && !self.mapping_reference.is_empty()
                && self.mapping_reference.len() <= 1024
                && !self.entity_ids.is_empty()
                && self.entity_ids.len() <= 1024
                && self.entity_ids.iter().all(|s| valid_key(s))
                && !self.features.is_empty()
                && self.rights.retention_hours > 0
                && self.rights.retention_hours <= 24 * 366 * 10
                && !self.rights.terms_reference.is_empty(),
            "invalid reviewed intelligence source",
        )
    }
    fn permits(&self, audience: Audience) -> bool {
        self.rights.permits(match audience {
            Audience::Owner => Consumer::Display,
            Audience::ExternalLlm => Consumer::ExternalLlm,
            Audience::LocalInference => Consumer::LocalInference,
            Audience::ModelTraining => Consumer::ModelTraining,
            Audience::Export => Consumer::Export,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntelligenceConfig {
    pub schema_version: u32,
    pub scope: IntelligenceScope,
    #[serde(default = "enabled_features")]
    pub enabled: BTreeSet<FeatureKind>,
    #[serde(default)]
    pub sources: Vec<FeatureSource>,
    /// Stable internal entity -> reviewed external issuer/CIK/topic/region aliases.
    #[serde(default)]
    pub entity_aliases: BTreeMap<String, BTreeSet<String>>,
    /// Cross-asset subject -> explicitly reviewed contextual entities.
    #[serde(default)]
    pub cross_asset_context: BTreeMap<String, BTreeSet<String>>,
    #[serde(default = "max_input_bytes")]
    pub max_input_bytes: usize,
    #[serde(default = "max_query_records")]
    pub max_query_records: usize,
}
fn enabled_features() -> BTreeSet<FeatureKind> {
    FeatureKind::ALL.into_iter().collect()
}
fn max_input_bytes() -> usize {
    1_048_576
}
fn max_query_records() -> usize {
    128
}
impl IntelligenceConfig {
    /// Research calculations are enabled; source entitlements remain explicit.
    pub fn research_defaults(scope: IntelligenceScope) -> Self {
        Self {
            enabled: enabled_features(),
            ..Self::disabled(scope)
        }
    }

    /// Explicit opt-out, including for callers that need only a selected feature.
    pub fn disabled(scope: IntelligenceScope) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            scope,
            enabled: BTreeSet::new(),
            sources: vec![],
            entity_aliases: BTreeMap::new(),
            cross_asset_context: BTreeMap::new(),
            max_input_bytes: max_input_bytes(),
            max_query_records: max_query_records(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        require(
            self.schema_version == SCHEMA_VERSION
                && (1024..=4_194_304).contains(&self.max_input_bytes)
                && (1..=1024).contains(&self.max_query_records)
                && self.sources.len() <= 128,
            "invalid intelligence configuration limits",
        )?;
        let mut ids = BTreeSet::new();
        for s in &self.sources {
            s.validate()?;
            require(ids.insert(&s.source_id), "duplicate intelligence source")?;
        }
        require(
            [&self.entity_aliases, &self.cross_asset_context]
                .into_iter()
                .all(|m| {
                    m.len() <= 1024
                        && m.iter().all(|(entity, aliases)| {
                            valid_key(entity)
                                && aliases.len() <= 64
                                && aliases.iter().all(|s| valid_key(s))
                        })
                }),
            "invalid intelligence entity aliases",
        )?;
        Ok(())
    }
    pub fn revision(&self) -> Result<String> {
        digest(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredObservation {
    pub observation: FeatureObservation,
    /// Set by the trusted repository, never backdated to source availability.
    pub ingested_at: DateTime<Utc>,
    pub retain_until: DateTime<Utc>,
    pub registry_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvalidationTarget {
    Observation,
    Source,
    Lineage,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvalidationReason {
    Correction,
    Deleted,
    RightsRevoked,
    EntitlementChanged,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invalidation {
    pub scope: IntelligenceScope,
    pub target: InvalidationTarget,
    pub target_id: String,
    pub at: DateTime<Utc>,
    pub reason: InvalidationReason,
    /// Deletion/rights erasure overrides historical replay as well.
    pub erase_content: bool,
}
impl Invalidation {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        require(
            valid_key(&self.target_id)
                && (!matches!(
                    self.reason,
                    InvalidationReason::Deleted | InvalidationReason::RightsRevoked
                ) || self.erase_content),
            "invalid intelligence invalidation or mandatory erasure",
        )
    }
    pub fn matches(&self, observation: &FeatureObservation) -> bool {
        self.scope == observation.scope
            && match self.target {
                InvalidationTarget::Observation => observation.observation_id == self.target_id,
                InvalidationTarget::Source => observation
                    .evidence
                    .iter()
                    .any(|e| e.source_id == self.target_id),
                InvalidationTarget::Lineage => observation
                    .evidence
                    .iter()
                    .any(|e| e.lineage_ids.contains(&self.target_id)),
            }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RejectedInput {
    pub scope: IntelligenceScope,
    pub observation_id: Option<String>,
    pub feature: FeatureKind,
    pub input_hash: String,
    pub code: String,
    pub rejected_at: DateTime<Utc>,
    pub retain_until: DateTime<Utc>,
    /// Only current reviewed sources that authorize audit markers are included.
    pub source_revisions: BTreeMap<String, String>,
    pub lineage_ids: BTreeSet<String>,
}
impl RejectedInput {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        require(
            self.feature != FeatureKind::Tax
                && valid_hash(&self.input_hash)
                && self.observation_id.as_deref().is_none_or(valid_key)
                && valid_key(&self.code)
                && self.rejected_at < self.retain_until
                && !self.source_revisions.is_empty()
                && self.source_revisions.len() <= 64
                && self
                    .source_revisions
                    .iter()
                    .all(|(id, rev)| valid_key(id) && !rev.is_empty() && rev.len() <= 128)
                && !self.lineage_ids.is_empty()
                && self.lineage_ids.len() <= 16_384
                && self.lineage_ids.iter().all(|id| valid_key(id)),
            "invalid licensed rejection marker",
        )
    }
    pub fn matches(&self, invalidation: &Invalidation) -> bool {
        self.scope == invalidation.scope
            && match invalidation.target {
                InvalidationTarget::Observation => {
                    self.observation_id.as_deref() == Some(&invalidation.target_id)
                }
                InvalidationTarget::Source => {
                    self.source_revisions.contains_key(&invalidation.target_id)
                }
                InvalidationTarget::Lineage => self.lineage_ids.contains(&invalidation.target_id),
            }
    }
}

/// Repositories have no execution port. Inserts are immutable/idempotent and
/// erasure removes both input content and dependent derived content atomically.
pub trait IntelligenceRepository {
    fn register(&mut self, config: &IntelligenceConfig) -> Result<String>;
    fn insert(
        &mut self,
        observation: &FeatureObservation,
        at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        registry_hash: &str,
    ) -> Result<StoredObservation>;
    fn find(&mut self, scope: &IntelligenceScope, id: &str) -> Result<Option<StoredObservation>>;
    fn candidates(
        &mut self,
        scope: &IntelligenceScope,
        feature: FeatureKind,
        entity: &str,
        at: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<StoredObservation>>;
    fn has_erased(
        &mut self,
        scope: &IntelligenceScope,
        feature: FeatureKind,
        entity: &str,
        at: DateTime<Utc>,
    ) -> Result<bool>;
    fn invalidations(&mut self, scope: &IntelligenceScope) -> Result<Vec<Invalidation>>;
    fn invalidate(&mut self, invalidation: &Invalidation) -> Result<()>;
    fn reject(&mut self, rejection: &RejectedInput) -> Result<()>;
    fn save_assessment(
        &mut self,
        assessment: &AssessmentRecord,
        retain_until: DateTime<Utc>,
    ) -> Result<()>;
    fn purge(&mut self, now: DateTime<Utc>) -> Result<()>;
}

fn rejection(
    repository: &mut impl IntelligenceRepository,
    config: &IntelligenceConfig,
    observation: &FeatureObservation,
    code: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    // Denied audit rights must not themselves create a retained content hash.
    if observation.payload.kind() == FeatureKind::Tax
        || config.validate().is_err()
        || observation.evidence.is_empty()
        || observation.evidence.len() > 64
        || bind_payload_evidence(observation).is_err()
    {
        return Ok(());
    }
    let mut retain_until = None;
    let mut source_revisions = BTreeMap::new();
    let mut lineage_ids = BTreeSet::new();
    for evidence in &observation.evidence {
        let Some(source) = config
            .sources
            .iter()
            .find(|s| s.source_id == evidence.source_id)
        else {
            return Ok(());
        };
        if evidence.validate().is_err()
            || !source.enabled
            || !source.audit_markers
            || !source.capability.current(now)
            || source.provider != evidence.provider
            || source.rights_revision != evidence.rights_revision
            || evidence.received_at > now
        {
            return Ok(());
        }
        let Some(end) = evidence
            .received_at
            .checked_add_signed(Duration::hours(i64::from(source.rights.retention_hours)))
        else {
            return Ok(());
        };
        let end = end.min(evidence.expires_at);
        if end <= now {
            return Ok(());
        }
        retain_until = Some(retain_until.map_or(end, |old: DateTime<Utc>| old.min(end)));
        source_revisions.insert(source.source_id.clone(), source.rights_revision.clone());
        lineage_ids.extend(evidence.lineage_ids.iter().cloned());
    }
    if repository.invalidations(&config.scope)?.iter().any(|i| {
        i.erase_content
            && match i.target {
                InvalidationTarget::Observation => i.target_id == observation.observation_id,
                InvalidationTarget::Source => source_revisions.contains_key(&i.target_id),
                InvalidationTarget::Lineage => lineage_ids.contains(&i.target_id),
            }
    }) {
        return Ok(());
    }
    let Some(retain_until) = retain_until else {
        return Ok(());
    };
    repository.reject(&RejectedInput {
        scope: config.scope.clone(),
        observation_id: valid_key(&observation.observation_id)
            .then(|| observation.observation_id.clone()),
        feature: observation.payload.kind(),
        input_hash: digest(observation)?,
        code: code.into(),
        rejected_at: now,
        retain_until,
        source_revisions,
        lineage_ids,
    })
}

fn check(
    config: &IntelligenceConfig,
    observation: &FeatureObservation,
    audience: Audience,
    at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<()> {
    config.validate()?;
    observation.validate()?;
    require(
        config.scope == observation.scope && at <= now,
        "intelligence scope or clock mismatch",
    )?;
    let kind = observation.payload.kind();
    require(config.enabled.contains(&kind), "feature_disabled")?;
    require(
        kind != FeatureKind::Tax || audience == Audience::Owner,
        "tax_owner_only",
    )?;
    require(
        observation.payload_hash == digest(&observation.payload)?,
        "feature_payload_hash_mismatch",
    )?;
    require(
        observation.payload.subject().is_none_or(|subject| {
            subject == observation.entity_id
                || config
                    .entity_aliases
                    .get(&observation.entity_id)
                    .is_some_and(|ids| ids.contains(subject))
        }),
        "feature_subject_mapping_mismatch",
    )?;
    let bytes = serde_json::to_vec(observation)
        .map_err(|_| Error("invalid feature serialization".into()))?;
    require(bytes.len() <= config.max_input_bytes, "feature_input_limit")?;
    bind_payload_evidence(observation)?;
    for e in &observation.evidence {
        let same_subject = e.entity_id == observation.entity_id
            || config
                .entity_aliases
                .get(&observation.entity_id)
                .is_some_and(|aliases| aliases.contains(&e.entity_id));
        let reviewed_context = kind == FeatureKind::CrossAsset
            && config
                .cross_asset_context
                .get(&observation.entity_id)
                .is_some_and(|entities| entities.contains(&e.entity_id));
        require(
            same_subject || reviewed_context,
            "feature_evidence_subject_mismatch",
        )?;
        let s = config
            .sources
            .iter()
            .find(|s| s.source_id == e.source_id)
            .ok_or_else(|| Error("unregistered_feature_source".into()))?;
        require(
            s.enabled
                && s.provider == e.provider
                && s.rights_revision == e.rights_revision
                && s.entity_ids.contains(&e.entity_id)
                && s.entity_ids.contains(&observation.entity_id)
                && s.features.contains(&kind)
                && s.capability.current(at)
                && s.capability.current(now)
                && s.audit_markers
                && s.permits(audience),
            "feature_source_rights_or_mapping_unavailable",
        )?;
        let retention_end = e
            .received_at
            .checked_add_signed(Duration::hours(i64::from(s.rights.retention_hours)))
            .ok_or_else(|| Error("feature_retention_overflow".into()))?;
        require(
            e.received_at <= at && e.available_at <= at && e.expires_at > at && retention_end > now,
            "feature_unavailable_at_decision_or_expired",
        )?;
    }
    Ok(())
}

fn bind_payload_evidence(observation: &FeatureObservation) -> Result<()> {
    fn visit(
        value: &serde_json::Value,
        inherited_source: Option<&str>,
        evidence: &[SourceEvidence],
    ) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                let source = map
                    .get("source_id")
                    .and_then(serde_json::Value::as_str)
                    .or(inherited_source);
                if source.is_some_and(|id| !evidence.iter().any(|e| e.source_id == id)) {
                    return false;
                }
                let supported = |reference: &str| {
                    evidence.iter().any(|e| {
                        source.is_none_or(|id| e.source_id == id)
                            && (reference == e.source_reference
                                || reference
                                    .strip_prefix(&e.source_reference)
                                    .is_some_and(|s| s.starts_with('#')))
                    })
                };
                for (key, v) in map {
                    if (key == "reference" || key.ends_with("_ref") || key.ends_with("_reference"))
                        && v.as_str().is_some_and(|r| !supported(r))
                    {
                        return false;
                    }
                    if (key == "references"
                        || key.ends_with("_refs")
                        || key.ends_with("_references"))
                        && v.as_array().is_some_and(|rows| {
                            rows.iter()
                                .filter_map(serde_json::Value::as_str)
                                .any(|r| !supported(r))
                        })
                    {
                        return false;
                    }
                    if !visit(v, source, evidence) {
                        return false;
                    }
                }
                true
            }
            serde_json::Value::Array(rows) => rows
                .iter()
                .all(|row| visit(row, inherited_source, evidence)),
            _ => true,
        }
    }
    let payload = serde_json::to_value(&observation.payload)
        .map_err(|_| Error("invalid feature serialization".into()))?;
    require(
        visit(&payload, None, &observation.evidence),
        "feature_payload_evidence_binding_mismatch",
    )
}

/// The calling analyst supplies its trusted receipt clock. No retroactive ingest.
pub fn ingest(
    repository: &mut impl IntelligenceRepository,
    config: &IntelligenceConfig,
    observation: &FeatureObservation,
    now: DateTime<Utc>,
) -> Result<StoredObservation> {
    if observation.payload.kind() == FeatureKind::Tax {
        return Err(Error(
            "raw tax context cannot enter research storage; use the ephemeral owner comparator"
                .into(),
        ));
    }
    config.validate()?;
    // A reviewed revocation/disablement must reach persistence even when this
    // particular observation is rejected before insertion.
    let registry_hash = repository.register(config)?;
    let result = ingest_checked(repository, config, observation, now, &registry_hash);
    if result.is_err() {
        rejection(repository, config, observation, "ingestion_rejected", now)?;
    }
    result
}

fn ingest_checked(
    repository: &mut impl IntelligenceRepository,
    config: &IntelligenceConfig,
    observation: &FeatureObservation,
    now: DateTime<Utc>,
    registry_hash: &str,
) -> Result<StoredObservation> {
    check(config, observation, Audience::Owner, now, now)?;
    require(
        !repository
            .invalidations(&observation.scope)?
            .iter()
            .any(|i| i.matches(observation)),
        "invalidated source cannot be reimported",
    )?;
    if let Some(existing) = repository.find(&config.scope, &observation.observation_id)? {
        require(
            digest(&existing.observation)? == digest(observation)?,
            "immutable intelligence observation collision",
        )?;
        return Ok(existing);
    }
    if let Some(previous) = &observation.supersedes {
        let old = repository
            .find(&observation.scope, previous)?
            .ok_or_else(|| Error("superseded observation is missing".into()))?;
        require(
            old.observation.payload.kind() == observation.payload.kind()
                && old.observation.entity_id == observation.entity_id
                && old.ingested_at <= now,
            "correction changes feature, entity or chronology",
        )?;
    }
    // Also reject malformed calculations before retaining a normalized payload.
    trady_policy::intelligence::assess(&observation.payload, now)
        .map_err(|_| Error("feature input failed deterministic validation".into()))?;
    let stored = repository.insert(
        observation,
        now,
        retention_end(config, observation)?,
        registry_hash,
    )?;
    if let Some(previous) = &observation.supersedes {
        repository.invalidate(&Invalidation {
            scope: observation.scope.clone(),
            target: InvalidationTarget::Observation,
            target_id: previous.clone(),
            at: stored.ingested_at,
            reason: InvalidationReason::Correction,
            erase_content: false,
        })?;
    }
    Ok(stored)
}

fn unavailable(
    scope: &IntelligenceScope,
    feature: FeatureKind,
    entity: &str,
    at: DateTime<Utc>,
    now: DateTime<Utc>,
    status: AssessmentStatus,
    reason: &str,
) -> AssessmentRecord {
    AssessmentRecord {
        schema_version: SCHEMA_VERSION,
        feature_version: FEATURE_VERSION.into(),
        assessment_id: String::new(),
        feature,
        scope: scope.clone(),
        entity_id: entity.into(),
        decision_at: at,
        computed_at: now,
        expires_at: None,
        input_hash: None,
        registry_hash: None,
        observation_id: None,
        status,
        reasons: vec![reason.into()],
        evidence: vec![],
        result: None,
        calibration: "RESEARCH_ONLY_UNCALIBRATED".into(),
        entry_authorized: false,
    }
}
fn identify(mut record: AssessmentRecord) -> Result<AssessmentRecord> {
    record.assessment_id = format!("intel_{}", digest(&record)?);
    Ok(record)
}

fn retention_end(
    config: &IntelligenceConfig,
    observation: &FeatureObservation,
) -> Result<DateTime<Utc>> {
    observation
        .evidence
        .iter()
        .map(|e| {
            let source = config
                .sources
                .iter()
                .find(|s| s.source_id == e.source_id)
                .ok_or_else(|| Error("unregistered_feature_source".into()))?;
            e.received_at
                .checked_add_signed(Duration::hours(i64::from(source.rights.retention_hours)))
                .ok_or_else(|| Error("feature_retention_overflow".into()))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .min()
        .ok_or_else(|| Error("feature requires evidence".into()))
}

fn output_status(result: &FeatureAssessment, partial: bool) -> Result<AssessmentStatus> {
    let value = serde_json::to_value(result)
        .map_err(|_| Error("nonserializable feature assessment".into()))?;
    let body = &value["assessment"];
    let flag = body
        .get("status")
        .or_else(|| body.get("risk_of_unreliable_attention"))
        .or_else(|| body.get("outcome"))
        .or_else(|| body.get("coverage"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if let FeatureAssessment::Accounting(accounting) = result {
        let count = accounting
            .signals
            .iter()
            .filter(|s| s.value.is_some())
            .count();
        return Ok(if count == 0 {
            AssessmentStatus::Abstained
        } else if partial || count < accounting.signals.len() {
            AssessmentStatus::Partial
        } else {
            AssessmentStatus::Available
        });
    }
    Ok(match flag {
        "UNAVAILABLE" | "ESTIMATE_UNAVAILABLE" | "MISSING_APPROVED_CONTEXT" => {
            AssessmentStatus::Unavailable
        }
        "ABSTAIN" | "ABSTAINED" | "INCONCLUSIVE" | "UNKNOWN" | "AUCTION" | "HALTED" => {
            AssessmentStatus::Abstained
        }
        "INVALIDATED" => AssessmentStatus::Invalidated,
        "PARTIAL" | "REQUIRES_REVIEW" | "REVIEW_REQUIRED" | "ELEVATED" => AssessmentStatus::Partial,
        _ if partial => AssessmentStatus::Partial,
        _ => AssessmentStatus::Available,
    })
}

pub fn assess_ephemeral(
    config: &IntelligenceConfig,
    observation: &FeatureObservation,
    mode: Mode,
    audience: Audience,
    at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<AssessmentRecord> {
    config.validate()?;
    require(
        config.scope == observation.scope && valid_key(&observation.entity_id) && at <= now,
        "intelligence scope, entity or clock mismatch",
    )?;
    let kind = observation.payload.kind();
    if mode == Mode::Live {
        return identify(unavailable(
            &observation.scope,
            kind,
            &observation.entity_id,
            at,
            now,
            AssessmentStatus::Unavailable,
            "research_only_live_mode_denied",
        ));
    }
    if kind == FeatureKind::Tax && audience != Audience::Owner {
        return identify(unavailable(
            &observation.scope,
            kind,
            &observation.entity_id,
            at,
            now,
            AssessmentStatus::Unavailable,
            "tax_owner_only",
        ));
    }
    if let Err(error) = check(config, observation, audience, at, now) {
        // Known gate errors contain no source text or personal input.
        let reason = match error.0.as_str() {
            "feature_disabled" => "feature_disabled",
            "feature_unavailable_at_decision_or_expired" => {
                "feature_unavailable_at_decision_or_expired"
            }
            _ => "source_rights_mapping_or_input_unavailable",
        };
        return identify(unavailable(
            &observation.scope,
            kind,
            &observation.entity_id,
            at,
            now,
            AssessmentStatus::Unavailable,
            reason,
        ));
    }
    let result = match trady_policy::intelligence::assess(&observation.payload, at) {
        Ok(result) => result,
        Err(_) => {
            return identify(unavailable(
                &observation.scope,
                kind,
                &observation.entity_id,
                at,
                now,
                AssessmentStatus::Rejected,
                "feature_calculation_rejected",
            ));
        }
    };
    let partial = observation
        .evidence
        .iter()
        .any(|e| !e.complete || !e.gaps.is_empty());
    let mut record = unavailable(
        &observation.scope,
        kind,
        &observation.entity_id,
        at,
        now,
        output_status(&result, partial)?,
        "uncalibrated_research_no_expected_return_or_order_authority",
    );
    record.input_hash = Some(observation.payload_hash.clone());
    record.registry_hash = Some(config.revision()?);
    record.observation_id = Some(observation.observation_id.clone());
    record.expires_at = observation.evidence.iter().map(|e| e.expires_at).min();
    if kind != FeatureKind::Tax {
        record.evidence = observation.evidence.clone();
    }
    record.result = Some(result);
    identify(record)
}

pub struct ReadRequest<'a> {
    pub scope: &'a IntelligenceScope,
    pub feature: FeatureKind,
    pub entity: &'a str,
    pub mode: Mode,
    pub audience: Audience,
    pub at: DateTime<Utc>,
    pub now: DateTime<Utc>,
}
pub fn read(
    repository: &mut impl IntelligenceRepository,
    config: &IntelligenceConfig,
    query: ReadRequest<'_>,
) -> Result<AssessmentRecord> {
    let ReadRequest {
        scope,
        feature,
        entity,
        mode,
        audience,
        at,
        now,
    } = query;
    config.validate()?;
    if feature != FeatureKind::Tax {
        repository.register(config)?;
    }
    scope.validate()?;
    require(
        config.scope == *scope && valid_key(entity) && at <= now,
        "invalid intelligence read scope or time",
    )?;
    if feature != FeatureKind::Tax {
        repository.purge(now)?;
    }
    if !config.enabled.contains(&feature) || feature == FeatureKind::Tax || mode == Mode::Live {
        let reason = if feature == FeatureKind::Tax {
            "tax_ephemeral_owner_comparison_required"
        } else if mode == Mode::Live {
            "research_only_live_mode_denied"
        } else {
            "feature_disabled"
        };
        return identify(unavailable(
            scope,
            feature,
            entity,
            at,
            now,
            AssessmentStatus::Unavailable,
            reason,
        ));
    }
    let invalidations = repository.invalidations(scope)?;
    let candidates = repository.candidates(scope, feature, entity, at, config.max_query_records)?;
    let mut reason = "no_available_source_vintage";
    let mut status = AssessmentStatus::Unavailable;
    if candidates.is_empty() && repository.has_erased(scope, feature, entity, at)? {
        reason = "input_content_erased_or_retention_expired";
        status = AssessmentStatus::Invalidated;
    }
    for stored in candidates {
        let o = &stored.observation;
        if o.scope != *scope
            || o.entity_id != entity
            || o.payload.kind() != feature
            || stored.ingested_at > at
        {
            continue;
        }
        if invalidations
            .iter()
            .any(|i| i.matches(o) && (i.erase_content || i.at <= at))
        {
            reason = "source_or_observation_invalidated";
            status = AssessmentStatus::Invalidated;
            continue;
        }
        let record = assess_ephemeral(config, o, mode, audience, at, now)?;
        if record.result.is_some() {
            if repository
                .invalidations(scope)?
                .iter()
                .any(|i| i.matches(o) && (i.erase_content || i.at <= at))
            {
                return identify(unavailable(
                    scope,
                    feature,
                    entity,
                    at,
                    now,
                    AssessmentStatus::Invalidated,
                    "source_or_observation_invalidated_during_read",
                ));
            }
            if o.evidence.iter().all(|e| {
                config
                    .sources
                    .iter()
                    .any(|s| s.source_id == e.source_id && s.derived_storage)
            }) {
                repository
                    .save_assessment(&record, stored.retain_until.min(retention_end(config, o)?))?;
            }
            return Ok(record);
        }
        reason = "source_rights_mapping_or_input_unavailable";
    }
    identify(unavailable(scope, feature, entity, at, now, status, reason))
}

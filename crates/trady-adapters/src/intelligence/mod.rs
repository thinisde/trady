//! Bounded normalized imports and read-only tools. No live vendor is assumed.

pub mod book;
pub mod ownership;
pub mod tools;

use chrono::{DateTime, Utc};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};
use trady_application::{contracts::parse_json, intelligence::*};
use trady_domain::{Error, Result, intelligence::*, require};

/// Port for a licensed provider's normalized snapshot or reviewed local import.
/// Implementations have no broker credentials or mutation methods.
pub trait ReadOnlyFeatureSource {
    fn observation(&mut self, max_bytes: usize) -> Result<FeatureObservation>;
}

pub struct JsonFeatureSource<'a> {
    pub path: &'a Path,
}
impl ReadOnlyFeatureSource for JsonFeatureSource<'_> {
    fn observation(&mut self, max_bytes: usize) -> Result<FeatureObservation> {
        require(
            (1024..=4_194_304).contains(&max_bytes),
            "invalid feature import byte limit",
        )?;
        let file = File::open(self.path).map_err(|_| Error("feature import unavailable".into()))?;
        require(
            file.metadata()
                .map_err(|_| Error("feature import metadata unavailable".into()))?
                .is_file(),
            "feature import must be a regular file",
        )?;
        let mut bytes = Vec::new();
        file.take(max_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error("feature import read failed".into()))?;
        require(
            bytes.len() <= max_bytes,
            "feature import exceeds byte limit",
        )?;
        let text =
            std::str::from_utf8(&bytes).map_err(|_| Error("feature import is not UTF-8".into()))?;
        serde_json::from_value(parse_json(text)?)
            .map_err(|_| Error("invalid typed feature observation".into()))
    }
}

/// Deterministic offline repository, also used by forward-only replay fixtures.
type ObservationKey = (String, String, String);
type ErasedMetadata = (FeatureKind, String, DateTime<Utc>);
#[derive(Default)]
pub struct MemoryIntelligenceRepository {
    observations: BTreeMap<ObservationKey, StoredObservation>,
    erased: BTreeMap<ObservationKey, ErasedMetadata>,
    pub invalidations: Vec<Invalidation>,
    pub rejections: Vec<RejectedInput>,
    pub assessments: BTreeMap<String, (AssessmentRecord, DateTime<Utc>)>,
    pub registries: BTreeMap<String, IntelligenceConfig>,
}
fn key(scope: &IntelligenceScope, id: &str) -> (String, String, String) {
    (scope.owner_id.clone(), scope.account_id.clone(), id.into())
}
impl IntelligenceRepository for MemoryIntelligenceRepository {
    fn register(&mut self, config: &IntelligenceConfig) -> Result<String> {
        config.validate()?;
        let hash = config.revision()?;
        self.registries
            .entry(hash.clone())
            .or_insert_with(|| config.clone());
        Ok(hash)
    }
    fn insert(
        &mut self,
        observation: &FeatureObservation,
        at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        registry_hash: &str,
    ) -> Result<StoredObservation> {
        observation.validate()?;
        require(
            observation.payload.kind() != FeatureKind::Tax
                && retain_until > at
                && self.registries.contains_key(registry_hash),
            "tax or expired input cannot be retained in research",
        )?;
        let id = key(&observation.scope, &observation.observation_id);
        require(
            !self.erased.contains_key(&id),
            "erased observation ID cannot be reused",
        )?;
        if let Some(existing) = self.observations.get(&id) {
            require(
                trady_application::research_jobs::digest(&existing.observation)?
                    == trady_application::research_jobs::digest(observation)?,
                "immutable observation ID collision",
            )?;
            return Ok(existing.clone());
        }
        let stored = StoredObservation {
            observation: observation.clone(),
            ingested_at: at,
            retain_until,
            registry_hash: registry_hash.into(),
        };
        self.observations.insert(id, stored.clone());
        if let Some(previous) = &observation.supersedes {
            self.invalidate(&Invalidation {
                scope: observation.scope.clone(),
                target: InvalidationTarget::Observation,
                target_id: previous.clone(),
                at,
                reason: InvalidationReason::Correction,
                erase_content: false,
            })?;
        }
        Ok(stored)
    }
    fn find(&mut self, scope: &IntelligenceScope, id: &str) -> Result<Option<StoredObservation>> {
        Ok(self.observations.get(&key(scope, id)).cloned())
    }
    fn candidates(
        &mut self,
        scope: &IntelligenceScope,
        feature: FeatureKind,
        entity: &str,
        at: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<StoredObservation>> {
        require(
            (1..=1024).contains(&limit),
            "invalid feature candidate limit",
        )?;
        let mut rows: Vec<_> = self
            .observations
            .values()
            .filter(|v| {
                v.observation.scope == *scope
                    && v.observation.entity_id == entity
                    && v.observation.payload.kind() == feature
                    && v.ingested_at <= at
                    && v.observation.evidence.iter().all(|e| e.received_at <= at)
            })
            .cloned()
            .collect();
        rows.sort_by(|a, b| {
            b.ingested_at.cmp(&a.ingested_at).then_with(|| {
                b.observation
                    .observation_id
                    .cmp(&a.observation.observation_id)
            })
        });
        rows.truncate(limit);
        Ok(rows)
    }
    fn has_erased(
        &mut self,
        scope: &IntelligenceScope,
        feature: FeatureKind,
        entity: &str,
        at: DateTime<Utc>,
    ) -> Result<bool> {
        Ok(self.erased.iter().any(|(k, (f, e, t))| {
            k.0 == scope.owner_id
                && k.1 == scope.account_id
                && *f == feature
                && e == entity
                && *t <= at
        }))
    }
    fn invalidations(&mut self, scope: &IntelligenceScope) -> Result<Vec<Invalidation>> {
        Ok(self
            .invalidations
            .iter()
            .filter(|i| i.scope == *scope)
            .cloned()
            .collect())
    }
    fn invalidate(&mut self, invalidation: &Invalidation) -> Result<()> {
        invalidation.validate()?;
        if invalidation.erase_content {
            self.rejections.retain(|r| !r.matches(invalidation));
            let ids: Vec<_> = self
                .observations
                .iter()
                .filter(|(_, s)| invalidation.matches(&s.observation))
                .map(|(k, _)| k.clone())
                .collect();
            for id in &ids {
                if let Some(s) = self.observations.remove(id) {
                    self.erased.insert(
                        id.clone(),
                        (
                            s.observation.payload.kind(),
                            s.observation.entity_id,
                            s.ingested_at,
                        ),
                    );
                }
            }
            self.assessments.retain(|_, (a, _)| {
                !(a.scope == invalidation.scope
                    && (a
                        .observation_id
                        .as_ref()
                        .is_some_and(|id| ids.iter().any(|k| &k.2 == id))
                        || a.evidence.iter().any(|e| match invalidation.target {
                            InvalidationTarget::Observation => {
                                a.observation_id.as_deref() == Some(&invalidation.target_id)
                            }
                            InvalidationTarget::Source => e.source_id == invalidation.target_id,
                            InvalidationTarget::Lineage => {
                                e.lineage_ids.contains(&invalidation.target_id)
                            }
                        })))
            });
        }
        if !self.invalidations.iter().any(|i| {
            i.scope == invalidation.scope
                && i.target == invalidation.target
                && i.target_id == invalidation.target_id
                && i.at == invalidation.at
                && i.reason == invalidation.reason
                && i.erase_content == invalidation.erase_content
        }) {
            self.invalidations.push(invalidation.clone());
        }
        Ok(())
    }
    fn reject(&mut self, rejection: &RejectedInput) -> Result<()> {
        rejection.validate()?;
        if self
            .invalidations
            .iter()
            .any(|i| i.erase_content && rejection.matches(i))
        {
            return Ok(());
        }
        self.rejections.push(rejection.clone());
        Ok(())
    }
    fn save_assessment(
        &mut self,
        assessment: &AssessmentRecord,
        retain_until: DateTime<Utc>,
    ) -> Result<()> {
        require(
            assessment.feature != FeatureKind::Tax
                && !assessment.entry_authorized
                && retain_until > assessment.computed_at,
            "invalid research assessment retention",
        )?;
        if let Some((existing, _)) = self.assessments.get(&assessment.assessment_id) {
            require(
                serde_json::to_value(existing).ok() == serde_json::to_value(assessment).ok(),
                "immutable assessment ID collision",
            )?;
        } else {
            self.assessments.insert(
                assessment.assessment_id.clone(),
                (assessment.clone(), retain_until),
            );
        }
        Ok(())
    }
    fn purge(&mut self, now: DateTime<Utc>) -> Result<()> {
        let expired: Vec<_> = self
            .observations
            .iter()
            .filter(|(_, s)| s.retain_until <= now)
            .map(|(k, _)| k.clone())
            .collect();
        for id in expired {
            if let Some(s) = self.observations.remove(&id) {
                self.erased.insert(
                    id,
                    (
                        s.observation.payload.kind(),
                        s.observation.entity_id,
                        s.ingested_at,
                    ),
                );
            }
        }
        self.assessments.retain(|_, (_, end)| *end > now);
        self.rejections.retain(|r| r.retain_until > now);
        Ok(())
    }
}

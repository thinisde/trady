//! Analyst-only, immutable feature vintages and atomic content invalidations.

use crate::{connect_local, verify_role};
use chrono::{DateTime, Utc};
use postgres::{Client, GenericClient};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use trady_application::{intelligence::*, research_jobs::digest};
use trady_domain::{Error, Result, intelligence::*, require};

fn db(_: postgres::Error) -> Error {
    Error("intelligence database operation failed".into())
}
fn encode(value: &impl Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error("intelligence record encoding failed".into()))
}
fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|_| Error("invalid stored intelligence record".into()))
}
fn text(value: &impl Serialize) -> Result<String> {
    encode(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error("invalid intelligence enum".into()))
}
fn dependencies(observation: &FeatureObservation) -> (Value, Value) {
    let sources: std::collections::BTreeSet<_> = observation
        .evidence
        .iter()
        .map(|e| e.source_id.clone())
        .collect();
    let lineages: std::collections::BTreeSet<_> = observation
        .evidence
        .iter()
        .flat_map(|e| e.lineage_ids.iter().cloned())
        .collect();
    (json!(sources), json!(lineages))
}
fn lock(client: &mut impl GenericClient, scope: &IntelligenceScope) -> Result<()> {
    scope.validate()?;
    client
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1||':'||$2,7910))",
            &[&scope.owner_id, &scope.account_id],
        )
        .map_err(db)?;
    Ok(())
}
fn refresh(client: &mut impl GenericClient, scope: &IntelligenceScope) -> Result<()> {
    scope.validate()?;
    client
        .query_one(
            "SELECT research.refresh_intelligence($1,$2)",
            &[&scope.owner_id, &scope.account_id],
        )
        .map_err(db)?;
    Ok(())
}
fn load_invalidations(
    client: &mut impl GenericClient,
    scope: &IntelligenceScope,
) -> Result<Vec<Invalidation>> {
    client.query("SELECT document FROM research.intelligence_invalidations WHERE owner_id=$1 AND account_id=$2 ORDER BY invalidated_at,invalidation_id",
        &[&scope.owner_id,&scope.account_id]).map_err(db)?.into_iter().map(|r| decode(r.get(0))).collect()
}
fn invalidate_transaction(
    client: &mut impl GenericClient,
    invalidation: &Invalidation,
) -> Result<()> {
    invalidation.validate()?;
    let hash = digest(invalidation)?;
    let kind = text(&invalidation.target)?;
    let document = encode(invalidation)?;
    client.execute("INSERT INTO research.intelligence_invalidations(invalidation_id,owner_id,account_id,target_kind,target_id,invalidated_at,erase_content,document) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
        &[&hash,&invalidation.scope.owner_id,&invalidation.scope.account_id,&kind,&invalidation.target_id,&invalidation.at,&invalidation.erase_content,&document]).map_err(db)?;
    // Database triggers own erasure, including callers that insert SQL directly.
    Ok(())
}

pub struct PgIntelligenceRepository {
    client: Client,
}
impl PgIntelligenceRepository {
    pub fn connect(dsn: &str) -> Result<Self> {
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, "trady_analyst")?;
        Ok(Self { client })
    }
}
impl IntelligenceRepository for PgIntelligenceRepository {
    fn register(&mut self, config: &IntelligenceConfig) -> Result<String> {
        config.validate()?;
        let hash = config.revision()?;
        let document = encode(config)?;
        let mut tx = self.client.transaction().map_err(db)?;
        lock(&mut tx, &config.scope)?;
        tx.execute("INSERT INTO research.intelligence_registries(registry_hash,document) VALUES($1,$2) ON CONFLICT DO NOTHING", &[&hash,&document]).map_err(db)?;
        let old: Value = tx
            .query_one(
                "SELECT document FROM research.intelligence_registries WHERE registry_hash=$1",
                &[&hash],
            )
            .map_err(db)?
            .get(0);
        require(old == document, "immutable intelligence registry collision")?;
        tx.query_one(
            "SELECT research.activate_intelligence_registry($1)",
            &[&hash],
        )
        .map_err(db)?;
        tx.commit().map_err(db)?;
        Ok(hash)
    }
    fn insert(
        &mut self,
        observation: &FeatureObservation,
        _at: DateTime<Utc>,
        retain_until: DateTime<Utc>,
        registry_hash: &str,
    ) -> Result<StoredObservation> {
        observation.validate()?;
        refresh(&mut self.client, &observation.scope)?;
        require(
            observation.payload.kind() != FeatureKind::Tax && valid_hash(registry_hash),
            "tax or invalid registry cannot enter research storage",
        )?;
        let mut tx = self.client.transaction().map_err(db)?;
        lock(&mut tx, &observation.scope)?;
        let ingested_at: DateTime<Utc> = tx
            .query_one("SELECT clock_timestamp()", &[])
            .map_err(db)?
            .get(0);
        require(
            retain_until > ingested_at
                && observation
                    .evidence
                    .iter()
                    .all(|e| e.received_at <= ingested_at),
            "expired or future intelligence input",
        )?;
        require(
            !load_invalidations(&mut tx, &observation.scope)?
                .iter()
                .any(|i| i.matches(observation)),
            "invalidated intelligence cannot be reimported",
        )?;
        let identity: &[&(dyn postgres::types::ToSql + Sync)] = &[
            &observation.scope.owner_id,
            &observation.scope.account_id,
            &observation.observation_id,
        ];
        if let Some(row) = tx.query_opt("SELECT document,ingested_at,retain_until,registry_hash FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND observation_id=$3", identity).map_err(db)? {
            let old: Value = row.get::<_,Option<Value>>(0).ok_or_else(|| Error("erased intelligence ID cannot be reused".into()))?;
            require(old == encode(observation)?, "immutable intelligence observation collision")?;
            return Ok(StoredObservation { observation: decode(old)?, ingested_at: row.get(1), retain_until: row.get(2), registry_hash: row.get(3) });
        }
        let (source_ids, lineage_ids) = dependencies(observation);
        let feature = text(&observation.payload.kind())?;
        let document = encode(observation)?;
        let canonical = serde_json::to_string(observation)
            .map_err(|_| Error("intelligence record encoding failed".into()))?;
        let content_hash = digest(observation)?;
        let retain_until = tx.query_one("INSERT INTO research.intelligence_observations(owner_id,account_id,observation_id,feature,entity_id,content_hash,registry_hash,ingested_at,retain_until,source_ids,lineage_ids,document,canonical) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING retain_until",
            &[&observation.scope.owner_id,&observation.scope.account_id,&observation.observation_id,&feature,&observation.entity_id,&content_hash,&registry_hash,&ingested_at,&retain_until,&source_ids,&lineage_ids,&document,&canonical]).map_err(db)?.get(0);
        // The database validates the predecessor and records its correction marker
        // in the same statement, including when the caller uses SQL directly.
        tx.commit().map_err(db)?;
        Ok(StoredObservation {
            observation: observation.clone(),
            ingested_at,
            retain_until,
            registry_hash: registry_hash.into(),
        })
    }
    fn find(&mut self, scope: &IntelligenceScope, id: &str) -> Result<Option<StoredObservation>> {
        scope.validate()?;
        require(valid_key(id), "invalid intelligence observation ID")?;
        refresh(&mut self.client, scope)?;
        self.client.query_opt("SELECT document,ingested_at,retain_until,registry_hash FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND observation_id=$3 AND document IS NOT NULL AND retain_until>clock_timestamp()",
            &[&scope.owner_id,&scope.account_id,&id]).map_err(db)?.map(|r| Ok(StoredObservation {
                observation: decode(r.get(0))?, ingested_at:r.get(1), retain_until:r.get(2), registry_hash:r.get(3) })).transpose()
    }
    fn candidates(
        &mut self,
        scope: &IntelligenceScope,
        feature: FeatureKind,
        entity: &str,
        at: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<StoredObservation>> {
        scope.validate()?;
        require(
            valid_key(entity) && (1..=1024).contains(&limit),
            "invalid intelligence query",
        )?;
        refresh(&mut self.client, scope)?;
        let feature = text(&feature)?;
        self.client.query("SELECT document,ingested_at,retain_until,registry_hash FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND feature=$3 AND entity_id=$4 AND ingested_at<=$5 AND document IS NOT NULL AND retain_until>clock_timestamp() ORDER BY ingested_at DESC,observation_id DESC LIMIT $6",
            &[&scope.owner_id,&scope.account_id,&feature,&entity,&at,&(limit as i64)]).map_err(db)?.into_iter()
            .map(|r| Ok(StoredObservation { observation:decode(r.get(0))?,ingested_at:r.get(1),retain_until:r.get(2),registry_hash:r.get(3) })).collect()
    }
    fn has_erased(
        &mut self,
        scope: &IntelligenceScope,
        feature: FeatureKind,
        entity: &str,
        at: DateTime<Utc>,
    ) -> Result<bool> {
        scope.validate()?;
        refresh(&mut self.client, scope)?;
        let kind = text(&feature)?;
        Ok(self.client.query_one("SELECT EXISTS(SELECT 1 FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND feature=$3 AND entity_id=$4 AND ingested_at<=$5 AND document IS NULL)",
            &[&scope.owner_id,&scope.account_id,&kind,&entity,&at]).map_err(db)?.get(0))
    }
    fn invalidations(&mut self, scope: &IntelligenceScope) -> Result<Vec<Invalidation>> {
        scope.validate()?;
        refresh(&mut self.client, scope)?;
        load_invalidations(&mut self.client, scope)
    }
    fn invalidate(&mut self, invalidation: &Invalidation) -> Result<()> {
        invalidation.validate()?;
        refresh(&mut self.client, &invalidation.scope)?;
        let mut tx = self.client.transaction().map_err(db)?;
        lock(&mut tx, &invalidation.scope)?;
        let now: DateTime<Utc> = tx
            .query_one("SELECT clock_timestamp()", &[])
            .map_err(db)?
            .get(0);
        require(invalidation.at <= now, "future intelligence invalidation")?;
        invalidate_transaction(&mut tx, invalidation)?;
        tx.commit().map_err(db)
    }
    fn reject(&mut self, rejection: &RejectedInput) -> Result<()> {
        rejection.validate()?;
        refresh(&mut self.client, &rejection.scope)?;
        let mut tx = self.client.transaction().map_err(db)?;
        lock(&mut tx, &rejection.scope)?;
        let now: DateTime<Utc> = tx
            .query_one("SELECT clock_timestamp()", &[])
            .map_err(db)?
            .get(0);
        require(
            rejection.retain_until > now && rejection.rejected_at <= now,
            "expired or future rejection marker",
        )?;
        if load_invalidations(&mut tx, &rejection.scope)?
            .iter()
            .any(|i| i.erase_content && rejection.matches(i))
        {
            return Ok(());
        }
        let hash = digest(rejection)?;
        let sources = json!(rejection.source_revisions.keys().collect::<Vec<_>>());
        let lineages = json!(rejection.lineage_ids);
        tx.execute("INSERT INTO research.intelligence_rejections(rejection_id,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,document) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
            &[&hash,&rejection.scope.owner_id,&rejection.scope.account_id,&rejection.observation_id,&sources,&lineages,&rejection.retain_until,&encode(rejection)?]).map_err(db)?;
        tx.commit().map_err(db)
    }
    fn save_assessment(
        &mut self,
        assessment: &AssessmentRecord,
        retain_until: DateTime<Utc>,
    ) -> Result<()> {
        require(
            assessment.feature != FeatureKind::Tax
                && !assessment.entry_authorized
                && assessment.calibration == "RESEARCH_ONLY_UNCALIBRATED",
            "invalid research-only assessment",
        )?;
        refresh(&mut self.client, &assessment.scope)?;
        let id = assessment
            .observation_id
            .as_ref()
            .ok_or_else(|| Error("assessment requires observation".into()))?;
        let mut tx = self.client.transaction().map_err(db)?;
        lock(&mut tx, &assessment.scope)?;
        let row = tx.query_opt("SELECT document,retain_until FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND observation_id=$3 AND document IS NOT NULL AND retain_until>clock_timestamp()",
            &[&assessment.scope.owner_id,&assessment.scope.account_id,id]).map_err(db)?
            .ok_or_else(|| Error("intelligence input erased or expired".into()))?;
        let observation: FeatureObservation = decode(row.get(0))?;
        require(
            !load_invalidations(&mut tx, &assessment.scope)?
                .iter()
                .any(|i| {
                    i.matches(&observation) && (i.erase_content || i.at <= assessment.decision_at)
                }),
            "assessment dependencies invalidated",
        )?;
        let end = retain_until.min(row.get(1));
        let (sources, lineages) = dependencies(&observation);
        let document = encode(assessment)?;
        tx.execute("INSERT INTO research.intelligence_assessments(assessment_id,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,document) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
            &[&assessment.assessment_id,&assessment.scope.owner_id,&assessment.scope.account_id,id,&sources,&lineages,&end,&document]).map_err(db)?;
        let old: Value = tx
            .query_one(
                "SELECT document FROM research.intelligence_assessments WHERE assessment_id=$1",
                &[&assessment.assessment_id],
            )
            .map_err(db)?
            .get(0);
        require(
            old == document,
            "immutable intelligence assessment collision",
        )?;
        tx.commit().map_err(db)
    }
    fn purge(&mut self, _now: DateTime<Utc>) -> Result<()> {
        // Cleanup follows database wall time even during historical replay.
        self.client
            .query_one("SELECT research.refresh_intelligence()", &[])
            .map_err(db)?;
        Ok(())
    }
}

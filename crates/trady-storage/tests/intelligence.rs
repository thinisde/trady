//! Runs exclusively against scripts/test-postgres.py's disposable database.
#[path = "../../trady-adapters/tests/intelligence_support/mod.rs"]
mod intelligence_support;

use chrono::{DateTime, Duration, Utc};
use intelligence_support::*;
use postgres::{Client, NoTls};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use trady_application::{harness::new_id, intelligence::*, research_jobs::digest};
use trady_domain::{Mode, intelligence::*};
use trady_storage::{intelligence::PgIntelligenceRepository, migrate};

fn dsn(role: &str) -> String {
    if let Ok(value) = std::env::var(format!("{}_DSN", role.to_ascii_uppercase())) {
        return value;
    }
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        std::env::var("TRADY_TEST_PORT").unwrap()
    )
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn intelligence_immutable_storage_corrections_erasure_and_role_isolation() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let at = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let (mut config, mut observation, _) = envelope_at(input(FeatureKind::Accounting), at);
    config.scope.owner_id = new_id("intelligence_owner");
    observation.scope = config.scope.clone();
    let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
    let first = ingest(&mut repository, &config, &observation, Utc::now()).unwrap();
    let duplicate = ingest(&mut repository, &config, &observation, Utc::now()).unwrap();
    assert_eq!(first.ingested_at, duplicate.ingested_at);
    let mut changed = observation.clone();
    changed.evidence[0].complete = false;
    assert!(ingest(&mut repository, &config, &changed, Utc::now()).is_err());
    let mut missing_predecessor = observation.clone();
    missing_predecessor.observation_id = "missing_predecessor_correction".into();
    missing_predecessor.supersedes = Some("missing_predecessor".into());
    assert!(ingest(&mut repository, &config, &missing_predecessor, Utc::now()).is_err());
    assert!(
        repository
            .find(&config.scope, &missing_predecessor.observation_id)
            .unwrap()
            .is_none()
    );
    let mut corrected = observation.clone();
    corrected.observation_id = "corrected_observation".into();
    corrected.supersedes = Some(observation.observation_id.clone());
    let latest = ingest(&mut repository, &config, &corrected, Utc::now()).unwrap();
    let historical = read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: FeatureKind::Accounting,
            entity: &observation.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at: first.ingested_at,
            now: Utc::now(),
        },
    )
    .unwrap();
    assert_eq!(
        historical.observation_id.as_deref(),
        Some(observation.observation_id.as_str())
    );
    let current = read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: FeatureKind::Accounting,
            entity: &observation.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at: latest.ingested_at,
            now: Utc::now(),
        },
    )
    .unwrap();
    assert_eq!(
        current.observation_id.as_deref(),
        Some(corrected.observation_id.as_str())
    );
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    let params: &[&(dyn postgres::types::ToSql + Sync)] =
        &[&config.scope.owner_id, &config.scope.account_id];
    let rejected = analyst
        .query(
            "SELECT document FROM research.intelligence_rejections WHERE owner_id=$1 AND account_id=$2",
            params,
        )
        .unwrap();
    assert_eq!(rejected.len(), 2);
    for row in rejected {
        let marker: serde_json::Value = row.get(0);
        assert_eq!(marker["code"], "ingestion_rejected");
        assert!(!marker["source_revisions"].as_object().unwrap().is_empty());
    }
    assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_assessments WHERE owner_id=$1 AND account_id=$2",params).unwrap().get::<_,i64>(0),2);
    assert!(analyst.execute("UPDATE research.intelligence_observations SET document=document WHERE owner_id=$1 AND account_id=$2",params).is_err());
    assert!(analyst.execute("UPDATE research.intelligence_observations SET ingested_at=clock_timestamp() WHERE owner_id=$1 AND account_id=$2",params).is_err());
    let mut malformed = corrected.clone();
    malformed.observation_id = "invalid id with private context".into();
    malformed.scope.owner_id = "untrusted private owner".into();
    assert!(ingest(&mut repository, &config, &malformed, Utc::now()).is_err());
    let rejected:serde_json::Value=analyst.query_one("SELECT document FROM research.intelligence_rejections WHERE owner_id=$1 AND account_id=$2 AND observation_id IS NULL",params).unwrap().get(0);
    assert!(rejected["observation_id"].is_null());
    assert!(!rejected.to_string().contains("private"));
    // Predecessor content may expire or be erased after the correction was stored.
    analyst.execute("UPDATE research.intelligence_observations SET document=NULL WHERE owner_id=$1 AND account_id=$2 AND observation_id=$3",
        &[&config.scope.owner_id,&config.scope.account_id,&observation.observation_id]).unwrap();
    assert!(
        repository
            .find(&config.scope, &observation.observation_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        ingest(&mut repository, &config, &corrected, Utc::now())
            .unwrap()
            .ingested_at,
        latest.ingested_at
    );
    let mut no_audit = config.clone();
    for source in &mut no_audit.sources {
        source.audit_markers = false;
    }
    assert!(ingest(&mut repository, &no_audit, &corrected, Utc::now()).is_err());
    // Registering withdrawn audit rights erases previously retained markers too.
    assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_rejections WHERE owner_id=$1 AND account_id=$2",params).unwrap().get::<_,i64>(0),0);
    for role in [
        "trady_executor",
        "trady_gateway",
        "trady_lab",
        "trady_shadow",
    ] {
        let mut client = Client::connect(&dsn(role), NoTls).unwrap();
        assert!(
            client
                .query(
                    "SELECT document FROM research.intelligence_observations",
                    &[]
                )
                .is_err(),
            "{role}"
        );
        assert!(
            client
                .query(
                    "SELECT document FROM research.intelligence_assessments",
                    &[]
                )
                .is_err(),
            "{role}"
        );
        assert!(
            PgIntelligenceRepository::connect(&dsn(role)).is_err(),
            "{role}"
        );
    }
    let mut other = config.scope.clone();
    other.owner_id = new_id("different_owner");
    assert!(
        repository
            .find(&other, &observation.observation_id)
            .unwrap()
            .is_none()
    );
    repository
        .invalidate(&Invalidation {
            scope: config.scope.clone(),
            target: InvalidationTarget::Lineage,
            target_id: observation.evidence[0].lineage_ids[0].clone(),
            at: Utc::now(),
            reason: InvalidationReason::RightsRevoked,
            erase_content: true,
        })
        .unwrap();
    assert!(
        repository
            .find(&config.scope, &observation.observation_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_assessments WHERE owner_id=$1 AND account_id=$2",params).unwrap().get::<_,i64>(0),0);
    assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_rejections WHERE owner_id=$1 AND account_id=$2",params).unwrap().get::<_,i64>(0),0);
    assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND document IS NULL",params).unwrap().get::<_,i64>(0),2);
    let erased = read(
        &mut repository,
        &no_audit,
        ReadRequest {
            scope: &config.scope,
            feature: FeatureKind::Accounting,
            entity: &observation.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at: first.ingested_at,
            now: Utc::now(),
        },
    )
    .unwrap();
    assert_eq!(erased.status, AssessmentStatus::Invalidated);
    corrected.observation_id = "new_id_deleted_lineage".into();
    corrected.supersedes = None;
    assert!(ingest(&mut repository, &config, &corrected, Utc::now()).is_err());
}

fn fresh_intelligence() -> (IntelligenceConfig, FeatureObservation) {
    let at = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let (mut config, mut observation, _) = envelope_at(input(FeatureKind::Accounting), at);
    config.scope.owner_id = new_id("sql_intelligence");
    observation.scope = config.scope.clone();
    (config, observation)
}

fn sql_observation(
    client: &mut Client,
    observation: &FeatureObservation,
    registry: &str,
    document: &Value,
    sources: &Value,
) -> Result<u64, postgres::Error> {
    let lineages: BTreeSet<_> = observation
        .evidence
        .iter()
        .flat_map(|e| e.lineage_ids.iter().cloned())
        .collect();
    let canonical = serde_json::from_value::<FeatureObservation>(document.clone())
        .ok()
        .and_then(|o| serde_json::to_string(&o).ok())
        .unwrap_or_else(|| serde_json::to_string(document).unwrap());
    let feature = serde_json::to_value(observation.payload.kind()).unwrap();
    client.execute("INSERT INTO research.intelligence_observations(owner_id,account_id,observation_id,feature,entity_id,content_hash,registry_hash,retain_until,source_ids,lineage_ids,document,canonical) VALUES($1,$2,$3,$10,$4,$5,$6,clock_timestamp()+interval '1 year',$7,$8,$9,$11)",
        &[&observation.scope.owner_id,&observation.scope.account_id,&observation.observation_id,&observation.entity_id,&digest(observation).unwrap(),&registry,&sources,&json!(lineages),&document,&feature.as_str().unwrap(),&canonical])
}

fn sources(observation: &FeatureObservation) -> Value {
    json!(
        observation
            .evidence
            .iter()
            .map(|e| &e.source_id)
            .collect::<BTreeSet<_>>()
    )
}

fn sql_invalidation(
    client: &mut Client,
    invalidation: &Invalidation,
) -> Result<u64, postgres::Error> {
    let kind = serde_json::to_value(invalidation.target).unwrap();
    client.execute("INSERT INTO research.intelligence_invalidations(invalidation_id,owner_id,account_id,target_kind,target_id,invalidated_at,erase_content,document) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        &[&digest(invalidation).unwrap(),&invalidation.scope.owner_id,&invalidation.scope.account_id,&kind.as_str().unwrap(),&invalidation.target_id,&invalidation.at,&invalidation.erase_content,&serde_json::to_value(invalidation).unwrap()])
}

fn retained_rejection(
    repository: &mut PgIntelligenceRepository,
    config: &IntelligenceConfig,
    observation: &FeatureObservation,
) {
    repository.reject(&rejection(config, observation)).unwrap();
}

fn rejection(config: &IntelligenceConfig, observation: &FeatureObservation) -> RejectedInput {
    RejectedInput {
        scope: config.scope.clone(),
        observation_id: Some(observation.observation_id.clone()),
        feature: FeatureKind::Accounting,
        input_hash: digest(observation).unwrap(),
        code: "synthetic_rejection".into(),
        rejected_at: Utc::now(),
        retain_until: Utc::now() + Duration::hours(1),
        source_revisions: config
            .sources
            .iter()
            .map(|s| (s.source_id.clone(), s.rights_revision.clone()))
            .collect::<BTreeMap<_, _>>(),
        lineage_ids: observation
            .evidence
            .iter()
            .flat_map(|e| e.lineage_ids.iter().cloned())
            .collect(),
    }
}

fn sql_rejection(client: &mut Client, marker: &RejectedInput) -> Result<u64, postgres::Error> {
    client.execute("INSERT INTO research.intelligence_rejections(rejection_id,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,document) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        &[&digest(marker).unwrap(),&marker.scope.owner_id,&marker.scope.account_id,&marker.observation_id,&json!(marker.source_revisions.keys().collect::<Vec<_>>()),&json!(marker.lineage_ids),&marker.retain_until,&serde_json::to_value(marker).unwrap()])
}

fn sql_assessment(client: &mut Client, record: &AssessmentRecord) -> Result<u64, postgres::Error> {
    client.execute("INSERT INTO research.intelligence_assessments(assessment_id,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,document) SELECT $1,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,$2 FROM research.intelligence_observations WHERE owner_id=$3 AND account_id=$4 AND observation_id=$5",
        &[&record.assessment_id,&serde_json::to_value(record).unwrap(),&record.scope.owner_id,&record.scope.account_id,&record.observation_id])
}

fn assert_requires_read_committed(error: postgres::Error) {
    assert_eq!(
        error.code(),
        Some(&postgres::error::SqlState::FEATURE_NOT_SUPPORTED),
        "{error:?}"
    );
    assert_eq!(
        error.as_db_error().unwrap().message(),
        "STORAGE_REQUIRES_READ_COMMITTED"
    );
}

fn fixed_snapshot(scope: &IntelligenceScope, isolation: &str) -> Client {
    let mut client = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    client
        .batch_execute(&format!("BEGIN ISOLATION LEVEL {isolation}"))
        .unwrap();
    client.query_one("SELECT registry_hash FROM research.intelligence_scopes WHERE owner_id=$1 AND account_id=$2",&[&scope.owner_id,&scope.account_id]).unwrap();
    client
}

fn assert_physical_dependents(client: &mut Client, scope: &IntelligenceScope, count: i64) {
    // Use the disposable administrator so RLS cannot hide retained JSON rows.
    for table in ["intelligence_assessments", "intelligence_rejections"] {
        assert_eq!(
            client
                .query_one(
                    &format!(
                        "SELECT count(*) FROM research.{table} WHERE owner_id=$1 AND account_id=$2"
                    ),
                    &[&scope.owner_id, &scope.account_id]
                )
                .unwrap()
                .get::<_, i64>(0),
            count,
            "{table}"
        );
    }
}

fn retained_assessment(
    repository: &mut PgIntelligenceRepository,
    config: &IntelligenceConfig,
    observation: &FeatureObservation,
    at: DateTime<Utc>,
) -> AssessmentRecord {
    read(
        repository,
        config,
        ReadRequest {
            scope: &config.scope,
            feature: FeatureKind::Accounting,
            entity: &observation.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at,
            now: Utc::now(),
        },
    )
    .unwrap()
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_rejects_missing_misbound_and_tax_content_without_caller_checks() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let (config, observation) = fresh_intelligence();
    let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
    let registry = repository.register(&config).unwrap();
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    let good = serde_json::to_value(&observation).unwrap();
    let mut missing_feature = good.clone();
    missing_feature["payload"]
        .as_object_mut()
        .unwrap()
        .remove("feature");
    let mut wrong_scope = good.clone();
    wrong_scope["scope"]["owner_id"] = json!("different_owner");
    let mut nested_tax = good.clone();
    nested_tax["payload"]["input"]["peers"] =
        json!([{"feature":"TAX","input":{"private":"forbidden"}}]);
    let mut extra_tax = good.clone();
    extra_tax["payload"]["input"]["tax_lots"] = json!([]);
    let mut no_evidence = good.clone();
    no_evidence["evidence"] = Value::Null;
    for document in [
        json!({}),
        missing_feature,
        wrong_scope,
        nested_tax,
        extra_tax,
        no_evidence,
    ] {
        assert!(
            sql_observation(
                &mut analyst,
                &observation,
                &registry,
                &document,
                &sources(&observation)
            )
            .is_err()
        );
    }
    assert!(
        sql_observation(
            &mut analyst,
            &observation,
            &registry,
            &good,
            &json!(["unrelated_source"])
        )
        .is_err()
    );
    sql_observation(
        &mut analyst,
        &observation,
        &registry,
        &good,
        &sources(&observation),
    )
    .unwrap();
    let stored = repository
        .find(&config.scope, &observation.observation_id)
        .unwrap()
        .unwrap();
    assert!(
        stored.retain_until
            <= observation
                .evidence
                .iter()
                .map(|e| e.expires_at)
                .min()
                .unwrap()
    );
    let record = retained_assessment(&mut repository, &config, &observation, stored.ingested_at);
    assert!(record.result.is_some());
    let good_record = serde_json::to_value(record).unwrap();
    let insert = "INSERT INTO research.intelligence_assessments(assessment_id,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,document) SELECT $1,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,$2 FROM research.intelligence_observations WHERE owner_id=$3 AND account_id=$4 AND observation_id=$5";
    for field in [
        "feature",
        "entry_authorized",
        "calibration",
        "observation_id",
    ] {
        let mut document = good_record.clone();
        let id = new_id("sql_assessment");
        document["assessment_id"] = json!(id);
        document.as_object_mut().unwrap().remove(field);
        assert!(
            analyst
                .execute(
                    insert,
                    &[
                        &id,
                        &document,
                        &config.scope.owner_id,
                        &config.scope.account_id,
                        &observation.observation_id
                    ]
                )
                .is_err(),
            "{field}"
        );
    }
    let mut document = good_record;
    let id = new_id("sql_assessment");
    document["assessment_id"] = json!(id);
    document["entry_authorized"] = json!("false");
    assert!(
        analyst
            .execute(
                insert,
                &[
                    &id,
                    &document,
                    &config.scope.owner_id,
                    &config.scope.account_id,
                    &observation.observation_id
                ]
            )
            .is_err()
    );
    retained_rejection(&mut repository, &config, &observation);
    let marker: Value = analyst
        .query_one(
            "SELECT document FROM research.intelligence_rejections WHERE owner_id=$1 AND account_id=$2",
            &[&config.scope.owner_id, &config.scope.account_id],
        )
        .unwrap()
        .get(0);
    for field in ["feature", "scope", "source_revisions", "lineage_ids"] {
        let mut document = marker.clone();
        document.as_object_mut().unwrap().remove(field);
        let id = digest(&(field, &document)).unwrap();
        assert!(analyst.execute("INSERT INTO research.intelligence_rejections(rejection_id,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,document) SELECT $1,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,$2 FROM research.intelligence_rejections WHERE owner_id=$3 AND account_id=$4 LIMIT 1",&[&id,&document,&config.scope.owner_id,&config.scope.account_id]).is_err(),"{field}");
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_invalidation_erases_all_dependency_kinds_and_prevents_reimport() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    for target in [
        InvalidationTarget::Observation,
        InvalidationTarget::Source,
        InvalidationTarget::Lineage,
    ] {
        let (config, mut observation) = fresh_intelligence();
        let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
        let stored = ingest(&mut repository, &config, &observation, Utc::now()).unwrap();
        let assessment =
            retained_assessment(&mut repository, &config, &observation, stored.ingested_at);
        assert!(assessment.result.is_some());
        retained_rejection(&mut repository, &config, &observation);
        let target_id = match target {
            InvalidationTarget::Observation => observation.observation_id.clone(),
            InvalidationTarget::Source => observation.evidence[0].source_id.clone(),
            InvalidationTarget::Lineage => observation.evidence[0].lineage_ids[0].clone(),
        };
        let mut invalidation = Invalidation {
            scope: config.scope.clone(),
            target,
            target_id,
            at: Utc::now(),
            reason: InvalidationReason::RightsRevoked,
            erase_content: false,
        };
        let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
        assert!(sql_invalidation(&mut analyst, &invalidation).is_err());
        invalidation.erase_content = true;
        invalidation.at = Utc::now() + Duration::hours(1);
        assert!(sql_invalidation(&mut analyst, &invalidation).is_err());
        invalidation.at = Utc::now();
        sql_invalidation(&mut analyst, &invalidation).unwrap();
        for table in ["intelligence_assessments", "intelligence_rejections"] {
            assert_eq!(analyst.query_one(&format!("SELECT count(*) FROM research.{table} WHERE owner_id=$1 AND account_id=$2"),&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,i64>(0),0);
        }
        assert!(analyst.query_one("SELECT document IS NULL AND canonical IS NULL FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND observation_id=$3",&[&config.scope.owner_id,&config.scope.account_id,&observation.observation_id]).unwrap().get::<_,bool>(0));
        observation.observation_id = "reimported".into();
        let document = serde_json::to_value(&observation).unwrap();
        assert!(
            sql_observation(
                &mut analyst,
                &observation,
                &stored.registry_hash,
                &document,
                &sources(&observation)
            )
            .is_err()
        );
        assert!(
            repository
                .find(&config.scope, &stored.observation.observation_id)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_direct_erasure_expiry_and_registry_withdrawals_remove_derived_content() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    for transition in [
        "document_erasure",
        "expiry",
        "disabled",
        "storage",
        "audit",
        "capability",
        "revision",
        "derived",
        "shortened_retention",
    ] {
        let (mut config, mut observation) = fresh_intelligence();
        if transition == "shortened_retention" {
            for evidence in &mut observation.evidence {
                evidence.observed_at -= Duration::hours(2);
                evidence.published_at -= Duration::hours(2);
                evidence.available_at -= Duration::hours(2);
                evidence.received_at -= Duration::hours(2);
            }
        }
        let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
        let stored = ingest(&mut repository, &config, &observation, Utc::now()).unwrap();
        assert!(
            retained_assessment(&mut repository, &config, &observation, stored.ingested_at)
                .result
                .is_some()
        );
        retained_rejection(&mut repository, &config, &observation);
        let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
        let args: &[&(dyn postgres::types::ToSql + Sync)] =
            &[&config.scope.owner_id, &config.scope.account_id];
        if transition == "document_erasure" {
            analyst.execute("UPDATE research.intelligence_observations SET document=NULL WHERE owner_id=$1 AND account_id=$2",args).unwrap();
        } else if transition == "expiry" {
            let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
            admin.execute("UPDATE research.intelligence_observations SET retain_until=clock_timestamp()-interval '1 second' WHERE owner_id=$1 AND account_id=$2",args).unwrap();
            assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND document IS NOT NULL",args).unwrap().get::<_,i64>(0),0);
            assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_assessments WHERE owner_id=$1 AND account_id=$2",args).unwrap().get::<_,i64>(0),0);
            repository.purge(Utc::now() - Duration::days(365)).unwrap();
        } else {
            for source in &mut config.sources {
                match transition {
                    "disabled" => source.enabled = false,
                    "storage" => source.rights.normalized_storage = false,
                    "audit" => source.audit_markers = false,
                    "capability" => {
                        source.capability.valid_until = Some(Utc::now() - Duration::seconds(1))
                    }
                    "revision" => {
                        source.rights_revision = "withdrawn_revision".into();
                        source.capability.revision = source.rights_revision.clone();
                    }
                    "derived" => source.derived_storage = false,
                    "shortened_retention" => source.rights.retention_hours = 1,
                    _ => unreachable!(),
                }
            }
            repository.register(&config).unwrap();
        }
        let args: &[&(dyn postgres::types::ToSql + Sync)] =
            &[&config.scope.owner_id, &config.scope.account_id];
        assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_assessments WHERE owner_id=$1 AND account_id=$2",args).unwrap().get::<_,i64>(0),0,"{transition}");
        let erased:bool=analyst.query_one("SELECT document IS NULL AND canonical IS NULL FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2",args).unwrap().get(0);
        assert_eq!(erased, transition != "derived", "{transition}");
        if transition != "derived" {
            assert!(
                repository
                    .find(&config.scope, &observation.observation_id)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_retired_registry_cannot_be_reactivated_by_stale_callers() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let (config, mut observation) = fresh_intelligence();
    let mut current = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
    let mut stale = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
    let original = ingest(&mut current, &config, &observation, Utc::now()).unwrap();
    let mut snapshot = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    snapshot
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ")
        .unwrap();
    snapshot.query_one("SELECT registry_hash FROM research.intelligence_scopes WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap();
    let mut read_snapshot = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    read_snapshot
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ")
        .unwrap();
    read_snapshot.query_one("SELECT registry_hash FROM research.intelligence_scopes WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap();
    let mut withdrawn = config.clone();
    for source in &mut withdrawn.sources {
        source.rights.normalized_storage = false;
    }
    let replacement = current.register(&withdrawn).unwrap();
    assert!(read_snapshot.query("SELECT document FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND document IS NOT NULL",&[&config.scope.owner_id,&config.scope.account_id]).is_err());
    read_snapshot.batch_execute("ROLLBACK").unwrap();
    assert!(stale.register(&config).is_err());
    observation.observation_id = "stale_registry_different_observation".into();
    assert!(
        sql_observation(
            &mut snapshot,
            &observation,
            &original.registry_hash,
            &serde_json::to_value(&observation).unwrap(),
            &sources(&observation)
        )
        .is_err()
    );
    snapshot.batch_execute("ROLLBACK").unwrap();
    assert!(
        stale
            .insert(
                &observation,
                Utc::now(),
                Utc::now() + Duration::hours(1),
                &original.registry_hash
            )
            .is_err()
    );
    assert!(ingest(&mut stale, &config, &observation, Utc::now()).is_err());
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    assert!(
        sql_observation(
            &mut analyst,
            &observation,
            &original.registry_hash,
            &serde_json::to_value(&observation).unwrap(),
            &sources(&observation)
        )
        .is_err()
    );
    let active:String=analyst.query_one("SELECT registry_hash FROM research.intelligence_scopes WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get(0);
    assert_eq!(active, replacement);
    // A new reviewed revision can grant new content, but cannot revive erased
    // lineage under a different observation ID. Earlier hashes remain retired.
    let mut reviewed = config.clone();
    for source in &mut reviewed.sources {
        source.rights_revision = "reviewed_rights_v2".into();
        source.capability.revision = source.rights_revision.clone();
        source.capability.reviewed_at = Some(Utc::now());
    }
    for evidence in &mut observation.evidence {
        evidence.rights_revision = "reviewed_rights_v2".into();
    }
    let reviewed_hash = current.register(&reviewed).unwrap();
    assert!(
        sql_observation(
            &mut analyst,
            &observation,
            &reviewed_hash,
            &serde_json::to_value(&observation).unwrap(),
            &sources(&observation)
        )
        .is_err()
    );
    for evidence in &mut observation.evidence {
        evidence.lineage_ids = vec![new_id("reviewed_lineage")];
        evidence.content_hash = digest(&(&evidence.content_hash, "reviewed_new_content")).unwrap();
    }
    assert!(
        current
            .insert(
                &observation,
                Utc::now(),
                Utc::now() + Duration::hours(1),
                &reviewed_hash
            )
            .is_ok()
    );
    observation.observation_id = original.observation.observation_id;
    assert!(
        current
            .insert(
                &observation,
                Utc::now(),
                Utc::now() + Duration::hours(1),
                &reviewed_hash
            )
            .is_err()
    );
    assert!(stale.register(&withdrawn).is_err());
    assert!(stale.register(&config).is_err());
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_direct_erasure_blocks_new_ids_markers_and_stale_snapshots() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let (config, mut observation) = fresh_intelligence();
    let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
    let stored = ingest(&mut repository, &config, &observation, Utc::now()).unwrap();
    retained_rejection(&mut repository, &config, &observation);
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    let mut marker:Value=analyst.query_one("SELECT document FROM research.intelligence_rejections WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get(0);
    let mut snapshot = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    snapshot
        .batch_execute("BEGIN ISOLATION LEVEL REPEATABLE READ")
        .unwrap();
    snapshot.query_one("SELECT registry_hash FROM research.intelligence_scopes WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap();
    analyst.execute("UPDATE research.intelligence_observations SET document=NULL WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap();
    observation.observation_id = "same_erased_content_new_id".into();
    let document = serde_json::to_value(&observation).unwrap();
    assert!(
        sql_observation(
            &mut snapshot,
            &observation,
            &stored.registry_hash,
            &document,
            &sources(&observation)
        )
        .is_err()
    );
    snapshot.batch_execute("ROLLBACK").unwrap();
    assert!(
        sql_observation(
            &mut analyst,
            &observation,
            &stored.registry_hash,
            &document,
            &sources(&observation)
        )
        .is_err()
    );
    marker["observation_id"] = json!(observation.observation_id);
    let id = digest(&marker).unwrap();
    assert!(analyst.execute("INSERT INTO research.intelligence_rejections(rejection_id,owner_id,account_id,observation_id,source_ids,lineage_ids,retain_until,document) VALUES($1,$2,$3,$4,$5,$6,($7->>'retain_until')::timestamptz,$7)",&[&id,&config.scope.owner_id,&config.scope.account_id,&observation.observation_id,&sources(&observation),&marker["lineage_ids"],&marker]).is_err());
    assert_eq!(analyst.query_one("SELECT count(*) FROM research.intelligence_rejections WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,i64>(0),0);
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_accepts_all_nine_typed_feature_envelopes_and_hashes() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    for feature in FeatureKind::ALL
        .into_iter()
        .filter(|f| *f != FeatureKind::Tax)
    {
        let at = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
        let (mut config, mut observation, _) = envelope_at(input(feature), at);
        config.scope.owner_id = new_id("sql_feature_schema");
        observation.scope = config.scope.clone();
        let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
        let registry = repository.register(&config).unwrap();
        let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
        sql_observation(
            &mut analyst,
            &observation,
            &registry,
            &serde_json::to_value(&observation).unwrap(),
            &sources(&observation),
        )
        .unwrap_or_else(|error| panic!("{feature:?}: {error}"));
        assert!(
            repository
                .find(&config.scope, &observation.observation_id)
                .unwrap()
                .is_some(),
            "{feature:?}"
        );
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_empty_history_snapshots_cannot_miss_invalidations_or_new_tombstones() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    for isolation in ["REPEATABLE READ", "SERIALIZABLE"] {
        for transition in ["source_invalidation", "created_then_erased_lineage"] {
            let (config, mut observation) = fresh_intelligence();
            let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
            let registry = repository.register(&config).unwrap();
            // Both snapshots predate every observation, invalidation and tombstone
            // in this scope. No registry row is changed after this point.
            let mut writer = fixed_snapshot(&config.scope, isolation);
            let mut marker_writer = fixed_snapshot(&config.scope, isolation);
            let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
            if transition == "source_invalidation" {
                sql_invalidation(
                    &mut analyst,
                    &Invalidation {
                        scope: config.scope.clone(),
                        target: InvalidationTarget::Source,
                        target_id: observation.evidence[0].source_id.clone(),
                        at: Utc::now(),
                        reason: InvalidationReason::RightsRevoked,
                        erase_content: true,
                    },
                )
                .unwrap();
            } else {
                analyst
                    .batch_execute("BEGIN ISOLATION LEVEL READ COMMITTED")
                    .unwrap();
                sql_observation(
                    &mut analyst,
                    &observation,
                    &registry,
                    &serde_json::to_value(&observation).unwrap(),
                    &sources(&observation),
                )
                .unwrap();
                assert_eq!(analyst.execute("UPDATE research.intelligence_observations SET document=NULL WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap(),1);
                analyst.batch_execute("COMMIT").unwrap();
            }
            for snapshot in [&mut writer, &mut marker_writer] {
                assert_eq!(snapshot.query_one("SELECT count(*) FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,i64>(0),0);
                assert_eq!(snapshot.query_one("SELECT count(*) FROM research.intelligence_invalidations WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,i64>(0),0);
            }
            observation.observation_id = new_id("empty_snapshot_reimport");
            let document = serde_json::to_value(&observation).unwrap();
            let marker = rejection(&config, &observation);
            assert_requires_read_committed(
                sql_observation(
                    &mut writer,
                    &observation,
                    &registry,
                    &document,
                    &sources(&observation),
                )
                .unwrap_err(),
            );
            writer.batch_execute("ROLLBACK").unwrap();
            assert_requires_read_committed(sql_rejection(&mut marker_writer, &marker).unwrap_err());
            marker_writer.batch_execute("ROLLBACK").unwrap();
            // A supported retry must see the actual revocation/tombstone too.
            let error = sql_observation(
                &mut writer,
                &observation,
                &registry,
                &document,
                &sources(&observation),
            )
            .unwrap_err();
            assert_eq!(
                error.as_db_error().unwrap().message(),
                if transition == "source_invalidation" {
                    "INVALIDATED_INTELLIGENCE_CANNOT_BE_REIMPORTED"
                } else {
                    "ERASED_INTELLIGENCE_LINEAGE_CANNOT_BE_REIMPORTED"
                }
            );
            let error = sql_rejection(&mut marker_writer, &marker).unwrap_err();
            assert_eq!(
                error.as_db_error().unwrap().message(),
                "ERASED_INTELLIGENCE_REJECTION"
            );
            assert_eq!(admin.query_one("SELECT count(*) FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2 AND document IS NOT NULL",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,i64>(0),0);
            assert_physical_dependents(&mut admin, &config.scope, 0);
        }
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_stale_erasers_cannot_miss_later_committed_dependents() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    for isolation in ["REPEATABLE READ", "SERIALIZABLE"] {
        for operation in ["direct_erasure", "source_invalidation", "refresh"] {
            let (config, observation) = fresh_intelligence();
            let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
            let stored = ingest(&mut repository, &config, &observation, Utc::now()).unwrap();
            // Obtain a valid application-produced assessment before establishing
            // the old snapshot, then remove it. Publication below is direct SQL.
            let mut record =
                retained_assessment(&mut repository, &config, &observation, stored.ingested_at);
            assert!(record.result.is_some());
            record.assessment_id = new_id("later_committed_assessment");
            let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
            analyst.execute("DELETE FROM research.intelligence_assessments WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap();
            assert_physical_dependents(&mut admin, &config.scope, 0);
            let mut eraser = fixed_snapshot(&config.scope, isolation);
            assert_eq!(eraser.query_one("SELECT count(*) FROM research.intelligence_assessments WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,i64>(0),0);
            assert_eq!(eraser.query_one("SELECT count(*) FROM research.intelligence_rejections WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,i64>(0),0);
            analyst
                .batch_execute("BEGIN ISOLATION LEVEL READ COMMITTED")
                .unwrap();
            assert_eq!(sql_assessment(&mut analyst, &record).unwrap(), 1);
            assert_eq!(
                sql_rejection(&mut analyst, &rejection(&config, &observation)).unwrap(),
                1
            );
            analyst.batch_execute("COMMIT").unwrap();
            assert_physical_dependents(&mut admin, &config.scope, 1);
            if operation == "refresh" {
                // Expire only the new children. The parent and scope MVCC row
                // versions stay unchanged throughout the inverse schedule.
                for table in ["intelligence_assessments", "intelligence_rejections"] {
                    admin.execute(&format!("UPDATE research.{table} SET retain_until=clock_timestamp()-interval '1 second' WHERE owner_id=$1 AND account_id=$2"),&[&config.scope.owner_id,&config.scope.account_id]).unwrap();
                }
            }
            let invalidation = Invalidation {
                scope: config.scope.clone(),
                target: InvalidationTarget::Source,
                target_id: observation.evidence[0].source_id.clone(),
                at: Utc::now(),
                reason: InvalidationReason::RightsRevoked,
                erase_content: true,
            };
            for unsupported in [true, false] {
                let result = match operation {
                    "direct_erasure" => eraser.execute("UPDATE research.intelligence_observations SET document=NULL WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]),
                    "source_invalidation" => sql_invalidation(&mut eraser, &invalidation),
                    "refresh" => eraser.query_one("SELECT research.refresh_intelligence($1,$2)",&[&config.scope.owner_id,&config.scope.account_id]).map(|_| 0),
                    _ => unreachable!(),
                };
                if unsupported {
                    assert_requires_read_committed(result.unwrap_err());
                    eraser.batch_execute("ROLLBACK").unwrap();
                    assert_physical_dependents(&mut admin, &config.scope, 1);
                    assert!(admin.query_one("SELECT document IS NOT NULL AND canonical IS NOT NULL FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,bool>(0));
                } else {
                    result.unwrap();
                    assert_physical_dependents(&mut admin, &config.scope, 0);
                    assert_eq!(admin.query_one("SELECT document IS NULL AND canonical IS NULL FROM research.intelligence_observations WHERE owner_id=$1 AND account_id=$2",&[&config.scope.owner_id,&config.scope.account_id]).unwrap().get::<_,bool>(0),operation != "refresh");
                }
            }
        }
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn sql_intelligence_registry_assessment_and_empty_refresh_require_read_committed() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let (config, observation) = fresh_intelligence();
    let mut repository = PgIntelligenceRepository::connect(&dsn("trady_analyst")).unwrap();
    let stored = ingest(&mut repository, &config, &observation, Utc::now()).unwrap();
    let mut record =
        retained_assessment(&mut repository, &config, &observation, stored.ingested_at);
    record.assessment_id = new_id("fixed_snapshot_assessment");
    let (unregistered, _) = fresh_intelligence();
    for isolation in ["REPEATABLE READ", "SERIALIZABLE"] {
        for operation in [
            "registry_insert",
            "registry_activate",
            "assessment",
            "empty_refresh",
        ] {
            let mut snapshot = fixed_snapshot(&config.scope, isolation);
            let result = match operation {
                "registry_insert" => snapshot.execute("INSERT INTO research.intelligence_registries(registry_hash,document) VALUES($1,$2)",&[&unregistered.revision().unwrap(),&serde_json::to_value(&unregistered).unwrap()]),
                "registry_activate" => snapshot.query_one("SELECT research.activate_intelligence_registry($1)",&[&stored.registry_hash]).map(|_| 0),
                "assessment" => sql_assessment(&mut snapshot, &record),
                "empty_refresh" => snapshot.query_one("SELECT research.refresh_intelligence($1,$2)",&[&unregistered.scope.owner_id,&unregistered.scope.account_id]).map(|_| 0),
                _ => unreachable!(),
            };
            assert_requires_read_committed(result.unwrap_err());
            snapshot.batch_execute("ROLLBACK").unwrap();
        }
    }
    repository.register(&unregistered).unwrap();
}

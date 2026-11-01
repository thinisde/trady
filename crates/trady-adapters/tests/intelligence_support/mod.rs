#![allow(dead_code)]
//! Synthetic fixtures shared by adapter, CLI and database acceptance tests.

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use trady_application::{intelligence::*, research_jobs::digest};
use trady_domain::{
    Currency,
    financial::{AccessStatus, Capability, DataRights},
    intelligence::*,
};

pub fn time(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().to_utc()
}
pub fn input(feature: FeatureKind) -> FeatureInput {
    let raw = match feature {
        FeatureKind::Options => {
            include_str!("../../../../schemas/fixtures/intelligence/options.json")
        }
        FeatureKind::Coordination => {
            include_str!("../../../../schemas/fixtures/intelligence/coordination.json")
        }
        FeatureKind::Event => include_str!("../../../../schemas/fixtures/intelligence/event.json"),
        FeatureKind::ShortInterest => {
            include_str!("../../../../schemas/fixtures/intelligence/short_interest.json")
        }
        FeatureKind::Insider => {
            include_str!("../../../../schemas/fixtures/intelligence/insider.json")
        }
        FeatureKind::CrossAsset => {
            include_str!("../../../../schemas/fixtures/intelligence/cross_asset.json")
        }
        FeatureKind::Attention => {
            include_str!("../../../../schemas/fixtures/intelligence/attention.json")
        }
        FeatureKind::Accounting => {
            include_str!("../../../../schemas/fixtures/intelligence/accounting.json")
        }
        FeatureKind::Microstructure => {
            include_str!("../../../../schemas/fixtures/intelligence/microstructure.json")
        }
        FeatureKind::Tax => include_str!("../../../../schemas/fixtures/intelligence/tax.json"),
    };
    serde_json::from_value(
        json!({"feature":feature,"input":serde_json::from_str::<Value>(raw).unwrap()}),
    )
    .unwrap()
}
pub fn fixture_time(feature: FeatureKind) -> DateTime<Utc> {
    time(match feature {
        FeatureKind::Options
        | FeatureKind::ShortInterest
        | FeatureKind::CrossAsset
        | FeatureKind::Microstructure => "2026-10-30T20:00:00Z",
        FeatureKind::Event => "2026-10-30T15:40:00Z",
        _ => "2026-10-31T14:00:00Z",
    })
}
fn refs(value: &Value, sources: &mut BTreeSet<String>, references: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                if key == "source_id"
                    && let Some(s) = v.as_str()
                {
                    sources.insert(s.into());
                }
                if (key == "reference" || key.ends_with("_ref") || key.ends_with("_reference"))
                    && let Some(s) = v.as_str()
                {
                    references.insert(s.into());
                }
                if (key == "references" || key.ends_with("_refs") || key.ends_with("_references"))
                    && let Some(a) = v.as_array()
                {
                    for s in a.iter().filter_map(Value::as_str) {
                        references.insert(s.into());
                    }
                }
                refs(v, sources, references);
            }
        }
        Value::Array(rows) => {
            for row in rows {
                refs(row, sources, references);
            }
        }
        _ => {}
    }
}
pub fn envelope(feature: FeatureKind) -> (IntelligenceConfig, FeatureObservation, DateTime<Utc>) {
    let at = fixture_time(feature);
    envelope_at(input(feature), at)
}
pub fn envelope_at(
    payload: FeatureInput,
    at: DateTime<Utc>,
) -> (IntelligenceConfig, FeatureObservation, DateTime<Utc>) {
    let feature = payload.kind();
    let entity_id = payload.subject().unwrap_or("synthetic_macro").to_owned();
    let mut sources = BTreeSet::new();
    let mut references = BTreeSet::new();
    refs(
        &serde_json::to_value(&payload).unwrap(),
        &mut sources,
        &mut references,
    );
    if sources.is_empty() {
        sources.insert("synthetic_source".into());
    }
    if references.is_empty() {
        references.insert("synthetic_fixture".into());
    }
    let scope = IntelligenceScope {
        owner_id: "synthetic_owner".into(),
        account_id: "paper_synthetic".into(),
    };
    let mut config = IntelligenceConfig::disabled(scope.clone());
    config.enabled.insert(feature);
    config.sources = sources
        .iter()
        .map(|source_id| FeatureSource {
            source_id: source_id.clone(),
            provider: "synthetic_only".into(),
            enabled: true,
            rights_revision: "synthetic_rights_v1".into(),
            mapping_reference: "synthetic_fixture_mapping".into(),
            entity_ids: BTreeSet::from([entity_id.clone()]),
            features: BTreeSet::from([feature]),
            capability: Capability {
                status: AccessStatus::Verified,
                revision: "synthetic_rights_v1".into(),
                evidence_reference: "synthetic_fixture_not_vendor_entitlement".into(),
                reviewed_at: Some(at - Duration::days(365)),
                valid_until: Some(at + Duration::days(365)),
                credit_cost: 1,
            },
            rights: DataRights {
                normalized_storage: true,
                display: true,
                external_llm: false,
                local_inference: true,
                model_training: false,
                export: false,
                retention_hours: 24 * 365,
                terms_reference: "synthetic-fixture-only".into(),
            },
            derived_storage: true,
            audit_markers: true,
        })
        .collect();
    let evidence = sources
        .iter()
        .flat_map(|source_id| {
            references.iter().map(|reference| SourceEvidence {
                source_id: source_id.clone(),
                provider: "synthetic_only".into(),
                rights_revision: "synthetic_rights_v1".into(),
                entity_id: entity_id.clone(),
                currency: Some(Currency::new("USD").unwrap()),
                unit: "normalized_research".into(),
                venue: None,
                adjustment_basis: "as_filed_or_explicit_input_basis".into(),
                observed_at: at - Duration::days(1),
                published_at: at - Duration::seconds(2),
                available_at: at - Duration::seconds(1),
                received_at: at,
                expires_at: at + Duration::days(7),
                source_reference: reference.clone(),
                content_hash: digest(&payload).unwrap(),
                lineage_ids: vec!["same_underlying_fixture_origin".into()],
                complete: true,
                gaps: vec![],
            })
        })
        .collect();
    let observation = FeatureObservation {
        schema_version: 1,
        observation_id: "fixture_observation".into(),
        scope,
        entity_id,
        payload_hash: digest(&payload).unwrap(),
        evidence,
        supersedes: None,
        payload,
    };
    (config, observation, at)
}

mod intelligence_support;

use chrono::{Duration, Utc};
use intelligence_support::*;
use rust_decimal::Decimal;
use serde_json::json;
use std::collections::BTreeSet;
use std::time::{Duration as WallDuration, Instant};
use trady_adapters::intelligence::{MemoryIntelligenceRepository, book::*, tools::*};
use trady_application::{harness::*, intelligence::*, research_jobs::digest};
use trady_domain::{Mode, intelligence::*};

#[test]
fn research_defaults_enable_all_features_without_granting_source_permissions() {
    let scope = IntelligenceScope {
        owner_id: "default_owner".into(),
        account_id: "paper_default".into(),
    };
    let defaults = IntelligenceConfig::research_defaults(scope.clone());
    defaults.validate().unwrap();
    let all: BTreeSet<_> = FeatureKind::ALL.into_iter().collect();
    assert_eq!(defaults.enabled, all);
    assert!(defaults.sources.is_empty());
    assert_eq!(tool_specs(&defaults).len(), 9);
    assert!(
        tool_specs(&defaults)
            .iter()
            .all(|spec| spec.name != FeatureKind::Tax.tool())
    );

    let mut document = json!({"schema_version": 1, "scope": scope});
    let omitted: IntelligenceConfig = serde_json::from_value(document.clone()).unwrap();
    omitted.validate().unwrap();
    assert_eq!(omitted.enabled, all);
    document["enabled"] = json!([]);
    let disabled: IntelligenceConfig = serde_json::from_value(document.clone()).unwrap();
    assert!(disabled.enabled.is_empty());
    assert!(tool_specs(&disabled).is_empty());
    document["enabled"] = json!(["ACCOUNTING"]);
    let selected: IntelligenceConfig = serde_json::from_value(document).unwrap();
    assert_eq!(selected.enabled, BTreeSet::from([FeatureKind::Accounting]));
    assert!(IntelligenceConfig::disabled(scope).enabled.is_empty());

    for feature in FeatureKind::ALL {
        let (_, mut observation, at) = envelope(feature);
        observation.scope = defaults.scope.clone();
        let result = assess_ephemeral(
            &defaults,
            &observation,
            Mode::Paper,
            Audience::Owner,
            at,
            at,
        )
        .unwrap();
        assert_eq!(result.status, AssessmentStatus::Unavailable, "{feature:?}");
        assert!(result.result.is_none(), "{feature:?}");
        assert!(!result.entry_authorized);
        assert_eq!(
            result.reasons,
            ["source_rights_mapping_or_input_unavailable"]
        );
    }
}

#[test]
fn all_ten_features_run_through_the_rights_aware_service_without_order_authority() {
    for feature in FeatureKind::ALL {
        let (mut config, observation, at) = envelope(feature);
        config.enabled = IntelligenceConfig::research_defaults(config.scope.clone()).enabled;
        let record =
            assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at).unwrap();
        assert!(record.result.is_some(), "{feature:?}: {:?}", record.reasons);
        assert_ne!(record.status, AssessmentStatus::Rejected, "{feature:?}");
        assert!(!record.entry_authorized);
        assert_eq!(record.calibration, "RESEARCH_ONLY_UNCALIBRATED");
        assert_eq!(
            record.registry_hash.as_deref(),
            Some(config.revision().unwrap().as_str())
        );
        let mut bad = serde_json::to_value(&observation.payload).unwrap();
        bad["input"]["order_authorized"] = json!(true);
        assert!(
            serde_json::from_value::<FeatureInput>(bad).is_err(),
            "{feature:?}"
        );
    }
}

#[test]
fn explicit_opt_out_and_live_mode_are_denied_for_all_ten_features() {
    for feature in FeatureKind::ALL {
        let (mut config, observation, at) = envelope(feature);
        config.enabled.clear();
        let result =
            assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at).unwrap();
        assert_eq!(result.status, AssessmentStatus::Unavailable);
        assert!(result.result.is_none());
        config.enabled.insert(feature);
        let result =
            assess_ephemeral(&config, &observation, Mode::Live, Audience::Owner, at, at).unwrap();
        assert_eq!(result.reasons, ["research_only_live_mode_denied"]);
        assert!(result.result.is_none());
    }
}

#[test]
fn current_rights_subject_lineage_and_payload_hash_cannot_be_forged() {
    let (mut config, mut o, at) = envelope(FeatureKind::ShortInterest);
    assert!(
        assess_ephemeral(&config, &o, Mode::Paper, Audience::ExternalLlm, at, at)
            .unwrap()
            .result
            .is_none()
    );
    config.sources[0].rights.external_llm = true;
    assert!(
        assess_ephemeral(&config, &o, Mode::Shadow, Audience::ExternalLlm, at, at)
            .unwrap()
            .result
            .is_some()
    );
    config.sources[0].capability.valid_until = Some(at);
    assert!(
        assess_ephemeral(&config, &o, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
    config.sources[0].capability.valid_until = Some(at + Duration::days(1));
    o.payload_hash = "0".repeat(64);
    assert!(
        assess_ephemeral(&config, &o, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
    o.payload_hash = digest(&o.payload).unwrap();
    o.entity_id = "different_issuer".into();
    assert!(
        assess_ephemeral(&config, &o, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
    o.entity_id = o.payload.subject().unwrap().into();
    o.evidence[0].source_reference = "unrelated_source".into();
    assert!(
        assess_ephemeral(&config, &o, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
    o.evidence[0].source_reference = "synthetic_short_interest_v1".into();
    o.evidence[0].available_at = at + Duration::seconds(1);
    assert!(
        assess_ephemeral(&config, &o, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
}

#[test]
fn evidence_must_match_subject_or_an_explicit_feature_relationship() {
    let (mut config, mut observation, at) = envelope(FeatureKind::ShortInterest);
    config.sources[0]
        .entity_ids
        .insert("unrelated_issuer".into());
    for evidence in &mut observation.evidence {
        evidence.entity_id = "unrelated_issuer".into();
    }
    assert!(
        assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
    config.cross_asset_context.insert(
        observation.entity_id.clone(),
        BTreeSet::from(["unrelated_issuer".into()]),
    );
    assert!(
        assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
    config.entity_aliases.insert(
        observation.entity_id.clone(),
        BTreeSet::from(["unrelated_issuer".into()]),
    );
    assert!(
        assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_some()
    );

    let (mut config, mut observation, at) = envelope(FeatureKind::CrossAsset);
    config.sources[0].entity_ids.insert("reviewed_index".into());
    observation.evidence[0].entity_id = "reviewed_index".into();
    assert!(
        assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
    config.cross_asset_context.insert(
        observation.entity_id.clone(),
        BTreeSet::from(["reviewed_index".into()]),
    );
    assert!(
        assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_some()
    );
}

#[test]
fn citations_preserve_source_reference_pairs_and_inherit_series_source() {
    for feature in [FeatureKind::Attention, FeatureKind::Coordination] {
        let (mut config, mut observation, at) = envelope(feature);
        let original_source = config.sources[0].source_id.clone();
        let mut other = config.sources[0].clone();
        other.source_id = "unrelated_source".into();
        config.sources.push(other);
        // Every reference still exists, but under the wrong source.
        for evidence in &mut observation.evidence {
            evidence.source_id = "unrelated_source".into();
        }
        let mut own = observation.evidence[0].clone();
        own.source_id = original_source;
        own.source_reference = "unrelated_reference".into();
        observation.evidence.push(own);
        assert!(
            assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at)
                .unwrap()
                .result
                .is_none(),
            "{feature:?}"
        );
    }
}

#[test]
fn options_event_baseline_is_a_licensed_evidence_dependency() {
    let at = fixture_time(FeatureKind::Options);
    let mut payload = input(FeatureKind::Options);
    let FeatureInput::Options(options) = &mut payload else {
        panic!("fixture")
    };
    options.event = Some(trady_domain::intelligence::options::OptionsEvent {
        event_at: at + Duration::days(3),
        published_at: at,
        available_at: at,
        source_ref: "synthetic_options_event".into(),
        non_event_variance_rate_annualized: Some(0.04),
        baseline_source_ref: Some("unlicensed_baseline".into()),
        baseline_available_at: Some(at),
    });
    let (config, mut observation, _) = envelope_at(payload, at);
    assert!(
        assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_some()
    );
    observation
        .evidence
        .retain(|e| e.source_reference != "unlicensed_baseline");
    assert!(
        assess_ephemeral(&config, &observation, Mode::Paper, Audience::Owner, at, at)
            .unwrap()
            .result
            .is_none()
    );
}

#[test]
fn erasure_upgrade_is_retained_and_cannot_recreate_rejection_hashes() {
    let (config, observation, at) = envelope(FeatureKind::Accounting);
    let mut repository = MemoryIntelligenceRepository::default();
    ingest(&mut repository, &config, &observation, at).unwrap();
    let mut invalidation = Invalidation {
        scope: config.scope.clone(),
        target: InvalidationTarget::Source,
        target_id: config.sources[0].source_id.clone(),
        at,
        reason: InvalidationReason::Correction,
        erase_content: false,
    };
    repository.invalidate(&invalidation).unwrap();
    invalidation.erase_content = true;
    repository.invalidate(&invalidation).unwrap();
    assert!(
        repository
            .invalidations(&config.scope)
            .unwrap()
            .iter()
            .any(|i| i.erase_content)
    );
    assert!(ingest(&mut repository, &config, &observation, at).is_err());
    assert!(repository.rejections.is_empty());
}

#[test]
fn valid_registries_reach_persistence_before_ingest_rejection_and_read_early_returns() {
    let (mut config, mut observation, at) = envelope(FeatureKind::Accounting);
    let mut repository = MemoryIntelligenceRepository::default();
    observation.payload_hash = "0".repeat(64);
    assert!(ingest(&mut repository, &config, &observation, at).is_err());
    assert!(
        repository
            .registries
            .contains_key(&config.revision().unwrap())
    );

    config.enabled.clear();
    let disabled_revision = config.revision().unwrap();
    let response = read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: FeatureKind::Accounting,
            entity: &observation.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at,
            now: at,
        },
    )
    .unwrap();
    assert!(response.result.is_none());
    assert!(repository.registries.contains_key(&disabled_revision));

    config.sources[0].audit_markers = false;
    assert!(
        read(
            &mut repository,
            &config,
            ReadRequest {
                scope: &config.scope,
                feature: FeatureKind::Accounting,
                entity: &observation.entity_id,
                mode: Mode::Paper,
                audience: Audience::Owner,
                at: at + Duration::seconds(1),
                now: at,
            }
        )
        .is_err()
    );
    assert!(
        repository
            .registries
            .contains_key(&config.revision().unwrap())
    );
}

#[test]
fn tax_read_does_not_register_or_persist_its_configuration() {
    let (config, observation, at) = envelope(FeatureKind::Tax);
    let mut repository = MemoryIntelligenceRepository::default();
    let response = read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: FeatureKind::Tax,
            entity: &observation.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at,
            now: at,
        },
    )
    .unwrap();
    assert!(response.result.is_none());
    assert!(repository.registries.is_empty());
    assert!(repository.assessments.is_empty());
    assert!(repository.rejections.is_empty());
}

#[test]
fn rejection_markers_use_trusted_scope_and_follow_audit_rights_retention_and_erasure() {
    let (mut config, mut observation, at) = envelope(FeatureKind::Accounting);
    let mut repository = MemoryIntelligenceRepository::default();
    observation.observation_id = "private freeform identifier".into();
    observation.scope.owner_id = "private freeform owner".into();
    assert!(ingest(&mut repository, &config, &observation, at).is_err());
    assert_eq!(repository.rejections.len(), 1);
    assert_eq!(repository.rejections[0].scope, config.scope);
    assert!(repository.rejections[0].observation_id.is_none());
    assert!(
        !serde_json::to_string(&repository.rejections)
            .unwrap()
            .contains("private freeform")
    );
    repository.purge(at + Duration::days(7)).unwrap();
    assert!(repository.rejections.is_empty());
    config.sources[0].audit_markers = false;
    assert!(ingest(&mut repository, &config, &observation, at).is_err());
    assert!(repository.rejections.is_empty());
    config.sources[0].audit_markers = true;
    ingest(&mut repository, &config, &observation, at).unwrap_err();
    assert_eq!(repository.rejections.len(), 1);
    repository
        .invalidate(&Invalidation {
            scope: config.scope.clone(),
            target: InvalidationTarget::Source,
            target_id: config.sources[0].source_id.clone(),
            at,
            reason: InvalidationReason::RightsRevoked,
            erase_content: true,
        })
        .unwrap();
    assert!(repository.rejections.is_empty());
    ingest(&mut repository, &config, &observation, at).unwrap_err();
    assert!(repository.rejections.is_empty());
}

#[test]
fn collision_and_correction_failures_keep_only_licensed_rejection_markers() {
    let (mut config, observation, at) = envelope(FeatureKind::Accounting);
    let mut repository = MemoryIntelligenceRepository::default();
    let stored = ingest(&mut repository, &config, &observation, at).unwrap();
    let mut collision = observation.clone();
    collision.evidence[0].complete = false;
    assert!(ingest(&mut repository, &config, &collision, at).is_err());

    let mut correction = observation.clone();
    correction.observation_id = "missing_predecessor_correction".into();
    correction.supersedes = Some("missing_predecessor".into());
    assert!(ingest(&mut repository, &config, &correction, at).is_err());
    assert_eq!(repository.rejections.len(), 2);
    for (rejected, input) in repository.rejections.iter().zip([&collision, &correction]) {
        assert_eq!(rejected.input_hash, digest(input).unwrap());
        assert_eq!(rejected.scope, config.scope);
        assert_eq!(rejected.code, "ingestion_rejected");
        assert_eq!(rejected.retain_until, at + Duration::days(7));
    }
    let retained = repository
        .find(&config.scope, &observation.observation_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        digest(&retained.observation).unwrap(),
        digest(&stored.observation).unwrap()
    );
    assert!(repository.invalidations.is_empty());

    repository
        .invalidate(&Invalidation {
            scope: config.scope.clone(),
            target: InvalidationTarget::Lineage,
            target_id: observation.evidence[0].lineage_ids[0].clone(),
            at,
            reason: InvalidationReason::Deleted,
            erase_content: true,
        })
        .unwrap();
    assert!(repository.rejections.is_empty());
    assert!(ingest(&mut repository, &config, &correction, at).is_err());
    assert!(repository.rejections.is_empty());
    config.sources[0].audit_markers = false;
    assert!(ingest(&mut repository, &config, &collision, at).is_err());
    assert!(repository.rejections.is_empty());
}

#[test]
fn identical_correction_retry_succeeds_after_predecessor_expiry() {
    let (mut config, original, at) = envelope(FeatureKind::Accounting);
    for source in &mut config.sources {
        source.rights.retention_hours = 1;
    }
    let mut corrected = original.clone();
    corrected.observation_id = "correction_retry".into();
    corrected.supersedes = Some(original.observation_id.clone());
    for evidence in &mut corrected.evidence {
        evidence.received_at = at + Duration::minutes(30);
    }
    let mut repository = MemoryIntelligenceRepository::default();
    ingest(&mut repository, &config, &original, at).unwrap();
    let saved = ingest(
        &mut repository,
        &config,
        &corrected,
        at + Duration::minutes(30),
    )
    .unwrap();
    repository.purge(at + Duration::minutes(70)).unwrap();
    assert!(
        repository
            .find(&config.scope, &original.observation_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        ingest(
            &mut repository,
            &config,
            &corrected,
            at + Duration::minutes(70)
        )
        .unwrap()
        .ingested_at,
        saved.ingested_at
    );
}

#[test]
fn ingest_is_idempotent_and_corrections_preserve_historical_vintages() {
    let (config, o, at) = envelope(FeatureKind::ShortInterest);
    let mut repository = MemoryIntelligenceRepository::default();
    let first = ingest(&mut repository, &config, &o, at).unwrap();
    assert_eq!(
        ingest(&mut repository, &config, &o, at + Duration::seconds(1))
            .unwrap()
            .ingested_at,
        first.ingested_at
    );
    let mut collision = o.clone();
    collision.evidence[0].complete = false;
    assert!(
        ingest(
            &mut repository,
            &config,
            &collision,
            at + Duration::seconds(2)
        )
        .is_err()
    );
    let mut revised = o.clone();
    revised.observation_id = "corrected_observation".into();
    revised.supersedes = Some(o.observation_id.clone());
    ingest(
        &mut repository,
        &config,
        &revised,
        at + Duration::seconds(10),
    )
    .unwrap();
    let original = read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: o.payload.kind(),
            entity: &o.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at: at + Duration::seconds(5),
            now: at + Duration::seconds(20),
        },
    )
    .unwrap();
    assert_eq!(
        original.observation_id.as_deref(),
        Some(o.observation_id.as_str())
    );
    let current = read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: o.payload.kind(),
            entity: &o.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at: at + Duration::seconds(11),
            now: at + Duration::seconds(20),
        },
    )
    .unwrap();
    assert_eq!(
        current.observation_id.as_deref(),
        Some(revised.observation_id.as_str())
    );
    assert_eq!(repository.registries.len(), 1);
    assert_eq!(repository.assessments.len(), 2);
    let before_ingest = read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: o.payload.kind(),
            entity: &o.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at: at - Duration::seconds(1),
            now: at + Duration::seconds(20),
        },
    )
    .unwrap();
    assert!(before_ingest.result.is_none());
}

#[test]
fn rights_erasure_removes_dependent_payloads_and_assessments_even_from_replay() {
    let (config, o, at) = envelope(FeatureKind::ShortInterest);
    let mut repository = MemoryIntelligenceRepository::default();
    ingest(&mut repository, &config, &o, at).unwrap();
    read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: o.payload.kind(),
            entity: &o.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at,
            now: at,
        },
    )
    .unwrap();
    assert_eq!(repository.assessments.len(), 1);
    repository
        .invalidate(&Invalidation {
            scope: config.scope.clone(),
            target: InvalidationTarget::Lineage,
            target_id: o.evidence[0].lineage_ids[0].clone(),
            at: at + Duration::seconds(2),
            reason: InvalidationReason::Deleted,
            erase_content: true,
        })
        .unwrap();
    assert!(
        repository
            .find(&config.scope, &o.observation_id)
            .unwrap()
            .is_none()
    );
    assert!(repository.rejections.is_empty());
    assert!(repository.assessments.is_empty());
    let replay = read(
        &mut repository,
        &config,
        ReadRequest {
            scope: &config.scope,
            feature: o.payload.kind(),
            entity: &o.entity_id,
            mode: Mode::Paper,
            audience: Audience::Owner,
            at,
            now: at + Duration::seconds(3),
        },
    )
    .unwrap();
    assert_eq!(replay.status, AssessmentStatus::Invalidated);
    let mut reimport = o;
    reimport.observation_id = "new_id_same_deleted_origin".into();
    assert!(
        ingest(
            &mut repository,
            &config,
            &reimport,
            at + Duration::seconds(3)
        )
        .is_err()
    );
}

#[test]
fn tax_context_is_ephemeral_owner_only_and_never_a_general_model_tool() {
    let (config, o, at) = envelope(FeatureKind::Tax);
    assert!(tool_specs(&config).is_empty());
    let mut repository = MemoryIntelligenceRepository::default();
    assert!(ingest(&mut repository, &config, &o, at).is_err());
    assert!(
        repository
            .find(&config.scope, &o.observation_id)
            .unwrap()
            .is_none()
    );
    let mut malformed = o.clone();
    malformed.observation_id = "private cost basis in invalid id".into();
    malformed.scope.owner_id = "private context".into();
    assert!(ingest(&mut repository, &config, &malformed, at).is_err());
    assert!(repository.rejections.is_empty());
    for audience in [
        Audience::ExternalLlm,
        Audience::LocalInference,
        Audience::ModelTraining,
        Audience::Export,
    ] {
        let result = assess_ephemeral(&config, &o, Mode::Paper, audience, at, at).unwrap();
        assert_eq!(result.reasons, ["tax_owner_only"]);
        assert!(result.result.is_none());
        assert!(result.evidence.is_empty());
    }
    let result = assess_ephemeral(&config, &o, Mode::Paper, Audience::Owner, at, at).unwrap();
    assert!(result.result.is_some());
    assert!(result.evidence.is_empty());
    let output = serde_json::to_string(&result).unwrap();
    for private in [
        "cost_basis",
        "tax_identifier",
        "position_lots",
        "annual_state",
        "broker_entity",
    ] {
        assert!(!output.contains(private), "{private}");
    }
}

fn update(sequence: u64, at: chrono::DateTime<Utc>, change: BookChange) -> BookUpdate {
    BookUpdate {
        instrument_id: "stock".into(),
        venue: "XNAS".into(),
        sequence,
        exchange_at: at,
        received_at: at,
        change,
    }
}
fn snapshot_change() -> BookChange {
    BookChange::Snapshot {
        bids: vec![BookLevel {
            price: 100.into(),
            size: 800.into(),
        }],
        asks: vec![BookLevel {
            price: Decimal::new(10001, 2),
            size: 450.into(),
        }],
    }
}
#[test]
fn bounded_book_requires_snapshot_recovery_after_gap_and_projects_typed_research() {
    let at = time("2026-10-30T20:00:00Z");
    let mut builder = SequenceBookBuilder::new(
        FeedMode::SequencedUpdates,
        1,
        2,
        2,
        1000,
        10,
        Decimal::new(1, 2),
    )
    .unwrap();
    builder.apply(update(1, at, snapshot_change()), at).unwrap();
    let initial_epoch = builder
        .research_book("stock", "XNAS", at, "feed_epoch", "licensed_source")
        .unwrap()
        .epoch;
    let delta = BookChange::Delta {
        side: BookSide::Bid,
        price: 100.into(),
        size: 900.into(),
    };
    assert!(builder.apply(update(3, at, delta.clone()), at).is_err());
    assert!(builder.snapshot("stock", "XNAS", at).is_err());
    assert!(builder.apply(update(2, at, snapshot_change()), at).is_err());
    builder.apply(update(4, at, snapshot_change()), at).unwrap();
    builder.apply(update(5, at, delta), at).unwrap();
    assert_eq!(builder.history_len("stock", "XNAS"), 2);
    let projected = builder
        .research_book("stock", "XNAS", at, "feed_epoch", "licensed_source")
        .unwrap();
    assert_eq!(projected.sequence, 5);
    assert_ne!(initial_epoch, projected.epoch);
    assert_eq!(projected.bids[0].size_shares, Decimal::from(900));
    assert!(
        builder
            .snapshot("stock", "XNAS", at + Duration::milliseconds(1001))
            .is_err()
    );
    let mut other = update(1, at, snapshot_change());
    other.instrument_id = "other_stock".into();
    assert!(builder.apply(other, at).is_err());
}

#[test]
fn bounded_depth_resyncs_instead_of_forgetting_a_level_needed_by_later_deletions() {
    let at = fixture_time(FeatureKind::Microstructure);
    let mut builder =
        SequenceBookBuilder::new(FeedMode::SequencedUpdates, 1, 2, 4, 1000, 10, 1.into()).unwrap();
    builder
        .apply(
            update(
                1,
                at,
                BookChange::Snapshot {
                    bids: vec![
                        BookLevel {
                            price: 100.into(),
                            size: 1.into(),
                        },
                        BookLevel {
                            price: 99.into(),
                            size: 1.into(),
                        },
                    ],
                    asks: vec![BookLevel {
                        price: 102.into(),
                        size: 1.into(),
                    }],
                },
            ),
            at,
        )
        .unwrap();
    for (sequence, price, size) in [(2, 101, 1), (3, 101, 0), (4, 98, 1), (5, 100, 0)] {
        assert!(
            builder
                .apply(
                    update(
                        sequence,
                        at,
                        BookChange::Delta {
                            side: BookSide::Bid,
                            price: price.into(),
                            size: size.into()
                        }
                    ),
                    at
                )
                .is_err()
        );
        assert!(builder.snapshot("stock", "XNAS", at).is_err());
    }
    builder
        .apply(
            update(
                6,
                at,
                BookChange::Snapshot {
                    bids: vec![
                        BookLevel {
                            price: 99.into(),
                            size: 1.into(),
                        },
                        BookLevel {
                            price: 98.into(),
                            size: 1.into(),
                        },
                    ],
                    asks: vec![BookLevel {
                        price: 102.into(),
                        size: 1.into(),
                    }],
                },
            ),
            at,
        )
        .unwrap();
    assert_eq!(
        builder.snapshot("stock", "XNAS", at).unwrap().bids[0].price,
        Decimal::from(99)
    );
}

#[derive(Default)]
struct EmptyTools;
impl ToolExecutor for EmptyTools {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![]
    }
    fn revision(&self) -> String {
        "empty".into()
    }
    fn execute(
        &mut self,
        _: &str,
        _: &serde_json::Value,
        _: &Session,
        _: Instant,
        _: &CancellationToken,
    ) -> trady_domain::Result<ToolOutput> {
        Ok(ToolOutput::error("DENIED", "tool_not_registered"))
    }
}
#[test]
fn model_tools_are_scoped_and_permission_checked_and_disabled_is_compatible() {
    let (mut config, mut o, _) = envelope_at(input(FeatureKind::Accounting), Utc::now());
    config.sources[0].rights.external_llm = true;
    let mut repository = MemoryIntelligenceRepository::default();
    ingest(&mut repository, &config, &o, Utc::now()).unwrap();
    let scope = Scope {
        owner_id: config.scope.owner_id.clone(),
        account_id: config.scope.account_id.clone(),
        policy_version: 1,
    };
    let session = Session::new(
        scope,
        ProviderIdentity {
            provider: "test".into(),
            model: "test".into(),
            endpoint: "local".into(),
        },
        RunLimits::default(),
        "governing".into(),
        "tools".into(),
        vec![],
    )
    .unwrap();
    let mut tools = IntelligenceTools {
        base: EmptyTools,
        repository: Some(repository),
        config: Some(config.clone()),
        mode: Mode::Paper,
    };
    assert_eq!(tools.specs().len(), 1);
    let deadline = Instant::now() + WallDuration::from_secs(2);
    let output = tools
        .execute(
            FeatureKind::Accounting.tool(),
            &json!({"entity_id":o.entity_id}),
            &session,
            deadline,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(output.data.is_some());
    assert_eq!(output.data.unwrap()["entry_authorized"], false);
    let denied = tools
        .execute(
            FeatureKind::Accounting.tool(),
            &json!({"entity_id":o.entity_id,"account_id":"other"}),
            &session,
            deadline,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(denied.status, "DENIED");
    o.scope.owner_id = "different_owner".into();
    tools.config.as_mut().unwrap().scope = o.scope;
    assert_eq!(
        tools
            .execute(
                FeatureKind::Accounting.tool(),
                &json!({"entity_id":o.entity_id}),
                &session,
                deadline,
                &CancellationToken::default()
            )
            .unwrap()
            .status,
        "DENIED"
    );
    let disabled: IntelligenceTools<_, MemoryIntelligenceRepository> = IntelligenceTools {
        base: EmptyTools,
        repository: None,
        config: None,
        mode: Mode::Paper,
    };
    assert!(disabled.specs().is_empty());
    assert_eq!(disabled.revision(), "empty");
}

struct NoDurableSessions;
impl SessionRepository for NoDurableSessions {
    fn create(&mut self, _: &Session) -> trady_domain::Result<()> {
        panic!("no durable writes allowed")
    }
    fn load(&mut self, _: &Scope, _: &str) -> trady_domain::Result<Session> {
        panic!("no durable reads allowed")
    }
    fn list(&mut self, _: &Scope) -> trady_domain::Result<Vec<Session>> {
        panic!("no durable reads allowed")
    }
    fn acquire(
        &mut self,
        _: &Scope,
        _: &str,
        _: &str,
        _: u32,
    ) -> trady_domain::Result<SessionLease> {
        panic!("no durable acquire allowed")
    }
    fn save(&mut self, _: &mut SessionLease) -> trady_domain::Result<()> {
        panic!("no durable writes allowed")
    }
    fn release(&mut self, _: &SessionLease) -> trady_domain::Result<()> {
        panic!("no durable release allowed")
    }
}
struct NoModelRequests;
impl LlmProvider for NoModelRequests {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider: "fixture".into(),
            model: "fixture".into(),
            endpoint: "local".into(),
        }
    }
    fn complete(
        &mut self,
        _: &ModelRequest,
        _: &CancellationToken,
        _: &mut dyn FnMut(&str),
    ) -> std::result::Result<Completion, ProviderError> {
        panic!("expired or durable context must never reach a provider")
    }
}
#[test]
fn intelligence_chat_is_ephemeral_and_rechecks_context_before_every_model_request() {
    let at = Utc::now();
    let (mut config, observation, _) = envelope_at(input(FeatureKind::Accounting), at);
    config.sources[0].rights.external_llm = true;
    config.sources[0].derived_storage = false;
    let mut repository = MemoryIntelligenceRepository::default();
    ingest(&mut repository, &config, &observation, at).unwrap();
    let mut tools = IntelligenceTools {
        base: EmptyTools,
        repository: Some(repository),
        config: Some(config.clone()),
        mode: Mode::Paper,
    };
    assert!(tools.requires_ephemeral_session());
    let scope = Scope {
        owner_id: config.scope.owner_id.clone(),
        account_id: config.scope.account_id.clone(),
        policy_version: 1,
    };
    let mut session = Session::new(
        scope.clone(),
        NoModelRequests.identity(),
        RunLimits::default(),
        "paper research".into(),
        tools.revision(),
        vec![],
    )
    .unwrap();
    let harness = AgentHarness::new().unwrap();
    let error = harness
        .respond(
            &mut NoDurableSessions,
            &mut NoModelRequests,
            &mut tools,
            RunInput {
                scope: &scope,
                session_id: &session.id,
                input: Some("research"),
                cancel: &CancellationToken::default(),
            },
            &mut |_| {},
        )
        .unwrap_err();
    assert!(error.0.contains("ephemeral"));
    let output = tools
        .execute(
            FeatureKind::Accounting.tool(),
            &json!({"entity_id":observation.entity_id}),
            &session,
            Instant::now() + WallDuration::from_secs(2),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(tools.repository.as_ref().unwrap().assessments.is_empty());
    let mut data = output.data.unwrap();
    assert!(data["result"]["result"].is_object());
    session.runs.push(RunRecord {
        id: "fixture_run".into(),
        status: RunStatus::Completed,
        started_at: at,
        deadline_at: at + Duration::minutes(1),
        model_turns: 1,
        tool_rounds: 1,
        tool_calls: 1,
        tool_result_bytes: 0,
        retry_used: false,
        calls: vec![ToolRecord {
            call_id: "fixture_call".into(),
            tool: FeatureKind::Accounting.tool().into(),
            version: FEATURE_VERSION.into(),
            arguments: json!({"entity_id":observation.entity_id}),
            arguments_hash: content_hash("fixture"),
            started: true,
            result: Some(json!({"data":data})),
        }],
        pending_calls: vec![],
        pending_native: false,
        attempts: vec![],
        manifests: vec![],
        events: vec![],
        final_message: None,
    });
    tools.validate_context(&session).unwrap();
    tools.config.as_mut().unwrap().sources[0]
        .rights
        .external_llm = false;
    assert!(tools.validate_context(&session).is_err());
    tools.config = Some(config.clone());
    data["result"]["expires_at"] = json!(at - Duration::seconds(1));
    session.runs[0].calls[0].result = Some(json!({"data":data}));
    let mut sessions = trady_adapters::sessions::MemorySessionRepository::default();
    assert!(sessions.is_ephemeral());
    sessions.create(&session).unwrap();
    let final_message = harness
        .respond(
            &mut sessions,
            &mut NoModelRequests,
            &mut tools,
            RunInput {
                scope: &scope,
                session_id: &session.id,
                input: Some("continue"),
                cancel: &CancellationToken::default(),
            },
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(final_message["reply"]["status"], "INCOMPLETE");
    session.runs[0].calls[0].result.as_mut().unwrap()["data"]["result"]["expires_at"] =
        json!(at + Duration::days(1));
    tools
        .repository
        .as_mut()
        .unwrap()
        .invalidate(&Invalidation {
            scope: config.scope,
            target: InvalidationTarget::Source,
            target_id: config.sources[0].source_id.clone(),
            at: Utc::now(),
            reason: InvalidationReason::Deleted,
            erase_content: true,
        })
        .unwrap();
    assert!(tools.validate_context(&session).is_err());
}

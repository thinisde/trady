//! Offline fixtures only; probabilities here are test data, never production results.
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use trady_application::{
    harness::{Scope, content_hash},
    research_jobs::*,
    sentiment::SentimentProvider,
    telegram::{IncomingMessage, IncomingUpdate, OwnerAccess, gateway::*},
};
use trady_domain::{Mode, Result, research::*, sentiment::*};

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .unwrap()
        .to_utc()
}
fn policy() -> JobPolicy {
    JobPolicy {
        scope: Scope {
            owner_id: "owner_fixture".into(),
            account_id: "paper_fixture_jobs".into(),
            policy_version: 3,
        },
        mode: Mode::Paper,
        telegram_user_id: 12345,
        bot_id: 98765,
        source_ids: BTreeSet::from(["source_a".into(), "source_b".into(), "source_c".into()]),
        instrument_ids: BTreeSet::from(["stock_fixture".into()]),
        registry_hash: content_hash("reviewed-registry-fixture"),
        sentiment_revision: Some(FINBERT_REVISION.into()),
        daily_job_limit: 10,
        pending_job_limit: 5,
        enabled: true,
    }
}
fn item(letter: &str) -> SourceItem {
    let at = now();
    let text = format!("Offline {letter} source fixture about the issuer");
    SourceItem {
        item_id: content_hash(&format!("item-{letter}")),
        source_id: format!("source_{letter}"),
        external_id: letter.into(),
        canonical_url: format!("https://{letter}.example.test/item"),
        lineage_id: content_hash(&format!("origin-{letter}")),
        content_hash: content_hash(&text),
        parser_version: "fixture-1".into(),
        publisher: letter.into(),
        feed_title: None,
        title: Some(text.clone()),
        summary: None,
        text: Some(text),
        published_at: Some(at - Duration::minutes(10)),
        event_at: Some(at - Duration::minutes(10)),
        first_seen_at: at - Duration::minutes(5),
        fetched_at: at - Duration::minutes(5),
        expires_at: at + Duration::hours(2),
        language: Some("en".into()),
        rights: SourceRights {
            store_content: true,
            llm_processing: true,
            retention_hours: 24,
            terms_reference: "scoped offline test license".into(),
        },
        content_status: ContentStatus::Available,
        social: None,
        related_urls: vec![],
        gaps: vec![],
    }
}
fn packet(items: &[SourceItem]) -> VerifiedResearchPacket {
    let pol = policy();
    VerifiedResearchPacket {
        id: "packet_fixture".into(),
        scope: pol.scope.clone(),
        instrument_id: "stock_fixture".into(),
        profile_revision_id: Some("profile_fixture_1".into()),
        registry_hash: pol.registry_hash.clone(),
        sources: items
            .iter()
            .map(|item| SourceDependency {
                item_id: item.item_id.clone(),
                source_id: item.source_id.clone(),
                content_hash: item.content_hash.clone(),
                lineage_id: item.lineage_id.clone(),
                registry_hash: pol.registry_hash.clone(),
                source_registry_hash: content_hash(&item.source_id),
                source_rights: item.rights.clone(),
                item_rights: item.rights.clone(),
                rights_hash: digest(&item.rights).unwrap(),
                expires_at: item.expires_at,
                verification: Verification {
                    item_id: item.item_id.clone(),
                    instrument_id: "stock_fixture".into(),
                    verification_reference: "test-verifier-fixture-1".into(),
                    issuer_mapping_verified: true,
                    primary_verified: item.source_id == "source_a",
                    reporting_origin: Some(item.source_id.clone()),
                    origin_verified: true,
                    ambiguities: vec![],
                },
            })
            .collect(),
        claims: vec![ClaimReview {
            claim_id: "claim_fixture_1".into(),
            supporting_item_ids: items.iter().map(|i| i.item_id.clone()).collect(),
            contradicting_item_ids: vec![],
            ambiguities: vec![],
        }],
        created_at: now(),
        expires_at: now() + Duration::hours(1),
    }
}
struct FixtureProvider;
impl SentimentProvider for FixtureProvider {
    fn model_revision(&self) -> &str {
        FINBERT_REVISION
    }
    fn predict(&self, documents: &[SentimentDocument]) -> Result<SentimentBatch> {
        Ok(SentimentBatch {
            schema_version: 1,
            model_id: FINBERT_MODEL.into(),
            model_revision: FINBERT_REVISION.into(),
            predictions: documents
                .iter()
                .map(|d| SentimentPrediction {
                    item_id: d.item_id.clone(),
                    text_sha256: d.text_sha256.clone(),
                    probabilities: SentimentProbabilities {
                        positive: "0.7".parse().unwrap(),
                        negative: "0.1".parse().unwrap(),
                        neutral: "0.2".parse().unwrap(),
                    },
                    truncated: false,
                })
                .collect(),
        })
    }
}

#[test]
fn research_namespaces_are_explicit_and_mode_bound_without_execution_authority() {
    let mut p = policy();
    for (mode, prefix) in [
        (Mode::Paper, "paper_"),
        (Mode::Shadow, "shadow_"),
        (Mode::Live, "live_"),
    ] {
        p.mode = mode;
        p.scope.account_id = format!("{prefix}fixture");
        assert!(p.validate().is_ok());
        for other in ["paper_", "shadow_", "live_"]
            .into_iter()
            .filter(|s| *s != prefix)
        {
            p.scope.account_id = format!("{other}fixture");
            assert!(p.validate().is_err());
        }
    }
    p.scope.account_id = "unscoped_account".into();
    assert!(p.scope.validate().is_err());
    p.scope.account_id = "live_".into();
    assert!(p.scope.validate().is_err());
}

#[test]
fn checked_assessments_bind_revision_content_rights_scope_and_independent_claims() {
    let items = vec![item("a"), item("b")];
    let p = packet(&items);
    let policy = policy();
    let assessment =
        trady_application::sentiment::assess_sentiment(&FixtureProvider, &items, now()).unwrap();
    let reference =
        readiness_reference("sentiment_fixture", &assessment, &p, &items, &policy, now()).unwrap();
    assert_eq!(reference.packet_hash, digest(&p).unwrap());
    assert_eq!(reference.assessment_hash, digest(&assessment).unwrap());
    assert_eq!(reference.scored_at, assessment.scored_at);
    assert_eq!(
        reference.maximum_negative_probability,
        rust_decimal::Decimal::new(1, 1)
    );
    assert_eq!(reference.expires_at, now() + Duration::minutes(15));
    assert!(
        readiness_reference(
            "sentiment_fixture",
            &assessment,
            &p,
            &items,
            &policy,
            now() + Duration::minutes(15)
        )
        .is_err()
    );
    let mut wrong = policy.clone();
    wrong.scope.account_id = "live_fixture_jobs".into();
    wrong.mode = Mode::Live;
    assert!(
        readiness_reference("sentiment_fixture", &assessment, &p, &items, &wrong, now()).is_err()
    );
    let mut wrong = policy.clone();
    wrong.sentiment_revision = Some("f".repeat(40));
    assert!(
        readiness_reference("sentiment_fixture", &assessment, &p, &items, &wrong, now()).is_err()
    );
    let mut changed = items.clone();
    changed[0].content_hash = content_hash("corrected source");
    assert!(check_dependencies(&p, &changed, &policy, now()).is_err());
    let mut revoked = items.clone();
    revoked[0].rights.llm_processing = false;
    assert!(check_dependencies(&p, &revoked, &policy, now()).is_err());
    let mut wrong = p.clone();
    wrong.sources[0].verification.issuer_mapping_verified = false;
    assert!(check_dependencies(&wrong, &items, &policy, now()).is_err());
    assert!(
        readiness_reference(
            "sentiment_fixture",
            &assessment,
            &p,
            &items,
            &policy,
            now() + Duration::hours(2)
        )
        .is_err()
    );
    let mut duplicate = assessment;
    duplicate.evidence[1] = duplicate.evidence[0].clone();
    assert!(
        readiness_reference("sentiment_fixture", &duplicate, &p, &items, &policy, now()).is_err()
    );
}

#[test]
fn provenance_clusters_are_transitive_and_unknown_origins_or_reposts_never_confirm_claims() {
    let mut items = vec![item("a"), item("b"), item("c")];
    let b = items[1].canonical_url.clone();
    let c = items[2].canonical_url.clone();
    items[0].related_urls.push(b);
    items[1].related_urls.push(c);
    let p = packet(&items);
    assert_eq!(syndication_clusters(&items, &p.sources).len(), 1);
    let items = vec![item("a"), item("b")];
    let mut p = packet(&items);
    let assessment =
        trady_application::sentiment::assess_sentiment(&FixtureProvider, &items, now()).unwrap();
    p.sources[1].verification.origin_verified = false;
    p.sources[1].verification.reporting_origin = None;
    assert!(check_dependencies(&p, &items, &policy(), now()).is_ok());
    assert!(
        readiness_reference(
            "sentiment_fixture",
            &assessment,
            &p,
            &items,
            &policy(),
            now()
        )
        .is_err()
    );
    p.sources[1].verification.origin_verified = true;
    p.sources[1].verification.reporting_origin = Some("source_b".into());
    p.claims[0]
        .ambiguities
        .push("Repost attribution unresolved".into());
    assert!(
        readiness_reference(
            "sentiment_fixture",
            &assessment,
            &p,
            &items,
            &policy(),
            now()
        )
        .is_err()
    );
}

#[test]
fn control_context_hash_tracks_every_financial_binding_but_not_refresh_time() {
    let mut c = OwnerControlContext {
        scope: policy().scope,
        mode: Mode::Paper,
        plan_revision: Some(content_hash("plan")),
        holding_revision: Some(content_hash("holding")),
        holding_id: Some("position_fixture".into()),
        instrument_id: Some("stock_fixture".into()),
        quantity: Some("1.25".into()),
        observed_at: now(),
    };
    c.validate(now()).unwrap();
    let hash = c.binding_hash().unwrap();
    c.observed_at = now() + Duration::seconds(1);
    assert_eq!(hash, c.binding_hash().unwrap());
    c.quantity = Some("1.26".into());
    assert_ne!(hash, c.binding_hash().unwrap());
    c.observed_at = now() - Duration::seconds(61);
    assert!(c.validate(now()).is_err());
    c.observed_at = now();
    c.quantity = None;
    assert!(c.validate(now()).is_err());
    c.plan_revision = None;
    c.holding_revision = None;
    c.holding_id = None;
    c.instrument_id = None;
    c.quantity = None;
    assert!(c.validate(now()).is_ok());
}

#[test]
fn owner_authentication_precedes_command_parsing_and_cannot_switch_modes() {
    let access = OwnerAccess {
        allowed_user_ids: vec![12345],
        allowed_chat_ids: vec![12345],
    };
    let mut update = IncomingUpdate {
        update_id: 4,
        message: Some(IncomingMessage {
            sender_id: 12345,
            chat_id: 12345,
            private_chat: true,
            sender_is_bot: false,
            sent_at: now(),
            text: "/close_position".into(),
        }),
    };
    assert!(matches!(
        authenticate(
            &access,
            &update,
            98765,
            "fixture_bot",
            &policy().scope,
            now()
        )
        .unwrap()
        .unwrap()
        .command,
        GatewayCommand::ClosePosition
    ));
    update.message.as_mut().unwrap().sender_id = 999;
    assert!(
        authenticate(
            &access,
            &update,
            98765,
            "fixture_bot",
            &policy().scope,
            now()
        )
        .unwrap()
        .is_none()
    );
    assert!(matches!(
        GatewayCommand::parse("/mode live", "fixture_bot"),
        Some(GatewayCommand::Unsupported(_))
    ));
    assert!(GatewayCommand::parse("/pause@another_bot", "fixture_bot").is_none());
    assert!(matches!(
        GatewayCommand::parse("sell it", "fixture_bot"),
        Some(GatewayCommand::Ask(_))
    ));
    assert!(matches!(
        GatewayCommand::parse("/pause_entries extra", "fixture_bot"),
        Some(GatewayCommand::Unsupported(_))
    ));
}

#[test]
fn work_bounds_reject_url_authority_foreign_instruments_and_unapproved_models() {
    let p = policy();
    assert!(
        Work::SourceLookup {
            source_id: "source_a".into(),
            external_id: "https://evil.test".into()
        }
        .validate(&p)
        .is_err()
    );
    assert!(
        Work::Research {
            instrument_id: Some("unapproved".into())
        }
        .validate(&p)
        .is_err()
    );
    assert!(
        Work::Sentiment {
            packet_id: "packet_fixture".into(),
            model_revision: "a".repeat(40)
        }
        .validate(&p)
        .is_err()
    );
    let holding = HoldingWatch {
        holding_id: "holding_fixture".into(),
        instrument_id: "stock_fixture".into(),
        revision_hash: content_hash("revision"),
        active: true,
    };
    let key = watchdog_key(&p.scope, &holding, now(), 300).unwrap();
    assert_eq!(
        key,
        watchdog_key(&p.scope, &holding, now() + Duration::seconds(10), 300).unwrap()
    );
    assert_ne!(
        key,
        watchdog_key(&p.scope, &holding, now() + Duration::seconds(300), 300).unwrap()
    );
    assert!(
        serde_json::from_value::<Work>(
            json!({"kind":"RESEARCH","instrument_id":null,"can_trade":true})
        )
        .is_err()
    );
}

struct FixtureRepository {
    packet: VerifiedResearchPacket,
    items: Vec<SourceItem>,
    stored: Option<Value>,
}
impl ResearchJobRepository for FixtureRepository {
    fn policy(&mut self, _: &Scope) -> Result<JobPolicy> {
        Ok(policy())
    }
    fn enqueue(&mut self, _: &Scope, _: &str, _: &Work) -> Result<ResearchJob> {
        unreachable!("not used by sentiment fixture")
    }
    fn claim(&mut self, _: &Scope, _: &str, _: u32) -> Result<Option<JobLease>> {
        unreachable!("not used by sentiment fixture")
    }
    fn complete(&mut self, _: &JobLease, _: &Value) -> Result<()> {
        unreachable!("not used by sentiment fixture")
    }
    fn fail(&mut self, _: &JobLease, _: &str) -> Result<()> {
        unreachable!("not used by sentiment fixture")
    }
    fn packet(
        &mut self,
        _: &Scope,
        _: &str,
        _: DateTime<Utc>,
    ) -> Result<(VerifiedResearchPacket, Vec<SourceItem>)> {
        Ok((self.packet.clone(), self.items.clone()))
    }
    fn store_sentiment(
        &mut self,
        _: &JobLease,
        _: &VerifiedResearchPacket,
        a: &SentimentAssessment,
        _: DateTime<Utc>,
    ) -> Result<Value> {
        let value = serde_json::to_value(a).unwrap();
        self.stored = Some(value.clone());
        Ok(value)
    }
}
#[test]
fn sentiment_job_uses_provider_outputs_and_scores_one_representative_per_syndication_cluster() {
    let mut items = vec![item("a"), item("b"), item("c")];
    items[2].lineage_id = items[1].lineage_id.clone();
    let packet = packet(&items);
    let mut repo = FixtureRepository {
        packet: packet.clone(),
        items,
        stored: None,
    };
    let job = ResearchJob {
        id: "job_fixture".into(),
        scope: policy().scope,
        work: Work::Sentiment {
            packet_id: packet.id,
            model_revision: FINBERT_REVISION.into(),
        },
        policy_hash: digest(&policy()).unwrap(),
        status: JobStatus::Running,
        created_at: now(),
        attempts: 1,
        generation: 1,
        result: None,
        error_code: None,
    };
    score_sentiment_job(
        &mut repo,
        &FixtureProvider,
        &JobLease {
            job,
            worker_id: "worker_fixture".into(),
            generation: 1,
        },
        now(),
    )
    .unwrap();
    let stored = repo.stored.unwrap();
    assert_eq!(stored["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(stored["evidence"][0]["probabilities"]["positive"], "0.7");
}

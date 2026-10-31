//! TEST ONLY data and loopback listeners; no provider purchase, real history or trade.
use super::*;
use chrono::Duration;
use rust_decimal::Decimal;
use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration as StdDuration,
};
use trady_application::{
    autonomous::{Dossier, SkillEvidence},
    optimization::{ApprovedFormula, digest},
};
use trady_domain::{Money, screening::ScreeningObservations};
use trady_policy::{ConfidenceInputs, EntryCosts, EntryRange, InstrumentRules, limits::*};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn directory() -> PathBuf {
    std::env::temp_dir().join(format!(
        "trady-inputs-worker-tests-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().to_utc()
}
fn d(s: &str) -> Decimal {
    Decimal::from_str_exact(s).unwrap()
}
fn money(s: &str) -> Money {
    Money {
        amount: d(s),
        currency: Currency::new("EUR").unwrap(),
    }
}
fn zero() -> EntryCosts {
    EntryCosts {
        fixed: money("0"),
        rate: Decimal::ZERO,
        uncertainty: money("0"),
        fx: None,
    }
}
fn skills() -> Vec<String> {
    [
        "conversation-and-tools",
        "search-planning",
        "market-research",
        "news-analysis",
        "source-comparison",
        "entry-readiness",
        "sector-selection",
        "sector-TEST_ONLY",
        "subsector-TEST_ONLY",
        "company-profile",
        "relationship-analysis",
        "holding-watchdog",
        "perpetual-review",
    ]
    .iter()
    .map(|v| (*v).to_owned())
    .collect()
}
fn config(now: DateTime<Utc>) -> InputsServiceConfig {
    let p = FormulaParameters {
        version: 1,
        family: FormulaFamily::Trend,
        entry_allowance_fraction: Decimal::ZERO,
        stop_atr_multiple: d("2"),
        target_net_fraction: d("0.03"),
        max_holding_sessions: 1,
        atr_lookback: 14,
        bar_seconds: 60,
    };
    let authority = FormulaAuthority {
        envelope: FormulaEnvelope {
            max_entry_allowance_fraction: d("0.005"),
            min_stop_atr_multiple: d("1"),
            max_stop_atr_multiple: d("4"),
            min_target_net_fraction: d("0.01"),
            max_target_net_fraction: d("0.03"),
        },
        max_account_risk_fraction: d("0.01"),
        prediction: None,
        active: ApprovedFormula {
            revision_id: "TEST_ONLY_FORMULA".into(),
            formula_hash: digest(&p).unwrap(),
            policy_hash: digest(&Policy::default()).unwrap(),
            parameters: p,
            approved_at: now - Duration::days(1),
            effective_from: now - Duration::hours(1),
            valid_until: now + Duration::days(7),
            proof_report_id: "TEST_ONLY_PROOF".into(),
            parity_report_id: "TEST_ONLY_PARITY".into(),
            evaluation_report_id: "TEST_ONLY_EVALUATION".into(),
            shadow_report_id: "TEST_ONLY_SHADOW".into(),
            owner_approval_id: "TEST_ONLY_OWNER".into(),
        },
    };
    let source = |group: &str, primary| SourcePolicy {
        provider: "TEST ONLY provider".into(),
        allowed_hosts: vec!["test-only.invalid".into()],
        independence_group: group.into(),
        may_be_primary: primary,
        rights: SourceRights {
            store_content: true,
            llm_processing: true,
            retention_hours: 168,
            terms_reference: "TEST ONLY synthetic license".into(),
        },
        rights_valid_until: now + Duration::days(14),
        kinds: vec![
            ImportKind::Calendar,
            ImportKind::Session,
            ImportKind::Quote,
            ImportKind::Costs,
            ImportKind::Evidence,
            ImportKind::Research,
            ImportKind::Universe,
            ImportKind::Forecast,
        ],
    };
    InputsServiceConfig {
        config_version: 1,
        store_directory: directory(),
        listen: "127.0.0.1:0".parse().unwrap(),
        workers: 2,
        request_timeout_ms: 200,
        maximum_body_bytes: 262_144,
        maximum_response_bytes: 1024 * 1024,
        sources: [
            ("primary".into(), source("TEST_ONLY_ISSUER", true)),
            ("independent".into(), source("TEST_ONLY_WIRE", false)),
            ("copied".into(), source("TEST_ONLY_ISSUER", false)),
            ("operator".into(), source("TEST_ONLY_OPERATOR", false)),
        ]
        .into(),
        reviewers: [(
            "TEST_ONLY_REVIEWER".into(),
            ReviewerPolicy {
                independence_group: "TEST_ONLY_AUDIT".into(),
                may_verify_primary: true,
            },
        )]
        .into(),
        accounts: vec![ServiceAccount {
            owner_id: "TEST_ONLY_OWNER".into(),
            account_id: AccountId::new("paper_TEST_ONLY").unwrap(),
            currency: Currency::new("EUR").unwrap(),
            calendar_id: "TEST_ONLY_XNAS".into(),
            policy: Policy::default(),
            formula_authority: authority,
            approved_skills: skills()
                .into_iter()
                .map(|s| (s, digest(&"TEST ONLY skill").unwrap()))
                .collect(),
        }],
        model: None,
    }
}
fn quote(now: DateTime<Utc>) -> Quote {
    Quote {
        instrument_id: InstrumentId::new("TEST_ONLY_STOCK").unwrap(),
        currency: Currency::new("EUR").unwrap(),
        bid: d("99.99"),
        ask: d("100"),
        observed_at: now,
        received_at: now,
        tradable: true,
    }
}
fn calendar(now: DateTime<Utc>) -> CalendarDocument {
    CalendarDocument {
        calendar_id: "TEST_ONLY_XNAS".into(),
        listing_mic: "XNAS".into(),
        observed_at: now,
        available_at: now,
        valid_until: now + Duration::days(6),
        coverage_start: now.date_naive(),
        coverage_end: (now + Duration::days(4)).date_naive(),
        sessions: vec![
            CalendarSession {
                session_id: "TEST_ONLY_FRIDAY".into(),
                opens_at: at("2026-10-30T13:30:00Z"),
                closes_at: at("2026-10-30T20:00:00Z"),
            },
            CalendarSession {
                session_id: "TEST_ONLY_MONDAY".into(),
                opens_at: at("2026-11-02T14:30:00Z"),
                closes_at: at("2026-11-02T21:00:00Z"),
            },
            CalendarSession {
                session_id: "TEST_ONLY_SPECIAL_CLOSE".into(),
                opens_at: at("2026-11-03T14:30:00Z"),
                closes_at: at("2026-11-03T18:00:00Z"),
            },
        ],
        closures: vec![
            ClosedDate {
                date: at("2026-10-31T00:00:00Z").date_naive(),
                reason: "TEST ONLY confirmed closure".into(),
            },
            ClosedDate {
                date: at("2026-11-01T00:00:00Z").date_naive(),
                reason: "TEST ONLY confirmed closure".into(),
            },
        ],
    }
}
fn import(
    id: &str,
    source: &str,
    lineage: &str,
    payload: ImportPayload,
    deps: Vec<String>,
    now: DateTime<Utc>,
) -> SourceImport {
    let (observed, available, until) = match &payload {
        ImportPayload::Quote(q) => (q.observed_at, q.received_at, now + Duration::seconds(60)),
        ImportPayload::Costs(c) => (c.observed_at, c.observed_at, c.valid_until),
        ImportPayload::Calendar(c) => (c.observed_at, c.available_at, c.valid_until),
        ImportPayload::Universe(u) => (u.as_of, u.available_at, u.expires_at),
        ImportPayload::Evidence(e) => (
            e.record.observed_at,
            e.record.observed_at,
            e.record.valid_until,
        ),
        ImportPayload::Session(s) => (
            s.session.as_ref().unwrap().observed_at,
            now,
            now + Duration::minutes(5),
        ),
        ImportPayload::Research(r) => (r.observed_at, r.observed_at, r.valid_until),
        ImportPayload::Forecast(f) => (
            f.observation.observed_at,
            f.observation.available_at,
            f.observation.valid_until,
        ),
    };
    let raw = serde_json::to_string(&payload).unwrap();
    SourceImport {
        schema_version: 1,
        id: id.into(),
        source_id: source.into(),
        provider: "TEST ONLY provider".into(),
        canonical_url: format!("https://test-only.invalid/{id}"),
        origin_lineage_id: lineage.into(),
        observed_at: observed,
        available_at: available,
        imported_at: now,
        valid_until: until,
        retain_until: now + Duration::days(7),
        raw_document_sha256: trady_application::screening::hex_digest(raw.as_bytes()),
        raw_document: raw,
        dependencies: deps,
        payload,
    }
}
fn review(i: &SourceImport, now: DateTime<Utc>) -> SourceReview {
    SourceReview {
        schema_version: 1,
        import_id: i.id.clone(),
        import_sha256: digest(i).unwrap(),
        raw_document_sha256: i.raw_document_sha256.clone(),
        reviewer_id: "TEST_ONLY_REVIEWER".into(),
        reviewed_at: now,
        valid_until: i.valid_until,
        source_identity_checked: true,
        rights_checked: true,
        original_availability_checked: true,
        mapping_checked: true,
        original_lineage_checked: true,
        model_self_attestation: false,
        review_note: "TEST ONLY independent review of synthetic test bytes".into(),
    }
}
fn verified(store: &ImportStore, i: &SourceImport, now: DateTime<Utc>) {
    store.import(i, now).unwrap();
    store.review(&review(i, now), now).unwrap();
}

#[test]
fn staging_review_serving_freshness_and_conflicting_same_time_quotes() {
    let now = at("2026-10-30T15:00:00Z");
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let service = InputsService::open(cfg.clone()).unwrap();
    let i = import(
        "TEST_ONLY_QUOTE",
        "operator",
        "TEST_ONLY_QUOTE_ORIGIN",
        ImportPayload::Quote(quote(now)),
        vec![],
        now,
    );
    service.store.import(&i, now).unwrap();
    assert_eq!(
        service
            .respond("GET", "/quote/TEST_ONLY_STOCK", &[], now)
            .status,
        503
    );
    service.store.review(&review(&i, now), now).unwrap();
    let response = service.respond("GET", "/quote/TEST_ONLY_STOCK", &[], now);
    assert_eq!(response.status, 200);
    assert_eq!(
        serde_json::from_slice::<Quote>(&response.body).unwrap(),
        quote(now)
    );
    let reopened = InputsService::open(cfg).unwrap();
    assert!(reopened.quote(&quote(now).instrument_id, now).is_ok());
    assert_eq!(
        reopened
            .respond(
                "GET",
                "/quote/TEST_ONLY_STOCK",
                &[],
                now + Duration::seconds(61)
            )
            .status,
        503
    );
    let mut q = quote(now);
    q.ask = d("101");
    let conflict = import(
        "TEST_ONLY_CONFLICT",
        "independent",
        "TEST_ONLY_OTHER_QUOTE_ORIGIN",
        ImportPayload::Quote(q),
        vec![],
        now,
    );
    verified(&service.store, &conflict, now);
    assert_eq!(
        service
            .respond("GET", "/quote/TEST_ONLY_STOCK", &[], now)
            .status,
        503
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_validation_rejects_self_verification_revisions_future_data_and_missing_dependencies() {
    let now = at("2026-10-30T15:00:00Z");
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let store = ImportStore::open(cfg.clone()).unwrap();
    let i = import(
        "TEST_ONLY_QUOTE",
        "operator",
        "TEST_ONLY_ORIGIN",
        ImportPayload::Quote(quote(now)),
        vec![],
        now,
    );
    store.import(&i, now).unwrap();
    let mut r = review(&i, now);
    r.model_self_attestation = true;
    assert!(store.review(&r, now).is_err());
    let mut r = review(&i, now);
    r.import_sha256 = "a".repeat(64);
    assert!(store.review(&r, now).is_err());
    let mut changed = i.clone();
    changed.raw_document.push(' ');
    changed.raw_document_sha256 =
        trady_application::screening::hex_digest(changed.raw_document.as_bytes());
    assert!(store.import(&changed, now).is_err());
    let mut bad = i.clone();
    bad.id = "TEST_ONLY_BAD_HOST".into();
    bad.canonical_url = "https://attacker.invalid/data".into();
    assert!(store.import(&bad, now).is_err());
    let mut bad = i.clone();
    bad.id = "TEST_ONLY_FUTURE".into();
    bad.available_at = now + Duration::seconds(1);
    assert!(store.import(&bad, now).is_err());
    let mut bad = i.clone();
    bad.id = "TEST_ONLY_MISSING_DEP".into();
    bad.dependencies.push("TEST_ONLY_ABSENT".into());
    store.import(&bad, now).unwrap();
    assert!(store.review(&review(&bad, now), now).is_err());
    let mut restricted = cfg;
    restricted
        .sources
        .get_mut("operator")
        .unwrap()
        .rights
        .store_content = false;
    assert!(ImportStore::open(restricted).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn retention_prunes_original_content_and_identity_cannot_be_recycled() {
    let now = at("2026-10-30T15:00:00Z");
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let store = ImportStore::open(cfg).unwrap();
    let mut i = import(
        "TEST_ONLY_QUOTE",
        "operator",
        "TEST_ONLY_ORIGIN",
        ImportPayload::Quote(quote(now)),
        vec![],
        now,
    );
    i.retain_until = now + Duration::minutes(1);
    verified(&store, &i, now);
    assert_eq!(store.prune_expired(now + Duration::minutes(2)).unwrap(), 1);
    assert!(store.staged(&i.id).is_err());
    assert!(store.import(&i, now).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn explicit_calendar_handles_dst_special_close_and_closed_dates_without_weekday_guessing() {
    let now = at("2026-10-30T15:00:00Z");
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let service = InputsService::open(cfg).unwrap();
    let c = calendar(now);
    let i = import(
        "TEST_ONLY_CALENDAR",
        "operator",
        "TEST_ONLY_EXCHANGE",
        ImportPayload::Calendar(c.clone()),
        vec![],
        now,
    );
    verified(&service.store, &i, now);
    assert_eq!(
        service
            .calendar(&c.calendar_id, now)
            .unwrap()
            .deadline(now, 2)
            .unwrap(),
        at("2026-11-02T21:00:00Z")
    );
    assert_eq!(c.deadline(now, 3).unwrap(), at("2026-11-03T18:00:00Z"));
    assert!(c.deadline(now, 4).is_err());
    let closed = service
        .session(
            &AccountId::new("paper_TEST_ONLY").unwrap(),
            at("2026-10-31T15:00:00Z"),
        )
        .unwrap();
    assert!(closed.session.is_none());
    assert!(service.session(&closed.account_id, now).is_err());
    let mut missing = c.clone();
    missing.closures.pop();
    assert!(missing.validate(now).is_err());
    let active = c.active_session(now).unwrap().unwrap();
    let session = SessionPacket {
        account_id: closed.account_id.clone(),
        session: Some(trady_application::autonomous::MarketSession {
            session_id: active.session_id,
            opens_at: active.opens_at,
            closes_at: active.closes_at,
            observed_at: now,
            opening_equity: money("1000"),
            deposits: money("20"),
            withdrawals: money("5"),
        }),
    };
    verified(
        &service.store,
        &import(
            "TEST_ONLY_ACCOUNTING",
            "operator",
            "TEST_ONLY_BROKER",
            ImportPayload::Session(session),
            vec![],
            now,
        ),
        now,
    );
    assert!(
        service
            .session(&closed.account_id, now)
            .unwrap()
            .session
            .is_some()
    );
    assert!(
        service
            .session(&closed.account_id, now + Duration::seconds(61))
            .is_err()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

fn research_fixture(
    service: &InputsService,
    secondary_source: &str,
    secondary_lineage: &str,
    now: DateTime<Utc>,
) -> SourceImport {
    verified(
        &service.store,
        &import(
            "TEST_ONLY_CALENDAR",
            "operator",
            "TEST_ONLY_EXCHANGE",
            ImportPayload::Calendar(calendar(now)),
            vec![],
            now,
        ),
        now,
    );
    let mut records = Vec::new();
    for (id, kind, source, lineage, primary) in [
        (
            "TEST_ONLY_PRIMARY",
            "source",
            "primary",
            "TEST_ONLY_ORIGINAL_PRIMARY",
            true,
        ),
        (
            "TEST_ONLY_SECONDARY",
            "source",
            secondary_source,
            secondary_lineage,
            false,
        ),
        (
            "TEST_ONLY_PROFILE",
            "company_profile",
            "operator",
            "TEST_ONLY_PROFILE_ORIGIN",
            false,
        ),
        (
            "TEST_ONLY_RELATIONSHIPS",
            "relationships",
            "operator",
            "TEST_ONLY_RELATIONSHIPS_ORIGIN",
            false,
        ),
        (
            "TEST_ONLY_WATCHDOG",
            "watchdog",
            "operator",
            "TEST_ONLY_WATCHDOG_ORIGIN",
            false,
        ),
        (
            "TEST_ONLY_ECONOMICS",
            "economics",
            "operator",
            "TEST_ONLY_ECONOMICS_ORIGIN",
            false,
        ),
    ] {
        let artifact = serde_json::json!({"test_only":true,"kind":kind,"content":"TEST ONLY finalized artifact"});
        let record = EvidenceRecord {
            id: id.into(),
            account_id: AccountId::new("paper_TEST_ONLY").unwrap(),
            instrument_id: InstrumentId::new("TEST_ONLY_STOCK").unwrap(),
            policy_version: 1,
            kind: kind.into(),
            revision_hash: digest(&artifact).unwrap(),
            observed_at: now,
            valid_until: now + Duration::minutes(5),
            dependencies: vec![],
            source_lineage: if kind == "source" {
                Some(lineage.into())
            } else {
                None
            },
            primary_verified: primary,
            material_conflict: false,
        };
        verified(
            &service.store,
            &import(
                id,
                source,
                lineage,
                ImportPayload::Evidence(EvidenceArtifact {
                    record: record.clone(),
                    artifact,
                }),
                vec![],
                now,
            ),
            now,
        );
        records.push(record);
    }
    let account = &service.config().accounts[0];
    let confidence = ConfidenceInputs {
        primary_verified: true,
        independent_lineages: 1,
        ..ConfidenceInputs::default()
    };
    let base = trady_policy::confidence(&confidence).total();
    let until = now + Duration::minutes(5);
    let formula = FormulaEvidence {
        snapshot_id: "TEST_ONLY_BARS".into(),
        family: FormulaFamily::Trend,
        signal_eligible: true,
        atr: d("0.5"),
        atr_lookback: 14,
        bar_seconds: 60,
        completed_bars: 30,
        last_completed_at: now - Duration::seconds(45),
        available_at: now - Duration::seconds(30),
        valid_until: until,
        minimum_stop_distance: d("0.01"),
        noise_distance: d("0.1"),
        base_confidence_score: base,
        confidence_score: base,
        sentiment_entry_allowed: true,
        sentiment_target_ceiling: d("0.03"),
        exceptional_verified: false,
    };
    let deps = records.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
    let packet = AnalystPacket {
        schema_version: 1,
        owner_id: account.owner_id.clone(),
        account_id: account.account_id.clone(),
        policy_version: 1,
        observed_at: now,
        valid_until: until,
        retain_until: now + Duration::hours(1),
        screening_observations: ScreeningObservations::default(),
        proposals: vec![crate::autonomous::ResearchDraft {
            proposal_id: "TEST_ONLY_PROPOSAL".into(),
            instrument_id: InstrumentId::new("TEST_ONLY_STOCK").unwrap(),
            currency: Currency::new("EUR").unwrap(),
            valid_until: until,
            deadline_at: at("2026-10-30T20:00:00Z"),
            entry_range: EntryRange {
                minimum: d("99"),
                maximum: d("101"),
                currency: Currency::new("EUR").unwrap(),
            },
            confidence_inputs: confidence,
            formula_evidence: formula,
            entry_costs: zero(),
            exit_costs: zero(),
            instrument_rules: InstrumentRules {
                price_tick: d("0.01"),
                quantity_step: d("0.01"),
                minimum_quantity: d("0.01"),
                currency_unit: d("0.01"),
            },
            dossier: Dossier {
                id: "TEST_ONLY_DOSSIER".into(),
                finalized: true,
                sector_skill: "sector-TEST_ONLY".into(),
                subsector_skill: "subsector-TEST_ONLY".into(),
                records,
                skills: skills()
                    .into_iter()
                    .map(|name| SkillEvidence {
                        content_hash: account.approved_skills[&name].clone(),
                        name,
                        artifact_ids: vec!["TEST_ONLY_PROFILE".into()],
                    })
                    .collect(),
                blockers: vec![],
            },
            feature_schema_version: "TEST_ONLY_FEATURES".into(),
            features: vec![],
            perpetual_review_reason: "TEST ONLY: native advice still required".into(),
            sentiment_reference: None,
        }],
    };
    import(
        "TEST_ONLY_RESEARCH",
        "operator",
        "TEST_ONLY_RESEARCH_ORIGIN",
        ImportPayload::Research(packet),
        deps,
        now,
    )
}

#[test]
fn research_uses_actual_reviewed_artifacts_and_common_origins_are_not_independent() {
    let now = at("2026-10-30T15:00:00Z");
    for (source, lineage, expected) in [
        ("independent", "TEST_ONLY_INDEPENDENT_ORIGIN", 200),
        ("copied", "TEST_ONLY_OTHER_URL_SAME_OWNER", 503),
        ("independent", "TEST_ONLY_ORIGINAL_PRIMARY", 503),
    ] {
        let cfg = config(now);
        let dir = cfg.store_directory.clone();
        let service = InputsService::open(cfg).unwrap();
        let i = research_fixture(&service, source, lineage, now);
        verified(&service.store, &i, now);
        assert_eq!(
            service
                .respond("GET", "/research/paper_TEST_ONLY", &[], now)
                .status,
            expected
        );
        if expected == 200 {
            let mut forged = i.clone();
            forged.id = "TEST_ONLY_FORGED_RESEARCH".into();
            if let ImportPayload::Research(p) = &mut forged.payload {
                p.proposals[0].dossier.records[0].primary_verified = false;
            }
            // A new operator review cannot change the independently stored evidence artifact.
            forged.raw_document = serde_json::to_string(&forged.payload).unwrap();
            forged.raw_document_sha256 =
                trady_application::screening::hex_digest(forged.raw_document.as_bytes());
            verified(&service.store, &forged, now);
            assert!(
                service
                    .validate_research(
                        &forged,
                        match &forged.payload {
                            ImportPayload::Research(p) => p,
                            _ => unreachable!(),
                        },
                        now
                    )
                    .is_err()
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

fn append_secondary(
    service: &InputsService,
    research: &mut SourceImport,
    id: &str,
    source: &str,
    lineage: &str,
    now: DateTime<Utc>,
) {
    let artifact = serde_json::json!({"test_only":true,"original_lineage":lineage});
    let record = EvidenceRecord {
        id: id.into(),
        account_id: AccountId::new("paper_TEST_ONLY").unwrap(),
        instrument_id: InstrumentId::new("TEST_ONLY_STOCK").unwrap(),
        policy_version: 1,
        kind: "source".into(),
        revision_hash: digest(&artifact).unwrap(),
        observed_at: now,
        valid_until: now + Duration::minutes(5),
        dependencies: vec![],
        source_lineage: Some(lineage.into()),
        primary_verified: false,
        material_conflict: false,
    };
    verified(
        &service.store,
        &import(
            id,
            source,
            lineage,
            ImportPayload::Evidence(EvidenceArtifact {
                record: record.clone(),
                artifact,
            }),
            vec![],
            now,
        ),
        now,
    );
    let ImportPayload::Research(packet) = &mut research.payload else {
        unreachable!()
    };
    packet.proposals[0].dossier.records.push(record);
    research.dependencies.push(id.into());
}

#[test]
fn corroboration_requires_distinct_secondary_lineages_and_ownership_groups() {
    let now = at("2026-10-30T15:00:00Z");
    type CorroborationCase<'a> = (&'a [(&'a str, &'a str)], u8, u16);
    let cases: &[CorroborationCase<'_>] = &[
        (&[("operator", "TEST_ONLY_SECONDARY_ORIGIN")], 2, 503), // Two owners copy one original.
        (&[("independent", "TEST_ONLY_OTHER_ORIGIN")], 2, 503),  // One owner, two originals.
        (&[("operator", "TEST_ONLY_OTHER_ORIGIN")], 2, 200),     // Both dimensions independent.
        (
            &[
                ("independent", "TEST_ONLY_OTHER_ORIGIN"),
                ("operator", "TEST_ONLY_SECONDARY_ORIGIN"),
            ],
            2,
            200,
        ), // Requires rematching.
        (
            &[
                ("independent", "TEST_ONLY_OTHER_ORIGIN"),
                ("independent", "TEST_ONLY_THIRD_ORIGIN"),
                ("operator", "TEST_ONLY_SECONDARY_ORIGIN"),
                ("wire_three", "TEST_ONLY_SECONDARY_ORIGIN"),
            ],
            3,
            503,
        ), // Three owners/lineages, only two independent pairs.
    ];
    for (extra, declared, expected) in cases {
        let mut cfg = config(now);
        let mut source = cfg.sources["independent"].clone();
        source.independence_group = "TEST_ONLY_THIRD_WIRE".into();
        cfg.sources.insert("wire_three".into(), source);
        let dir = cfg.store_directory.clone();
        let service = InputsService::open(cfg).unwrap();
        let mut research =
            research_fixture(&service, "independent", "TEST_ONLY_SECONDARY_ORIGIN", now);
        for (n, (source, lineage)) in extra.iter().enumerate() {
            append_secondary(
                &service,
                &mut research,
                &format!("TEST_ONLY_EXTRA_SOURCE_{n}"),
                source,
                lineage,
                now,
            );
        }
        let ImportPayload::Research(packet) = &mut research.payload else {
            unreachable!()
        };
        let draft = &mut packet.proposals[0];
        draft.confidence_inputs.independent_lineages = *declared;
        let score = trady_policy::confidence(&draft.confidence_inputs).total();
        draft.formula_evidence.base_confidence_score = score;
        draft.formula_evidence.confidence_score = score;
        research.raw_document = serde_json::to_string(&research.payload).unwrap();
        research.raw_document_sha256 =
            trady_application::screening::hex_digest(research.raw_document.as_bytes());
        verified(&service.store, &research, now);
        assert_eq!(
            service
                .respond("GET", "/research/paper_TEST_ONLY", &[], now)
                .status,
            *expected
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn retention_cleanup_recovers_cumulative_and_legacy_index_capacity_without_reusing_ids() {
    let start = at("2026-10-30T15:00:00Z");
    let cfg = config(start);
    let dir = cfg.store_directory.clone();
    let store = ImportStore::open(cfg).unwrap().with_capacity(2);
    let mut now = start;
    for round in 0..3 {
        for n in 0..2 {
            let mut i = import(
                &format!("TEST_ONLY_ROUND_{round}_{n}"),
                "operator",
                "TEST_ONLY_ORIGIN",
                ImportPayload::Quote(quote(now)),
                vec![],
                now,
            );
            i.retain_until = now + Duration::minutes(1);
            verified(&store, &i, now);
            assert!(
                store
                    .latest(ImportKind::Quote, "TEST_ONLY_STOCK", now)
                    .is_ok()
            );
        }
        let extra = import(
            &format!("TEST_ONLY_OVERFLOW_{round}"),
            "operator",
            "TEST_ONLY_ORIGIN",
            ImportPayload::Quote(quote(now)),
            vec![],
            now,
        );
        assert!(store.import(&extra, now).is_err());
        now += Duration::minutes(2);
        assert_eq!(store.prune_expired(now).unwrap(), 2);
        assert_eq!(std::fs::read_dir(dir.join("index")).unwrap().count(), 0);
        assert_eq!(
            std::fs::read_dir(dir.join("tombstones")).unwrap().count(),
            (round + 1) * 2
        );
    }
    // Simulate cumulative expired index metadata left by an older version. More
    // than twice the active cap must still be recoverable by streaming cleanup.
    for n in 0..5 {
        let id = format!("TEST_ONLY_LEGACY_{n}");
        let index = serde_json::json!({"id":id,"kind":"QUOTE","key":"TEST_ONLY_STOCK","import_sha256":digest(&(id.clone(),"TEST ONLY expired index")).unwrap(),
            "observed_at":start,"available_at":start,"valid_until":start+Duration::minutes(1),"retain_until":start+Duration::minutes(1)});
        trady_application::model_lab::write_immutable(
            dir.join("index")
                .join(format!("{}.json", digest(&id).unwrap())),
            &index,
        )
        .unwrap();
    }
    assert!(
        store
            .latest(ImportKind::Quote, "TEST_ONLY_STOCK", now)
            .is_err()
    );
    assert_eq!(store.prune_expired(now).unwrap(), 0);
    assert_eq!(std::fs::read_dir(dir.join("index")).unwrap().count(), 0);
    assert_eq!(
        std::fs::read_dir(dir.join("tombstones")).unwrap().count(),
        11
    );
    let fresh = import(
        "TEST_ONLY_FRESH_AFTER_CLEANUP",
        "operator",
        "TEST_ONLY_ORIGIN",
        ImportPayload::Quote(quote(now)),
        vec![],
        now,
    );
    verified(&store, &fresh, now);
    assert_eq!(
        store
            .latest(ImportKind::Quote, "TEST_ONLY_STOCK", now)
            .unwrap()
            .id,
        fresh.id
    );
    let recycled = import(
        "TEST_ONLY_ROUND_0_0",
        "operator",
        "TEST_ONLY_ORIGIN",
        ImportPayload::Quote(quote(now)),
        vec![],
        now,
    );
    assert!(
        store
            .import(&recycled, now)
            .unwrap_err()
            .0
            .contains("cannot be reused")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn active_index_bounds_are_checked_before_review_publication_and_during_lookup() {
    let now = at("2026-10-30T15:00:00Z");
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let full = ImportStore::open(cfg.clone()).unwrap();
    for n in 0..2 {
        verified(
            &full,
            &import(
                &format!("TEST_ONLY_ACTIVE_{n}"),
                "operator",
                "TEST_ONLY_ORIGIN",
                ImportPayload::Quote(quote(now)),
                vec![],
                now,
            ),
            now,
        );
    }
    let staged = import(
        "TEST_ONLY_STAGED_OVERFLOW",
        "operator",
        "TEST_ONLY_ORIGIN",
        ImportPayload::Quote(quote(now)),
        vec![],
        now,
    );
    full.import(&staged, now).unwrap();
    let limited = ImportStore::open(cfg).unwrap().with_capacity(2);
    assert!(
        limited
            .review(&review(&staged, now), now)
            .unwrap_err()
            .0
            .contains("capacity")
    );
    assert!(
        !dir.join("reviews")
            .join(format!("{}.json", digest(&staged.id).unwrap()))
            .exists()
    );
    assert!(limited.resolve(&staged.id, now).is_err());
    // A previously larger store cannot be read through a smaller active cap.
    full.review(&review(&staged, now), now).unwrap();
    assert!(
        limited
            .latest(ImportKind::Quote, "TEST_ONLY_STOCK", now)
            .unwrap_err()
            .0
            .contains("active input index capacity")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn store_writers_share_an_os_lock_and_release_it_without_deleting_the_lock_file() {
    let now = at("2026-10-30T15:00:00Z");
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let store = ImportStore::open(cfg).unwrap();
    let i = import(
        "TEST_ONLY_LOCKED_REVIEW",
        "operator",
        "TEST_ONLY_ORIGIN",
        ImportPayload::Quote(quote(now)),
        vec![],
        now,
    );
    store.import(&i, now).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join(".writes.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(
        store
            .review(&review(&i, now), now)
            .unwrap_err()
            .0
            .contains("busy")
    );
    assert!(store.prune_expired(now).unwrap_err().0.contains("busy"));
    drop(lock);
    store.review(&review(&i, now), now).unwrap();
    assert!(
        store
            .latest(ImportKind::Quote, "TEST_ONLY_STOCK", now)
            .is_ok()
    );
    assert!(dir.join(".writes.lock").is_file());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cost_fee_currency_matches_the_configured_account_on_import_and_read() {
    let now = at("2026-10-30T15:00:00Z");
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let service = InputsService::open(cfg.clone()).unwrap();
    let mut wrong = zero();
    wrong.fixed.currency = Currency::new("USD").unwrap();
    wrong.uncertainty.currency = Currency::new("USD").unwrap();
    let packet = |costs| CostsPacket {
        instrument_id: InstrumentId::new("TEST_ONLY_STOCK").unwrap(),
        observed_at: now,
        valid_until: now + Duration::minutes(5),
        costs,
    };
    let mismatch = import(
        "TEST_ONLY_WRONG_CURRENCY",
        "operator",
        "TEST_ONLY_ORIGIN",
        ImportPayload::Costs(packet(wrong)),
        vec![],
        now,
    );
    assert!(
        service
            .store
            .import(&mismatch, now)
            .unwrap_err()
            .0
            .contains("account currency")
    );
    let valid = import(
        "TEST_ONLY_CORRECT_CURRENCY",
        "operator",
        "TEST_ONLY_ORIGIN",
        ImportPayload::Costs(packet(zero())),
        vec![],
        now,
    );
    verified(&service.store, &valid, now);
    assert!(service.costs(&packet(zero()).instrument_id, now).is_ok());
    let mut changed = cfg;
    changed.accounts[0].currency = Currency::new("USD").unwrap();
    let changed = InputsService::open(changed).unwrap();
    assert!(changed.costs(&packet(zero()).instrument_id, now).is_err());
    assert_eq!(
        changed
            .respond("GET", "/costs/TEST_ONLY_STOCK", &[], now)
            .status,
        503
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn costs_require_positive_current_fx_into_the_account_currency() {
    let now = at("2026-10-30T15:00:00Z");
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let service = InputsService::open(cfg).unwrap();
    let fx = trady_policy::FxObservation {
        from: Currency::new("USD").unwrap(),
        to: Currency::new("EUR").unwrap(),
        rate: d("0.95"),
        observed_at: now,
    };
    let mut invalid = vec![];
    let mut reverse = fx.clone();
    std::mem::swap(&mut reverse.from, &mut reverse.to);
    invalid.push(reverse);
    let mut same = fx.clone();
    same.from = same.to.clone();
    invalid.push(same);
    let mut zero_rate = fx.clone();
    zero_rate.rate = Decimal::ZERO;
    invalid.push(zero_rate);
    let mut stale = fx.clone();
    stale.observed_at = now - Duration::seconds(61);
    invalid.push(stale);
    let mut future = fx.clone();
    future.observed_at = now + Duration::seconds(1);
    invalid.push(future);
    for (n, fx) in invalid.into_iter().enumerate() {
        let mut costs = zero();
        costs.fx = Some(fx);
        let packet = CostsPacket {
            instrument_id: InstrumentId::new("TEST_ONLY_STOCK").unwrap(),
            observed_at: now,
            valid_until: now + Duration::minutes(5),
            costs,
        };
        let invalid = import(
            &format!("TEST_ONLY_INVALID_FX_{n}"),
            "operator",
            "TEST_ONLY_ORIGIN",
            ImportPayload::Costs(packet),
            vec![],
            now,
        );
        assert!(service.store.import(&invalid, now).is_err());
    }
    let mut costs = zero();
    let mut aging = fx;
    aging.observed_at = now - Duration::seconds(59);
    costs.fx = Some(aging);
    let packet = CostsPacket {
        instrument_id: InstrumentId::new("TEST_ONLY_STOCK").unwrap(),
        observed_at: now,
        valid_until: now + Duration::minutes(5),
        costs,
    };
    verified(
        &service.store,
        &import(
            "TEST_ONLY_VALID_FX",
            "operator",
            "TEST_ONLY_ORIGIN",
            ImportPayload::Costs(packet.clone()),
            vec![],
            now,
        ),
        now,
    );
    assert!(service.costs(&packet.instrument_id, now).is_ok());
    // The costs themselves are two seconds old; only their underlying FX expired.
    assert!(
        service
            .costs(&packet.instrument_id, now + Duration::seconds(2))
            .is_err()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn authenticated_http_is_bounded_and_read_token_cannot_import_or_review() {
    let now = Utc::now();
    let cfg = config(now);
    let dir = cfg.store_directory.clone();
    let service = InputsService::open(cfg).unwrap();
    verified(
        &service.store,
        &import(
            "TEST_ONLY_HTTP_QUOTE",
            "operator",
            "TEST_ONLY_ORIGIN",
            ImportPayload::Quote(quote(now)),
            vec![],
            now,
        ),
        now,
    );
    let token = "TEST_ONLY_PRIVATE_BEARER_TOKEN_123456789";
    let server = HttpServer::bind(service, token.into()).unwrap();
    let address = server.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    let worker = thread::spawn(move || server.serve_until(signal));
    let request = |raw: String| {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(StdDuration::from_secs(3)))
            .unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut output = String::new();
        stream.read_to_string(&mut output).unwrap();
        output
    };
    let auth = format!("Authorization: Bearer {token}\r\n");
    let valid = request(format!(
        "GET /quote/TEST_ONLY_STOCK HTTP/1.1\r\nHost: localhost\r\n{auth}\r\n"
    ));
    assert!(valid.starts_with("HTTP/1.1 200"));
    assert!(valid.contains("Cache-Control: no-store"));
    assert!(!valid.contains(token));
    assert!(
        request("GET /quote/TEST_ONLY_STOCK HTTP/1.1\r\nHost: localhost\r\n\r\n".into())
            .starts_with("HTTP/1.1 401")
    );
    assert!(
        request(format!(
            "GET /quote/TEST_ONLY_STOCK HTTP/1.1\r\nHost: localhost\r\n{auth}{auth}\r\n"
        ))
        .starts_with("HTTP/1.1 400")
    );
    assert!(request(format!("POST /predictions/paper_TEST_ONLY HTTP/1.1\r\nHost: localhost\r\n{auth}Content-Type: application/json\r\nContent-Length: 999999\r\n\r\n")).starts_with("HTTP/1.1 413"));
    assert!(request(format!("POST /import/source HTTP/1.1\r\nHost: localhost\r\n{auth}Content-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}")).starts_with("HTTP/1.1 404"));
    assert!(request(format!("POST /predictions/paper_TEST_ONLY HTTP/1.1\r\nHost: localhost\r\n{auth}Content-Type: application/json\r\nContent-Length: 2\r\n\r\n")).starts_with("HTTP/1.1 400"));
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

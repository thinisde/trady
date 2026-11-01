//! Synthetic position fixtures in a disposable database; no financial approval.
use chrono::{Duration, Utc};
use postgres::{Client, NoTls};
use rust_decimal::Decimal;
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
};
use trady_application::{autonomous::*, optimization::digest};
use trady_application::{
    harness::Scope,
    research_jobs::JobPolicy,
    telegram::gateway::{GatewayCommand, GatewayRequest},
};
use trady_domain::{perpetual::*, *};
use trady_policy::{
    Policy,
    holding::{HoldingPlan, HoldingState, TrailingSettings},
};
use trady_storage::research_jobs::PgResearchJobs;
use trady_storage::{autonomous::PgTradingJournal, migrate, trading_research::PgTradingResearch};

fn dsn(role: &str) -> String {
    if let Ok(value) = env::var(format!("{}_DSN", role.to_ascii_uppercase())) {
        return value;
    }
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        env::var("TRADY_TEST_PORT").unwrap()
    )
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn queued_holding_research_retention_removes_payloads_and_keeps_scoped_identities() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let mut administrator = Client::connect(&dsn("trady_migrator"), NoTls).unwrap();
    administrator.batch_execute("SET ROLE trady_owner").unwrap();
    for (owner, id, expired) in [
        ("fixture_retention_owner", "expired", true),
        ("fixture_retention_owner", "current", false),
        ("other_retention_owner", "foreign", true),
    ] {
        let deadline = Utc::now()
            + if expired {
                -Duration::seconds(1)
            } else {
                Duration::hours(1)
            };
        administrator.execute("INSERT INTO research.holding_updates VALUES($1,'paper_retention_fixture',$2,'fixture_position',1,$3,$3,'{\"fixture\":\"retained research\"}')",&[&owner,&id,&deadline]).unwrap();
        administrator.execute("INSERT INTO research.holding_update_identities VALUES($1,'paper_retention_fixture',$2,$3)",&[&owner,&id,&"a".repeat(64)]).unwrap();
    }
    let mut retention = PgTradingResearch::connect_retention(
        &dsn("trady_analyst"),
        "fixture_retention_owner".into(),
        AccountId::new("paper_retention_fixture").unwrap(),
    )
    .unwrap();
    retention.purge_expired().unwrap();
    let ids:Vec<String>=administrator.query("SELECT update_id FROM research.holding_updates WHERE account_id='paper_retention_fixture' ORDER BY update_id",&[]).unwrap().into_iter().map(|row|row.get(0)).collect();
    assert_eq!(ids, ["current", "foreign"]);
    let count:i64=administrator.query_one("SELECT count(*) FROM research.holding_update_identities WHERE account_id='paper_retention_fixture'",&[]).unwrap().get(0);
    assert_eq!(count, 3);
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn holding_queue_locks_sources_and_tracks_later_retention_changes() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let mut administrator = Client::connect(&dsn("trady_migrator"), NoTls).unwrap();
    administrator.batch_execute("SET ROLE trady_owner").unwrap();
    let owner = "fixture_queue_retention_owner";
    let account = "paper_queue_retention_fixture";
    let now = Utc::now();
    let expiry = now + Duration::minutes(2);
    let assessment_retention = now + Duration::seconds(30);
    let binding = serde_json::json!({"arguments":{"position_id":"fixture_position"}});
    let assessment = serde_json::json!({
        "assessment":{"assessment_id":"fixture_assessment","status":"OK","valid_until":expiry},
        "context":{"scenario":{"binding":binding}}
    });
    let dossier = serde_json::json!({"id":"fixture_dossier","fixture":"TEST ONLY"});
    let review = serde_json::json!({"fixture":"TEST ONLY"});
    let update = serde_json::json!({
        "id":"fixture_update","position_id":"fixture_position","expected_plan_version":1,
        "valid_until":expiry,"assessment":assessment,"review":review,"research":{"dossier":dossier}
    });
    let config = serde_json::json!({"owner_id":owner});
    let state = serde_json::json!({"plan":{
        "candidate":{"proposal_id":"fixture_position"},
        "holding_state":{"plan":{"version":1}},"exit_latch":null
    }});
    administrator.execute("INSERT INTO execution.autonomous_accounts(account_id,config,document) VALUES($1,$2,$3)",&[&account,&config,&state]).unwrap();
    for (kind, id, document, retention) in [
        (
            "assessment",
            "fixture_assessment",
            assessment.clone(),
            assessment_retention,
        ),
        (
            "dossier",
            "fixture_dossier",
            dossier,
            expiry + Duration::minutes(1),
        ),
        (
            "review",
            "fixture_review",
            serde_json::json!({"review":review,"scenario":binding}),
            expiry + Duration::minutes(1),
        ),
    ] {
        administrator
            .execute(
                "INSERT INTO research.trading_records VALUES($1,$2,$3,$4,$5,$6)",
                &[&owner, &account, &kind, &id, &document, &retention],
            )
            .unwrap();
    }
    let expected: chrono::DateTime<Utc> = administrator.query_one("SELECT retain_until FROM research.trading_records WHERE owner_id=$1 AND account_id=$2 AND kind='assessment'",&[&owner,&account]).unwrap().get(0);
    // Establish an old snapshot before another transaction publishes its copy.
    let mut stale = Client::connect(&dsn("trady_migrator"), NoTls).unwrap();
    stale
        .batch_execute("SET ROLE trady_owner; BEGIN ISOLATION LEVEL REPEATABLE READ")
        .unwrap();
    stale.query_one("SELECT retain_until FROM research.trading_records WHERE owner_id=$1 AND account_id=$2 AND kind='assessment'",&[&owner,&account]).unwrap();
    // Stop the queue after dependency validation, at its identity lock. A source
    // deletion at this point must wait, rather than disappearing from the minimum.
    administrator.batch_execute("BEGIN").unwrap();
    administrator
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("{owner}:{account}:fixture_update")],
        )
        .unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let queued = update.clone();
    let queuer = std::thread::spawn(move || {
        let mut client = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
        let pid: i32 = client
            .query_one("SELECT pg_backend_pid()", &[])
            .unwrap()
            .get(0);
        sender.send(pid).unwrap();
        client.query_one(
            "SELECT research.queue_holding_update($1,$2,$3)",
            &[&owner, &account, &queued],
        )
    });
    let pid = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let waiting = loop {
        let waiting: bool = administrator.query_one("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND locktype='advisory' AND NOT granted)",&[&pid]).unwrap().get(0);
        if waiting || std::time::Instant::now() >= deadline {
            break waiting;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let mut cleanup = Client::connect(&dsn("trady_migrator"), NoTls).unwrap();
    cleanup
        .batch_execute("SET ROLE trady_owner; SET lock_timeout='100ms'")
        .unwrap();
    let deletion = if waiting {
        Some(cleanup.execute("DELETE FROM research.trading_records WHERE owner_id=$1 AND account_id=$2 AND kind='assessment'",&[&owner,&account]))
    } else {
        None
    };
    administrator.batch_execute("COMMIT").unwrap();
    queuer.join().unwrap().unwrap();
    assert!(waiting, "queue did not reach its identity lock");
    assert_eq!(
        deletion.unwrap().unwrap_err().code(),
        Some(&postgres::error::SqlState::LOCK_NOT_AVAILABLE)
    );
    let retained: chrono::DateTime<Utc> = administrator
        .query_one(
            "SELECT retain_until FROM research.holding_updates WHERE owner_id=$1 AND account_id=$2",
            &[&owner, &account],
        )
        .unwrap()
        .get(0);
    assert_eq!(retained, expected);
    // A stale snapshot cannot shorten a source while missing the queued row in
    // its trigger. Queue publication creates a source MVCC version and forces
    // that writer to retry with a fresh snapshot.
    let shortened = Utc::now() + Duration::seconds(5);
    let stale_write = stale.query_one(
        "SELECT research.put_trading_record($1,$2,'assessment','fixture_assessment',$3,$4)",
        &[&owner, &account, &assessment, &shortened],
    );
    stale.batch_execute("ROLLBACK").unwrap();
    assert_eq!(
        stale_write.unwrap_err().code().map(|code| code.code()),
        Some("40001")
    );
    // A later license-retention reduction must also shorten the embedded copy.
    let shortened = Utc::now() + Duration::seconds(5);
    administrator
        .execute(
            "SELECT research.put_trading_record($1,$2,'assessment','fixture_assessment',$3,$4)",
            &[&owner, &account, &assessment, &shortened],
        )
        .unwrap();
    let retained: chrono::DateTime<Utc> = administrator
        .query_one(
            "SELECT retain_until FROM research.holding_updates WHERE owner_id=$1 AND account_id=$2",
            &[&owner, &account],
        )
        .unwrap()
        .get(0);
    let expected: chrono::DateTime<Utc> = administrator.query_one("SELECT retain_until FROM research.trading_records WHERE owner_id=$1 AND account_id=$2 AND kind='assessment'",&[&owner,&account]).unwrap().get(0);
    assert_eq!(retained, expected);
    administrator.execute("DELETE FROM research.trading_records WHERE owner_id=$1 AND account_id=$2 AND kind='review'",&[&owner,&account]).unwrap();
    let payloads: i64 = administrator
        .query_one(
            "SELECT count(*) FROM research.holding_updates WHERE owner_id=$1 AND account_id=$2",
            &[&owner, &account],
        )
        .unwrap()
        .get(0);
    assert_eq!(payloads, 0);
    let identities: i64 = administrator.query_one("SELECT count(*) FROM research.holding_update_identities WHERE owner_id=$1 AND account_id=$2",&[&owner,&account]).unwrap().get(0);
    assert_eq!(identities, 1);
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn holding_advice_requires_fresh_executor_economics_and_cannot_reverse_exit() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let now = Utc::now();
    let account = AccountId::new("paper_verified_holding").unwrap();
    let instrument = InstrumentId::new("fixture_stock").unwrap();
    let currency = Currency::new("EUR").unwrap();
    let money = |amount: &str| serde_json::json!({"amount":amount,"currency":"EUR"});
    let costs =
        serde_json::json!({"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null});
    let candidate:EntryCandidate=serde_json::from_value(serde_json::json!({
        "proposal_id":"fixture_position","account_id":account,"account_snapshot_id":"fixture_entry",
        "policy_version":1,"instrument_id":instrument,"currency":"EUR","created_at":now-Duration::minutes(10),
        "valid_until":now-Duration::minutes(5),"entry_range":{"minimum":"99.9","maximum":"100","currency":"EUR"},
        "stop_price":"99","target_net_fraction":"0.01","max_holding_sessions":3,"deadline_at":now+Duration::hours(1),
        "exceptional_verified":false,"confidence_inputs":{"primary_verified":true,"catalyst":"DIRECT","independent_lineages":2,
            "positive_15m_return":true,"above_vwap":true,"relative_volume":"2","spread_fraction":"0.0001",
            "round_trip_cost_fraction":"0.0001","catalyst_age_hours":1,"sources_current":true,"continuing_relevance":true},
        "entry_costs":costs,"exit_costs":costs,
        "instrument_rules":{"price_tick":"0.01","quantity_step":"0.01","minimum_quantity":"0.01","currency_unit":"0.01"},
        "dossier":{"id":"fixture_dossier","finalized":false,"sector_skill":"fixture_sector","subsector_skill":"fixture_group","records":[],"skills":[],"blockers":[]},
        "screening":{"universe_snapshot_id":"fixture_universe","screening_id":"fixture_screening","finalist":instrument,"observed_at":now,"valid_until":now+Duration::minutes(5)},
        "advisor":{"assessment_id":"fixture_advice","model_revision_id":"fixture_model","scenario_hash":"a".repeat(64),"account_snapshot_id":"fixture_entry",
           "assessed_at":now,"valid_until":now+Duration::seconds(60),"assessed_bid":"100","assessed_ask":"100","quote_drift_tolerance":"0.001",
           "usable":false,"calibration_ok":false,"drift_ok":false,"review_reason":"TEST ONLY: no entry authority"},"formula":null
    })).unwrap();
    let policy = Policy::default();
    let holding_state = HoldingState::new(
        instrument.clone(),
        Decimal::new(1, 2),
        HoldingPlan {
            version: 1,
            stop_price: 99.into(),
            target_net_fraction: Decimal::new(1, 2),
            deadline: now + Duration::hours(1),
            trailing: TrailingSettings::default(),
        },
        &policy,
    )
    .unwrap();
    let plan = ActivePlan {
        candidate: candidate.clone(),
        acquired_quantity: Decimal::ONE,
        entry_session: "fixture_session".into(),
        entry_session_close: now + Duration::hours(2),
        entered_at: now - Duration::minutes(10),
        first_order: 0,
        holding_state,
        holding_sessions: BTreeSet::from(["fixture_session".into()]),
        exit_latch: None,
        market_exit_required: false,
        data_unusable_since: None,
    };
    let config = TradingConfig {
        config_version: 1,
        account_id: account.clone(),
        mode: Mode::Paper,
        base_currency: currency.clone(),
        owner_id: "fixture_holding_owner".into(),
        policy,
        capabilities: Capabilities::default(),
        approved_skills: BTreeMap::new(),
        live_release: None,
        limits_authority: None,
    };
    let observation = ExecutionObservation {
        snapshot: AccountSnapshot {
            snapshot_id: "fixture_observation".into(),
            account_id: account.clone(),
            mode: Mode::Paper,
            spendable_cash: Money {
                amount: 900.into(),
                currency: currency.clone(),
            },
            positions: vec![Position {
                instrument_id: instrument.clone(),
                quantity: Decimal::ONE,
            }],
            orders: vec![],
            observed_at: now,
        },
        position_costs: BTreeMap::from([(
            instrument.clone(),
            Money {
                amount: 100.into(),
                currency: currency.clone(),
            },
        )]),
        available_quantities: BTreeMap::from([(instrument.clone(), Decimal::ONE)]),
        total_equity: Money {
            amount: 1000.into(),
            currency: currency.clone(),
        },
    };
    let mut journal = PgTradingJournal::connect(&dsn("trady_executor")).unwrap();
    let fence = journal
        .acquire(&config, &WorkerId::new("fixture_worker").unwrap(), 60)
        .unwrap();
    let mut state = TradingState {
        revision: 1,
        position_state: PositionState::OpenProtected,
        plan: Some(plan),
        last_snapshot: Some(observation.snapshot.clone()),
        ..TradingState::default()
    };
    journal.save(&fence, 0, &state).unwrap();
    let mut research = PgTradingResearch::connect(
        &dsn("trady_analyst"),
        config.owner_id.clone(),
        account.clone(),
    )
    .unwrap();
    assert!(research.load_verified_holding("fixture_position").is_err());
    journal
        .record_holding_observation(&fence, &observation, &candidate.exit_costs)
        .unwrap();
    let verified = research.load_verified_holding("fixture_position").unwrap();
    assert_eq!(
        verified.basis(now).unwrap().acquisition_cost.amount,
        Decimal::from(100)
    );
    let mut context = AssessmentContext {
        scenario: ScenarioSnapshot {
            binding: ScenarioBinding {
                arguments: AssessArguments {
                    instrument_id: instrument.clone(),
                    scenario_calculation_id: "pending".into(),
                    market_snapshot_id: "fixture_market".into(),
                    profile_review_id: "fixture_profile".into(),
                    screening_artifact_id: "fixture_screening".into(),
                    position_id: Some("fixture_position".into()),
                },
                currency: currency.clone(),
                entry_price: 100.into(),
                stop_price: 99.into(),
                target_price: Decimal::new(1015, 1),
                estimated_cost_fraction: Decimal::ZERO,
                horizon_seconds: 3600,
                deadline_at: now + Duration::hours(1),
            },
            calculated_at: now,
            valid_until: now + Duration::seconds(30),
            market: MarketObservation {
                instrument_id: instrument.clone(),
                snapshot_id: "fixture_market".into(),
                observed_at: now,
                valid_until: now + Duration::seconds(30),
                executable_price: 100.into(),
                fx_rate: Decimal::ONE,
            },
        },
        features: FeatureSnapshot {
            snapshot_id: "fixture_features".into(),
            schema_version: "fixture_schema".into(),
            as_of: now,
            valid_until: now + Duration::seconds(30),
            features: vec![FeatureValue {
                name: "fixture_signal".into(),
                value: 1.0,
                lineage: vec![FeatureLineage {
                    source_id: "fixture_source".into(),
                    source_revision: "fixture_revision".into(),
                    evidence_id: "fixture_profile".into(),
                    observed_at: now - Duration::seconds(1),
                    available_at: now - Duration::seconds(1),
                }],
            }],
        },
    };
    context.scenario.binding.arguments.scenario_calculation_id = verified
        .calculation_id(&context.scenario.binding, now)
        .unwrap();
    verified.validate_context(&context, now).unwrap();
    let mut changed = context.clone();
    changed.scenario.binding.entry_price += Decimal::ONE;
    assert!(verified.validate_context(&changed, now).is_err());
    changed = context.clone();
    changed.scenario.binding.stop_price = 98.into();
    assert!(verified.validate_context(&changed, now).is_err());
    let mut stale = verified.clone();
    stale.observation.snapshot.observed_at = now - Duration::seconds(61);
    assert!(stale.basis(now).is_err());
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    assert!(
        analyst
            .batch_execute(
                "UPDATE execution.holding_observations SET observed_at=clock_timestamp()"
            )
            .is_err()
    );
    assert!(analyst.batch_execute("SELECT execution.record_holding_observation('paper_verified_holding','fixture_worker',1,'{}')").is_err());
    let mut foreign =
        PgTradingResearch::connect(&dsn("trady_analyst"), "other_owner".into(), account).unwrap();
    assert!(foreign.load_verified_holding("fixture_position").is_err());
    let mut forged = state.clone();
    forged.revision += 1;
    let active = forged.plan.as_mut().unwrap();
    let mut proposed = active.holding_state.plan().clone();
    proposed.version += 1;
    active.holding_state = HoldingState::new(
        instrument.clone(),
        Decimal::new(1, 2),
        proposed,
        &config.policy,
    )
    .unwrap();
    active.candidate.sentiment_reference =
        Some(trady_application::research_jobs::SentimentReadinessRef {
            assessment_id: "missing_assessment".into(),
            assessment_hash: "c".repeat(64),
            packet_id: "missing_packet".into(),
            packet_hash: "d".repeat(64),
            scope: Scope {
                owner_id: config.owner_id.clone(),
                account_id: config.account_id.as_str().into(),
                policy_version: config.policy.version,
            },
            instrument_id: instrument.as_str().into(),
            model_revision: "e".repeat(40),
            scored_at: now,
            maximum_negative_probability: Decimal::ZERO,
            expires_at: now + Duration::minutes(1),
            dependency_hash: "f".repeat(64),
        });
    assert!(
        journal
            .save(&fence, state.revision, &forged)
            .unwrap_err()
            .to_string()
            .contains("sentiment")
    );
    let mut executor = Client::connect(&dsn("trady_executor"), NoTls).unwrap();
    assert!(executor.batch_execute("SELECT execution.save_trader_core('paper_verified_holding','fixture_worker',1,1,'{}')").is_err());
    verify_atomic_sentiment_activation(&config, &mut journal, &fence, &mut state);
    state.plan.as_mut().unwrap().exit_latch = Some("fixture_stop_reached".into());
    let previous = state.revision;
    state.revision += 1;
    journal.save(&fence, previous, &state).unwrap();
    assert!(research.load_verified_holding("fixture_position").is_err());
    verify_durable_owner_controls(&config, &mut journal, &fence, &mut state);
    assert!(!digest(&context).unwrap().is_empty());
    journal.release(&fence).unwrap();
}

fn verify_atomic_sentiment_activation(
    config: &TradingConfig,
    journal: &mut PgTradingJournal,
    fence: &Fence,
    state: &mut TradingState,
) {
    use trady_application::{research::*, research_jobs::*};
    use trady_domain::{research::*, sentiment::*};
    use trady_storage::research::PgResearchRepository;
    let at = chrono::DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let scope = Scope {
        owner_id: config.owner_id.clone(),
        account_id: config.account_id.as_str().into(),
        policy_version: config.policy.version,
    };
    let source_ids = BTreeSet::from([
        "holding_fixture_primary".into(),
        "holding_fixture_independent".into(),
    ]);
    let mut jobs = PgResearchJobs::connect_analyst(&dsn("trady_analyst")).unwrap();
    jobs.install_policy(&JobPolicy {
        scope: scope.clone(),
        mode: Mode::Paper,
        telegram_user_id: 71001,
        bot_id: 71002,
        source_ids: source_ids.clone(),
        instrument_ids: BTreeSet::from(["fixture_stock".into()]),
        registry_hash: digest(&source_ids).unwrap(),
        sentiment_revision: Some(FINBERT_REVISION.into()),
        daily_job_limit: 10,
        pending_job_limit: 10,
        enabled: true,
    })
    .unwrap();
    let mut observations = PgResearchRepository::connect(&dsn("trady_analyst")).unwrap();
    let mut sources = vec![];
    let mut items = vec![];
    for id in &source_ids {
        let source = SourceConfig {
            id: id.clone(),
            kind: SourceKind::PublisherRss,
            enabled: true,
            url: format!("https://example.com/{id}/feed"),
            allowed_hosts: vec!["example.com".into()],
            credential_env: None,
            interval_seconds: 60,
            daily_request_limit: 10,
            query: None,
            x_account: None,
            x_rule: None,
            rights: SourceRights {
                store_content: true,
                llm_processing: true,
                retention_hours: 24,
                terms_reference: "synthetic atomic activation fixture".into(),
            },
        };
        let old = observations.state(id).unwrap();
        let source_state = prepare_state(&source, old.clone()).unwrap();
        let text = format!("Synthetic {id} fixture for atomic plan dependency tests");
        let item = SourceItem {
            item_id: digest(&(id, at)).unwrap(),
            source_id: id.clone(),
            external_id: id.clone(),
            canonical_url: format!("https://example.com/{id}/item"),
            lineage_id: digest(id).unwrap(),
            content_hash: trady_application::harness::content_hash(&text),
            parser_version: "fixture-1".into(),
            publisher: id.clone(),
            feed_title: None,
            title: Some(text.clone()),
            summary: None,
            text: Some(text),
            published_at: Some(at - Duration::minutes(1)),
            event_at: Some(at - Duration::minutes(1)),
            first_seen_at: at,
            fetched_at: at,
            expires_at: at + Duration::hours(2),
            language: Some("en".into()),
            rights: source.rights.clone(),
            content_status: ContentStatus::Available,
            social: None,
            related_urls: vec![],
            gaps: vec![],
        };
        observations
            .commit_page(
                id,
                old.revision,
                &source_state,
                std::slice::from_ref(&item),
                at,
            )
            .unwrap();
        sources.push(source);
        items.push(item);
    }
    let verifications = items
        .iter()
        .map(|i| Verification {
            item_id: i.item_id.clone(),
            instrument_id: "fixture_stock".into(),
            verification_reference: "synthetic trusted verifier fixture".into(),
            issuer_mapping_verified: true,
            primary_verified: i.source_id == "holding_fixture_primary",
            reporting_origin: Some(i.source_id.clone()),
            origin_verified: true,
            ambiguities: vec![],
        })
        .collect();
    let claims = vec![ClaimReview {
        claim_id: "fixture_claim".into(),
        supporting_item_ids: items.iter().map(|i| i.item_id.clone()).collect(),
        contradicting_item_ids: vec![],
        ambiguities: vec![],
    }];
    let packet = jobs
        .publish_verified_packet(
            &scope,
            "fixture_stock",
            None,
            verifications,
            claims,
            at + Duration::hours(1),
            at,
        )
        .unwrap();
    let assessment = SentimentAssessment {
        schema_version: SENTIMENT_VERSION,
        model_id: FINBERT_MODEL.into(),
        model_revision: FINBERT_REVISION.into(),
        scored_at: at,
        evidence: items
            .iter()
            .map(|i| SentimentEvidence {
                item_id: i.item_id.clone(),
                lineage_id: i.lineage_id.clone(),
                text_sha256: trady_application::sentiment::text_sha256(i.text.as_ref().unwrap()),
                observed_at: i.event_at.unwrap(),
                expires_at: i.expires_at,
                probabilities: SentimentProbabilities {
                    positive: Decimal::new(7, 1),
                    negative: Decimal::new(1, 1),
                    neutral: Decimal::new(2, 1),
                },
            })
            .collect(),
    };
    jobs.enqueue(
        &scope,
        "holding_fixture_sentiment",
        &Work::Sentiment {
            packet_id: packet.id.clone(),
            model_revision: FINBERT_REVISION.into(),
        },
    )
    .unwrap();
    let lease = jobs
        .claim(&scope, "fixture_sentiment_worker", 30)
        .unwrap()
        .unwrap();
    let result = jobs
        .store_sentiment(&lease, &packet, &assessment, at)
        .unwrap();
    let reference = jobs
        .checked_readiness_reference(&scope, result["assessment_id"].as_str().unwrap(), at)
        .unwrap();
    let mut updated = state.clone();
    updated.revision += 1;
    let active = updated.plan.as_mut().unwrap();
    let mut proposed = active.holding_state.plan().clone();
    proposed.version += 1;
    active.holding_state = HoldingState::new(
        active.candidate.instrument_id.clone(),
        Decimal::new(1, 2),
        proposed,
        &config.policy,
    )
    .unwrap();
    active.candidate.sentiment_reference = Some(reference);
    journal.save(fence, state.revision, &updated).unwrap();
    *state = updated;

    // A rights change invalidates acceptance in the database transaction itself.
    sources[0].rights.llm_processing = false;
    let old = observations.state(&sources[0].id).unwrap();
    let revoked = prepare_state(&sources[0], old.clone()).unwrap();
    observations
        .commit_page(&sources[0].id, old.revision, &revoked, &[], at)
        .unwrap();
    let mut rejected = state.clone();
    rejected.revision += 1;
    let active = rejected.plan.as_mut().unwrap();
    let mut proposed = active.holding_state.plan().clone();
    proposed.version += 1;
    active.holding_state = HoldingState::new(
        active.candidate.instrument_id.clone(),
        Decimal::new(1, 2),
        proposed,
        &config.policy,
    )
    .unwrap();
    assert!(
        journal
            .save(fence, state.revision, &rejected)
            .unwrap_err()
            .to_string()
            .contains("sentiment")
    );
    // Reconciliation and protection stay available after research is invalidated.
    let previous = state.revision;
    state.revision += 1;
    journal.save(fence, previous, state).unwrap();
}

fn verify_durable_owner_controls(
    config: &TradingConfig,
    journal: &mut PgTradingJournal,
    fence: &Fence,
    state: &mut TradingState,
) {
    let scope = Scope {
        owner_id: config.owner_id.clone(),
        account_id: config.account_id.as_str().into(),
        policy_version: config.policy.version,
    };
    let mut analyst = PgResearchJobs::connect_analyst(&dsn("trady_analyst")).unwrap();
    analyst
        .install_policy(&JobPolicy {
            scope: scope.clone(),
            mode: Mode::Paper,
            telegram_user_id: 71001,
            bot_id: 71002,
            source_ids: BTreeSet::from(["fixture_source".into()]),
            instrument_ids: BTreeSet::from(["fixture_stock".into()]),
            registry_hash: "b".repeat(64),
            sentiment_revision: None,
            daily_job_limit: 10,
            pending_job_limit: 10,
            enabled: true,
        })
        .unwrap();
    let mut gateway = PgResearchJobs::connect_gateway(&dsn("trady_gateway")).unwrap();
    let request = |update_id, command| GatewayRequest {
        bot_id: 71002,
        update_id,
        sender_id: 71001,
        chat_id: 71001,
        scope: scope.clone(),
        command,
    };
    let defaults = OwnerControls {
        pause_entries: false,
        close_position: false,
    };
    let synchronize = |repo: &mut PgTradingJournal, s: &TradingState, controls: &OwnerControls| {
        repo.synchronize_owner_controls(fence, config, s, controls, Utc::now())
            .unwrap()
    };
    assert!(!synchronize(journal, state, &defaults).pause_entries);
    let pause = request(1, GatewayCommand::PauseEntries);
    let receipt = gateway
        .accept_gateway(&pause, None, "", Utc::now())
        .unwrap();
    assert!(receipt.contains("REQUESTED"));
    assert_eq!(
        gateway
            .accept_gateway(&pause, None, "", Utc::now())
            .unwrap(),
        receipt
    );
    assert!(synchronize(journal, state, &defaults).pause_entries);
    // A replacement process observes the durable pause with unchanged deployment defaults.
    let mut restarted = PgTradingJournal::connect(&dsn("trady_executor")).unwrap();
    assert!(synchronize(&mut restarted, state, &defaults).pause_entries);
    let preview = gateway
        .accept_gateway(
            &request(2, GatewayCommand::ResumeEntries),
            None,
            "",
            Utc::now(),
        )
        .unwrap();
    let token = preview.split("/confirm ").nth(1).unwrap().trim().to_owned();
    let confirmed = request(3, GatewayCommand::Confirm(token));
    assert!(
        gateway
            .accept_gateway(&confirmed, None, "", Utc::now())
            .unwrap()
            .contains("REQUESTED")
    );
    assert!(!synchronize(journal, state, &defaults).pause_entries);
    assert!(
        gateway
            .accept_gateway(&confirmed, None, "", Utc::now())
            .unwrap()
            .contains("REQUESTED")
    );
    assert!(!synchronize(journal, state, &defaults).pause_entries);

    // Confirmation occurred, but the holding changed before executor acceptance.
    let preview = gateway
        .accept_gateway(
            &request(4, GatewayCommand::ClosePosition),
            None,
            "",
            Utc::now(),
        )
        .unwrap();
    let token = preview.split("/confirm ").nth(1).unwrap().trim().to_owned();
    gateway
        .accept_gateway(
            &request(5, GatewayCommand::Confirm(token)),
            None,
            "",
            Utc::now(),
        )
        .unwrap();
    state.last_snapshot.as_mut().unwrap().positions[0].quantity = Decimal::new(5, 1);
    let previous = state.revision;
    state.revision += 1;
    journal.save(fence, previous, state).unwrap();
    let controls = synchronize(journal, state, &defaults);
    assert!(!controls.close_position);
    assert!(
        gateway
            .pending_deliveries(71002, 50)
            .unwrap()
            .iter()
            .any(|d| d.text.contains("REJECTED"))
    );

    let preview = gateway
        .accept_gateway(
            &request(6, GatewayCommand::ClosePosition),
            None,
            "",
            Utc::now(),
        )
        .unwrap();
    let token = preview.split("/confirm ").nth(1).unwrap().trim().to_owned();
    gateway
        .accept_gateway(
            &request(7, GatewayCommand::Confirm(token)),
            None,
            "",
            Utc::now(),
        )
        .unwrap();
    let controls = synchronize(journal, state, &defaults);
    assert!(controls.pause_entries && controls.close_position);
    assert!(synchronize(&mut restarted, state, &defaults).close_position);
    // Configured operator pauses also override an existing persisted unpaused latch.
    let controls = synchronize(
        journal,
        state,
        &OwnerControls {
            pause_entries: true,
            close_position: false,
        },
    );
    assert!(controls.pause_entries && controls.close_position);
    assert!(
        gateway
            .pending_deliveries(71002, 50)
            .unwrap()
            .iter()
            .any(|d| d.text.contains("ACCEPTED"))
    );

    // Closing an unfilled entry remains pending until cancellation reconciliation.
    let mut pending = state.clone();
    pending.last_snapshot.as_mut().unwrap().positions.clear();
    pending.position_state = PositionState::EntryPending;
    pending.plan.as_mut().unwrap().acquired_quantity = Decimal::ZERO;
    let previous = pending.revision;
    pending.revision += 1;
    journal.save(fence, previous, &pending).unwrap();
    synchronize(journal, &pending, &defaults);
    let preview = gateway
        .accept_gateway(
            &request(8, GatewayCommand::ClosePosition),
            None,
            "",
            Utc::now(),
        )
        .unwrap();
    let token = preview.split("/confirm ").nth(1).unwrap().trim().to_owned();
    gateway
        .accept_gateway(
            &request(9, GatewayCommand::Confirm(token)),
            None,
            "",
            Utc::now(),
        )
        .unwrap();
    let controls = synchronize(journal, &pending, &defaults);
    assert!(controls.close_position);
    let mut raw = Client::connect(&dsn("trady_gateway"), NoTls).unwrap();
    let status: String = raw
        .query_one(
            "SELECT status FROM control.owner_controls WHERE bot_id=71002 AND update_id=8",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(status, "ACCEPTED");
    // An unresolved broker order still prevents COMPLETE even without a plan.
    pending.plan = None;
    pending
        .last_snapshot
        .as_mut()
        .unwrap()
        .orders
        .push(BrokerOrder {
            broker_order_id: "fixture_pending_order".into(),
            client_intent_id: None,
            instrument_id: InstrumentId::new("fixture_stock").unwrap(),
            side: Side::Buy,
            quantity: Decimal::ONE,
            filled_quantity: Decimal::ZERO,
            status: OrderStatus::Open,
        });
    let previous = pending.revision;
    pending.revision += 1;
    journal.save(fence, previous, &pending).unwrap();
    synchronize(journal, &pending, &defaults);
    let status: String = raw
        .query_one(
            "SELECT status FROM control.owner_controls WHERE bot_id=71002 AND update_id=8",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(status, "ACCEPTED");
    pending.last_snapshot.as_mut().unwrap().orders.clear();
    let previous = pending.revision;
    pending.revision += 1;
    journal.save(fence, previous, &pending).unwrap();
    synchronize(journal, &pending, &defaults);
    let status: String = raw
        .query_one(
            "SELECT status FROM control.owner_controls WHERE bot_id=71002 AND update_id=8",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(status, "COMPLETE");
    *state = pending;
}

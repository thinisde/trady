//! Synthetic integration fixtures only; none is a proof, backtest or live approval.
//! Runs exclusively in scripts/test-postgres.py's disposable PostgreSQL server.
use chrono::{Duration, Utc};
use postgres::{Client, NoTls};
use rust_decimal::Decimal;
use serde_json::json;
use std::{
    collections::BTreeMap,
    env,
    sync::{Arc, Barrier},
    thread,
};
use trady_application::optimization::*;
use trady_domain::*;
use trady_policy::{Policy, limits::*};
use trady_storage::{migrate, weekly::*};

fn dsn(role: &str) -> String {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    let key = if role == "trady_releaser" {
        "TRADY_RELEASE_DSN".into()
    } else {
        format!("{}_DSN", role.to_ascii_uppercase())
    };
    if let Ok(value) = env::var(key) {
        return value;
    }
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        env::var("TRADY_TEST_PORT").unwrap()
    )
}
fn initial(account: &str) -> WeeklyState {
    let now = Utc::now();
    let plan = ExperimentPlan {
        campaign_id: "weekly_test_campaign".into(),
        hypothesis: "synthetic transactional test".into(),
        envelope: FormulaEnvelope {
            max_entry_allowance_fraction: Decimal::new(5, 3),
            min_stop_atr_multiple: Decimal::ONE,
            max_stop_atr_multiple: Decimal::from(4),
            min_target_net_fraction: Decimal::new(1, 2),
            max_target_net_fraction: Decimal::new(3, 2),
        },
        candidates: vec![FormulaParameters {
            version: 1,
            family: FormulaFamily::Trend,
            entry_allowance_fraction: Decimal::new(1, 3),
            stop_atr_multiple: Decimal::from(2),
            target_net_fraction: Decimal::new(3, 2),
            max_holding_sessions: 3,
            atr_lookback: 14,
            bar_seconds: 60,
        }],
        max_cumulative_trials: 1,
        wall_clock_seconds: 3600,
        objective: Objective {
            drawdown_weight: Decimal::new(1, 1),
            instability_weight: Decimal::new(2, 1),
            complexity_weight: Decimal::ZERO,
        },
        minimum_improvement: Decimal::new(1, 3),
        min_outer_intervals: 6,
        min_closed_trades: 100,
        min_shadow_weeks: 4,
        min_shadow_closed_trades: 20,
        development_folds: vec![
            SessionInterval {
                session_id: "old".into(),
                opens_at: now - Duration::days(60),
                closes_at: now - Duration::days(40),
            },
            SessionInterval {
                session_id: "new".into(),
                opens_at: now - Duration::days(30),
                closes_at: now - Duration::days(10),
            },
        ],
        development_fold_sessions: 14,
    };
    let mut hashes: BTreeMap<String, String> = [
        "calendar",
        "features",
        "models",
        "calibrators",
        "fusion",
        "labels",
        "costs",
        "rust_build",
        "engine",
        "driver",
        "lean_toolchain",
        "mathlib",
        "skill",
        "prompt",
    ]
    .into_iter()
    .map(|s| (s.into(), "a".repeat(64)))
    .collect();
    hashes.insert("objective".into(), digest(&plan.objective).unwrap());
    hashes.insert("splits".into(), plan.splits_hash().unwrap());
    hashes.insert("gates".into(), plan.gates_hash().unwrap());
    hashes.insert("policy".into(), digest(&Policy::default()).unwrap());
    let run = RunManifest {
        schema_version: 1,
        account_id: AccountId::new(account).unwrap(),
        mode: Mode::Paper,
        source_commit: "b".repeat(40),
        source_clean: true,
        champion_id: "fixture_champion".into(),
        dataset_hash: "c".repeat(64),
        cutoff_at: now - Duration::hours(1),
        completed_sessions: vec![SessionInterval {
            session_id: "completed".into(),
            opens_at: now - Duration::days(1),
            closes_at: now - Duration::hours(2),
        }],
        next_session: SessionInterval {
            session_id: "future".into(),
            opens_at: now + Duration::days(2),
            closes_at: now + Duration::days(2) + Duration::hours(6),
        },
        activation_deadline: now + Duration::days(1),
        component_hashes: hashes,
        random_seed: 1,
    };
    new_weekly_state(run, plan, Policy::default(), now).unwrap()
}
fn trial(state: &WeeklyState) -> TrialRecord {
    let parameters = state.plan.candidates[0].clone();
    TrialRecord {
        candidate_id: format!("candidate_{}", digest(&parameters).unwrap()),
        parameters,
        registered_at: Utc::now(),
        development: None,
        failure: None,
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn weekly_store_restart_concurrency_fencing_and_campaign_budget_are_durable() {
    migrate(&dsn("trady_migrator")).unwrap();
    let state = initial("paper_weekly_durable");
    let id = state.manifest.run_id().unwrap();
    let mut store = PgWeeklyStore::connect(&dsn("trady_lab"), "first", 1).unwrap();
    store.freeze(&state).unwrap();
    let frozen = store.load(&id).unwrap();
    let reserved = trial(&frozen);
    store.reserve_trial(&id, &reserved, 1).unwrap();
    assert_eq!(store.load(&id).unwrap().trials.len(), 1);
    assert!(
        PgWeeklyStore::connect(&dsn("trady_lab"), "other", 30)
            .unwrap()
            .freeze(&state)
            .is_err()
    );
    // Simulate a process dying without releasing its lease, then race two restarts.
    drop(store);
    thread::sleep(std::time::Duration::from_millis(1200));
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|n| {
            let state = state.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let mut store =
                    PgWeeklyStore::connect(&dsn("trady_lab"), &format!("restart_{n}"), 30).unwrap();
                barrier.wait();
                match store.freeze(&state) {
                    Ok(()) => {
                        let resumed = store.load(&state.manifest.run_id().unwrap()).unwrap();
                        assert_eq!(resumed.started_at, frozen.started_at);
                        assert_eq!(resumed.trials.len(), 1);
                        true
                    }
                    Err(_) => false,
                }
            })
        })
        .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|won| *won)
            .count(),
        1
    );
    // New campaign/run names do not reset a dataset's cumulative trial budget.
    let mut next = state.clone();
    next.manifest.random_seed += 1;
    next.plan.campaign_id = "different_campaign".into();
    let mut later = PgWeeklyStore::connect(&dsn("trady_lab"), "later", 30).unwrap();
    later.freeze(&next).unwrap();
    assert!(
        later
            .reserve_trial(&next.manifest.run_id().unwrap(), &trial(&next), 1)
            .is_err()
    );
    later.release().unwrap();
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    assert!(
        analyst
            .query_one(
                "SELECT weekly.freeze($1,$2,$3,$4)",
                &[
                    &id,
                    &serde_json::to_value(&state).unwrap(),
                    &"intruder",
                    &30_i32
                ]
            )
            .is_err()
    );
    assert!(
        analyst
            .batch_execute("UPDATE weekly.runs SET document='{}'")
            .is_err()
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn weekly_checkpoints_are_monotonic_and_interrupted_holdout_is_never_replayed() {
    migrate(&dsn("trady_migrator")).unwrap();
    let state = initial("paper_weekly_holdout");
    let id = state.manifest.run_id().unwrap();
    let mut store = PgWeeklyStore::connect(&dsn("trady_lab"), "holdout", 30).unwrap();
    store.freeze(&state).unwrap();
    let reserved = trial(&state);
    store.reserve_trial(&id, &reserved, 1).unwrap();
    let mut loaded = store.load(&id).unwrap();
    let original = loaded.clone();
    loaded.trials.clear();
    assert!(store.checkpoint(&id, &loaded).is_err());
    loaded = original;
    loaded.trials[0].failure = Some("synthetic_failed_trial".into());
    store.checkpoint(&id, &loaded).unwrap();
    loaded.trials[0].failure = None;
    assert!(store.checkpoint(&id, &loaded).is_err());
    store.release().unwrap();
    assert!(store.checkpoint(&id, &loaded).is_err());
    // A fresh synthetic trial tests only the holdout ledger. No release authority.
    let state = initial("paper_weekly_holdout_consumption");
    let id = state.manifest.run_id().unwrap();
    let mut store = PgWeeklyStore::connect(&dsn("trady_lab"), "sealed", 30).unwrap();
    store.freeze(&state).unwrap();
    let mut ledger = store.holdouts();
    let reserved = trial(&state);
    store.reserve_trial(&id, &reserved, 1).unwrap();
    let mut loaded = store.load(&id).unwrap();
    let hash = trial_input_hash(&state.manifest, &state.plan, &reserved).unwrap();
    let report = ToolReport {
        report_id: "synthetic_fixture".into(),
        tool_id: "fixture".into(),
        input_hash: hash,
        output_hash: "a".repeat(64),
        exit_status: 0,
        duration_ms: 1,
        recorded_at: Utc::now(),
    };
    loaded.trials[0].development = Some(DevelopmentReport {
        report: report.clone(),
        folds: state
            .plan
            .development_folds
            .iter()
            .map(|interval| FoldMetrics {
                interval: interval.clone(),
                mean_log_return: Decimal::ZERO,
                max_drawdown: Decimal::ZERO,
                closed_trades: 0,
                complete: true,
            })
            .collect(),
    });
    loaded.verification = Some(VerificationReport {
        proof: report.clone(),
        parity: report,
        approved_axioms_only: true,
        non_vacuous: true,
        exact_numeric_parity: true,
        safety_definitions_unchanged: true,
    });
    store.checkpoint(&id, &loaded).unwrap();
    assert!(
        ledger
            .claim(&state.manifest, &state.plan, &reserved)
            .unwrap()
            .is_none()
    );
    assert!(
        ledger
            .claim(&state.manifest, &state.plan, &reserved)
            .is_err()
    );
    store.release().unwrap();
    assert!(store.checkpoint(&id, &loaded).is_err());
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn release_evidence_requires_independent_roles_real_sessions_and_untampered_content() {
    migrate(&dsn("trady_migrator")).unwrap();
    let state = initial("paper_weekly_release");
    let candidate = ReleaseCandidate {
        revision_id: "test_revision".into(),
        account_id: state.manifest.account_id.clone(),
        mode: Mode::Paper,
        owner_id: "test_owner".into(),
        policy_hash: digest(&state.policy).unwrap(),
        formula_hash: digest(&state.plan.candidates[0]).unwrap(),
        source_hash: "a".repeat(64),
        calendar_hash: "b".repeat(64),
        model_hashes: BTreeMap::from([("fixture_model".into(), "c".repeat(64))]),
        parameters: state.plan.candidates[0].clone(),
        envelope: state.plan.envelope,
        effective_from: Utc::now() + Duration::days(40),
        valid_until: Utc::now() + Duration::days(47),
        supersedes_release_id: None,
    };
    let mut lab = PgReleaseStore::connect(&dsn("trady_lab"), "trady_lab").unwrap();
    let id = lab.candidate(&candidate).unwrap();
    let mut releaser = PgReleaseStore::connect(&dsn("trady_releaser"), "trady_releaser").unwrap();
    assert!(
        releaser
            .advance(&id, 0, "ACTIVE", &BTreeMap::new())
            .is_err()
    );
    let mut evidence = evidence_envelope(&candidate, "verification", Utc::now()).unwrap();
    for name in [
        "proof_hash",
        "parity_hash",
        "evaluation_hash",
        "verifier_hash",
    ] {
        evidence[name] = json!("d".repeat(64));
    }
    evidence["source_hash"] = json!(candidate.source_hash);
    evidence["passed"] = json!(true);
    evidence["proof_report_id"] = json!("fixture_proof");
    evidence["parity_report_id"] = json!("fixture_parity");
    evidence["evaluation_report_id"] = json!("fixture_evaluation");
    evidence["evidence_class"] = json!("TEST");
    evidence["scope"] = json!("FORMULA_V1");
    evidence["checks"] = json!({"clean_build":true,"approved_axioms":true,"non_vacuity":true,"full_numeric_parity":true,"protected_safety":true,"sealed_evaluation":true,"calibration":true,"schema_compatibility":true,"execution_protection":true,"source_permissions":true});
    assert!(lab.record_evidence("verification", &evidence).is_err());
    let mut verifier = PgReleaseStore::connect(&dsn("trady_verifier"), "trady_verifier").unwrap();
    assert!(
        verifier
            .record_evidence("verification", &json!({"kind":"verification"}))
            .is_err()
    );
    // The trusted recorder rejects explicitly nonproduction evidence as well.
    assert!(verifier.record_evidence("verification", &evidence).is_err());
    evidence["evidence_class"] = json!("PRODUCTION");
    let evidence_id = verifier.record_evidence("verification", &evidence).unwrap();
    releaser
        .advance(
            &id,
            0,
            "VERIFIED",
            &BTreeMap::from([("verification".into(), evidence_id.clone())]),
        )
        .unwrap();
    assert!(
        releaser
            .advance(&id, 1, "SHADOW", &BTreeMap::new())
            .is_err()
    );
    assert!(
        releaser
            .advance(&id, 1, "APPROVED", &BTreeMap::new())
            .is_err()
    );
    assert!(releaser.active_formula(&id).is_err());
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    admin
        .execute(
            "UPDATE release.evidence SET canonical=$1 WHERE evidence_id=$2",
            &[&"{}", &evidence_id],
        )
        .unwrap();
    assert!(releaser.resolve(&id).is_err());
    let mut shadow = PgReleaseStore::connect(&dsn("trady_shadow"), "trady_shadow").unwrap();
    assert!(shadow.record_evidence("owner", &evidence).is_err());
    assert!(lab.resolve(&id).is_err());
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn weekly_sql_null_fences_missing_holdout_bindings_and_ledger_fields_fail_closed() {
    migrate(&dsn("trady_migrator")).unwrap();
    let state = initial("paper_weekly_sql_nulls");
    let id = state.manifest.run_id().unwrap();
    let document = serde_json::to_value(&state).unwrap();
    let mut lab = Client::connect(&dsn("trady_lab"), NoTls).unwrap();
    assert!(
        lab.query_one(
            "SELECT weekly.freeze($1,$2,'sql_fixture',NULL)",
            &[&id, &document]
        )
        .is_err()
    );
    let generation: i64 = lab
        .query_one(
            "SELECT weekly.freeze($1,$2,'sql_fixture',30)",
            &[&id, &document],
        )
        .unwrap()
        .get(0);
    assert!(
        lab.query_one("SELECT weekly.load_run($1,NULL)", &[&id])
            .is_err()
    );
    assert!(
        lab.query_one("SELECT weekly.renew($1,$2,NULL)", &[&id, &generation])
            .is_err()
    );
    assert!(
        lab.query_one("SELECT weekly.release_run($1,NULL)", &[&id])
            .is_err()
    );
    assert!(
        lab.query_one(
            "SELECT weekly.freeze_v6($1,$2,'sql_fixture',30)",
            &[&id, &document]
        )
        .is_err()
    );
    let reserved = trial(&state);
    let trial_json = serde_json::to_value(&reserved).unwrap();
    assert!(
        lab.query_one(
            "SELECT weekly.reserve_trial($1,$2,$3,NULL)",
            &[&id, &generation, &trial_json]
        )
        .is_err()
    );
    lab.query_one(
        "SELECT weekly.reserve_trial($1,$2,$3,1)",
        &[&id, &generation, &trial_json],
    )
    .unwrap();
    let mut loaded: serde_json::Value = lab
        .query_one("SELECT weekly.load_run($1,$2)", &[&id, &generation])
        .unwrap()
        .get(0);
    let hash = "a".repeat(64);
    loaded["trials"][0]["development"] = json!({"report":{"input_hash":hash}});
    loaded["verification"] = json!({"proof":{"input_hash":hash},"parity":{"input_hash":hash},"approved_axioms_only":true,"non_vacuous":true,"exact_numeric_parity":true,"safety_definitions_unchanged":true});
    lab.query_one(
        "SELECT weekly.checkpoint($1,$2,$3)",
        &[&id, &generation, &loaded],
    )
    .unwrap();
    lab.query_one(
        "SELECT weekly.claim_holdout($1,$2,$3,$4)",
        &[&id, &generation, &reserved.candidate_id, &hash],
    )
    .unwrap();
    for report in [
        serde_json::Value::Null,
        json!({}),
        json!({"run_id":id,"report":{"input_hash":null}}),
    ] {
        assert!(
            lab.query_one(
                "SELECT weekly.finish_holdout($1,$2,$3)",
                &[&id, &generation, &report]
            )
            .is_err()
        );
    }
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    let mut verifier = Client::connect(&dsn("trady_verifier"), NoTls).unwrap();
    assert!(analyst.query_one("SELECT weekly.record_ledger('forecast','paper_weekly_sql_nulls','paper','missing_fields','{}')",&[]).is_err());
    assert!(verifier.query_one("SELECT weekly.record_ledger('reconciliation','paper_weekly_sql_nulls','paper','missing_fields','{}')",&[]).is_err());
    // A valid prospective forecast remains recordable through the SQL boundary.
    let forecast_id = "valid_sql_forecast";
    let canonical=serde_json::to_string(&json!({"metadata":{"prediction_id":forecast_id,"generated_at":Utc::now()-Duration::seconds(1),"horizon":{"deadline_at":Utc::now()+Duration::hours(1)}}})).unwrap();
    analyst
        .query_one(
            "SELECT weekly.record_ledger('forecast','paper_weekly_sql_nulls','paper',$1,$2)",
            &[&forecast_id, &canonical],
        )
        .unwrap();
    assert!(
        verifier
            .query_one(
                "SELECT weekly.record_ledger('label','paper_weekly_sql_nulls','paper',$1,'{}')",
                &[&forecast_id]
            )
            .is_err()
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn release_sql_null_transitions_and_expired_runtime_readiness_fail_closed() {
    migrate(&dsn("trady_migrator")).unwrap();
    let state = initial("paper_release_sql_nulls");
    let candidate = ReleaseCandidate {
        revision_id: "sql_expiry_fixture".into(),
        account_id: state.manifest.account_id.clone(),
        mode: Mode::Paper,
        owner_id: "sql_fixture_owner".into(),
        policy_hash: digest(&state.policy).unwrap(),
        formula_hash: digest(&state.plan.candidates[0]).unwrap(),
        source_hash: "a".repeat(64),
        calendar_hash: "b".repeat(64),
        model_hashes: BTreeMap::from([("fixture".into(), "c".repeat(64))]),
        parameters: state.plan.candidates[0].clone(),
        envelope: state.plan.envelope,
        effective_from: Utc::now() - Duration::hours(1),
        valid_until: Utc::now() + Duration::days(1),
        supersedes_release_id: None,
    };
    let mut lab = PgReleaseStore::connect(&dsn("trady_lab"), "trady_lab").unwrap();
    let id = lab.candidate(&candidate).unwrap();
    let mut releaser = Client::connect(&dsn("trady_releaser"), NoTls).unwrap();
    for query in [
        "SELECT release.advance($1,NULL,'VERIFIED','{}')",
        "SELECT release.advance($1,0,NULL,'{}')",
        "SELECT release.advance($1,0,'VERIFIED',NULL)",
        "SELECT release.advance_v6($1,0,'VERIFIED','{}')",
    ] {
        assert!(releaser.query_one(query, &[&id]).is_err(), "{query}");
    }
    let mut verifier = PgReleaseStore::connect(&dsn("trady_verifier"), "trady_verifier").unwrap();
    let mut readiness = evidence_envelope(&candidate, "readiness", Utc::now()).unwrap();
    readiness["evidence_class"] = json!("PRODUCTION");
    readiness["ready"] = json!(true);
    readiness["valid_until"] = json!(Utc::now() + Duration::minutes(2));
    readiness["account_id"] = json!(candidate.account_id);
    readiness["mode"] = json!("paper");
    readiness["snapshot_hash"] = json!("d".repeat(64));
    readiness["capabilities_hash"] = json!("e".repeat(64));
    let readiness_id = verifier.record_evidence("readiness", &readiness).unwrap();
    let mut wrong = readiness.clone();
    wrong["ready"] = serde_json::Value::Null;
    assert!(verifier.record_evidence("readiness", &wrong).is_err());
    wrong = readiness.clone();
    wrong["account_id"] = json!("paper_wrong_scope");
    assert!(verifier.record_evidence("readiness", &wrong).is_err());
    // Administrator-only fixture construction isolates the read-time gate. It
    // neither simulates a production promotion nor grants service write access.
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    let owner_id = format!("evidence_{}", "f".repeat(64));
    let mut approval =
        evidence_envelope(&candidate, "owner", Utc::now() - Duration::hours(2)).unwrap();
    for (key, value) in [
        ("owner_id", json!(candidate.owner_id)),
        ("account_id", json!(candidate.account_id)),
        ("mode", json!("paper")),
        ("policy_hash", json!(candidate.policy_hash)),
        ("decision", json!("APPROVE")),
        ("authority", json!("OWNER_LIVE")),
        ("valid_until", json!(Utc::now() + Duration::hours(4))),
    ] {
        approval[key] = value;
    }
    let owner_canonical = serde_json::to_string(&approval).unwrap();
    let subject = candidate.subject_hash().unwrap();
    admin.execute("INSERT INTO release.evidence(evidence_id,subject_hash,kind,canonical,content_hash,producer) VALUES($1,$2,'owner',$3,weekly.hash($3),'trady_release_owner')",&[&owner_id,&subject,&owner_canonical]).unwrap();
    admin.execute("UPDATE release.candidates SET state='ACTIVE',evidence=jsonb_build_object('owner',$2::text,'readiness',$3::text) WHERE release_id=$1",&[&id,&owner_id,&readiness_id]).unwrap();
    admin
        .execute(
            "INSERT INTO release.active(account_id,mode,release_id) VALUES($1,'paper',$2)",
            &[&candidate.account_id.as_str(), &id],
        )
        .unwrap();
    let mut executor = PgActiveReleaseReader::connect(&dsn("trady_executor")).unwrap();
    assert!(
        executor
            .resolve_active(&candidate.account_id, Mode::Paper)
            .is_ok()
    );
    readiness["valid_until"] = json!(Utc::now() - Duration::seconds(1));
    let expired = serde_json::to_string(&readiness).unwrap();
    admin.execute("UPDATE release.evidence SET canonical=$1,content_hash=weekly.hash($1) WHERE evidence_id=$2",&[&expired,&readiness_id]).unwrap();
    assert!(
        executor
            .resolve_active(&candidate.account_id, Mode::Paper)
            .is_err()
    );
    // Missing expiry must fail in the same path, even with a matching content hash.
    readiness.as_object_mut().unwrap().remove("valid_until");
    let missing = serde_json::to_string(&readiness).unwrap();
    admin.execute("UPDATE release.evidence SET canonical=$1,content_hash=weekly.hash($1) WHERE evidence_id=$2",&[&missing,&readiness_id]).unwrap();
    assert!(
        executor
            .resolve_active(&candidate.account_id, Mode::Paper)
            .is_err()
    );
    readiness["recorded_at"] = json!(Utc::now());
    readiness["valid_until"] = json!(Utc::now() + Duration::minutes(2));
    let renewed = verifier.record_evidence("readiness", &readiness).unwrap();
    let resolved = executor
        .resolve_active(&candidate.account_id, Mode::Paper)
        .unwrap();
    assert_eq!(resolved["evidence"]["readiness"], renewed);
    readiness["recorded_at"] = json!(Utc::now());
    readiness["ready"] = json!(false);
    verifier.record_evidence("readiness", &readiness).unwrap();
    assert!(
        executor
            .resolve_active(&candidate.account_id, Mode::Paper)
            .is_err()
    );
}

fn active_release_fixture(account: &str, revision: &str) -> (String, serde_json::Value) {
    let state = initial(account);
    let candidate = ReleaseCandidate {
        revision_id: revision.into(),
        account_id: state.manifest.account_id.clone(),
        mode: Mode::Paper,
        owner_id: "fixed_snapshot_fixture_owner".into(),
        policy_hash: digest(&state.policy).unwrap(),
        formula_hash: digest(&state.plan.candidates[0]).unwrap(),
        source_hash: "a".repeat(64),
        calendar_hash: "b".repeat(64),
        model_hashes: BTreeMap::from([("fixture".into(), "c".repeat(64))]),
        parameters: state.plan.candidates[0].clone(),
        envelope: state.plan.envelope,
        effective_from: Utc::now() - Duration::hours(1),
        valid_until: Utc::now() + Duration::days(1),
        supersedes_release_id: None,
    };
    let mut lab = PgReleaseStore::connect(&dsn("trady_lab"), "trady_lab").unwrap();
    let id = lab.candidate(&candidate).unwrap();
    let mut verifier = PgReleaseStore::connect(&dsn("trady_verifier"), "trady_verifier").unwrap();
    let mut readiness = evidence_envelope(&candidate, "readiness", Utc::now()).unwrap();
    for (key, value) in [
        ("evidence_class", json!("PRODUCTION")),
        ("ready", json!(true)),
        ("valid_until", json!(Utc::now() + Duration::minutes(2))),
        ("account_id", json!(candidate.account_id)),
        ("mode", json!("paper")),
        ("snapshot_hash", json!("d".repeat(64))),
        ("capabilities_hash", json!("e".repeat(64))),
    ] {
        readiness[key] = value;
    }
    let readiness_id = verifier.record_evidence("readiness", &readiness).unwrap();
    let mut approval =
        evidence_envelope(&candidate, "owner", Utc::now() - Duration::hours(2)).unwrap();
    for (key, value) in [
        ("owner_id", json!(candidate.owner_id)),
        ("account_id", json!(candidate.account_id)),
        ("mode", json!("paper")),
        ("policy_hash", json!(candidate.policy_hash)),
        ("decision", json!("APPROVE")),
        ("authority", json!("OWNER_LIVE")),
        ("valid_until", json!(Utc::now() + Duration::hours(4))),
    ] {
        approval[key] = value;
    }
    // Disposable administrator construction tests only runtime authority lookup;
    // it neither grants service writes nor bypasses the production promotion API.
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    let owner_id = format!("evidence_{}", digest(&approval).unwrap());
    let subject = candidate.subject_hash().unwrap();
    let canonical = serde_json::to_string(&approval).unwrap();
    admin.execute("INSERT INTO release.evidence(evidence_id,subject_hash,kind,canonical,content_hash,producer) VALUES($1,$2,'owner',$3,weekly.hash($3),'trady_release_owner')",&[&owner_id,&subject,&canonical]).unwrap();
    admin.execute("UPDATE release.candidates SET state='ACTIVE',evidence=jsonb_build_object('owner',$2::text,'readiness',$3::text) WHERE release_id=$1",&[&id,&owner_id,&readiness_id]).unwrap();
    admin.execute("INSERT INTO release.active(account_id,mode,release_id) VALUES($1,'paper',$2) ON CONFLICT(account_id,mode) DO UPDATE SET release_id=excluded.release_id",&[&account,&id]).unwrap();
    (id, readiness)
}

fn assert_release_requires_read_committed(error: postgres::Error) {
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

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn release_fixed_snapshots_cannot_reuse_readiness_or_replaced_active_pointer() {
    migrate(&dsn("trady_migrator")).unwrap();
    let account = "paper_release_fixed_snapshots";
    let (id, mut readiness) = active_release_fixture(account, "fixed_snapshot_first");
    let (replacement, _) = active_release_fixture(account, "fixed_snapshot_replacement");
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    let mut verifier = PgReleaseStore::connect(&dsn("trady_verifier"), "trady_verifier").unwrap();
    let mut current = Client::connect(&dsn("trady_executor"), NoTls).unwrap();
    for transition in ["negative_readiness", "pointer_replacement"] {
        admin
            .execute(
                "UPDATE release.active SET release_id=$1 WHERE account_id=$2 AND mode='paper'",
                &[&id, &account],
            )
            .unwrap();
        readiness["recorded_at"] = json!(Utc::now());
        readiness["valid_until"] = json!(Utc::now() + Duration::minutes(2));
        readiness["ready"] = json!(true);
        verifier.record_evidence("readiness", &readiness).unwrap();
        let resolved: serde_json::Value = current
            .query_one("SELECT release.current_active($1,'paper')", &[&account])
            .unwrap()
            .get(0);
        assert_eq!(resolved["release_id"], id);
        let mut snapshots = Vec::new();
        for isolation in ["REPEATABLE READ", "SERIALIZABLE"] {
            let mut snapshot = Client::connect(&dsn("trady_executor"), NoTls).unwrap();
            snapshot
                .batch_execute(&format!("BEGIN ISOLATION LEVEL {isolation}"))
                .unwrap();
            // Establish the executor's old snapshot while authority is positive.
            // The guarded API deliberately disallows even its first fixed-snapshot
            // authority lookup; a harmless SELECT pins the counterexample instead.
            snapshot.query_one("SELECT 1", &[]).unwrap();
            snapshots.push(snapshot);
        }
        if transition == "negative_readiness" {
            readiness["recorded_at"] = json!(Utc::now());
            readiness["ready"] = json!(false);
            verifier.record_evidence("readiness", &readiness).unwrap();
        } else {
            admin
                .execute(
                    "UPDATE release.active SET release_id=$1 WHERE account_id=$2 AND mode='paper'",
                    &[&replacement, &account],
                )
                .unwrap();
        }
        for mut snapshot in snapshots {
            assert_release_requires_read_committed(
                snapshot
                    .query_one("SELECT release.current_active($1,'paper')", &[&account])
                    .unwrap_err(),
            );
            snapshot.batch_execute("ROLLBACK").unwrap();
            let result =
                snapshot.query_one("SELECT release.current_active($1,'paper')", &[&account]);
            if transition == "negative_readiness" {
                assert_eq!(
                    result.unwrap_err().as_db_error().unwrap().message(),
                    "ACTIVE_RELEASE_AUTHORITY_UNAVAILABLE"
                );
            } else {
                assert_eq!(
                    result.unwrap().get::<_, serde_json::Value>(0)["release_id"],
                    replacement
                );
            }
        }
    }
    for isolation in ["REPEATABLE READ", "SERIALIZABLE"] {
        let mut releaser = Client::connect(&dsn("trady_releaser"), NoTls).unwrap();
        releaser
            .batch_execute(&format!("BEGIN ISOLATION LEVEL {isolation}"))
            .unwrap();
        assert_release_requires_read_committed(
            releaser
                .query_one("SELECT release.advance($1,0,'ACTIVE','{}')", &[&id])
                .unwrap_err(),
        );
        releaser.batch_execute("ROLLBACK").unwrap();
    }
}

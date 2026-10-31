//! Local helper/store fixtures exercise workflow decisions, not proofs or backtests.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::{cell::Cell, collections::BTreeMap};
use trady_application::{Clock, optimization::*};
use trady_domain::{AccountId, Error, Mode, Result, require};
use trady_policy::{Policy, limits::*};

fn d(value: &str) -> Decimal {
    Decimal::from_str_exact(value).unwrap()
}
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .unwrap()
        .to_utc()
}
struct TestClock(Cell<DateTime<Utc>>);
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.get()
    }
}
fn clock() -> TestClock {
    TestClock(Cell::new(now()))
}
fn session(id: &str, opens_at: DateTime<Utc>, closes_at: DateTime<Utc>) -> SessionInterval {
    SessionInterval {
        session_id: id.into(),
        opens_at,
        closes_at,
    }
}
fn initial() -> WeeklyState {
    let plan = ExperimentPlan {
        campaign_id: "fixture_campaign".into(),
        hypothesis: "Synthetic bounded parameter comparison".into(),
        envelope: FormulaEnvelope {
            max_entry_allowance_fraction: d("0.005"),
            min_stop_atr_multiple: Decimal::ONE,
            max_stop_atr_multiple: d("4"),
            min_target_net_fraction: d("0.01"),
            max_target_net_fraction: d("0.03"),
        },
        candidates: vec![FormulaParameters {
            version: 1,
            family: FormulaFamily::Trend,
            entry_allowance_fraction: d("0.001"),
            stop_atr_multiple: d("2"),
            target_net_fraction: d("0.03"),
            max_holding_sessions: 3,
            atr_lookback: 14,
            bar_seconds: 60,
        }],
        max_cumulative_trials: 2,
        wall_clock_seconds: 3600,
        objective: Objective {
            drawdown_weight: d("0.1"),
            instability_weight: d("0.2"),
            complexity_weight: Decimal::ZERO,
        },
        minimum_improvement: d("0.001"),
        min_outer_intervals: 6,
        min_closed_trades: 100,
        min_shadow_weeks: 4,
        min_shadow_closed_trades: 20,
        development_folds: vec![
            session(
                "older_fold",
                now() - Duration::days(60),
                now() - Duration::days(40),
            ),
            session(
                "newer_fold",
                now() - Duration::days(30),
                now() - Duration::days(10),
            ),
        ],
        development_fold_sessions: 14,
    };
    let mut component_hashes: BTreeMap<_, _> = [
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
    .map(|key| (key.into(), "a".repeat(64)))
    .collect();
    component_hashes.insert("objective".into(), digest(&plan.objective).unwrap());
    component_hashes.insert("splits".into(), plan.splits_hash().unwrap());
    component_hashes.insert("gates".into(), plan.gates_hash().unwrap());
    component_hashes.insert("policy".into(), digest(&Policy::default()).unwrap());
    let manifest = RunManifest {
        schema_version: 1,
        account_id: AccountId::new("paper_weekly_fixture").unwrap(),
        mode: Mode::Paper,
        source_commit: "b".repeat(40),
        source_clean: true,
        champion_id: "current_champion".into(),
        dataset_hash: "c".repeat(64),
        cutoff_at: now() - Duration::hours(1),
        completed_sessions: vec![session(
            "last_session",
            now() - Duration::days(1),
            now() - Duration::hours(2),
        )],
        next_session: session(
            "next_session",
            now() + Duration::days(2),
            now() + Duration::days(2) + Duration::hours(6),
        ),
        activation_deadline: now() + Duration::days(1),
        component_hashes,
        random_seed: 42,
    };
    new_weekly_state(manifest, plan, Policy::default(), now()).unwrap()
}

#[derive(Default)]
struct Store {
    states: BTreeMap<String, WeeklyState>,
    campaign_counts: BTreeMap<String, u32>,
    lease_lost: bool,
}
impl WeeklyStore for Store {
    fn freeze(&mut self, state: &WeeklyState) -> Result<()> {
        require(!self.lease_lost, "fixture lease lost")?;
        let id = state.manifest.run_id()?;
        if let Some(old) = self.states.get(&id) {
            require(
                old.manifest == state.manifest
                    && old.plan == state.plan
                    && old.policy == state.policy,
                "fixture frozen inputs changed",
            )
        } else {
            require(
                state.trials.is_empty()
                    && state.reconciliation.is_none()
                    && state.verification.is_none()
                    && state.evaluation.is_none()
                    && state.decision.is_none(),
                "fixture untrusted prepopulated evidence",
            )?;
            self.states.insert(id, state.clone());
            Ok(())
        }
    }
    fn load(&mut self, id: &str) -> Result<WeeklyState> {
        self.states
            .get(id)
            .cloned()
            .ok_or_else(|| Error("fixture run missing".into()))
    }
    fn reserve_trial(
        &mut self,
        id: &str,
        trial: &TrialRecord,
        cumulative_limit: u32,
    ) -> Result<()> {
        require(!self.lease_lost, "fixture lease lost")?;
        let state = self
            .states
            .get_mut(id)
            .ok_or_else(|| Error("fixture run missing".into()))?;
        let count = self
            .campaign_counts
            .entry(state.plan.campaign_id.clone())
            .or_default();
        require(
            cumulative_limit == state.plan.max_cumulative_trials
                && *count < cumulative_limit
                && !state
                    .trials
                    .iter()
                    .any(|old| old.candidate_id == trial.candidate_id),
            "fixture cumulative trial budget exhausted",
        )?;
        state.trials.push(trial.clone());
        *count += 1;
        Ok(())
    }
    fn checkpoint(&mut self, id: &str, state: &WeeklyState) -> Result<()> {
        require(!self.lease_lost, "fixture lease lost")?;
        let old = self
            .states
            .get(id)
            .ok_or_else(|| Error("fixture run missing".into()))?;
        require(
            old.manifest == state.manifest
                && old.plan == state.plan
                && old.policy == state.policy
                && old.started_at == state.started_at
                && old.trials.len() == state.trials.len(),
            "fixture immutable input or reservation changed",
        )?;
        for (before, after) in old.trials.iter().zip(&state.trials) {
            require(
                before.candidate_id == after.candidate_id
                    && before.parameters == after.parameters
                    && before.registered_at == after.registered_at
                    && before
                        .development
                        .as_ref()
                        .is_none_or(|value| Some(value) == after.development.as_ref())
                    && before
                        .failure
                        .as_ref()
                        .is_none_or(|value| Some(value) == after.failure.as_ref()),
                "fixture completed trial rewritten",
            )?;
        }
        self.states.insert(id.into(), state.clone());
        Ok(())
    }
}
struct Lab {
    calls: u32,
    return_score: Decimal,
    shadow_weeks: u32,
    proof_ok: bool,
    reuse_old_report: bool,
}
impl Default for Lab {
    fn default() -> Self {
        Self {
            calls: 0,
            return_score: d("0.01"),
            shadow_weeks: 4,
            proof_ok: true,
            reuse_old_report: false,
        }
    }
}
fn report(id: &str, run: &RunManifest, plan: &ExperimentPlan, trial: &TrialRecord) -> ToolReport {
    ToolReport {
        report_id: id.into(),
        tool_id: "fixture_helper".into(),
        input_hash: trial_input_hash(run, plan, trial).unwrap(),
        output_hash: "d".repeat(64),
        exit_status: 0,
        duration_ms: 1,
        recorded_at: now(),
    }
}
impl WeeklyLab for Lab {
    fn reconcile(&mut self, run: &RunManifest) -> Result<ReconciledWeek> {
        self.calls += 1;
        Ok(ReconciledWeek {
            reconciliation_id: "fixture_reconciliation".into(),
            run_id: run.run_id()?,
            cutoff_at: run.cutoff_at,
            unresolved_count: 0,
            champion_objective: Decimal::ZERO,
            cash_objective: Decimal::ZERO,
            closed_trades: 1,
            open_positions: 1,
            pending_labels: 1,
        })
    }
    fn develop(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<DevelopmentReport> {
        self.calls += 1;
        let mut report = report("fixture_development", run, plan, trial);
        if self.reuse_old_report {
            report.input_hash = digest(&trial.parameters)?;
        }
        Ok(DevelopmentReport {
            report,
            folds: plan
                .development_folds
                .iter()
                .map(|interval| FoldMetrics {
                    interval: interval.clone(),
                    mean_log_return: self.return_score,
                    max_drawdown: d("0.001"),
                    closed_trades: 10,
                    complete: true,
                })
                .collect(),
        })
    }
    fn verify(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<VerificationReport> {
        self.calls += 1;
        Ok(VerificationReport {
            proof: report("fixture_proof", run, plan, trial),
            parity: report("fixture_parity", run, plan, trial),
            approved_axioms_only: true,
            non_vacuous: self.proof_ok,
            exact_numeric_parity: true,
            safety_definitions_unchanged: true,
        })
    }
    fn evaluate(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<IndependentEvaluation> {
        self.calls += 1;
        Ok(IndependentEvaluation {
            report: report("fixture_independent_evaluation", run, plan, trial),
            run_id: run.run_id()?,
            dataset_hash: run.dataset_hash.clone(),
            plan_hash: digest(plan)?,
            outer_intervals: 6,
            closed_trades: 100,
            completed_shadow_weeks: self.shadow_weeks,
            shadow_closed_trades: 20,
            shadow_report_id: Some("fixture_shadow".into()),
            cost_stress_multiple: d("2"),
            improvement_lower_bound: d("0.005"),
            dependence_and_search_adjusted: true,
            causal_and_uncontaminated: true,
            risk_not_deteriorated: true,
            economic_feasibility_passed: true,
            neighborhood_checks_passed: true,
        })
    }
}

#[test]
fn candidate_decisions_never_authorize_entry_or_activation() {
    for (score, shadow_weeks, proof_ok, expected) in [
        (d("0.01"), 4, true, WeeklyDecisionKind::ApprovalRequired),
        (d("0.01"), 0, true, WeeklyDecisionKind::ShadowCandidate),
        (d("-0.01"), 4, true, WeeklyDecisionKind::Keep),
        (d("0.01"), 4, false, WeeklyDecisionKind::Blocked),
    ] {
        let mut lab = Lab {
            return_score: score,
            shadow_weeks,
            proof_ok,
            ..Lab::default()
        };
        let decision = run_weekly(&initial(), &mut Store::default(), &mut lab, &clock()).unwrap();
        assert_eq!(decision.decision, expected);
        assert!(!decision.entry_authorized && !decision.live_activation_authorized);
    }
}

#[test]
fn duplicate_run_does_not_reconsume_trials_or_evaluation() {
    let initial = initial();
    let mut store = Store::default();
    let mut lab = Lab::default();
    let first = run_weekly(&initial, &mut store, &mut lab, &clock()).unwrap();
    let calls = lab.calls;
    assert_eq!(
        run_weekly(&initial, &mut store, &mut lab, &clock()).unwrap(),
        first
    );
    assert_eq!(lab.calls, calls);
    assert_eq!(store.campaign_counts["fixture_campaign"], 1);
}

#[test]
fn resume_keeps_first_budget_and_does_not_replay_interrupted_trial() {
    let initial = initial();
    let id = initial.manifest.run_id().unwrap();
    let mut store = Store::default();
    store.freeze(&initial).unwrap();
    let parameters = initial.plan.candidates[0].clone();
    store
        .reserve_trial(
            &id,
            &TrialRecord {
                candidate_id: format!("candidate_{}", digest(&parameters).unwrap()),
                parameters,
                registered_at: now(),
                development: None,
                failure: None,
            },
            2,
        )
        .unwrap();
    let mut lab = Lab::default();
    let result = run_weekly(&initial, &mut store, &mut lab, &clock()).unwrap();
    assert_eq!(result.decision, WeeklyDecisionKind::Keep);
    assert_eq!(lab.calls, 1); // reconciliation only
    assert_eq!(
        store.states[&id].trials[0].failure.as_deref(),
        Some("INTERRUPTED_TRIAL_NOT_REPLAYED")
    );

    let mut store = Store::default();
    store.freeze(&initial).unwrap();
    let later = TestClock(Cell::new(now() + Duration::seconds(3601)));
    let mut retry = initial.clone();
    retry.started_at = later.now();
    let mut lab = Lab::default();
    assert_eq!(
        run_weekly(&retry, &mut store, &mut lab, &later)
            .unwrap()
            .decision,
        WeeklyDecisionKind::Blocked
    );
    assert_eq!(lab.calls, 0);
}

#[test]
fn related_runs_share_trial_budget_and_lost_lease_forbids_writes() {
    let initial = initial();
    let mut store = Store::default();
    store
        .campaign_counts
        .insert(initial.plan.campaign_id.clone(), 2);
    let decision = run_weekly(&initial, &mut store, &mut Lab::default(), &clock()).unwrap();
    assert_eq!(decision.decision, WeeklyDecisionKind::Blocked);
    assert!(store.states.values().all(|s| s.trials.is_empty()));
    let mut store = Store {
        lease_lost: true,
        ..Store::default()
    };
    assert!(run_weekly(&initial, &mut store, &mut Lab::default(), &clock()).is_err());
}

#[test]
fn frozen_gates_and_full_run_hash_reject_reused_evidence() {
    let mut changed = initial();
    changed.plan.min_closed_trades += 1;
    assert!(new_weekly_state(changed.manifest, changed.plan, changed.policy, now()).is_err());
    let mut lab = Lab {
        reuse_old_report: true,
        ..Lab::default()
    };
    let mut store = Store::default();
    assert_eq!(
        run_weekly(&initial(), &mut store, &mut lab, &clock())
            .unwrap()
            .decision,
        WeeklyDecisionKind::Keep
    );
    assert_eq!(
        store.states.values().next().unwrap().trials[0]
            .failure
            .as_deref(),
        Some("INVALID_DEVELOPMENT_REPORT")
    );
    let mut changed = initial();
    changed.plan.development_folds.truncate(1);
    assert!(changed.plan.validate(&changed.policy).is_err());
}

#[test]
fn deposits_are_not_performance_and_unvalued_gaps_reject() {
    let first = EquitySubperiod {
        opens_at: now(),
        closes_at: now() + Duration::hours(1),
        opening_equity: d("100"),
        closing_equity: d("200"),
        boundary_cash_flow: d("100"),
    };
    let second = EquitySubperiod {
        opens_at: first.closes_at,
        closes_at: now() + Duration::hours(2),
        opening_equity: d("200"),
        closing_equity: d("220"),
        boundary_cash_flow: Decimal::ZERO,
    };
    assert_eq!(
        time_weighted_return(std::slice::from_ref(&first)).unwrap(),
        Decimal::ZERO
    );
    assert_eq!(
        time_weighted_return(&[first.clone(), second.clone()]).unwrap(),
        d("0.1")
    );
    let mut invalid = second;
    invalid.opening_equity = d("201");
    assert!(time_weighted_return(&[first, invalid]).is_err());
}

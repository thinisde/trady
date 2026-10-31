//! Bounded, resumable offline workflow. Ports must resolve deployment-owned artifacts.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trady_domain::{AccountId, Error, Mode, Result, require};
use trady_policy::{
    Policy, add, div,
    limits::{FormulaEnvelope, FormulaParameters},
    mul, sub,
};

use super::{digest, valid_hash, valid_id};
use crate::Clock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionInterval {
    pub session_id: String,
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunManifest {
    pub schema_version: u32,
    pub account_id: AccountId,
    pub mode: Mode,
    pub source_commit: String,
    pub source_clean: bool,
    pub champion_id: String,
    pub dataset_hash: String,
    pub cutoff_at: DateTime<Utc>,
    /// Resolved by an approved exchange calendar, including holidays and DST.
    pub completed_sessions: Vec<SessionInterval>,
    pub next_session: SessionInterval,
    pub activation_deadline: DateTime<Utc>,
    /// Exact hashes/identities for calendar, features, costs, tools and approvals.
    pub component_hashes: BTreeMap<String, String>,
    pub random_seed: u64,
}

impl RunManifest {
    pub fn run_id(&self) -> Result<String> {
        Ok(format!("week_{}", digest(self)?))
    }

    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.schema_version == 1
                && valid_id(&self.champion_id)
                && valid_hash(&self.dataset_hash)
                && [40, 64].contains(&self.source_commit.len())
                && self.source_commit.bytes().all(|b| b.is_ascii_hexdigit())
                && !self.completed_sessions.is_empty()
                && self.completed_sessions.len() <= 14,
            "invalid weekly manifest identity or completed-session interval",
        )?;
        let prefix = match self.mode {
            Mode::Paper => "paper_",
            Mode::Shadow => "shadow_",
            Mode::Live => "live_",
        };
        require(
            self.account_id.as_str().starts_with(prefix),
            "weekly account/mode mismatch",
        )?;
        let mut previous_close = None;
        let mut ids = BTreeSet::new();
        for session in &self.completed_sessions {
            require(
                valid_id(&session.session_id)
                    && ids.insert(&session.session_id)
                    && session.opens_at < session.closes_at
                    && session.closes_at <= self.cutoff_at
                    && previous_close.is_none_or(|at| session.opens_at > at),
                "incomplete, duplicate or overlapping weekly exchange sessions",
            )?;
            previous_close = Some(session.closes_at);
        }
        require(
            valid_id(&self.next_session.session_id)
                && !ids.contains(&self.next_session.session_id)
                && self.cutoff_at <= now
                && self.cutoff_at < self.activation_deadline
                && self.activation_deadline <= self.next_session.opens_at
                && self.next_session.opens_at < self.next_session.closes_at,
            "invalid weekly cutoff or next-session deadline",
        )?;
        for key in [
            "calendar",
            "features",
            "models",
            "calibrators",
            "fusion",
            "labels",
            "policy",
            "costs",
            "rust_build",
            "engine",
            "driver",
            "lean_toolchain",
            "mathlib",
            "skill",
            "prompt",
            "objective",
            "splits",
            "gates",
        ] {
            require(
                self.component_hashes
                    .get(key)
                    .is_some_and(|v| valid_hash(v)),
                "weekly manifest is missing a pinned component hash",
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    pub drawdown_weight: Decimal,
    pub instability_weight: Decimal,
    pub complexity_weight: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentPlan {
    pub campaign_id: String,
    pub hypothesis: String,
    pub envelope: FormulaEnvelope,
    /// Frozen finite domain, at most a trend and a reversal family.
    pub candidates: Vec<FormulaParameters>,
    /// Includes failed and earlier related trials on reused data.
    pub max_cumulative_trials: u32,
    pub wall_clock_seconds: u32,
    pub objective: Objective,
    pub minimum_improvement: Decimal,
    pub min_outer_intervals: u32,
    pub min_closed_trades: u32,
    pub min_shadow_weeks: u32,
    pub min_shadow_closed_trades: u32,
    pub development_folds: Vec<SessionInterval>,
    /// Calendar-resolved counts, equal for each fold; wall-clock lengths may vary.
    pub development_fold_sessions: u16,
}

impl ExperimentPlan {
    pub fn validate(&self, policy: &Policy) -> Result<()> {
        self.envelope.validate(policy)?;
        require(
            valid_id(&self.campaign_id)
                && !self.hypothesis.trim().is_empty()
                && self.hypothesis.len() <= 4000
                && !self.candidates.is_empty()
                && self.candidates.len() <= 32
                && (1..=32).contains(&self.max_cumulative_trials)
                && (1..=86_400).contains(&self.wall_clock_seconds)
                && self.minimum_improvement >= Decimal::ZERO
                && self.min_outer_intervals >= 6
                && self.min_closed_trades >= 100
                && self.min_shadow_weeks >= 4
                && self.min_shadow_closed_trades > 0,
            "invalid bounded experiment or acceptance policy",
        )?;
        require(
            self.objective.drawdown_weight >= Decimal::ZERO
                && self.objective.instability_weight >= Decimal::ZERO
                && self.objective.complexity_weight >= Decimal::ZERO,
            "negative objective penalty",
        )?;
        let mut seen = BTreeSet::new();
        for parameters in &self.candidates {
            parameters.validate(&self.envelope, policy)?;
            require(
                seen.insert(digest(parameters)?),
                "duplicate formula configuration",
            )?;
        }
        require(
            (2..=64).contains(&self.development_folds.len()) && self.development_fold_sessions > 0,
            "missing or unbounded development folds",
        )?;
        let mut end = None;
        for fold in &self.development_folds {
            require(
                valid_id(&fold.session_id)
                    && fold.opens_at < fold.closes_at
                    && end.is_none_or(|at| fold.opens_at > at),
                "development folds overlap or are not chronological",
            )?;
            end = Some(fold.closes_at);
        }
        require(
            self.development_folds.last().is_some_and(|last| {
                last.closes_at - self.development_folds[0].opens_at > chrono::Duration::days(7)
            }),
            "strategy selection requires more than one week of history",
        )?;
        Ok(())
    }

    /// Acceptance settings are protected inputs, separate from tunable parameters.
    pub fn gates_hash(&self) -> Result<String> {
        digest(&(
            &self.envelope,
            self.minimum_improvement,
            self.min_outer_intervals,
            self.min_closed_trades,
            self.min_shadow_weeks,
            self.min_shadow_closed_trades,
            self.max_cumulative_trials,
            self.wall_clock_seconds,
        ))
    }

    pub fn splits_hash(&self) -> Result<String> {
        digest(&(&self.development_folds, self.development_fold_sessions))
    }
}

/// Produced by a deterministic evaluator, never copied from an agent's summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolReport {
    pub report_id: String,
    pub tool_id: String,
    pub input_hash: String,
    pub output_hash: String,
    pub exit_status: i32,
    pub duration_ms: u64,
    pub recorded_at: DateTime<Utc>,
}
impl ToolReport {
    fn validate(&self, input_hash: &str, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_id(&self.report_id)
                && valid_id(&self.tool_id)
                && self.input_hash == input_hash
                && valid_hash(&self.output_hash)
                && self.exit_status == 0
                && self.recorded_at <= now,
            "failed, mismatched or missing trusted tool report",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoldMetrics {
    pub interval: SessionInterval,
    /// Evaluator computes mean(log(1 + net session return)), including cash days.
    /// Returns at or below -100%, missing labels and partial folds are invalid.
    pub mean_log_return: Decimal,
    pub max_drawdown: Decimal,
    pub closed_trades: u32,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentReport {
    pub report: ToolReport,
    pub folds: Vec<FoldMetrics>,
}

pub fn development_objective(report: &DevelopmentReport, plan: &ExperimentPlan) -> Result<Decimal> {
    require(
        !report.folds.is_empty() && report.folds.len() == plan.development_folds.len(),
        "incomplete development folds",
    )?;
    let mut means = Vec::new();
    let mut max_drawdown = Decimal::ZERO;
    for (fold, expected) in report.folds.iter().zip(&plan.development_folds) {
        require(
            fold.interval == *expected
                && fold.complete
                && fold.max_drawdown >= Decimal::ZERO
                && fold.max_drawdown <= Decimal::ONE,
            "invalid, incomplete or differently split development report",
        )?;
        means.push(fold.mean_log_return);
        max_drawdown = max_drawdown.max(fold.max_drawdown);
    }
    means.sort();
    let middle = means.len() / 2;
    let median = if means.len() % 2 == 0 {
        div(add(means[middle - 1], means[middle])?, Decimal::from(2))?
    } else {
        means[middle]
    };
    // Fixed nearest-rank quartiles; the convention is part of the frozen objective.
    let iqr = sub(
        means[(3 * (means.len() - 1)) / 4],
        means[(means.len() - 1) / 4],
    )?;
    sub(
        sub(
            sub(median, mul(plan.objective.drawdown_weight, max_drawdown)?)?,
            mul(plan.objective.instability_weight, iqr)?,
        )?,
        mul(plan.objective.complexity_weight, Decimal::from(7))?,
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciledWeek {
    pub reconciliation_id: String,
    pub run_id: String,
    pub cutoff_at: DateTime<Utc>,
    pub unresolved_count: u32,
    pub champion_objective: Decimal,
    pub cash_objective: Decimal,
    /// Open holdings and pending outcome labels are explicitly excluded from wins.
    pub closed_trades: u32,
    pub open_positions: u32,
    pub pending_labels: u32,
}

/// Trusted verifier checks pinned theorem/definition hashes, transitive axioms,
/// non-vacuity witnesses and exact Lean/Rust/LEAN wire values independently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub proof: ToolReport,
    pub parity: ToolReport,
    pub approved_axioms_only: bool,
    pub non_vacuous: bool,
    pub exact_numeric_parity: bool,
    pub safety_definitions_unchanged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependentEvaluation {
    pub report: ToolReport,
    pub run_id: String,
    pub dataset_hash: String,
    pub plan_hash: String,
    pub outer_intervals: u32,
    pub closed_trades: u32,
    pub completed_shadow_weeks: u32,
    pub shadow_closed_trades: u32,
    pub shadow_report_id: Option<String>,
    pub cost_stress_multiple: Decimal,
    pub improvement_lower_bound: Decimal,
    pub dependence_and_search_adjusted: bool,
    pub causal_and_uncontaminated: bool,
    pub risk_not_deteriorated: bool,
    pub economic_feasibility_passed: bool,
    pub neighborhood_checks_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialRecord {
    pub candidate_id: String,
    pub parameters: FormulaParameters,
    pub registered_at: DateTime<Utc>,
    pub development: Option<DevelopmentReport>,
    pub failure: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WeeklyDecisionKind {
    Keep,
    ShadowCandidate,
    ApprovalRequired,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeeklyDecision {
    pub schema_version: u32,
    pub run_id: String,
    pub as_of: DateTime<Utc>,
    pub cutoff_at: DateTime<Utc>,
    pub decision: WeeklyDecisionKind,
    pub champion_id: String,
    pub candidate_id: Option<String>,
    pub reconciliation_id: Option<String>,
    pub experiment_ids: Vec<String>,
    pub verification_ids: Vec<String>,
    pub evaluation_id: Option<String>,
    pub reason_codes: Vec<String>,
    pub limitations: Vec<String>,
    pub next_session_id: String,
    pub entry_authorized: bool,
    pub live_activation_authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeeklyState {
    /// Frozen once. A resume never resets the elapsed-time budget.
    pub started_at: DateTime<Utc>,
    pub manifest: RunManifest,
    pub plan: ExperimentPlan,
    pub policy: Policy,
    pub reconciliation: Option<ReconciledWeek>,
    pub trials: Vec<TrialRecord>,
    pub verification: Option<VerificationReport>,
    pub evaluation: Option<IndependentEvaluation>,
    pub decision: Option<WeeklyDecision>,
}

/// Must lock one run, atomically freeze immutable inputs, persist each reserved
/// trial before execution and enforce cumulative campaign counts across runs.
/// Implementations use their own clock/fencing; losing the lease forbids saves.
/// Freeze compares manifest/plan/policy on retries, retains the first started_at,
/// and refuses prepopulated results on the first invocation. Checkpoints cannot
/// rewrite frozen inputs, remove trials or replace completed evidence.
pub trait WeeklyStore {
    fn freeze(&mut self, state: &WeeklyState) -> Result<()>;
    fn load(&mut self, run_id: &str) -> Result<WeeklyState>;
    fn reserve_trial(
        &mut self,
        run_id: &str,
        trial: &TrialRecord,
        cumulative_limit: u32,
    ) -> Result<()>;
    fn checkpoint(&mut self, run_id: &str, state: &WeeklyState) -> Result<()>;
}

/// Deployment-configured helpers only: no shell fragments, arbitrary dataset
/// paths, executable candidates, production credentials or broker mutation port.
/// These are trusted artifacts, not model-declared pass flags. Sealed evaluation
/// requests are idempotent and irrevocably consume the frozen candidate's trial.
pub trait WeeklyLab {
    fn reconcile(&mut self, run: &RunManifest) -> Result<ReconciledWeek>;
    fn develop(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<DevelopmentReport>;
    fn verify(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<VerificationReport>;
    fn evaluate(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<IndependentEvaluation>;
}

/// Every helper result is bound to the full frozen run, plan and parameters.
/// Matching parameter values alone do not make an old experiment independent.
pub fn trial_input_hash(
    run: &RunManifest,
    plan: &ExperimentPlan,
    trial: &TrialRecord,
) -> Result<String> {
    digest(&(run, plan, &trial.parameters))
}

pub fn new_weekly_state(
    manifest: RunManifest,
    plan: ExperimentPlan,
    policy: Policy,
    now: DateTime<Utc>,
) -> Result<WeeklyState> {
    manifest.validate(now)?;
    plan.validate(&policy)?;
    require(
        digest(&plan.objective)? == manifest.component_hashes["objective"]
            && plan.splits_hash()? == manifest.component_hashes["splits"]
            && plan.gates_hash()? == manifest.component_hashes["gates"]
            && digest(&policy)? == manifest.component_hashes["policy"],
        "weekly objective/splits/gates differ from the frozen manifest",
    )?;
    require(
        plan.development_folds
            .iter()
            .all(|f| f.closes_at <= manifest.cutoff_at),
        "development folds extend beyond the frozen information cutoff",
    )?;
    Ok(WeeklyState {
        started_at: now,
        manifest,
        plan,
        policy,
        reconciliation: None,
        trials: vec![],
        verification: None,
        evaluation: None,
        decision: None,
    })
}

/// Never promotes a formula. The returned recommendation requires the independent
/// deployment release boundary, even when every evidence gate passes.
pub fn run_weekly(
    initial: &WeeklyState,
    store: &mut impl WeeklyStore,
    lab: &mut impl WeeklyLab,
    clock: &impl Clock,
) -> Result<WeeklyDecision> {
    new_weekly_state(
        initial.manifest.clone(),
        initial.plan.clone(),
        initial.policy.clone(),
        clock.now(),
    )?;
    let run_id = initial.manifest.run_id()?;
    store.freeze(initial)?;
    let mut state = store.load(&run_id)?;
    require(
        state.manifest == initial.manifest
            && state.plan == initial.plan
            && state.policy == initial.policy,
        "RESUMED_RUN_INPUTS_CHANGED",
    )?;
    if let Some(decision) = &state.decision {
        require(
            decision.run_id == run_id
                && !decision.entry_authorized
                && !decision.live_activation_authorized,
            "INVALID_CACHED_RESEARCH_DECISION",
        )?;
        return Ok(decision.clone());
    }
    let started = state.started_at;
    let mut decision = WeeklyDecision {
        schema_version: 1,
        run_id: run_id.clone(),
        as_of: started,
        cutoff_at: state.manifest.cutoff_at,
        decision: WeeklyDecisionKind::Blocked,
        champion_id: state.manifest.champion_id.clone(),
        candidate_id: None,
        reconciliation_id: None,
        experiment_ids: vec![],
        verification_ids: vec![],
        evaluation_id: None,
        reason_codes: vec![],
        limitations: vec![
            "Independent proof, data and execution assumptions do not guarantee returns.".into(),
        ],
        next_session_id: state.manifest.next_session.session_id.clone(),
        entry_authorized: false,
        live_activation_authorized: false,
    };
    let outcome = run_stages(&mut state, store, lab, clock, started, &mut decision);
    if let Err(error) = outcome {
        decision.decision = WeeklyDecisionKind::Blocked;
        decision.reason_codes.push(error.0);
    }
    decision.as_of = clock.now();
    decision.experiment_ids = state
        .trials
        .iter()
        .map(|t| t.candidate_id.clone())
        .collect();
    state.decision = Some(decision.clone());
    store.checkpoint(&run_id, &state)?;
    Ok(decision)
}

fn within_budget(state: &WeeklyState, started: DateTime<Utc>, now: DateTime<Utc>) -> Result<()> {
    require(
        now >= started
            && (now - started).num_seconds() < i64::from(state.plan.wall_clock_seconds)
            && now < state.manifest.activation_deadline,
        "BUDGET_OR_ACTIVATION_DEADLINE_EXCEEDED",
    )
}

fn run_stages(
    state: &mut WeeklyState,
    store: &mut impl WeeklyStore,
    lab: &mut impl WeeklyLab,
    clock: &impl Clock,
    started: DateTime<Utc>,
    decision: &mut WeeklyDecision,
) -> Result<()> {
    let run_id = state.manifest.run_id()?;
    within_budget(state, started, clock.now())?;
    if state.reconciliation.is_none() {
        state.reconciliation = Some(lab.reconcile(&state.manifest)?);
        store.checkpoint(&run_id, state)?;
    }
    let reconciliation = state
        .reconciliation
        .as_ref()
        .ok_or_else(|| Error("RECONCILIATION_MISSING".into()))?;
    require(
        valid_id(&reconciliation.reconciliation_id)
            && reconciliation.run_id == run_id
            && reconciliation.cutoff_at == state.manifest.cutoff_at
            && reconciliation.unresolved_count == 0,
        "RECONCILIATION_UNRESOLVED_OR_MISMATCHED",
    )?;
    decision.reconciliation_id = Some(reconciliation.reconciliation_id.clone());
    let baseline = reconciliation
        .champion_objective
        .max(reconciliation.cash_objective);
    for parameters in state.plan.candidates.clone() {
        within_budget(state, started, clock.now())?;
        let candidate_id = format!("candidate_{}", digest(&parameters)?);
        if let Some(existing) = state
            .trials
            .iter_mut()
            .find(|t| t.candidate_id == candidate_id)
        {
            if existing.development.is_none() && existing.failure.is_none() {
                // A crashed external call might have incurred cost. Never repeat it silently.
                existing.failure = Some("INTERRUPTED_TRIAL_NOT_REPLAYED".into());
                store.checkpoint(&run_id, state)?;
            }
            continue;
        }
        let mut trial = TrialRecord {
            candidate_id,
            parameters,
            registered_at: clock.now(),
            development: None,
            failure: None,
        };
        store.reserve_trial(&run_id, &trial, state.plan.max_cumulative_trials)?;
        state.trials.push(trial.clone());
        match lab.develop(&state.manifest, &state.plan, &trial) {
            Ok(report) => {
                if report
                    .report
                    .validate(
                        &trial_input_hash(&state.manifest, &state.plan, &trial)?,
                        clock.now(),
                    )
                    .is_ok()
                    && development_objective(&report, &state.plan).is_ok()
                {
                    trial.development = Some(report);
                } else {
                    trial.failure = Some("INVALID_DEVELOPMENT_REPORT".into());
                }
            }
            Err(_) => trial.failure = Some("DEVELOPMENT_HELPER_FAILED".into()),
        }
        *state
            .trials
            .last_mut()
            .ok_or_else(|| Error("TRIAL_RESERVATION_LOST".into()))? = trial;
        store.checkpoint(&run_id, state)?;
    }
    within_budget(state, started, clock.now())?;
    let mut best: Option<(Decimal, TrialRecord)> = None;
    for trial in &state.trials {
        if let Some(report) = &trial.development {
            report.report.validate(
                &trial_input_hash(&state.manifest, &state.plan, trial)?,
                clock.now(),
            )?;
            let score = development_objective(report, &state.plan)?;
            if score > add(baseline, state.plan.minimum_improvement)?
                && best.as_ref().is_none_or(|(current, _)| score > *current)
            {
                best = Some((score, trial.clone()));
            }
        }
    }
    let Some((_, candidate)) = best else {
        decision.decision = WeeklyDecisionKind::Keep;
        decision
            .reason_codes
            .push("NO_CANDIDATE_BEATS_CHAMPION_AND_CASH".into());
        return Ok(());
    };
    decision.candidate_id = Some(candidate.candidate_id.clone());
    let candidate_hash = trial_input_hash(&state.manifest, &state.plan, &candidate)?;
    if state.verification.is_none() {
        state.verification = Some(lab.verify(&state.manifest, &state.plan, &candidate)?);
        store.checkpoint(&run_id, state)?;
    }
    within_budget(state, started, clock.now())?;
    let verification = state
        .verification
        .as_ref()
        .ok_or_else(|| Error("VERIFICATION_MISSING".into()))?;
    verification.proof.validate(&candidate_hash, clock.now())?;
    verification.parity.validate(&candidate_hash, clock.now())?;
    decision.verification_ids = vec![
        verification.proof.report_id.clone(),
        verification.parity.report_id.clone(),
    ];
    require(
        verification.approved_axioms_only
            && verification.non_vacuous
            && verification.exact_numeric_parity
            && verification.safety_definitions_unchanged,
        "PROOF_OR_NUMERIC_PARITY_FAILED",
    )?;
    if state.evaluation.is_none() {
        state.evaluation = Some(lab.evaluate(&state.manifest, &state.plan, &candidate)?);
        store.checkpoint(&run_id, state)?;
    }
    within_budget(state, started, clock.now())?;
    let evaluation = state
        .evaluation
        .as_ref()
        .ok_or_else(|| Error("EVALUATION_MISSING".into()))?;
    evaluation.report.validate(&candidate_hash, clock.now())?;
    decision.evaluation_id = Some(evaluation.report.report_id.clone());
    require(
        evaluation.run_id == run_id
            && evaluation.dataset_hash == state.manifest.dataset_hash
            && evaluation.plan_hash == digest(&state.plan)?,
        "INDEPENDENT_EVALUATION_BINDING_MISMATCH",
    )?;
    require(
        evaluation.causal_and_uncontaminated
            && evaluation.dependence_and_search_adjusted
            && evaluation.risk_not_deteriorated
            && evaluation.neighborhood_checks_passed
            && evaluation.cost_stress_multiple >= Decimal::from(2),
        "INDEPENDENT_ROBUSTNESS_GATES_FAILED",
    )?;
    if evaluation.improvement_lower_bound <= state.plan.minimum_improvement {
        decision.decision = WeeklyDecisionKind::Keep;
        decision
            .reason_codes
            .push("INDEPENDENT_IMPROVEMENT_NOT_ESTABLISHED".into());
    } else if evaluation.outer_intervals < state.plan.min_outer_intervals
        || evaluation.closed_trades < state.plan.min_closed_trades
        || evaluation.completed_shadow_weeks < state.plan.min_shadow_weeks
        || evaluation.shadow_closed_trades < state.plan.min_shadow_closed_trades
        || evaluation
            .shadow_report_id
            .as_ref()
            .is_none_or(|id| !valid_id(id))
    {
        decision.decision = WeeklyDecisionKind::ShadowCandidate;
        decision
            .reason_codes
            .push("INSUFFICIENT_INDEPENDENT_OR_PROSPECTIVE_OUTCOMES".into());
    } else if !state.manifest.source_clean || !evaluation.economic_feasibility_passed {
        return Err(Error(
            "RELEASE_SOURCE_OR_ECONOMIC_FEASIBILITY_FAILED".into(),
        ));
    } else {
        decision.decision = WeeklyDecisionKind::ApprovalRequired;
        decision
            .reason_codes
            .push("OWNER_DEPLOYMENT_APPROVAL_REQUIRED".into());
    }
    Ok(())
}

/// Cash flows occur at the explicitly valued subperiod boundary. Deposits and
/// withdrawals are removed before chaining; an open position remains unrealized.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquitySubperiod {
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
    pub opening_equity: Decimal,
    pub closing_equity: Decimal,
    pub boundary_cash_flow: Decimal,
}

pub fn time_weighted_return(periods: &[EquitySubperiod]) -> Result<Decimal> {
    require(
        !periods.is_empty() && periods.len() <= 10_000,
        "missing or unbounded equity subperiods",
    )?;
    let mut wealth = Decimal::ONE;
    let mut previous: Option<&EquitySubperiod> = None;
    for p in periods {
        require(
            p.opens_at < p.closes_at
                && p.opening_equity > Decimal::ZERO
                && previous.is_none_or(|old| {
                    old.closes_at == p.opens_at && old.closing_equity == p.opening_equity
                }),
            "equity subperiods are not continuously valued",
        )?;
        let adjusted_close = sub(p.closing_equity, p.boundary_cash_flow)?;
        require(
            adjusted_close > Decimal::ZERO,
            "undefined equity return at or below -100%",
        )?;
        wealth = mul(wealth, div(adjusted_close, p.opening_equity)?)?;
        previous = Some(p);
    }
    sub(wealth, Decimal::ONE)
}

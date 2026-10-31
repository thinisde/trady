//! Fenced offline experiments and deployment-owned release evidence.
//! The database functions, rather than the caller, own budgets and transitions.

use crate::{connect_local, db, verify_role};
use chrono::{DateTime, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use trady_application::optimization::*;
use trady_domain::{prediction::Prediction, *};
use trady_policy::limits::{FormulaEnvelope, FormulaParameters};

pub const WEEKLY_LAB_SQL: &str = include_str!("../../../migrations/0006_weekly_lab.sql");
const MAX_DOCUMENT: usize = 4 * 1024 * 1024;

fn canonical(value: &impl Serialize) -> Result<String> {
    // Value gives all callers the same sorted-object representation.
    let value =
        serde_json::to_value(value).map_err(|_| Error("weekly document encoding failed".into()))?;
    let text = serde_json::to_string(&value)
        .map_err(|_| Error("weekly document encoding failed".into()))?;
    require(text.len() <= MAX_DOCUMENT, "weekly document exceeds limit")?;
    Ok(text)
}
fn encode(value: &impl Serialize) -> Result<Value> {
    serde_json::from_str(&canonical(value)?).map_err(|_| Error("weekly encoding failed".into()))
}
fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|_| Error("invalid persisted weekly document".into()))
}
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Paper => "paper",
        Mode::Shadow => "shadow",
        Mode::Live => "live",
    }
}
fn connect_role(dsn: &str, role: &str) -> Result<Client> {
    let mut client = connect_local(dsn)?;
    verify_role(&mut client, role)?;
    client
        .batch_execute("SET statement_timeout='15s'; SET lock_timeout='5s'")
        .map_err(db)?;
    Ok(client)
}

pub struct PgWeeklyStore {
    client: Rc<RefCell<Client>>,
    generation: Rc<Cell<i64>>,
    run_id: Rc<RefCell<Option<String>>>,
    worker: String,
    ttl_seconds: i32,
}
impl PgWeeklyStore {
    pub fn connect(dsn: &str, worker: &str, ttl_seconds: i32) -> Result<Self> {
        require(
            valid_id(worker) && (1..=3600).contains(&ttl_seconds),
            "invalid weekly worker/lease",
        )?;
        Ok(Self {
            client: Rc::new(RefCell::new(connect_role(dsn, "trady_lab")?)),
            generation: Rc::new(Cell::new(0)),
            run_id: Rc::new(RefCell::new(None)),
            worker: worker.into(),
            ttl_seconds,
        })
    }
    /// Shares the *same database session and fence*, without giving the lab SQL access.
    pub fn holdouts(&self) -> PgHoldoutLedger {
        PgHoldoutLedger {
            client: self.client.clone(),
            generation: self.generation.clone(),
            run_id: self.run_id.clone(),
        }
    }
    pub fn renew(&mut self) -> Result<()> {
        let id = self.current_id()?;
        self.client
            .borrow_mut()
            .query_one(
                "SELECT weekly.renew($1,$2,$3)",
                &[&id, &self.generation.get(), &self.ttl_seconds],
            )
            .map_err(db)?;
        Ok(())
    }
    pub fn release(&mut self) -> Result<()> {
        let id = self.current_id()?;
        self.client
            .borrow_mut()
            .query_one(
                "SELECT weekly.release_run($1,$2)",
                &[&id, &self.generation.get()],
            )
            .map_err(db)?;
        self.generation.set(0);
        Ok(())
    }
    fn current_id(&self) -> Result<String> {
        self.run_id
            .borrow()
            .clone()
            .filter(|_| self.generation.get() > 0)
            .ok_or_else(|| Error("weekly run has no fence".into()))
    }
    fn check_id(&self, id: &str) -> Result<()> {
        require(self.current_id()? == id, "weekly run/fence mismatch")
    }
}
impl WeeklyStore for PgWeeklyStore {
    fn freeze(&mut self, state: &WeeklyState) -> Result<()> {
        // Reuse application validation, even for direct adapter callers.
        new_weekly_state(
            state.manifest.clone(),
            state.plan.clone(),
            state.policy.clone(),
            Utc::now(),
        )?;
        let id = state.manifest.run_id()?;
        require(
            self.run_id.borrow().as_ref().is_none_or(|old| old == &id),
            "one store cannot switch weekly runs",
        )?;
        let row = self
            .client
            .borrow_mut()
            .query_one(
                "SELECT weekly.freeze($1,$2,$3,$4)",
                &[&id, &encode(state)?, &self.worker, &self.ttl_seconds],
            )
            .map_err(db)?;
        self.generation.set(row.get(0));
        *self.run_id.borrow_mut() = Some(id);
        Ok(())
    }
    fn load(&mut self, run_id: &str) -> Result<WeeklyState> {
        self.check_id(run_id)?;
        let row = self
            .client
            .borrow_mut()
            .query_one(
                "SELECT weekly.load_run($1,$2)",
                &[&run_id, &self.generation.get()],
            )
            .map_err(db)?;
        decode(row.get(0))
    }
    fn reserve_trial(
        &mut self,
        run_id: &str,
        trial: &TrialRecord,
        cumulative_limit: u32,
    ) -> Result<()> {
        self.check_id(run_id)?;
        let limit =
            i32::try_from(cumulative_limit).map_err(|_| Error("trial limit overflow".into()))?;
        self.client
            .borrow_mut()
            .query_one(
                "SELECT weekly.reserve_trial($1,$2,$3,$4)",
                &[&run_id, &self.generation.get(), &encode(trial)?, &limit],
            )
            .map_err(db)?;
        Ok(())
    }
    fn checkpoint(&mut self, run_id: &str, state: &WeeklyState) -> Result<()> {
        self.check_id(run_id)?;
        self.client
            .borrow_mut()
            .query_one(
                "SELECT weekly.checkpoint($1,$2,$3)",
                &[&run_id, &self.generation.get(), &encode(state)?],
            )
            .map_err(db)?;
        self.renew()
    }
}

#[derive(Clone)]
pub struct PgHoldoutLedger {
    client: Rc<RefCell<Client>>,
    generation: Rc<Cell<i64>>,
    run_id: Rc<RefCell<Option<String>>>,
}
impl PgHoldoutLedger {
    /// NULL means the reservation just committed. A previous unfinished reservation
    /// fails closed; it is never silently replayed after a crash or timeout.
    pub fn claim(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<Option<IndependentEvaluation>> {
        let id = run.run_id()?;
        require(
            self.run_id.borrow().as_ref() == Some(&id),
            "holdout fence mismatch",
        )?;
        let row = self
            .client
            .borrow_mut()
            .query_one(
                "SELECT weekly.claim_holdout($1,$2,$3,$4)",
                &[
                    &id,
                    &self.generation.get(),
                    &trial.candidate_id,
                    &trial_input_hash(run, plan, trial)?,
                ],
            )
            .map_err(db)?;
        row.get::<_, Option<Value>>(0).map(decode).transpose()
    }
    pub fn finish(&mut self, run_id: &str, report: &IndependentEvaluation) -> Result<()> {
        require(
            self.run_id.borrow().as_deref() == Some(run_id),
            "holdout fence mismatch",
        )?;
        self.client
            .borrow_mut()
            .query_one(
                "SELECT weekly.finish_holdout($1,$2,$3)",
                &[&run_id, &self.generation.get(), &encode(report)?],
            )
            .map_err(db)?;
        Ok(())
    }
}

pub struct PgForecastLedger {
    client: Client,
}
impl PgForecastLedger {
    pub fn connect(dsn: &str, role: &str) -> Result<Self> {
        require(
            matches!(
                role,
                "trady_lab" | "trady_verifier" | "trady_shadow" | "trady_analyst"
            ),
            "unknown forecast-ledger role",
        )?;
        Ok(Self {
            client: connect_role(dsn, role)?,
        })
    }
    pub fn forecast(
        &mut self,
        account: &AccountId,
        mode: Mode,
        prediction: &Prediction,
    ) -> Result<String> {
        prediction.validate(Utc::now())?;
        self.record(
            "forecast",
            account,
            mode,
            &prediction.metadata.prediction_id,
            prediction,
        )
    }
    pub fn label(
        &mut self,
        account: &AccountId,
        mode: Mode,
        id: &str,
        label: &ForecastLabel,
    ) -> Result<String> {
        require(
            valid_hash(&label.forecast_hash)
                && valid_hash(&label.source_hash)
                && label.observed_through <= Utc::now()
                && valid_id(&label.label_version),
            "invalid forecast label",
        )?;
        self.record("label", account, mode, id, label)
    }
    pub fn reconciliation(&mut self, export: &ReconciledExport) -> Result<String> {
        export.validate()?;
        self.record(
            "reconciliation",
            &export.account_id,
            export.mode,
            &export.export_id,
            export,
        )
    }
    fn record(
        &mut self,
        kind: &str,
        account: &AccountId,
        mode: Mode,
        id: &str,
        value: &impl Serialize,
    ) -> Result<String> {
        let row = self
            .client
            .query_one(
                "SELECT weekly.record_ledger($1,$2,$3,$4,$5)",
                &[
                    &kind,
                    &account.as_str(),
                    &mode_name(mode),
                    &id,
                    &canonical(value)?,
                ],
            )
            .map_err(db)?;
        Ok(row.get(0))
    }
    /// Frozen as-of export, including unmatched predictions and unrealized holdings.
    pub fn export(
        &mut self,
        account: &AccountId,
        mode: Mode,
        cutoff: DateTime<Utc>,
    ) -> Result<Value> {
        require(cutoff <= Utc::now(), "future ledger cutoff")?;
        Ok(self
            .client
            .query_one(
                "SELECT weekly.export_ledger($1,$2,$3)",
                &[&account.as_str(), &mode_name(mode), &cutoff],
            )
            .map_err(db)?
            .get(0))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastLabel {
    pub forecast_hash: String,
    pub label_version: String,
    pub observed_through: DateTime<Utc>,
    pub source_hash: String,
    /// A versioned observed outcome; missing outcomes are kept pending, not zero-filled.
    pub outcome: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciledExport {
    pub export_id: String,
    pub account_id: AccountId,
    pub mode: Mode,
    pub cutoff_at: DateTime<Utc>,
    pub source_hashes: Vec<String>,
    pub report: ReconciledWeek,
    pub equity: Vec<EquitySubperiod>,
    pub open_holdings: Value,
    pub pending_orders: Value,
    pub unresolved_items: Vec<String>,
}
impl ReconciledExport {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_id(&self.export_id)
                && !self.source_hashes.is_empty()
                && self.source_hashes.iter().all(|h| valid_hash(h))
                && self.cutoff_at <= Utc::now()
                && self.report.cutoff_at == self.cutoff_at
                && usize::try_from(self.report.unresolved_count).ok()
                    == Some(self.unresolved_items.len())
                && self
                    .equity
                    .last()
                    .is_some_and(|p| p.closes_at == self.cutoff_at),
            "incomplete reconciled export",
        )?;
        time_weighted_return(&self.equity)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCandidate {
    pub revision_id: String,
    pub account_id: AccountId,
    pub mode: Mode,
    pub owner_id: String,
    pub policy_hash: String,
    pub formula_hash: String,
    pub source_hash: String,
    pub calendar_hash: String,
    pub model_hashes: BTreeMap<String, String>,
    pub parameters: FormulaParameters,
    pub envelope: FormulaEnvelope,
    pub effective_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub supersedes_release_id: Option<String>,
}
impl ReleaseCandidate {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_id(&self.revision_id)
                && valid_id(&self.owner_id)
                && [
                    self.policy_hash.as_str(),
                    self.formula_hash.as_str(),
                    self.source_hash.as_str(),
                    self.calendar_hash.as_str(),
                ]
                .iter()
                .all(|h| valid_hash(h))
                && self.formula_hash == digest(&self.parameters)?
                && self.effective_from < self.valid_until
                && !self.model_hashes.is_empty()
                && self
                    .model_hashes
                    .iter()
                    .all(|(id, h)| valid_id(id) && valid_hash(h))
                && self
                    .supersedes_release_id
                    .as_ref()
                    .is_none_or(|s| valid_id(s)),
            "invalid release candidate",
        )
    }
    pub fn subject_hash(&self) -> Result<String> {
        digest(&encode(self)?)
    }
}

pub struct PgReleaseStore {
    client: Client,
}
/// Runtime authority resolution needs only the executor's existing DSN.
/// This reader has no release mutation or attestation privilege.
pub struct PgActiveReleaseReader {
    client: Client,
}
impl PgActiveReleaseReader {
    pub fn connect(dsn: &str) -> Result<Self> {
        Ok(Self {
            client: connect_role(dsn, "trady_executor")?,
        })
    }
    pub fn resolve_active(&mut self, account: &AccountId, mode: Mode) -> Result<Value> {
        let row = self
            .client
            .query_one(
                "SELECT release.current_active($1,$2)",
                &[&account.as_str(), &mode_name(mode)],
            )
            .map_err(db)?;
        Ok(row.get(0))
    }
}
impl PgReleaseStore {
    pub fn connect(dsn: &str, role: &str) -> Result<Self> {
        require(
            matches!(
                role,
                "trady_lab"
                    | "trady_verifier"
                    | "trady_shadow"
                    | "trady_releaser"
                    | "trady_release_owner"
            ),
            "unknown release role",
        )?;
        Ok(Self {
            client: connect_role(dsn, role)?,
        })
    }
    pub fn candidate(&mut self, candidate: &ReleaseCandidate) -> Result<String> {
        candidate.validate()?;
        Ok(self
            .client
            .query_one("SELECT release.candidate($1)", &[&canonical(candidate)?])
            .map_err(db)?
            .get(0))
    }
    /// Only independent deployment roles can attest. Callers must run trusted
    /// verifiers; an analyst-supplied artifact ID or pass flag grants no access.
    pub fn record_evidence(&mut self, kind: &str, content: &Value) -> Result<String> {
        require(
            content.is_object() && content.get("kind").and_then(Value::as_str) == Some(kind),
            "invalid release evidence document",
        )?;
        Ok(self
            .client
            .query_one(
                "SELECT release.record_evidence($1,$2)",
                &[&kind, &canonical(content)?],
            )
            .map_err(db)?
            .get(0))
    }
    pub fn resolve(&mut self, id: &str) -> Result<Value> {
        require(valid_id(id), "invalid release id")?;
        Ok(self
            .client
            .query_one("SELECT release.resolve($1)", &[&id])
            .map_err(db)?
            .get(0))
    }
    pub fn advance(
        &mut self,
        id: &str,
        expected: i64,
        next: &str,
        evidence: &BTreeMap<String, String>,
    ) -> Result<Value> {
        require(
            valid_id(id) && expected >= 0 && evidence.values().all(|v| valid_id(v)),
            "invalid release transition",
        )?;
        Ok(self
            .client
            .query_one(
                "SELECT release.advance($1,$2,$3,$4)",
                &[&id, &expected, &next, &encode(evidence)?],
            )
            .map_err(db)?
            .get(0))
    }
    /// Resolves every gate again; this file is an output for runtime integration,
    /// never a way to turn a proposed candidate into an active authority.
    pub fn active_formula(&mut self, id: &str) -> Result<ApprovedFormula> {
        let resolved = self.resolve(id)?;
        require(
            resolved["state"] == "ACTIVE" && resolved["is_current_active"] == true,
            "release is not the current active strategy",
        )?;
        let c: ReleaseCandidate = decode(resolved["candidate"].clone())?;
        c.validate()?;
        let verification_id = resolved["evidence"]["verification"]
            .as_str()
            .ok_or_else(|| Error("verification missing".into()))?;
        let verification = &resolved["documents"][verification_id]["content"];
        let gate = |field: &str| -> Result<String> {
            verification[field]
                .as_str()
                .map(str::to_owned)
                .filter(|s| valid_id(s))
                .ok_or_else(|| Error("resolved release report missing".into()))
        };
        require(c.valid_until > Utc::now(), "active formula expired")?;
        Ok(
            ApprovedFormula {
                revision_id: c.revision_id,
                formula_hash: c.formula_hash,
                policy_hash: c.policy_hash,
                parameters: c.parameters,
                approved_at:
                    decode(
                        resolved["documents"][resolved["evidence"]["owner"].as_str().unwrap_or("")]
                            ["content"]["recorded_at"]
                            .clone(),
                    )?,
                effective_from: c.effective_from,
                valid_until: c.valid_until,
                proof_report_id: gate("proof_report_id")?,
                parity_report_id: gate("parity_report_id")?,
                evaluation_report_id: gate("evaluation_report_id")?,
                shadow_report_id: format!(
                    "shadow_{}",
                    resolved["subject_hash"].as_str().unwrap_or("")
                ),
                owner_approval_id: resolved["evidence"]["owner"]
                    .as_str()
                    .ok_or_else(|| Error("owner approval missing".into()))?
                    .into(),
            },
        )
    }
}

pub fn evidence_envelope(
    candidate: &ReleaseCandidate,
    kind: &str,
    now: DateTime<Utc>,
) -> Result<Value> {
    let hash = candidate.subject_hash()?;
    Ok(json!({"kind":kind,"subject_hash":hash,"scope_hash":hash,"recorded_at":now}))
}

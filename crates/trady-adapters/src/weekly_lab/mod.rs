//! Deployment-configured, broker-free weekly tools. No shell/candidate code execution.
pub mod runner;

use chrono::Utc;
use runner::{HelperCommand, HelperOutput};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use trady_application::optimization::*;
use trady_domain::{Error, Result, require};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabConfig {
    pub schema_version: u32,
    pub reconcile: HelperCommand,
    pub develop: HelperCommand,
    pub proof: HelperCommand,
    pub parity: HelperCommand,
    pub evaluate: HelperCommand,
    pub approved_axioms: Vec<String>,
    pub required_theorems: Vec<String>,
    pub protected_safety_hash: String,
}
impl LabConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 1
                && valid_hash(&self.protected_safety_hash)
                && !self.required_theorems.is_empty()
                && self.required_theorems.iter().all(|t| valid_id(t))
                && !self.approved_axioms.iter().any(|a| a.contains("sorryAx")),
            "invalid weekly lab trust configuration",
        )?;
        for command in [
            &self.reconcile,
            &self.develop,
            &self.proof,
            &self.parity,
            &self.evaluate,
        ] {
            command.validate()?;
        }
        Ok(())
    }
}
pub struct HelperLab {
    config: LabConfig,
}
impl HelperLab {
    pub fn new(config: LabConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self { config })
    }
    pub fn config(&self) -> &LabConfig {
        &self.config
    }
    fn invoke<T: DeserializeOwned>(
        command: &HelperCommand,
        request: &Value,
        input_hash: &str,
    ) -> Result<(T, HelperOutput)> {
        let bytes = serde_json::to_vec(request)
            .map_err(|_| Error("weekly request encoding failed".into()))?;
        let output = command.run(&bytes, input_hash)?;
        let payload = output.payload(input_hash)?;
        Ok((payload, output))
    }
    pub fn verify_candidate(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<VerificationReport> {
        let hash = trial_input_hash(run, plan, trial)?;
        let request = request("verify", run, plan, trial, &hash)?;
        let (proof, proof_output): (ProofPayload, _) =
            Self::invoke(&self.config.proof, &request, &hash)?;
        let (parity, parity_output): (ParityPayload, _) =
            Self::invoke(&self.config.parity, &request, &hash)?;
        let theorem_set: BTreeSet<_> = proof.theorems.iter().collect();
        require(
            self.config
                .required_theorems
                .iter()
                .all(|t| theorem_set.contains(t))
                && proof
                    .axioms
                    .iter()
                    .all(|a| self.config.approved_axioms.contains(a))
                && proof.witness_count > 0
                && proof.toolchain_hash == run.component_hashes["lean_toolchain"]
                && proof.safety_hash == self.config.protected_safety_hash
                && proof.source_hash == run.component_hashes["rust_build"]
                && parity.source_hash == proof.source_hash
                && parity.safety_hash == proof.safety_hash
                && !parity.cases.is_empty()
                && parity.cases.len() <= 10000
                && parity.cases.iter().all(|c| c.rust == c.lean),
            "PROOF_OR_PARITY_ARTIFACT_INVALID",
        )?;
        // Qualification requires the reviewed full formula reference scope.
        // Finite executable parity retains the translation/compiler trust
        // boundary recorded by the pinned verifier.
        let complete = proof.scope == "FORMULA_V1" && parity.scope == "FORMULA_V1";
        Ok(VerificationReport {
            proof: proof_output.report,
            parity: parity_output.report,
            approved_axioms_only: true,
            non_vacuous: true,
            exact_numeric_parity: complete,
            safety_definitions_unchanged: true,
        })
    }
}
fn request(
    stage: &str,
    run: &RunManifest,
    plan: &ExperimentPlan,
    trial: &TrialRecord,
    hash: &str,
) -> Result<Value> {
    Ok(
        json!({"schema_version":1,"stage":stage,"input_hash":hash,"manifest":run,"plan":plan,"trial":trial,
        "run_id":run.run_id()?,"plan_hash":digest(plan)?,"parameters_hash":digest(&trial.parameters)?}),
    )
}
impl WeeklyLab for HelperLab {
    fn reconcile(&mut self, run: &RunManifest) -> Result<ReconciledWeek> {
        let hash = digest(run)?;
        let request =
            json!({"schema_version":1,"stage":"reconcile","input_hash":hash,"manifest":run});
        let (report, _): (ReconciledWeek, _) =
            Self::invoke(&self.config.reconcile, &request, &hash)?;
        require(
            report.run_id == run.run_id()?
                && report.cutoff_at == run.cutoff_at
                && valid_id(&report.reconciliation_id),
            "misbound reconciliation export",
        )?;
        Ok(report)
    }
    fn develop(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<DevelopmentReport> {
        let hash = trial_input_hash(run, plan, trial)?;
        let (payload, output): (DevelopmentPayload, _) = Self::invoke(
            &self.config.develop,
            &request("develop", run, plan, trial, &hash)?,
            &hash,
        )?;
        let report = DevelopmentReport {
            report: output.report,
            folds: payload.folds,
        };
        development_objective(&report, plan)?;
        Ok(report)
    }
    fn verify(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<VerificationReport> {
        self.verify_candidate(run, plan, trial)
    }
    /// Production callers MUST put a durable holdout claim/finish around this call.
    /// The CLI DurableLab wrapper owns that boundary; this adapter has no database credential.
    fn evaluate(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<IndependentEvaluation> {
        let hash = trial_input_hash(run, plan, trial)?;
        let (mut payload, output): (EvaluationPayload, _) = Self::invoke(
            &self.config.evaluate,
            &request("evaluate", run, plan, trial, &hash)?,
            &hash,
        )?;
        payload.report = output.report;
        require(
            payload.run_id == run.run_id()?
                && payload.dataset_hash == run.dataset_hash
                && payload.plan_hash == digest(plan)?
                && payload.report.recorded_at <= Utc::now(),
            "misbound independent evaluation",
        )?;
        Ok(payload)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DevelopmentPayload {
    folds: Vec<FoldMetrics>,
}
type EvaluationPayload = IndependentEvaluation;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProofPayload {
    scope: String,
    source_hash: String,
    safety_hash: String,
    toolchain_hash: String,
    axioms: Vec<String>,
    theorems: Vec<String>,
    witness_count: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParityPayload {
    scope: String,
    source_hash: String,
    safety_hash: String,
    cases: Vec<ParityCase>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParityCase {
    rust: Value,
    lean: Value,
}

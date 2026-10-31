//! Separate deployment identities resolve content, verify, approve and activate.
use crate::weekly::{exact_options, option, parse_time, read};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use trady_adapters::{
    credentials,
    weekly_lab::runner::{HelperCommand, PinnedFile, trusted_file},
};
use trady_application::optimization::{valid_hash, valid_id};
use trady_domain::*;
use trady_storage::weekly::{PgReleaseStore, ReleaseCandidate, evidence_envelope};

pub const HELP: &str = "\nDeployment-owned release lifecycle:\n  trady release candidate --file FILE\n  trady release show|formula --id ID\n  trady release verify --candidate FILE --helper-config FILE\n  trady release evidence --kind calendar|session|readiness --file FILE\n  trady release approve --candidate FILE --valid-until RFC3339\n  trady release advance --id ID --revision N --state VERIFIED|SHADOW|APPROVED|ACTIVE --evidence FILE\nUses separate TRADY_LAB_DSN, TRADY_VERIFIER_DSN, TRADY_SHADOW_DSN, TRADY_RELEASE_OWNER_DSN and TRADY_RELEASE_DSN identities.\n";
fn store(variable: &str, role: &str) -> Result<PgReleaseStore> {
    let dsn = credentials::required(variable)?.into_inner();
    PgReleaseStore::connect(&dsn, role)
}
pub fn run(args: &[String]) -> Result<Value> {
    match args.first().map(String::as_str) {
        Some("candidate") => {
            exact_options(args, &["--file"])?;
            let candidate: ReleaseCandidate = read(Path::new(option(args, "--file")?))?;
            let id = store("TRADY_LAB_DSN", "trady_lab")?.candidate(&candidate)?;
            Ok(
                json!({"release_id":id,"subject_hash":candidate.subject_hash()?,"state":"CANDIDATE"}),
            )
        }
        Some("show" | "formula") => {
            exact_options(args, &["--id"])?;
            let id = option(args, "--id")?;
            let mut db = store("TRADY_RELEASE_DSN", "trady_releaser")?;
            if args[0] == "formula" {
                serde_json::to_value(db.active_formula(id)?)
                    .map_err(|_| Error("formula encoding failed".into()))
            } else {
                db.resolve(id)
            }
        }
        Some("verify") => {
            exact_options(args, &["--candidate", "--helper-config"])?;
            let candidate: ReleaseCandidate = read(Path::new(option(args, "--candidate")?))?;
            candidate.validate()?;
            let helper_path = Path::new(option(args, "--helper-config")?);
            trusted_file(helper_path)?;
            let helper: HelperCommand = read(helper_path)?;
            let hash = candidate.subject_hash()?;
            let request = json!({"schema_version":1,"stage":"release-verify","input_hash":hash,"candidate":candidate});
            let bytes = serde_json::to_vec(&request)
                .map_err(|_| Error("release verification encoding failed".into()))?;
            let result = helper.run(&bytes, &hash)?;
            let payload: IndependentReleaseVerification = result.payload(&hash)?;
            payload.validate(&candidate)?;
            let mut content = evidence_envelope(&candidate, "verification", Utc::now())?;
            for (key, value) in serde_json::to_value(&payload)
                .map_err(|_| Error("release evidence encoding failed".into()))?
                .as_object()
                .ok_or_else(|| Error("invalid verification payload".into()))?
            {
                content[key] = value.clone();
            }
            content["passed"] = json!(true);
            content["tool_output_hash"] = json!(result.report.output_hash);
            content["verifier_hash"] = json!(helper.executable.sha256);
            let id = store("TRADY_VERIFIER_DSN", "trady_verifier")?
                .record_evidence("verification", &content)?;
            Ok(json!({"evidence_id":id,"subject_hash":hash}))
        }
        Some("evidence") => {
            exact_options(args, &["--kind", "--file"])?;
            let kind = option(args, "--kind")?;
            let (variable, role) = match kind {
                "calendar" | "readiness" => ("TRADY_VERIFIER_DSN", "trady_verifier"),
                "session" => ("TRADY_SHADOW_DSN", "trady_shadow"),
                _ => return Err(Error("unsupported evidence kind".into())),
            };
            let content: Value = read(Path::new(option(args, "--file")?))?;
            let id = store(variable, role)?.record_evidence(kind, &content)?;
            Ok(json!({"evidence_id":id}))
        }
        Some("approve") => {
            exact_options(args, &["--candidate", "--valid-until"])?;
            let candidate: ReleaseCandidate = read(Path::new(option(args, "--candidate")?))?;
            candidate.validate()?;
            let until = parse_time(option(args, "--valid-until")?)?;
            require(
                until > Utc::now() && until <= candidate.valid_until,
                "owner approval expiry is outside release scope",
            )?;
            let mut content = evidence_envelope(&candidate, "owner", Utc::now())?;
            content["owner_id"] = json!(candidate.owner_id);
            content["account_id"] = json!(candidate.account_id);
            content["mode"] = json!(candidate.mode);
            content["policy_hash"] = json!(candidate.policy_hash);
            content["valid_until"] = json!(until);
            content["decision"] = json!("APPROVE");
            // This explicit owner-identity command grants live authority. Test/demo
            // assertions submitted with any other database role cannot reach it.
            content["authority"] = json!("OWNER_LIVE");
            let id = store("TRADY_RELEASE_OWNER_DSN", "trady_release_owner")?
                .record_evidence("owner", &content)?;
            Ok(json!({"owner_evidence_id":id,"active":false}))
        }
        Some("advance") => {
            exact_options(args, &["--id", "--revision", "--state", "--evidence"])?;
            let revision = option(args, "--revision")?
                .parse::<i64>()
                .map_err(|_| Error("invalid release revision".into()))?;
            let evidence: BTreeMap<String, String> = read(Path::new(option(args, "--evidence")?))?;
            store("TRADY_RELEASE_DSN", "trady_releaser")?.advance(
                option(args, "--id")?,
                revision,
                option(args, "--state")?,
                &evidence,
            )
        }
        _ => Err(Error(HELP.into())),
    }
}
#[derive(serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndependentReleaseVerification {
    source_hash: String,
    proof_hash: String,
    parity_hash: String,
    evaluation_hash: String,
    evidence_class: String,
    proof_report_id: String,
    parity_report_id: String,
    evaluation_report_id: String,
    scope: String,
    checks: BTreeMap<String, bool>,
    artifacts: Vec<PinnedFile>,
}
impl IndependentReleaseVerification {
    fn validate(&self, candidate: &ReleaseCandidate) -> Result<()> {
        require(
            self.scope == "FORMULA_V1"
                && self.evidence_class == "PRODUCTION"
                && self.source_hash == candidate.source_hash
                && [&self.proof_hash, &self.parity_hash, &self.evaluation_hash]
                    .iter()
                    .all(|h| valid_hash(h))
                && [
                    &self.proof_report_id,
                    &self.parity_report_id,
                    &self.evaluation_report_id,
                ]
                .iter()
                .all(|id| valid_id(id)),
            "incomplete independent release verification",
        )?;
        for check in [
            "clean_build",
            "approved_axioms",
            "non_vacuity",
            "full_numeric_parity",
            "protected_safety",
            "sealed_evaluation",
            "calibration",
            "schema_compatibility",
            "execution_protection",
            "source_permissions",
        ] {
            require(
                self.checks.get(check) == Some(&true),
                "independent release gate failed or unavailable",
            )?;
        }
        for hash in [&self.proof_hash, &self.parity_hash, &self.evaluation_hash] {
            let artifact = self
                .artifacts
                .iter()
                .find(|a| &a.sha256 == hash)
                .ok_or_else(|| Error("release evidence content missing".into()))?;
            artifact.validate()?;
        }
        Ok(())
    }
}

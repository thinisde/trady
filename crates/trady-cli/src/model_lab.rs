//! Offline model tooling. No API purchases, broker access, promotion or startup.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};
use trady_application::{
    model_lab::*,
    optimization::{BoundCalculation, FormulaAuthority, digest},
};
use trady_domain::{Error, Money, Result, require};
use trady_policy::{Policy, limits::LimitsInput};

pub const HELP: &str = "\n  trady model-lab fit --dataset FILE --plan FILE --out FILE\n  trady model-lab evaluate --candidate FILE --holdout FILE --journal DIR --out FILE\n  trady model-lab bundle --candidate FILE --evaluation FILE --approval FILE --out FILE\n  trady model-lab offline-infer --bundle FILE --request FILE --out FILE\n  trady model-lab inspect --candidate FILE\n\nFitting creates an unapproved candidate. Evaluation consumes a separate sealed holdout durably.\nBundle requires independently authored approval evidence; deployment must pin its hash.\nTEST ONLY data cannot receive production approval. Offline inference preserves its diagnostic wrapper.\n";

fn options(args: &[String], names: &[&str]) -> Result<BTreeMap<String, PathBuf>> {
    require(
        args.len() == names.len() * 2,
        "incorrect model-lab options; see help",
    )?;
    let mut values = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        require(
            names.contains(&pair[0].as_str())
                && !pair[1].is_empty()
                && values
                    .insert(pair[0].clone(), PathBuf::from(&pair[1]))
                    .is_none(),
            "unknown or duplicate model-lab option",
        )?;
    }
    Ok(values)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfflineRequest {
    authority: FormulaAuthority,
    policy: Policy,
    input: LimitsInput,
    calculation: BoundCalculation,
    observation: ForecastObservation,
    opening_equity: Money,
    as_of: DateTime<Utc>,
}
pub fn run(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error("model-lab command required".into()))?;
    match command {
        "help" | "--help" if args.len() == 1 => Ok(json!({"help":HELP})),
        "fit" => {
            let p = options(&args[1..], &["--dataset", "--plan", "--out"])?;
            let data: ObservationDataset = read_document(&p["--dataset"])?;
            let plan: TrainingPlan = read_document(&p["--plan"])?;
            require(plan.fit_at <= Utc::now(), "fit time is in the future")?;
            let candidate = fit(&data, &plan)?;
            write_immutable(&p["--out"], &candidate)?;
            Ok(
                json!({"status":"UNAPPROVED_CANDIDATE","path":p["--out"],"sha256":candidate.hash()?,
                "test_only":candidate.test_only,"development_eligible":candidate.development_eligible,
                "training_rows":candidate.training_rows,"calibration_samples":candidate.calibration.samples,
                "validation_samples":candidate.validation.samples,"purged_rows":candidate.purged_rows,"pending_rows":candidate.pending_rows}),
            )
        }
        "evaluate" => {
            let p = options(
                &args[1..],
                &["--candidate", "--holdout", "--journal", "--out"],
            )?;
            let candidate: ModelCandidate = read_document(&p["--candidate"])?;
            let holdout: ObservationDataset = read_document(&p["--holdout"])?;
            let report = EvaluationJournal::open(&p["--journal"])?.evaluate(
                &candidate,
                &holdout,
                Utc::now(),
            )?;
            write_immutable(&p["--out"], &report)?;
            Ok(
                json!({"status":"INDEPENDENT_EVALUATION_RECORDED","path":p["--out"],"sha256":digest(&report)?,
                "test_only":report.test_only,"eligible":report.eligible,"samples":report.metrics.samples,
                "brier":report.metrics.multiclass_brier,"log_loss":report.metrics.log_loss,
                "pending_or_unsupported":report.pending_or_unsupported}),
            )
        }
        "bundle" => {
            let p = options(
                &args[1..],
                &["--candidate", "--evaluation", "--approval", "--out"],
            )?;
            let model = ApprovedModel {
                candidate: read_document(&p["--candidate"])?,
                evaluation: read_document(&p["--evaluation"])?,
                approval: read_document(&p["--approval"])?,
            };
            model.validate(Utc::now())?;
            write_immutable(&p["--out"], &model)?;
            Ok(
                json!({"status":"REVIEWED_BUNDLE_REQUIRES_DEPLOYMENT_PIN","path":p["--out"],"sha256":digest(&model)?,
                "model_id":model.candidate.plan.model_id,"calibration_report_id":model.report_id()?,
                "payoff_estimation_report_id":model.estimation_id()?,"cost_model_hash":model.candidate.cost_model_hash}),
            )
        }
        "offline-infer" => {
            let p = options(&args[1..], &["--bundle", "--request", "--out"])?;
            let model: ApprovedModel = read_document(&p["--bundle"])?;
            let request: OfflineRequest = read_document(&p["--request"])?;
            let forecast = infer_offline(
                &model,
                &request.authority,
                &request.policy,
                &request.input,
                &request.calculation,
                &request.observation,
                &request.opening_equity,
                request.as_of,
            )?;
            write_immutable(&p["--out"], &forecast)?;
            Ok(
                json!({"status":"OFFLINE_DIAGNOSTIC_ONLY","test_only":forecast.test_only,"deployment_eligible":false,"path":p["--out"]}),
            )
        }
        "inspect" => {
            let p = options(&args[1..], &["--candidate"])?;
            let c: ModelCandidate = read_document(&p["--candidate"])?;
            c.validate()?;
            Ok(
                json!({"status":"UNAPPROVED_CANDIDATE","sha256":c.hash()?,"model_id":c.plan.model_id,"test_only":c.test_only,
                "feature_schema_version":c.feature_schema_version,"feature_names":c.feature_names,"label_version":c.label_version,
                "cost_model_hash":c.cost_model_hash,"validation":c.validation,"supported_regimes":c.supported_regimes,"supported_horizons":c.supported_horizons}),
            )
        }
        _ => Err(Error("unsupported model-lab command; see help".into())),
    }
}

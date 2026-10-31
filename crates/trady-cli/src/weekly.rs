//! CLI glue for the real offline workflow. Helpers receive no database secret.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};
use trady_adapters::{
    SystemClock, credentials,
    weekly_lab::{
        HelperLab, LabConfig,
        runner::{MAX_INPUT_BYTES, trusted_file},
    },
};
use trady_application::{contracts::parse_json_bounded, optimization::*};
use trady_domain::*;
use trady_policy::{Policy, limits::*};
use trady_storage::weekly::{PgForecastLedger, PgHoldoutLedger, PgWeeklyStore};

pub const HELP: &str = "\nOffline weekly lab:\n  trady weekly check|run --config FILE\n  trady weekly export --account ID --mode paper|shadow|live --cutoff RFC3339\n  trady weekly forecast --account ID --mode paper|shadow|live --file FILE\n  trady weekly bridge --file FILE|-\n  trady weekly arithmetic-bridge --file FILE|-\n  trady weekly formula-bridge --file FILE|-\nUses TRADY_LAB_DSN for run/export and TRADY_ANALYST_DSN for forecast. Helpers run with an empty secret environment.\n";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunConfig {
    schema_version: u32,
    worker_id: String,
    lease_seconds: i32,
    initial: WeeklyState,
    helpers: LabConfig,
}
struct DurableLab {
    helpers: HelperLab,
    holdouts: PgHoldoutLedger,
}
impl WeeklyLab for DurableLab {
    fn reconcile(&mut self, run: &RunManifest) -> Result<ReconciledWeek> {
        self.helpers.reconcile(run)
    }
    fn develop(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<DevelopmentReport> {
        self.helpers.develop(run, plan, trial)
    }
    fn verify(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<VerificationReport> {
        self.helpers.verify(run, plan, trial)
    }
    fn evaluate(
        &mut self,
        run: &RunManifest,
        plan: &ExperimentPlan,
        trial: &TrialRecord,
    ) -> Result<IndependentEvaluation> {
        if let Some(report) = self.holdouts.claim(run, plan, trial)? {
            return Ok(report);
        }
        let report = self.helpers.evaluate(run, plan, trial)?;
        self.holdouts.finish(&run.run_id()?, &report)?;
        Ok(report)
    }
}
pub fn run(args: &[String]) -> Result<Value> {
    let operation = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error(HELP.into()))?;
    match operation {
        "check" | "run" => {
            exact_options(args, &["--config"])?;
            let path = Path::new(option(args, "--config")?);
            trusted_file(path)?;
            let config: RunConfig = read(path)?;
            require(
                config.schema_version == 1
                    && valid_id(&config.worker_id)
                    && (1..=3600).contains(&config.lease_seconds),
                "invalid weekly run configuration",
            )?;
            config.helpers.validate()?;
            // Lease must outlast every bounded external call. DB clock still fences
            // a stalled caller; each successful checkpoint renews the lease.
            let longest = [
                &config.helpers.reconcile,
                &config.helpers.develop,
                &config.helpers.proof,
                &config.helpers.parity,
                &config.helpers.evaluate,
            ]
            .iter()
            .map(|c| c.timeout_seconds)
            .max()
            .unwrap_or(0);
            require(
                i64::from(config.lease_seconds) > i64::from(longest) * 2 + 30,
                "weekly lease too short for verification pair",
            )?;
            new_weekly_state(
                config.initial.manifest.clone(),
                config.initial.plan.clone(),
                config.initial.policy.clone(),
                Utc::now(),
            )?;
            if operation == "check" {
                return Ok(
                    json!({"status":"valid","run_id":config.initial.manifest.run_id()?,"entry_authorized":false}),
                );
            }
            let dsn = credentials::required("TRADY_LAB_DSN")?.into_inner();
            let mut store = PgWeeklyStore::connect(&dsn, &config.worker_id, config.lease_seconds)?;
            let mut lab = DurableLab {
                helpers: HelperLab::new(config.helpers)?,
                holdouts: store.holdouts(),
            };
            let outcome = run_weekly(&config.initial, &mut store, &mut lab, &SystemClock);
            let released = store.release();
            let decision = outcome?;
            released?;
            serde_json::to_value(decision)
                .map_err(|_| Error("weekly decision encoding failed".into()))
        }
        "export" => {
            exact_options(args, &["--account", "--mode", "--cutoff"])?;
            let account = AccountId::new(option(args, "--account")?)?;
            let mode = mode(option(args, "--mode")?)?;
            let cutoff = parse_time(option(args, "--cutoff")?)?;
            let dsn = credentials::required("TRADY_LAB_DSN")?.into_inner();
            PgForecastLedger::connect(&dsn, "trady_lab")?.export(&account, mode, cutoff)
        }
        "forecast" => {
            exact_options(args, &["--account", "--mode", "--file"])?;
            let account = AccountId::new(option(args, "--account")?)?;
            let mode = mode(option(args, "--mode")?)?;
            let prediction: trady_domain::prediction::Prediction =
                read(Path::new(option(args, "--file")?))?;
            let dsn = credentials::required("TRADY_ANALYST_DSN")?.into_inner();
            let hash = PgForecastLedger::connect(&dsn, "trady_analyst")?.forecast(
                &account,
                mode,
                &prediction,
            )?;
            Ok(json!({"prediction_id":prediction.metadata.prediction_id,"content_hash":hash}))
        }
        "bridge" => {
            exact_options(args, &["--file"])?;
            let request: FormulaBridgeRequest = read(Path::new(option(args, "--file")?))?;
            let limits = calculate_limits(
                &request.policy,
                &request.envelope,
                &request.parameters,
                request.max_account_risk_fraction,
                &request.input,
                request.as_of,
            )?;
            Ok(json!({"schema_version":1,"input_hash":digest(&request)?,"limits":limits}))
        }
        "arithmetic-bridge" => {
            exact_options(args, &["--file"])?;
            let request: ArithmeticRequest = read(Path::new(option(args, "--file")?))?;
            require(
                request.denominator > 0,
                "division denominator must be positive",
            )?;
            let floor = request.numerator / request.denominator;
            let ceil = floor
                .checked_add(u64::from(
                    !request.numerator.is_multiple_of(request.denominator),
                ))
                .ok_or_else(|| Error("rounding overflow".into()))?;
            Ok(json!({"floor":floor,"ceil":ceil,"cap":request.quantity.min(request.maximum)}))
        }
        "formula-bridge" => {
            exact_options(args, &["--file"])?;
            let request: trady_policy::limits::proof_bridge::Request =
                read(Path::new(option(args, "--file")?))?;
            serde_json::to_value(trady_policy::limits::proof_bridge::evaluate(&request))
                .map_err(|_| Error("formula bridge reply serialization failed".into()))
        }
        _ => Err(Error(HELP.into())),
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormulaBridgeRequest {
    policy: Policy,
    envelope: FormulaEnvelope,
    parameters: FormulaParameters,
    max_account_risk_fraction: rust_decimal::Decimal,
    input: LimitsInput,
    as_of: DateTime<Utc>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArithmeticRequest {
    numerator: u64,
    denominator: u64,
    quantity: u64,
    maximum: u64,
}
pub(crate) fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        std::io::stdin()
            .take(MAX_INPUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error("input read failed".into()))?;
    } else {
        fs::File::open(path)
            .and_then(|f| f.take(MAX_INPUT_BYTES as u64 + 1).read_to_end(&mut bytes))
            .map_err(|_| Error("input file unavailable".into()))?;
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| Error("invalid input encoding".into()))?;
    serde_json::from_value(parse_json_bounded(text, MAX_INPUT_BYTES)?)
        .map_err(|_| Error("input schema mismatch".into()))
}
pub(crate) fn option<'a>(args: &'a [String], name: &str) -> Result<&'a str> {
    let pos = args
        .iter()
        .position(|a| a == name)
        .ok_or_else(|| Error(format!("missing {name}")))?;
    args.get(pos + 1)
        .map(String::as_str)
        .ok_or_else(|| Error(format!("missing value for {name}")))
}
pub(crate) fn exact_options(args: &[String], names: &[&str]) -> Result<()> {
    require(args.len() == 1 + 2 * names.len(), "invalid command options")?;
    let mut seen = std::collections::BTreeSet::new();
    for pair in args[1..].chunks_exact(2) {
        require(
            names.contains(&pair[0].as_str()) && seen.insert(pair[0].as_str()),
            "unknown or repeated command option",
        )?;
    }
    Ok(())
}
fn mode(value: &str) -> Result<Mode> {
    match value {
        "paper" => Ok(Mode::Paper),
        "shadow" => Ok(Mode::Shadow),
        "live" => Ok(Mode::Live),
        _ => Err(Error("unknown mode".into())),
    }
}
pub(crate) fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|d| d.to_utc())
        .map_err(|_| Error("invalid RFC3339 timestamp".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weekly_cli_rejects_duplicate_unknown_and_ambiguous_options() {
        assert!(run(&["run", "--config", "a", "--config", "b"].map(String::from)).is_err());
        assert!(run(&["run", "--shell", "x"].map(String::from)).is_err());
        assert!(run(&["run", "--config"].map(String::from)).is_err());
    }
}

//! Offline research-only native fitting. No network, broker, or release authority.
use chrono::NaiveDate;
use perpetual::{Matrix, PerpetualBooster, booster::config::BoosterIO, objective::Objective};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, error::Error, fs, io::Write, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PURPOSE: &str = "RESEARCH_ONLY_HISTORICAL_REPLAY";

#[derive(Deserialize)]
struct Dataset {
    schema_version: u32,
    purpose: String,
    deployment_eligible: bool,
    history_sha256: String,
    feature_names: Vec<String>,
    feature_schema_version: String,
    label_version: String,
    partitions: BTreeMap<String, Vec<Row>>,
}

#[derive(Clone, Deserialize)]
struct Row {
    instrument_id: String,
    date: NaiveDate,
    feature_start: NaiveDate,
    feature_end: NaiveDate,
    label_end: NaiveDate,
    features: Vec<f64>,
    label: f64,
}

#[derive(Clone, Deserialize, Serialize)]
struct Calibration {
    slope: f64,
    intercept: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct FitSettings {
    learning_budget: f32,
    iteration_limit: usize,
}

impl Default for FitSettings {
    fn default() -> Self {
        Self {
            learning_budget: 1.0,
            iteration_limit: 32,
        }
    }
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn native_json(model: &PerpetualBooster) -> Result<String> {
    // Perpetual serializes internal hash maps in randomized order. Re-encode
    // through serde_json's ordered Value maps (preserve_order is disabled in
    // this locked workspace) so identical models have identical artifact bytes.
    let value: Value = serde_json::from_str(&model.json_dump()?)?;
    Ok(serde_json::to_string(&value)?)
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16 * 1024 * 1024 {
        return Err("input must be a bounded regular file".into());
    }
    Ok(fs::read(path)?)
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.recursive(true).create(path)?;
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err("output directory must not be a symlink".into());
    }
    Ok(())
}

fn validate(data: &Dataset, partitions: &[&str]) -> Result<()> {
    if data.schema_version != 1
        || data.purpose != PURPOSE
        || data.deployment_eligible
        || data.partitions.len() != partitions.len()
        || data.feature_names.len() != 7
        || data.history_sha256.len() != 64
        || data.feature_schema_version != "daily_replay_features_v1"
        || data.label_version != "gross_barrier_1pct_5bars_v1"
    {
        return Err("unsupported research dataset".into());
    }
    let expected = [
        "return_1",
        "return_5",
        "return_20",
        "volatility_20",
        "range_20",
        "volume_ratio_20",
        "distance_sma_20",
    ];
    if data.feature_names.iter().map(String::as_str).ne(expected)
        || !data.history_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("feature schema or history digest mismatch".into());
    }
    for name in partitions {
        let rows = data.partitions.get(*name).ok_or("missing partition")?;
        if !(100..=20_000).contains(&rows.len()) {
            return Err("partition outside training resource bounds".into());
        }
        let mut previous = None;
        let mut identities = std::collections::BTreeSet::new();
        for row in rows {
            if row.features.len() != data.feature_names.len()
                || row.features.iter().any(|x| !x.is_finite())
                || ![0.0, 1.0].contains(&row.label)
                || !(row.feature_start < row.feature_end
                    && row.feature_end < row.date
                    && row.date <= row.label_end)
                || previous.is_some_and(|date| date > row.date)
                || row.label_end > chrono::Utc::now().date_naive()
                || !identities.insert((&row.instrument_id, row.date))
            {
                return Err("noncausal, duplicate or invalid training observation".into());
            }
            previous = Some(row.date);
        }
        if [0.0, 1.0]
            .iter()
            .any(|label| rows.iter().filter(|r| r.label == *label).count() < 20)
        {
            return Err("insufficient class support".into());
        }
    }
    Ok(())
}

fn column_major(rows: &[Row]) -> Vec<f64> {
    (0..rows[0].features.len())
        .flat_map(|column| rows.iter().map(move |row| row.features[column]))
        .collect()
}

fn predict(model: &PerpetualBooster, rows: &[Row]) -> Result<Vec<f64>> {
    let values = column_major(rows);
    let matrix = Matrix::new(&values, rows.len(), rows[0].features.len());
    let p = model.predict_proba(&matrix, false, false);
    if p.len() != rows.len() || p.iter().any(|x| !x.is_finite() || !(0.0..=1.0).contains(x)) {
        return Err("native predictor returned invalid probabilities".into());
    }
    Ok(p)
}

fn logit(p: f64) -> f64 {
    let p = p.clamp(1e-9, 1.0 - 1e-9);
    (p / (1.0 - p)).ln()
}

fn calibrated(p: f64, c: &Calibration) -> f64 {
    1.0 / (1.0 + (-(c.slope * logit(p) + c.intercept)).exp())
}

fn calibrate(probabilities: &[f64], rows: &[Row]) -> Calibration {
    let mut c = Calibration {
        slope: 1.0,
        intercept: 0.0,
    };
    // Fixed optimizer; only the calibration partition changes these coefficients.
    for _ in 0..1000 {
        let (mut slope, mut intercept) = (0.0, 0.0);
        for (p, row) in probabilities.iter().zip(rows) {
            let error = calibrated(*p, &c) - row.label;
            slope += error * logit(*p) / rows.len() as f64;
            intercept += error / rows.len() as f64;
        }
        c.slope = (c.slope - 0.02 * (slope + 0.001 * c.slope)).clamp(0.05, 5.0);
        c.intercept = (c.intercept - 0.02 * intercept).clamp(-10.0, 10.0);
    }
    c
}

fn metrics(rows: &[Row], probabilities: &[f64]) -> Value {
    let count = rows.len() as f64;
    let brier = rows
        .iter()
        .zip(probabilities)
        .map(|(r, p)| (p - r.label).powi(2))
        .sum::<f64>()
        / count;
    let log_loss = rows
        .iter()
        .zip(probabilities)
        .map(|(r, p)| {
            let p = p.clamp(1e-9, 1.0 - 1e-9);
            -(r.label * p.ln() + (1.0 - r.label) * (1.0 - p).ln())
        })
        .sum::<f64>()
        / count;
    json!({"samples":rows.len(),"positives":rows.iter().filter(|r|r.label==1.0).count(),
        "brier":brier,"log_loss":log_loss,
        "mean_probability":probabilities.iter().sum::<f64>()/count})
}

fn fit_native(train: &[Row], settings: FitSettings) -> Result<PerpetualBooster> {
    let values = column_major(train);
    let matrix = Matrix::new(&values, train.len(), train[0].features.len());
    let targets: Vec<_> = train.iter().map(|r| r.label).collect();
    let mut model = PerpetualBooster::default()
        .set_objective(Objective::LogLoss)
        .set_budget(settings.learning_budget)
        .set_seed(42);
    model.cfg.num_threads = Some(2);
    model.cfg.iteration_limit = Some(settings.iteration_limit);
    model.fit(&matrix, &targets, None, None)?;
    if model.trees.is_empty() {
        return Err("native fitting produced no trees".into());
    }
    Ok(model)
}

fn fit(
    data: Dataset,
    input_hash: &str,
    output: &Path,
    settings: FitSettings,
    tuning_report_sha256: Option<&str>,
) -> Result<Value> {
    validate(&data, &["train", "calibration"])?;
    let train = &data.partitions["train"];
    let cal = &data.partitions["calibration"];
    if train.iter().map(|r| r.label_end).max() >= cal.iter().map(|r| r.feature_start).min() {
        return Err("train/calibration intervals overlap".into());
    }
    directory(output)?;
    write(
        &output.join("fit-claim.json"),
        &serde_json::to_vec(&json!({"input_sha256":input_hash}))?,
    )?;
    let targets: Vec<_> = train.iter().map(|r| r.label).collect();
    let mut model = fit_native(train, settings)?;
    model.insert_metadata(
        "trady.feature_schema_version".into(),
        data.feature_schema_version.clone(),
    );
    model.insert_metadata(
        "trady.feature_names".into(),
        serde_json::to_string(&data.feature_names)?,
    );
    model.insert_metadata("trady.label_version".into(), data.label_version.clone());
    model.insert_metadata("trady.research_only".into(), "true".into());
    let native = native_json(&model)?;
    let restored = PerpetualBooster::from_json(&native)?;
    let probabilities = predict(&model, cal)?;
    if probabilities != predict(&restored, cal)? {
        return Err("serialized native model prediction mismatch".into());
    }
    let calibration = calibrate(&probabilities, cal);
    let calibrated_probabilities: Vec<_> = probabilities
        .iter()
        .map(|p| calibrated(*p, &calibration))
        .collect();
    let base_rate = targets.iter().sum::<f64>() / train.len() as f64;
    let report = json!({"schema_version":1,"purpose":PURPOSE,"deployment_eligible":false,
        "status":"FITTED_UNAPPROVED_RESEARCH_CANDIDATE","runtime_version":"3.0.0-rc.2",
        "dataset_sha256":input_hash,"history_sha256":data.history_sha256,
        "artifact_sha256":hash(native.as_bytes()),"feature_names":data.feature_names,
        "feature_schema_version":data.feature_schema_version,"label_version":data.label_version,
        "seed":42,"learning_budget":settings.learning_budget,
        "iteration_limit":settings.iteration_limit,"trees":model.trees.len(),
        "tuning_report_sha256":tuning_report_sha256,
        "training_rows":train.len(),"training_base_rate":base_rate,"calibration":calibration,
        "calibration_label_end":cal.iter().map(|r|r.label_end).max(),
        "calibration_metrics":metrics(cal,&calibrated_probabilities),
        "calibration_baseline":metrics(cal,&vec![base_rate;cal.len()]),
        "holdout_accessed":false});
    write(&output.join("native-model.json"), native.as_bytes())?;
    write(
        &output.join("candidate.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

fn supported(rows: &[Row]) -> bool {
    rows.len() >= 100
        && [0.0, 1.0]
            .iter()
            .all(|label| rows.iter().filter(|row| row.label == *label).count() >= 20)
}

/// Expanding windows inside the training partition only. Every feature/label
/// interval fits in one window; calibration never reads validation outcomes.
fn fold_rows(rows: &[Row], year: i32) -> Result<[Vec<Row>; 3]> {
    let calibration_start = NaiveDate::from_ymd_opt(year - 1, 7, 1).ok_or("invalid fold")?;
    let validation_start = NaiveDate::from_ymd_opt(year, 1, 1).ok_or("invalid fold")?;
    let validation_end = NaiveDate::from_ymd_opt(year, 12, 1).ok_or("invalid fold")?;
    let mut groups = [vec![], vec![], vec![]];
    for row in rows {
        let group = if row.label_end < calibration_start {
            Some(0)
        } else if row.feature_start >= calibration_start && row.label_end < validation_start {
            Some(1)
        } else if row.feature_start >= validation_start && row.label_end < validation_end {
            Some(2)
        } else {
            None
        };
        if let Some(index) = group {
            groups[index].push(row.clone());
        }
    }
    if !groups.iter().all(|rows| supported(rows)) {
        return Err("insufficient class support in a purged tuning fold".into());
    }
    Ok(groups)
}

fn tune(data: Dataset, input_hash: &str, output: &Path) -> Result<Value> {
    validate(&data, &["train", "calibration"])?;
    if data.partitions["train"].iter().map(|r| r.label_end).max()
        >= data.partitions["calibration"]
            .iter()
            .map(|r| r.feature_start)
            .min()
    {
        return Err("train/calibration intervals overlap".into());
    }
    // Fixed before any scores are computed. The existing exposed holdout is
    // neither accepted as input nor reread by this command.
    let years = [2019, 2020, 2021];
    let folds: Vec<_> = years
        .iter()
        .map(|year| fold_rows(&data.partitions["train"], *year))
        .collect::<Result<_>>()?;
    let settings: Vec<_> = [0.25, 0.5, 1.0]
        .into_iter()
        .flat_map(|learning_budget| {
            [8, 32].into_iter().map(move |iteration_limit| FitSettings {
                learning_budget,
                iteration_limit,
            })
        })
        .collect();
    directory(output)?;
    write(
        &output.join("tuning-claim.json"),
        &serde_json::to_vec_pretty(&json!({
            "dataset_sha256":input_hash,"settings":settings,"validation_years":years,
            "selection_rule":"minimum mean validation log loss; Brier breaks ties",
            "holdout_accessed":false,"deployment_eligible":false,
            "independent_production_evaluation":false
        }))?,
    )?;
    let mut candidates = vec![];
    let mut winner = None;
    for (index, setting) in settings.iter().enumerate() {
        let mut scores = vec![];
        let (mut loss, mut brier) = (0.0, 0.0);
        let (mut baseline_loss, mut baseline_brier) = (0.0, 0.0);
        for (year, [train, calibration_rows, validation]) in years.iter().zip(&folds) {
            let model = fit_native(train, *setting)?;
            let calibration = calibrate(&predict(&model, calibration_rows)?, calibration_rows);
            let probabilities: Vec<_> = predict(&model, validation)?
                .iter()
                .map(|p| calibrated(*p, &calibration))
                .collect();
            let score = metrics(validation, &probabilities);
            let rate = train.iter().map(|row| row.label).sum::<f64>() / train.len() as f64;
            let baseline = metrics(validation, &vec![rate; validation.len()]);
            loss += score["log_loss"].as_f64().ok_or("invalid score")?;
            brier += score["brier"].as_f64().ok_or("invalid score")?;
            baseline_loss += baseline["log_loss"].as_f64().ok_or("invalid baseline")?;
            baseline_brier += baseline["brier"].as_f64().ok_or("invalid baseline")?;
            scores.push(json!({"validation_year":year,"training_rows":train.len(),
                "calibration_rows":calibration_rows.len(),"validation_rows":validation.len(),
                "training_label_end":train.iter().map(|r|r.label_end).max(),
                "calibration_feature_start":calibration_rows.iter().map(|r|r.feature_start).min(),
                "calibration_label_end":calibration_rows.iter().map(|r|r.label_end).max(),
                "validation_feature_start":validation.iter().map(|r|r.feature_start).min(),
                "validation_label_end":validation.iter().map(|r|r.label_end).max(),
                "metrics":score,"training_prevalence_baseline":baseline,
                "calibration":calibration,"trees":model.trees.len()}));
        }
        loss /= folds.len() as f64;
        brier /= folds.len() as f64;
        if !loss.is_finite() || !brier.is_finite() {
            return Err("nonfinite tuning score".into());
        }
        if winner.is_none_or(|(_, best_loss, best_brier)| (loss, brier) < (best_loss, best_brier)) {
            winner = Some((index, loss, brier));
        }
        candidates.push(
            json!({"settings":setting,"mean_log_loss":loss,"mean_brier":brier,
            "mean_baseline_log_loss":baseline_loss/folds.len() as f64,
            "mean_baseline_brier":baseline_brier/folds.len() as f64,
            "folds":scores}),
        );
    }
    let (selected, loss, brier) = winner.ok_or("no tuning candidates")?;
    let advantage = loss
        < candidates[selected]["mean_baseline_log_loss"]
            .as_f64()
            .ok_or("missing baseline")?
        && brier
            < candidates[selected]["mean_baseline_brier"]
                .as_f64()
                .ok_or("missing baseline")?;
    let report = json!({"schema_version":1,"purpose":PURPOSE,"deployment_eligible":false,
        "status":if advantage {"WALK_FORWARD_ADVANTAGE_REQUIRES_FRESH_HOLDOUT"} else {"NO_WALK_FORWARD_ADVANTAGE"},
        "tuning_complete":true,"measured_validation_advantage":advantage,
        "dataset_sha256":input_hash,"history_sha256":data.history_sha256,
        "selected_index":selected,"selected_settings":settings[selected],"candidates":candidates,
        "selection_data":"original training partition only; 2019/2020/2021 validation folds",
        "holdout_accessed":false,"calibration_partition_used_for_selection":false,
        "independent_production_evaluation":false,"prospective_shadow_evidence":false,
        "profitability_established":false});
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    write(&output.join("tuning-report.json"), &report_bytes)?;
    let candidate = fit(
        data,
        input_hash,
        output,
        settings[selected],
        Some(&hash(&report_bytes)),
    )?;
    Ok(json!({"tuning":report,"candidate":candidate}))
}

fn validate_calibration(calibration: &Calibration, base_rate: f64) -> Result<()> {
    if !(calibration.slope.is_finite()
        && (0.05..=5.0).contains(&calibration.slope)
        && calibration.intercept.is_finite()
        && (-10.0..=10.0).contains(&calibration.intercept)
        && base_rate.is_finite()
        && 0.0 < base_rate
        && base_rate < 1.0)
    {
        return Err("invalid calibration or training prevalence".into());
    }
    Ok(())
}

fn evaluate(data: Dataset, input_hash: &str, directory: &Path) -> Result<Value> {
    validate(&data, &["holdout"])?;
    let candidate_bytes = read(&directory.join("candidate.json"))?;
    let candidate: Value = serde_json::from_slice(&candidate_bytes)?;
    if candidate["purpose"] != PURPOSE
        || candidate["deployment_eligible"] != false
        || candidate["history_sha256"] != data.history_sha256
        || candidate["feature_names"] != serde_json::to_value(&data.feature_names)?
        || candidate["feature_schema_version"] != data.feature_schema_version
        || candidate["label_version"] != data.label_version
    {
        return Err("candidate/holdout identity mismatch".into());
    }
    let rows = &data.partitions["holdout"];
    let cal_end: NaiveDate = serde_json::from_value(candidate["calibration_label_end"].clone())?;
    if rows.iter().any(|r| r.feature_start <= cal_end) {
        return Err("holdout overlaps calibration".into());
    }
    let bytes = read(&directory.join("native-model.json"))?;
    if candidate["artifact_sha256"] != hash(&bytes) {
        return Err("native model hash mismatch".into());
    }
    let calibration: Calibration = serde_json::from_value(candidate["calibration"].clone())?;
    let base_rate = candidate["training_base_rate"]
        .as_f64()
        .ok_or("missing baseline")?;
    validate_calibration(&calibration, base_rate)?;
    write(
        &directory.join("holdout-claim.json"),
        &serde_json::to_vec(&json!({
        "dataset_sha256":input_hash,"artifact_sha256":hash(&bytes),
        "note":"Local research exposure claim, not an independent evaluator identity."}))?,
    )?;
    let model = PerpetualBooster::from_json(std::str::from_utf8(&bytes)?)?;
    let probabilities: Vec<_> = predict(&model, rows)?
        .iter()
        .map(|p| calibrated(*p, &calibration))
        .collect();
    let scores = metrics(rows, &probabilities);
    let baseline = metrics(rows, &vec![base_rate; rows.len()]);
    let better = scores["brier"].as_f64() < baseline["brier"].as_f64()
        && scores["log_loss"].as_f64() < baseline["log_loss"].as_f64();
    let report = json!({"schema_version":1,"purpose":PURPOSE,"deployment_eligible":false,
        "status":if better {"OFFLINE_SCORE_IMPROVEMENT_ONLY"}else{"NO_MEASURED_HOLDOUT_ADVANTAGE"},
        "holdout_sha256":input_hash,"artifact_sha256":hash(&bytes),
        "candidate_sha256":hash(&candidate_bytes),
        "metrics":scores,"training_prevalence_baseline":baseline,
        "first_date":rows.first().map(|r|r.date),"last_date":rows.last().map(|r|r.date),
        "model_refitted":false,"independent_production_evaluation":false,
        "prospective_shadow_evidence":false,"profitability_established":false});
    write(
        &directory.join("holdout-report.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

fn run() -> Result<Value> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 || !["fit", "tune", "evaluate"].contains(&args[0].as_str()) {
        return Err("usage: train_candidate fit|tune|evaluate DATASET FIT_DIRECTORY".into());
    }
    let bytes = read(Path::new(&args[1]))?;
    let data = serde_json::from_slice(&bytes)?;
    if args[0] == "fit" {
        fit(
            data,
            &hash(&bytes),
            Path::new(&args[2]),
            FitSettings::default(),
            None,
        )
    } else if args[0] == "tune" {
        tune(data, &hash(&bytes), Path::new(&args[2]))
    } else {
        evaluate(data, &hash(&bytes), Path::new(&args[2]))
    }
}

fn main() {
    match run() {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("JSON report")
        ),
        Err(error) => {
            eprintln!("research training failed: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dataset() -> Dataset {
        let start = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let rows: Vec<_> = (0..100)
            .map(|i| {
                let date = start + chrono::Duration::days(i);
                json!({"instrument_id":"RESEARCH_TEST","date":date,
                "feature_start":date-chrono::Duration::days(21),
                "feature_end":date-chrono::Duration::days(1),
                "label_end":date+chrono::Duration::days(4),
                "features":[0.0,0.0,0.0,0.0,0.0,0.0,0.0],"label":(i%2) as f64})
            })
            .collect();
        serde_json::from_value(json!({"schema_version":1,"purpose":PURPOSE,
            "deployment_eligible":false,"history_sha256":"a".repeat(64),
            "feature_names":["return_1","return_5","return_20","volatility_20",
                "range_20","volume_ratio_20","distance_sma_20"],
            "feature_schema_version":"daily_replay_features_v1",
            "label_version":"gross_barrier_1pct_5bars_v1",
            "partitions":{"holdout":rows}}))
        .unwrap()
    }

    #[test]
    fn held_out_data_cannot_enter_fit_partitions() {
        let data = dataset();
        assert!(validate(&data, &["holdout"]).is_ok());
        assert!(validate(&data, &["train", "calibration"]).is_err());
    }

    #[test]
    fn native_artifact_hash_does_not_depend_on_metadata_insertion_order() {
        let mut left = PerpetualBooster::default();
        let mut right = PerpetualBooster::default();
        for key in ["schema", "labels", "research", "features"] {
            left.insert_metadata(key.into(), format!("value-{key}"));
        }
        for key in ["features", "research", "labels", "schema"] {
            right.insert_metadata(key.into(), format!("value-{key}"));
        }
        assert_eq!(native_json(&left).unwrap(), native_json(&right).unwrap());
    }

    #[test]
    fn tuning_cannot_accept_holdout_or_create_an_exposure_claim() {
        let directory =
            std::env::temp_dir().join(format!("trady-tune-reject-{}", std::process::id()));
        assert!(tune(dataset(), &"b".repeat(64), &directory).is_err());
        assert!(!directory.exists());
    }

    #[test]
    fn tuning_folds_purge_entire_feature_and_label_intervals() {
        let mut rows = vec![];
        let start = NaiveDate::from_ymd_opt(2016, 1, 1).unwrap();
        for i in 0..2200 {
            let date = start + chrono::Duration::days(i);
            rows.push(Row {
                instrument_id: "RESEARCH_TEST".into(),
                date,
                feature_start: date - chrono::Duration::days(21),
                feature_end: date - chrono::Duration::days(1),
                label_end: date + chrono::Duration::days(4),
                features: vec![0.0; 7],
                label: (i % 2) as f64,
            });
        }
        for year in [2019, 2020, 2021] {
            let [train, calibration, validation] = fold_rows(&rows, year).unwrap();
            assert!(
                train.iter().map(|row| row.label_end).max()
                    < calibration.iter().map(|row| row.feature_start).min()
            );
            assert!(
                calibration.iter().map(|row| row.label_end).max()
                    < validation.iter().map(|row| row.feature_start).min()
            );
            assert!(validation.iter().all(|row| {
                row.feature_start >= NaiveDate::from_ymd_opt(year, 1, 1).unwrap()
                    && row.label_end < NaiveDate::from_ymd_opt(year, 12, 1).unwrap()
            }));
        }
    }

    #[test]
    fn future_features_and_duplicate_decisions_are_rejected() {
        let mut data = dataset();
        let rows = data.partitions.get_mut("holdout").unwrap();
        rows[0].feature_end = rows[0].date;
        assert!(validate(&data, &["holdout"]).is_err());
        let mut data = dataset();
        let rows = data.partitions.get_mut("holdout").unwrap();
        rows[1].date = rows[0].date;
        assert!(validate(&data, &["holdout"]).is_err());
    }

    #[test]
    fn feature_order_cannot_silently_change() {
        let mut data = dataset();
        data.feature_names.swap(0, 1);
        assert!(validate(&data, &["holdout"]).is_err());
    }

    #[test]
    fn altered_calibration_cannot_distort_holdout_comparison() {
        let valid = Calibration {
            slope: 1.0,
            intercept: 0.0,
        };
        assert!(validate_calibration(&valid, 0.5).is_ok());
        for rate in [-1.0, 0.0, 1.0, 2.0, f64::NAN] {
            assert!(validate_calibration(&valid, rate).is_err());
        }
        for invalid in [
            Calibration {
                slope: -1.0,
                intercept: 0.0,
            },
            Calibration {
                slope: 1.0,
                intercept: 100.0,
            },
        ] {
            assert!(validate_calibration(&invalid, 0.5).is_err());
        }
    }
}

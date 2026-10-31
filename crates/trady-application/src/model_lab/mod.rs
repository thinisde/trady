//! Offline, point-in-time four-outcome fitting. Artifacts carry no deployment authority.

mod fit;
mod inference;
mod journal;

pub use fit::{evaluate, fit};
pub use inference::{
    ForecastObservation, OfflinePrediction, SignalObservation, SupplementalSignals, infer_offline,
    infer_scenario,
};
pub use journal::{EvaluationJournal, read_document, write_immutable};

use crate::optimization::{digest, valid_hash, valid_id};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::{Error, Money, Result, prediction::ScenarioOutcome, require};

pub const FORMAT: &str = "trady-softmax-ridge-four-outcome-v1";
pub const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ROWS: usize = 20_000;
pub const MAX_FEATURES: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Includes the scenario in fitting rather than treating all targets/stops alike.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFeatures {
    pub target_net_fraction: Decimal,
    pub stop_distance_fraction: Decimal,
    pub spread_fraction: Decimal,
    pub exchange_sessions: u8,
}
impl ScenarioFeatures {
    pub fn validate(&self) -> Result<()> {
        require(
            self.target_net_fraction > Decimal::ZERO
                && self.target_net_fraction <= Decimal::new(10, 2)
                && self.stop_distance_fraction > Decimal::ZERO
                && self.stop_distance_fraction < Decimal::ONE
                && self.spread_fraction >= Decimal::ZERO
                && self.spread_fraction < Decimal::ONE
                && (1..=5).contains(&self.exchange_sessions),
            "unsupported scenario feature units",
        )
    }
    pub(crate) fn append(&self, values: &mut Vec<f64>) -> Result<()> {
        self.validate()?;
        values.extend([
            number(self.target_net_fraction)?,
            number(self.stop_distance_fraction)?,
            number(self.spread_fraction)?,
            f64::from(self.exchange_sessions),
        ]);
        Ok(())
    }
}

/// Original records, not retrospective narrative or synthetic financial history.
/// Hashes are resolved by independent deployment review; fitting cannot verify truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointInTimeRow {
    pub id: String,
    pub instrument_id: String,
    pub market_snapshot_id: String,
    pub scenario_hash: String,
    pub regime: String,
    pub feature_window_start_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub decision_at: DateTime<Utc>,
    pub label_end_at: DateTime<Utc>,
    pub outcome_available_at: DateTime<Utc>,
    pub source_record_sha256: String,
    pub execution_record_sha256: String,
    pub evidence_lineages: Vec<String>,
    pub features: Vec<Decimal>,
    pub scenario: ScenarioFeatures,
    /// None denotes unresolved, ambiguous, censored or partially reconciled execution.
    pub outcome: Option<ScenarioOutcome>,
    pub execution: Option<PayoffRecord>,
}

/// All amounts use one account currency. Gross P&L excludes the listed costs;
/// each charge enters net P&L exactly once. Non-fill records are required too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayoffRecord {
    pub acquisition_cost: Money,
    pub opening_equity: Money,
    pub gross_pnl: Money,
    pub fees: Money,
    pub fx_cost: Money,
    pub slippage: Money,
    pub cash_pnl: Money,
}
impl PayoffRecord {
    pub(crate) fn net_per_acquisition(&self) -> Result<f64> {
        let currency = &self.acquisition_cost.currency;
        require(
            self.acquisition_cost.amount > Decimal::ZERO
                && self.opening_equity.amount >= self.acquisition_cost.amount
                && [
                    &self.opening_equity,
                    &self.gross_pnl,
                    &self.fees,
                    &self.fx_cost,
                    &self.slippage,
                    &self.cash_pnl,
                ]
                .iter()
                .all(|v| &v.currency == currency)
                && [self.fees.amount, self.fx_cost.amount, self.slippage.amount]
                    .iter()
                    .all(|v| *v >= Decimal::ZERO),
            "unreconciled payoff currency, capital or charges",
        )?;
        let net = self
            .gross_pnl
            .amount
            .checked_sub(self.fees.amount)
            .and_then(|v| v.checked_sub(self.fx_cost.amount))
            .and_then(|v| v.checked_sub(self.slippage.amount))
            .and_then(|v| v.checked_div(self.acquisition_cost.amount))
            .ok_or_else(|| Error("payoff arithmetic overflow".into()))?;
        let net = number(net)?;
        require(
            (-2.0..=2.0).contains(&net),
            "payoff exceeds baseline supported range",
        )?;
        Ok(net)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationDataset {
    pub schema_version: u32,
    pub dataset_id: String,
    /// Test data are permanently ineligible for production approval.
    pub test_only: bool,
    pub feature_schema_version: String,
    pub feature_names: Vec<String>,
    pub label_version: String,
    pub cost_model_hash: String,
    pub rows: Vec<PointInTimeRow>,
}
impl ObservationDataset {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema_version == 1
                && valid_id(&self.dataset_id)
                && valid_id(&self.feature_schema_version)
                && valid_id(&self.label_version)
                && valid_hash(&self.cost_model_hash)
                && !self.rows.is_empty()
                && self.rows.len() <= MAX_ROWS
                && !self.feature_names.is_empty()
                && self.feature_names.len() <= MAX_FEATURES,
            "invalid or unbounded observation dataset",
        )?;
        let names: BTreeSet<_> = self.feature_names.iter().collect();
        require(
            names.len() == self.feature_names.len()
                && self.feature_names.iter().all(|v| valid_id(v)),
            "invalid feature names",
        )?;
        let mut ids = BTreeSet::new();
        let mut last = None;
        for row in &self.rows {
            require(
                valid_id(&row.id)
                    && ids.insert(&row.id)
                    && valid_id(&row.instrument_id)
                    && valid_id(&row.market_snapshot_id)
                    && valid_id(&row.regime)
                    && valid_hash(&row.scenario_hash)
                    && valid_hash(&row.source_record_sha256)
                    && valid_hash(&row.execution_record_sha256)
                    && row.features.len() == self.feature_names.len()
                    && row.feature_window_start_at <= row.observed_at
                    && row.observed_at <= row.available_at
                    && row.available_at <= row.decision_at
                    && row.decision_at < row.label_end_at
                    && row.label_end_at <= row.outcome_available_at
                    && last.is_none_or(|at| at <= row.decision_at)
                    && !row.evidence_lineages.is_empty()
                    && row.evidence_lineages.len() <= 64
                    && row.evidence_lineages.iter().all(|v| valid_id(v))
                    && row.evidence_lineages.iter().collect::<BTreeSet<_>>().len()
                        == row.evidence_lineages.len()
                    && (row.outcome.is_some() == row.execution.is_some()),
                "duplicate, nonchronological, unbound or noncausal observation",
            )?;
            row.scenario.validate()?;
            row.features
                .iter()
                .try_for_each(|v| number(*v).map(|_| ()))?;
            if let Some(record) = &row.execution {
                let net = record.net_per_acquisition()?;
                require(
                    match row.outcome {
                        Some(ScenarioOutcome::TargetFirst) => net > 0.0,
                        Some(ScenarioOutcome::StopFirst) => net < 0.0,
                        Some(ScenarioOutcome::NonFill) => net <= 0.0,
                        _ => true,
                    },
                    "outcome contradicts reconciled net payoff",
                )?;
            }
            last = Some(row.decision_at);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingPlan {
    pub model_id: String,
    pub train: Window,
    pub calibration: Window,
    pub validation: Window,
    pub sealed_holdout_start_at: DateTime<Utc>,
    pub embargo_seconds: u32,
    pub fit_at: DateTime<Utc>,
    pub iterations: u32,
    pub learning_rate: f64,
    pub ridge: f64,
    pub minimum_class_samples: u32,
    pub minimum_regime_samples: u32,
    pub maximum_validation_brier: f64,
    pub maximum_validation_log_loss: f64,
    pub uncertainty_quantile: f64,
}
impl TrainingPlan {
    pub fn validate(&self, test_only: bool) -> Result<()> {
        let embargo = Duration::seconds(i64::from(self.embargo_seconds));
        require(
            valid_id(&self.model_id)
                && self.train.start < self.train.end
                && self.calibration.start < self.calibration.end
                && self.validation.start < self.validation.end
                && self
                    .train
                    .end
                    .checked_add_signed(embargo)
                    .is_some_and(|t| t <= self.calibration.start)
                && self
                    .calibration
                    .end
                    .checked_add_signed(embargo)
                    .is_some_and(|t| t <= self.validation.start)
                && self
                    .validation
                    .end
                    .checked_add_signed(embargo)
                    .is_some_and(|t| t <= self.sealed_holdout_start_at)
                && self.validation.end <= self.fit_at
                && self.embargo_seconds <= 604_800
                && (50..=5000).contains(&self.iterations)
                && self.learning_rate.is_finite()
                && (0.0001..=0.3).contains(&self.learning_rate)
                && self.ridge.is_finite()
                && (0.000001..=10.0).contains(&self.ridge)
                && self.minimum_class_samples >= if test_only { 4 } else { 30 }
                && self.minimum_regime_samples >= if test_only { 8 } else { 100 }
                && self.maximum_validation_brier.is_finite()
                && (0.0..=0.75).contains(&self.maximum_validation_brier)
                && self.maximum_validation_log_loss.is_finite()
                && (0.0..=1.3863).contains(&self.maximum_validation_log_loss)
                && self.uncertainty_quantile.is_finite()
                && (0.8..=0.99).contains(&self.uncertainty_quantile),
            "invalid predeclared training plan or insufficient minimum support",
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReliabilityBin {
    pub samples: u32,
    pub mean_probability: f64,
    pub observed_frequency: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub samples: u32,
    pub class_counts: [u32; 4],
    pub multiclass_brier: f64,
    pub log_loss: f64,
    pub per_class_brier: [f64; 4],
    pub reliability: [Vec<ReliabilityBin>; 4],
    pub mean_absolute_payoff_error: f64,
    pub mean_realized_account_return: f64,
    pub selected_samples: u32,
    pub selected_mean_account_return: Option<f64>,
    pub cash_account_return: f64,
    pub regime_samples: BTreeMap<String, u32>,
    pub horizon_samples: BTreeMap<u8, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCandidate {
    pub schema_version: u32,
    pub format: String,
    pub test_only: bool,
    pub dataset_id: String,
    pub dataset_sha256: String,
    pub feature_schema_version: String,
    pub feature_names: Vec<String>,
    pub label_version: String,
    pub cost_model_hash: String,
    pub plan: TrainingPlan,
    pub center: Vec<f64>,
    pub scale: Vec<f64>,
    pub minimum: Vec<f64>,
    pub maximum: Vec<f64>,
    pub classifier: [Vec<f64>; 4],
    pub conditional_payoffs: [Vec<f64>; 4],
    pub temperature: f64,
    /// Empirical absolute forecast error in net return per all-in acquisition.
    pub uncertainty_radius: f64,
    pub supported_regimes: Vec<String>,
    pub supported_horizons: Vec<u8>,
    pub training_rows: u32,
    pub calibration: Metrics,
    pub validation: Metrics,
    pub pending_rows: u32,
    pub purged_rows: u32,
    pub development_eligible: bool,
}
impl ModelCandidate {
    pub fn hash(&self) -> Result<String> {
        digest(self)
    }
    pub fn validate(&self) -> Result<()> {
        self.plan.validate(self.test_only)?;
        let n = self.feature_names.len() + 4;
        require(
            self.schema_version == 1
                && self.format == FORMAT
                && valid_id(&self.dataset_id)
                && valid_hash(&self.dataset_sha256)
                && valid_id(&self.feature_schema_version)
                && valid_id(&self.label_version)
                && valid_hash(&self.cost_model_hash)
                && (1..=MAX_FEATURES).contains(&self.feature_names.len())
                && self.feature_names.iter().all(|v| valid_id(v))
                && self.feature_names.iter().collect::<BTreeSet<_>>().len()
                    == self.feature_names.len()
                && [&self.center, &self.scale, &self.minimum, &self.maximum]
                    .iter()
                    .all(|v| v.len() == n && v.iter().all(|x| x.is_finite()))
                && self.scale.iter().all(|x| *x > 0.0)
                && self.minimum.iter().zip(&self.maximum).all(|(a, b)| a <= b)
                && self
                    .classifier
                    .iter()
                    .chain(&self.conditional_payoffs)
                    .all(|v| v.len() == n + 1 && v.iter().all(|x| x.is_finite()))
                && self.temperature.is_finite()
                && (0.5..=3.0).contains(&self.temperature)
                && self.uncertainty_radius.is_finite()
                && self.uncertainty_radius >= 0.0
                && !self.supported_regimes.is_empty()
                && self.supported_regimes.len() <= 32
                && self.supported_regimes.iter().all(|v| valid_id(v))
                && !self.supported_horizons.is_empty()
                && self.supported_horizons.iter().all(|v| (1..=5).contains(v)),
            "corrupt or incompatible model candidate",
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationReport {
    pub schema_version: u32,
    pub candidate_sha256: String,
    pub holdout_id: String,
    pub holdout_sha256: String,
    pub test_only: bool,
    pub evaluation_started_at: DateTime<Utc>,
    pub evaluation_ended_at: DateTime<Utc>,
    pub evaluated_at: DateTime<Utc>,
    pub metrics: Metrics,
    pub by_regime: BTreeMap<String, Metrics>,
    pub by_horizon: BTreeMap<u8, Metrics>,
    pub empirical_uncertainty_radius: f64,
    pub pending_or_unsupported: u32,
    pub eligible: bool,
}

/// This record is authored by an independent deployment reviewer, never by fit.
/// Its exact bytes/hash must be pinned in deployment-owned service configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelApproval {
    pub schema_version: u32,
    pub candidate_sha256: String,
    pub evaluation_sha256: String,
    pub reviewer_id: String,
    pub approved_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub dataset_verification_sha256: String,
    pub prospective_shadow_sha256: String,
    pub owner_approval_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedModel {
    pub candidate: ModelCandidate,
    pub evaluation: EvaluationReport,
    pub approval: ModelApproval,
}
impl ApprovedModel {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        self.candidate.validate()?;
        let c = &self.candidate;
        let e = &self.evaluation;
        let a = &self.approval;
        require(
            a.schema_version == 1
                && e.schema_version == 1
                && !c.test_only
                && !e.test_only
                && c.development_eligible
                && e.eligible
                && a.candidate_sha256 == c.hash()?
                && e.candidate_sha256 == a.candidate_sha256
                && a.evaluation_sha256 == digest(e)?
                && valid_id(&a.reviewer_id)
                && valid_id(&e.holdout_id)
                && valid_hash(&e.holdout_sha256)
                && c.plan.fit_at < e.evaluation_started_at
                && c.plan.sealed_holdout_start_at <= e.evaluation_started_at
                && e.evaluation_started_at < e.evaluation_ended_at
                && e.evaluation_ended_at <= e.evaluated_at
                && e.evaluated_at <= a.approved_at
                && a.approved_at <= now
                && now < a.valid_until
                && [
                    a.dataset_verification_sha256.as_str(),
                    a.prospective_shadow_sha256.as_str(),
                    a.owner_approval_sha256.as_str(),
                ]
                .iter()
                .all(|v| valid_hash(v)),
            "model is unapproved, test-only, expired or lacks independent evaluation",
        )
    }
    pub fn report_id(&self) -> Result<String> {
        Ok(format!("evaluation_{}", digest(&self.evaluation)?))
    }
    pub fn estimation_id(&self) -> Result<String> {
        Ok(format!("payoffs_{}", digest(&self.evaluation)?))
    }
    pub fn calibrator_id(&self) -> Result<String> {
        Ok(format!("temperature_{}", self.candidate.hash()?))
    }
}

pub(crate) fn number(v: Decimal) -> Result<f64> {
    let n = v
        .to_string()
        .parse::<f64>()
        .map_err(|_| Error("numeric conversion failed".into()))?;
    require(
        n.is_finite() && n.abs() <= 1e12,
        "non-finite or excessive feature magnitude",
    )?;
    Ok(n)
}
pub(crate) fn decimal(v: f64) -> Result<Decimal> {
    require(v.is_finite(), "non-finite model output")?;
    Decimal::from_f64_retain(v)
        .map(|v| v.round_dp(12))
        .ok_or_else(|| Error("model output exceeds decimal range".into()))
}

#[cfg(test)]
mod tests;

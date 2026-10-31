//! Matured multiclass forecast scoring and chronological split checks.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::{Result, prediction::*, require};

use crate::{add, div, mul, sub};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PredictionPerformance {
    pub scored: u32,
    pub pending_or_censored: u32,
    /// Sum over all four classes, averaged over complete original forecasts.
    pub multiclass_brier_score: Option<Decimal>,
    pub regime_samples: BTreeMap<String, u32>,
}

pub fn score_matured_predictions(
    predictions: &[Prediction],
    outcomes: &[MaturedOutcome],
    cutoff: DateTime<Utc>,
) -> Result<PredictionPerformance> {
    require(
        predictions.len() <= 100_000 && outcomes.len() <= 100_000,
        "unbounded prediction history",
    )?;
    let mut labels = BTreeMap::new();
    for outcome in outcomes {
        require(
            valid_prediction_id(&outcome.prediction_id)
                && valid_prediction_hash(&outcome.scenario_hash)
                && valid_prediction_id(&outcome.label_version)
                && outcome.horizon_end_at <= outcome.available_at
                && labels
                    .insert(outcome.prediction_id.as_str(), outcome)
                    .is_none(),
            "duplicate, noncausal or invalid matured outcome",
        )?;
    }
    let mut result = PredictionPerformance {
        scored: 0,
        pending_or_censored: 0,
        multiclass_brier_score: None,
        regime_samples: BTreeMap::new(),
    };
    let mut seen = BTreeSet::new();
    let mut loss = Decimal::ZERO;
    for prediction in predictions {
        let m = &prediction.metadata;
        require(
            seen.insert(&m.prediction_id),
            "duplicate original prediction in performance history",
        )?;
        // Revalidate at the original decision instant, not today after its expiry.
        prediction.validate(m.horizon.decision_at)?;
        if m.generated_at > cutoff {
            continue;
        }
        let PredictionPayload::ScenarioOutcomes { probabilities } = &prediction.payload else {
            continue;
        };
        let Some(label) = labels.get(m.prediction_id.as_str()) else {
            result.pending_or_censored += 1;
            continue;
        };
        require(
            m.scenario_hash.as_ref() == Some(&label.scenario_hash)
                && label.label_version == m.label_version
                && label.horizon_end_at == m.horizon.deadline_at,
            "matured outcome does not match the original forecast scenario/horizon",
        )?;
        let Some(outcome) = label
            .outcome
            .filter(|_| label.available_at <= cutoff && label.horizon_end_at <= cutoff)
        else {
            result.pending_or_censored += 1;
            continue;
        };
        for (index, p) in probabilities.values().into_iter().enumerate() {
            let observed = if index == outcome.index() {
                Decimal::ONE
            } else {
                Decimal::ZERO
            };
            let error = sub(p, observed)?;
            loss = add(loss, mul(error, error)?)?;
        }
        result.scored += 1;
        *result.regime_samples.entry(m.regime.clone()).or_default() += 1;
    }
    if result.scored > 0 {
        result.multiclass_brier_score = Some(div(loss, Decimal::from(result.scored))?);
    }
    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChronologicalSplit {
    pub training_end_at: DateTime<Utc>,
    pub calibration_start_at: DateTime<Utc>,
    pub calibration_end_at: DateTime<Utc>,
    pub validation_start_at: DateTime<Utc>,
    pub validation_end_at: DateTime<Utc>,
    pub test_start_at: DateTime<Utc>,
    pub test_end_at: DateTime<Utc>,
    pub embargo_seconds: u32,
}
impl ChronologicalSplit {
    pub fn validate(&self) -> Result<()> {
        let embargo = Duration::seconds(i64::from(self.embargo_seconds));
        require(
            self.training_end_at
                .checked_add_signed(embargo)
                .is_some_and(|at| at < self.calibration_start_at)
                && self.calibration_start_at < self.calibration_end_at
                && self
                    .calibration_end_at
                    .checked_add_signed(embargo)
                    .is_some_and(|at| at < self.validation_start_at)
                && self.validation_start_at < self.validation_end_at
                && self
                    .validation_end_at
                    .checked_add_signed(embargo)
                    .is_some_and(|at| at < self.test_start_at)
                && self.test_start_at < self.test_end_at,
            "chronological partitions overlap or violate embargo",
        )
    }

    /// Caller supplies the actual exchange-calendar end of the entire label window.
    pub fn training_sample_eligible(
        &self,
        available_at: DateTime<Utc>,
        label_end_at: DateTime<Utc>,
    ) -> Result<bool> {
        self.validate()?;
        Ok(available_at <= self.training_end_at
            && label_end_at <= self.training_end_at
            && available_at <= label_end_at)
    }
}

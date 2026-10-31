//! Conservative calibrated baseline. Unlike prediction meanings are never averaged.

mod history;
pub use history::*;

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::{AccountId, InstrumentId, Money, Result, prediction::*, require};

use crate::{add, fresh, mul, sub};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionConfig {
    pub version: u32,
    pub revision_id: String,
    /// One approved revision per source: no learned weights or model selection.
    pub approved_models: BTreeMap<PredictionSource, String>,
    pub outcome_calibration_report_id: String,
    pub minimum_calibration_samples: u32,
    pub maximum_age_seconds: u32,
    pub maximum_uncertainty: Decimal,
}

impl FusionConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.version == 1
                && valid_prediction_id(&self.revision_id)
                && self.approved_models.len() == 4
                && self
                    .approved_models
                    .values()
                    .all(|v| valid_prediction_id(v))
                && valid_prediction_id(&self.outcome_calibration_report_id)
                && self.minimum_calibration_samples > 0
                && (1..=86_400).contains(&self.maximum_age_seconds)
                && self.maximum_uncertainty >= Decimal::ZERO,
            "invalid approved fusion baseline configuration",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FusionContext {
    pub instrument_id: InstrumentId,
    pub market_snapshot_id: String,
    pub scenario_hash: String,
    pub horizon: ForecastHorizon,
    pub regime: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FusionStatus {
    Ready,
    Abstain,
}

/// Construction is private and there is no Deserialize path for unchecked results.
/// A READY result is still predictive advice, never entry/release authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FusedPrediction {
    schema_version: u32,
    fusion_revision_id: String,
    context: FusionContext,
    evaluated_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    status: FusionStatus,
    forecast_id: Option<String>,
    probabilities: Option<OutcomeProbabilities>,
    uncertainty: Option<Decimal>,
    contributing_prediction_ids: Vec<String>,
    repeated_prediction_ids: Vec<String>,
    duplicated_lineages: BTreeMap<String, Vec<String>>,
    dependence_groups: BTreeMap<String, Vec<String>>,
    ablated_sources: BTreeSet<PredictionSource>,
    maximum_negative_sentiment: Option<Decimal>,
    reason_codes: Vec<String>,
}

impl FusedPrediction {
    pub fn status(&self) -> FusionStatus {
        self.status
    }
    pub fn context(&self) -> &FusionContext {
        &self.context
    }
    pub fn reason_codes(&self) -> &[String] {
        &self.reason_codes
    }
    pub fn duplicated_lineages(&self) -> &BTreeMap<String, Vec<String>> {
        &self.duplicated_lineages
    }
    pub fn dependence_groups(&self) -> &BTreeMap<String, Vec<String>> {
        &self.dependence_groups
    }
    pub fn maximum_negative_sentiment(&self) -> Option<Decimal> {
        self.maximum_negative_sentiment
    }
    pub fn probabilities(&self) -> Option<&OutcomeProbabilities> {
        self.probabilities.as_ref()
    }
    pub fn forecast_id(&self) -> Option<&str> {
        self.forecast_id.as_deref()
    }
}

pub struct PredictionFusionEngine {
    config: FusionConfig,
}

impl PredictionFusionEngine {
    pub fn new(config: FusionConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self { config })
    }
    pub fn config(&self) -> &FusionConfig {
        &self.config
    }

    pub fn fuse(
        &self,
        context: &FusionContext,
        predictions: &[Prediction],
        now: DateTime<Utc>,
    ) -> Result<FusedPrediction> {
        self.fuse_ablation(context, predictions, &BTreeSet::new(), now)
    }

    /// Offline contribution experiment. Excluding a required source is explicitly
    /// marked in the output and never creates a production-ready recommendation.
    pub fn fuse_ablation(
        &self,
        context: &FusionContext,
        predictions: &[Prediction],
        excluded: &BTreeSet<PredictionSource>,
        now: DateTime<Utc>,
    ) -> Result<FusedPrediction> {
        context.horizon.validate()?;
        require(
            valid_prediction_id(&context.market_snapshot_id)
                && valid_prediction_id(&context.regime)
                && valid_prediction_hash(&context.scenario_hash)
                && context.horizon.decision_at <= now
                && now < context.horizon.deadline_at
                && predictions.len() <= 128,
            "invalid or unbounded fusion context",
        )?;
        let mut output = FusedPrediction {
            schema_version: 1,
            fusion_revision_id: self.config.revision_id.clone(),
            context: context.clone(),
            evaluated_at: now,
            valid_until: context.horizon.deadline_at,
            status: FusionStatus::Abstain,
            forecast_id: None,
            probabilities: None,
            uncertainty: None,
            contributing_prediction_ids: vec![],
            repeated_prediction_ids: vec![],
            duplicated_lineages: BTreeMap::new(),
            dependence_groups: BTreeMap::new(),
            ablated_sources: excluded.clone(),
            maximum_negative_sentiment: None,
            reason_codes: vec![],
        };
        let mut seen: BTreeMap<&str, &Prediction> = BTreeMap::new();
        let mut sources = BTreeSet::new();
        let mut forecasts = Vec::new();
        for prediction in predictions {
            let m = &prediction.metadata;
            if let Some(previous) = seen.insert(&m.prediction_id, prediction) {
                if previous == prediction {
                    output.repeated_prediction_ids.push(m.prediction_id.clone());
                } else {
                    output
                        .reason_codes
                        .push("PREDICTION_ID_CONTENT_CHANGED".into());
                }
                continue;
            }
            if excluded.contains(&m.source) {
                continue;
            }
            if prediction.validate(now).is_err()
                || !fresh(
                    m.generated_at,
                    now,
                    i64::from(self.config.maximum_age_seconds),
                )
                || !fresh(
                    m.observed_at,
                    now,
                    i64::from(self.config.maximum_age_seconds),
                )
                || !fresh(
                    m.available_at,
                    now,
                    i64::from(self.config.maximum_age_seconds),
                )
                || m.instrument_id != context.instrument_id
                || m.market_snapshot_id != context.market_snapshot_id
                || m.horizon != context.horizon
                || m.regime != context.regime
                || self.config.approved_models.get(&m.source) != Some(&m.model_version)
                || m.scenario_hash
                    .as_ref()
                    .is_some_and(|h| h != &context.scenario_hash)
            {
                output
                    .reason_codes
                    .push("INVALID_STALE_OR_INCOMPATIBLE_PREDICTION".into());
                continue;
            }
            if m.calibration.status == CalibrationStatus::Drifted {
                output
                    .reason_codes
                    .push("PREDICTION_CALIBRATION_DRIFTED".into());
                continue;
            }
            sources.insert(m.source);
            output.valid_until = output.valid_until.min(m.valid_until);
            if let Some(age_deadline) =
                m.observed_at
                    .checked_add_signed(Duration::seconds(i64::from(
                        self.config.maximum_age_seconds,
                    )))
            {
                output.valid_until = output.valid_until.min(age_deadline);
            }
            output
                .contributing_prediction_ids
                .push(m.prediction_id.clone());
            for lineage in &m.evidence_lineages {
                output
                    .duplicated_lineages
                    .entry(lineage.clone())
                    .or_default()
                    .push(m.prediction_id.clone());
            }
            output
                .dependence_groups
                .entry(m.dependence_group_id.clone())
                .or_default()
                .push(m.prediction_id.clone());
            match &prediction.payload {
                PredictionPayload::Research {
                    material_conflict: true,
                    ..
                } => output
                    .reason_codes
                    .push("MATERIAL_RESEARCH_CONFLICT".into()),
                PredictionPayload::Sentiment { negative, .. } => {
                    output.maximum_negative_sentiment = Some(
                        output
                            .maximum_negative_sentiment
                            .unwrap_or(Decimal::ZERO)
                            .max(*negative),
                    );
                    if *negative >= Decimal::new(80, 2) {
                        output
                            .reason_codes
                            .push("ADVERSE_SENTIMENT_ENTRY_VETO".into());
                    }
                }
                PredictionPayload::Quantitative { indicators: q } => {
                    if [
                        q.momentum_return,
                        q.atr,
                        q.realized_volatility,
                        q.relative_volume,
                        q.vwap_distance_fraction,
                        q.relative_strength,
                        q.spread_fraction,
                    ]
                    .iter()
                    .any(Option::is_none)
                    {
                        output
                            .reason_codes
                            .push("QUANTITATIVE_COVERAGE_INCOMPLETE".into());
                    }
                }
                PredictionPayload::ScenarioOutcomes { probabilities } => {
                    let c = &m.calibration;
                    if c.status != CalibrationStatus::Validated
                        || c.sample_count < self.config.minimum_calibration_samples
                        || c.report_id.as_ref() != Some(&self.config.outcome_calibration_report_id)
                        || !c.supported_regimes.contains(&context.regime)
                    {
                        output
                            .reason_codes
                            .push("OUTCOME_CALIBRATION_OR_REGIME_UNSUPPORTED".into());
                    } else if let Some(u) = &m.uncertainty {
                        if u.unit == UncertaintyUnit::NetAccountReturn
                            && u.radius <= self.config.maximum_uncertainty
                        {
                            forecasts.push((
                                m.prediction_id.clone(),
                                probabilities.clone(),
                                u.radius,
                            ));
                        } else {
                            output
                                .reason_codes
                                .push("OUTCOME_UNCERTAINTY_UNSUPPORTED".into());
                        }
                    } else {
                        output
                            .reason_codes
                            .push("OUTCOME_UNCERTAINTY_UNKNOWN".into());
                    }
                }
                _ => {}
            }
        }
        for source in [
            PredictionSource::LlmResearch,
            PredictionSource::FinBert,
            PredictionSource::Perpetual,
            PredictionSource::Quantitative,
        ] {
            if !excluded.contains(&source) && !sources.contains(&source) {
                output
                    .reason_codes
                    .push(format!("REQUIRED_SOURCE_UNAVAILABLE_{source:?}"));
            }
        }
        output.duplicated_lineages.retain(|_, ids| ids.len() > 1);
        output.dependence_groups.retain(|_, ids| ids.len() > 1);
        if forecasts.len() != 1 {
            output
                .reason_codes
                .push("ONE_COMPLETE_CALIBRATED_OUTCOME_FORECAST_REQUIRED".into());
        }
        if !excluded.is_empty() {
            output
                .reason_codes
                .push("EXPLORATORY_ABLATION_NOT_ENTRY_READY".into());
        }
        output.reason_codes.sort();
        output.reason_codes.dedup();
        if output.reason_codes.is_empty() {
            let (id, probabilities, uncertainty) = forecasts.remove(0);
            output.status = FusionStatus::Ready;
            output.forecast_id = Some(id);
            output.probabilities = Some(probabilities);
            output.uncertainty = Some(uncertainty);
        }
        Ok(output)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeEstimates {
    pub scenario_hash: String,
    pub forecast_prediction_id: String,
    pub estimation_report_id: String,
    pub cost_model_hash: String,
    pub available_at: DateTime<Utc>,
    pub normalization: ReturnNormalization,
    /// Conditional net account returns: target-first, stop-first, expiry, non-fill.
    /// All fees/FX/spread/slippage are included once, normalized to common E0.
    pub net_account_returns: [Decimal; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnNormalization {
    pub account_id: AccountId,
    pub account_snapshot_id: String,
    pub opening_equity: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UtilityConfig {
    pub version: u32,
    pub lambda_risk: Decimal,
    pub lambda_uncertainty: Decimal,
    pub lambda_complexity: Decimal,
    pub complexity: u32,
    pub cash_hurdle: Decimal,
    pub approved_estimation_report_id: String,
    pub cost_model_hash: String,
    /// All earlier exits remain cash until this common, calendar-resolved instant.
    pub comparison_deadline_at: DateTime<Utc>,
}

impl UtilityConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.version == 1
                && self.lambda_risk >= Decimal::ZERO
                && self.lambda_uncertainty >= Decimal::ZERO
                && self.lambda_complexity >= Decimal::ZERO
                && self.cash_hurdle >= Decimal::ZERO
                && valid_prediction_id(&self.approved_estimation_report_id)
                && valid_prediction_hash(&self.cost_model_hash),
            "invalid frozen utility configuration",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScenarioUtility {
    scenario_hash: String,
    decision_at: DateTime<Utc>,
    comparison_deadline_at: DateTime<Utc>,
    evaluated_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    configuration: UtilityConfig,
    normalization: ReturnNormalization,
    expected_net_account_return: Decimal,
    expected_downside: Decimal,
    uncertainty: Decimal,
    utility: Decimal,
}
impl ScenarioUtility {
    pub fn utility(&self) -> Decimal {
        self.utility
    }
    pub fn expected_net_account_return(&self) -> Decimal {
        self.expected_net_account_return
    }
    pub fn expected_downside(&self) -> Decimal {
        self.expected_downside
    }
    pub fn scenario_hash(&self) -> &str {
        &self.scenario_hash
    }
}

/// Requires a full calibrated distribution AND independently resolved payoffs.
/// No return/loss/non-fill probability is inferred from a binary hit probability.
pub fn expected_utility(
    fusion: &FusedPrediction,
    payoffs: &OutcomeEstimates,
    config: &UtilityConfig,
    now: DateTime<Utc>,
) -> Result<ScenarioUtility> {
    config.validate()?;
    require(
        fusion.status == FusionStatus::Ready
            && fusion.evaluated_at <= now
            && now < fusion.valid_until
            && fusion.ablated_sources.is_empty()
            && fusion.context.scenario_hash == payoffs.scenario_hash
            && fusion.forecast_id.as_ref() == Some(&payoffs.forecast_prediction_id)
            && payoffs.estimation_report_id == config.approved_estimation_report_id
            && payoffs.cost_model_hash == config.cost_model_hash
            && payoffs.available_at <= fusion.context.horizon.decision_at
            && valid_prediction_id(&payoffs.normalization.account_snapshot_id)
            && payoffs.normalization.opening_equity.amount > Decimal::ZERO
            && fusion.context.horizon.deadline_at <= config.comparison_deadline_at,
        "utility requires compatible full forecasts and approved net payoff estimates",
    )?;
    let probabilities = fusion
        .probabilities
        .as_ref()
        .ok_or_else(|| trady_domain::Error("missing outcome distribution".into()))?;
    let uncertainty = fusion
        .uncertainty
        .ok_or_else(|| trady_domain::Error("missing outcome uncertainty".into()))?;
    let mut expected = Decimal::ZERO;
    let mut downside = Decimal::ZERO;
    for (p, r) in probabilities
        .values()
        .into_iter()
        .zip(payoffs.net_account_returns)
    {
        require(
            r >= -Decimal::ONE,
            "unleveraged account outcome below total account loss",
        )?;
        expected = add(expected, mul(p, r)?)?;
        if r < Decimal::ZERO {
            downside = add(downside, mul(p, sub(Decimal::ZERO, r)?)?)?;
        }
    }
    let utility = sub(
        sub(
            sub(expected, mul(config.lambda_risk, downside)?)?,
            mul(config.lambda_uncertainty, uncertainty)?,
        )?,
        mul(config.lambda_complexity, Decimal::from(config.complexity))?,
    )?;
    Ok(ScenarioUtility {
        scenario_hash: payoffs.scenario_hash.clone(),
        decision_at: fusion.context.horizon.decision_at,
        comparison_deadline_at: config.comparison_deadline_at,
        evaluated_at: now,
        valid_until: fusion.valid_until,
        configuration: config.clone(),
        normalization: payoffs.normalization.clone(),
        expected_net_account_return: expected,
        expected_downside: downside,
        uncertainty,
        utility,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UtilitySelection {
    Candidate {
        scenario_hash: String,
        utility: Decimal,
    },
    NoTrade {
        reason: String,
    },
}

pub fn select_scenario(
    scenarios: &[ScenarioUtility],
    config: &UtilityConfig,
    now: DateTime<Utc>,
) -> Result<UtilitySelection> {
    config.validate()?;
    require(
        scenarios.len() <= 32,
        "unbounded scenario search or invalid cash hurdle",
    )?;
    let mut seen = BTreeSet::new();
    let mut best: Option<&ScenarioUtility> = None;
    for scenario in scenarios {
        require(
            seen.insert(&scenario.scenario_hash)
                && scenario.configuration == *config
                && scenario.evaluated_at <= now
                && now < scenario.valid_until
                && scenarios.first().is_none_or(|first| {
                    first.decision_at == scenario.decision_at
                        && first.comparison_deadline_at == scenario.comparison_deadline_at
                        && first.normalization == scenario.normalization
                }),
            "duplicate scenario or incomparable utility horizon",
        )?;
        if scenario.utility > config.cash_hurdle
            && best.is_none_or(|old| scenario.utility > old.utility)
        {
            best = Some(scenario);
        }
    }
    Ok(match best {
        Some(best) => UtilitySelection::Candidate {
            scenario_hash: best.scenario_hash.clone(),
            utility: best.utility,
        },
        None => UtilitySelection::NoTrade {
            reason: "NO_SUPPORTED_SCENARIO_ABOVE_CASH".into(),
        },
    })
}

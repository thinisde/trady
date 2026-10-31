//! Provider-neutral prediction meanings. Sentiment and subjective confidence are
//! deliberately different payloads from calibrated trading-event probabilities.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{Error, InstrumentId, Result, require};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PredictionSource {
    LlmResearch,
    FinBert,
    Perpetual,
    Quantitative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PredictionTarget {
    ResearchEvidence,
    FinancialTextSentiment,
    TargetBeforeStop,
    ScenarioOutcomes,
    MarketIndicators,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CalibrationStatus {
    Unknown,
    Uncalibrated,
    Validated,
    Drifted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastHorizon {
    pub decision_at: DateTime<Utc>,
    pub deadline_at: DateTime<Utc>,
    pub exchange_sessions: u8,
}
impl ForecastHorizon {
    pub fn validate(&self) -> Result<()> {
        require(
            self.decision_at < self.deadline_at && (1..=5).contains(&self.exchange_sessions),
            "invalid prediction horizon",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UncertaintyUnit {
    ClassProbability,
    NetAccountReturn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictionUncertainty {
    pub unit: UncertaintyUnit,
    /// Nonnegative error radius in the declared units, not subjective confidence.
    pub radius: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationRecord {
    pub status: CalibrationStatus,
    pub calibrator_version: Option<String>,
    pub report_id: Option<String>,
    pub training_cutoff_at: Option<DateTime<Utc>>,
    pub validation_start_at: Option<DateTime<Utc>>,
    pub validation_end_at: Option<DateTime<Utc>>,
    pub sample_count: u32,
    pub out_of_sample: bool,
    pub supported_regimes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictionMetadata {
    pub schema_version: u32,
    pub prediction_id: String,
    pub source: PredictionSource,
    pub model_version: String,
    pub instrument_id: InstrumentId,
    pub market_snapshot_id: String,
    pub scenario_hash: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub generated_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub horizon: ForecastHorizon,
    pub target: PredictionTarget,
    pub target_definition: String,
    pub label_version: String,
    pub uncertainty: Option<PredictionUncertainty>,
    pub calibration: CalibrationRecord,
    pub regime: String,
    pub dependence_group_id: String,
    pub evidence_lineages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeProbabilities {
    pub target_first: Decimal,
    pub stop_first: Decimal,
    pub expiry: Decimal,
    pub non_fill: Decimal,
}
impl OutcomeProbabilities {
    pub fn values(&self) -> [Decimal; 4] {
        [
            self.target_first,
            self.stop_first,
            self.expiry,
            self.non_fill,
        ]
    }
    pub fn validate(&self) -> Result<()> {
        distribution(&self.values())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuantitativeIndicators {
    pub momentum_return: Option<Decimal>,
    pub atr: Option<Decimal>,
    pub realized_volatility: Option<Decimal>,
    pub relative_volume: Option<Decimal>,
    pub vwap_distance_fraction: Option<Decimal>,
    pub relative_strength: Option<Decimal>,
    pub spread_fraction: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PredictionPayload {
    Research {
        subjective_confidence: Option<u8>,
        material_conflict: bool,
    },
    Sentiment {
        positive: Decimal,
        negative: Decimal,
        neutral: Decimal,
    },
    BinaryTargetHit {
        probability: Decimal,
    },
    ScenarioOutcomes {
        probabilities: OutcomeProbabilities,
    },
    Quantitative {
        indicators: QuantitativeIndicators,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prediction {
    pub metadata: PredictionMetadata,
    pub payload: PredictionPayload,
}

pub fn valid_prediction_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}
pub fn valid_prediction_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn probability(value: Decimal) -> bool {
    value >= Decimal::ZERO && value <= Decimal::ONE
}
fn distribution(values: &[Decimal]) -> Result<()> {
    let mut total = Decimal::ZERO;
    for value in values {
        require(
            probability(*value),
            "prediction probability outside zero through one",
        )?;
        total = total
            .checked_add(*value)
            .ok_or_else(|| Error("prediction probability sum overflow".into()))?;
    }
    require(
        total == Decimal::ONE,
        "prediction probability partition must sum exactly to one",
    )
}

impl Prediction {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        let m = &self.metadata;
        m.horizon.validate()?;
        require(
            m.schema_version == 1
                && valid_prediction_id(&m.prediction_id)
                && valid_prediction_id(&m.model_version)
                && valid_prediction_id(&m.market_snapshot_id)
                && valid_prediction_id(&m.label_version)
                && valid_prediction_id(&m.regime)
                && valid_prediction_id(&m.dependence_group_id)
                && !m.target_definition.trim().is_empty()
                && m.target_definition.len() <= 2000
                && m.observed_at <= m.available_at
                && m.available_at <= m.generated_at
                && m.generated_at <= m.horizon.decision_at
                && m.horizon.decision_at <= now
                && m.generated_at <= now
                && now < m.valid_until
                && now < m.horizon.deadline_at,
            "invalid, stale or noncausal prediction metadata",
        )?;
        if let Some(hash) = &m.scenario_hash {
            require(
                valid_prediction_hash(hash),
                "invalid prediction scenario hash",
            )?;
        }
        require(
            m.evidence_lineages.len() <= 64 && m.calibration.supported_regimes.len() <= 32,
            "unbounded prediction lineage or regime support",
        )?;
        let mut lineages = BTreeSet::new();
        require(
            m.evidence_lineages
                .iter()
                .all(|s| valid_prediction_id(s) && lineages.insert(s)),
            "invalid or repeated lineage inside one prediction",
        )?;
        let mut regimes = BTreeSet::new();
        require(
            m.calibration
                .supported_regimes
                .iter()
                .all(|s| valid_prediction_id(s) && regimes.insert(s)),
            "invalid or repeated calibration regime",
        )?;
        if let Some(u) = &m.uncertainty {
            require(
                u.radius >= Decimal::ZERO
                    && (u.unit != UncertaintyUnit::ClassProbability || probability(u.radius)),
                "invalid prediction uncertainty or units",
            )?;
        }
        if m.calibration.status == CalibrationStatus::Validated {
            let c = &m.calibration;
            require(
                c.report_id.as_ref().is_some_and(|v| valid_prediction_id(v))
                    && c.calibrator_version
                        .as_ref()
                        .is_some_and(|v| valid_prediction_id(v))
                    && c.sample_count > 0
                    && c.out_of_sample,
                "validated prediction is missing calibration provenance",
            )?;
            let (Some(training), Some(start), Some(end)) = (
                c.training_cutoff_at,
                c.validation_start_at,
                c.validation_end_at,
            ) else {
                return Err(Error(
                    "validated prediction is missing chronological calibration windows".into(),
                ));
            };
            require(
                training < start && start < end && end < m.generated_at,
                "calibration is not out of sample or precedes no forecast",
            )?;
        }
        match &self.payload {
            PredictionPayload::Research {
                subjective_confidence,
                ..
            } => {
                require(
                    m.source == PredictionSource::LlmResearch
                        && m.target == PredictionTarget::ResearchEvidence
                        && subjective_confidence.is_none_or(|v| v <= 100),
                    "research confidence is not an outcome probability",
                )?;
            }
            PredictionPayload::Sentiment {
                positive,
                negative,
                neutral,
            } => {
                require(
                    m.source == PredictionSource::FinBert
                        && m.target == PredictionTarget::FinancialTextSentiment,
                    "sentiment target/source mismatch",
                )?;
                distribution(&[*positive, *negative, *neutral])?;
            }
            PredictionPayload::BinaryTargetHit { probability: p } => {
                require(
                    m.source == PredictionSource::Perpetual
                        && m.target == PredictionTarget::TargetBeforeStop
                        && m.scenario_hash.is_some()
                        && probability(*p),
                    "binary forecast target/source/probability mismatch",
                )?;
            }
            PredictionPayload::ScenarioOutcomes { probabilities } => {
                require(
                    m.source == PredictionSource::Perpetual
                        && m.target == PredictionTarget::ScenarioOutcomes
                        && m.scenario_hash.is_some(),
                    "scenario forecast target/source mismatch",
                )?;
                probabilities.validate()?;
            }
            PredictionPayload::Quantitative { indicators: q } => {
                require(
                    m.source == PredictionSource::Quantitative
                        && m.target == PredictionTarget::MarketIndicators
                        && q.atr.is_none_or(|v| v > Decimal::ZERO)
                        && q.realized_volatility.is_none_or(|v| v >= Decimal::ZERO)
                        && q.relative_volume.is_none_or(|v| v >= Decimal::ZERO)
                        && q.spread_fraction.is_none_or(probability),
                    "invalid quantitative indicator units",
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScenarioOutcome {
    TargetFirst,
    StopFirst,
    Expiry,
    NonFill,
}
impl ScenarioOutcome {
    pub fn index(self) -> usize {
        match self {
            Self::TargetFirst => 0,
            Self::StopFirst => 1,
            Self::Expiry => 2,
            Self::NonFill => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaturedOutcome {
    pub prediction_id: String,
    pub scenario_hash: String,
    pub label_version: String,
    pub horizon_end_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    /// None denotes a censored/ambiguous label; it cannot count as a win.
    pub outcome: Option<ScenarioOutcome>,
}

//! Advisory model contracts. Numerical advice never grants order or limit authority.

use crate::{Currency, InstrumentId, Result, require};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const TARGET_DEFINITION: &str =
    "Executable net target reached before stop or deadline; stop wins ambiguous observations";

pub fn valid_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessArguments {
    pub instrument_id: InstrumentId,
    pub scenario_calculation_id: String,
    pub market_snapshot_id: String,
    pub profile_review_id: String,
    pub screening_artifact_id: String,
    pub position_id: Option<String>,
}

impl AssessArguments {
    pub fn validate(&self) -> Result<()> {
        require(
            [
                &self.scenario_calculation_id,
                &self.market_snapshot_id,
                &self.profile_review_id,
                &self.screening_artifact_id,
            ]
            .into_iter()
            .all(|id| valid_reference(id))
                && self.position_id.as_deref().is_none_or(valid_reference),
            "invalid perpetual assessment reference",
        )
    }
}

/// Persisted economics terms; changing any term requires a new assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioBinding {
    pub arguments: AssessArguments,
    pub currency: Currency,
    pub entry_price: Decimal,
    pub stop_price: Decimal,
    pub target_price: Decimal,
    pub estimated_cost_fraction: Decimal,
    pub horizon_seconds: u32,
    pub deadline_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketObservation {
    pub instrument_id: InstrumentId,
    pub snapshot_id: String,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub executable_price: Decimal,
    pub fx_rate: Decimal,
}

impl MarketObservation {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_reference(&self.snapshot_id)
                && self.observed_at <= now
                && now < self.valid_until
                && self.executable_price > Decimal::ZERO
                && self.fx_rate > Decimal::ZERO,
            "invalid, future or stale perpetual market observation",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioSnapshot {
    pub binding: ScenarioBinding,
    pub calculated_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub market: MarketObservation,
}

impl ScenarioSnapshot {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        self.binding.arguments.validate()?;
        self.market.validate(now)?;
        let b = &self.binding;
        require(
            self.calculated_at <= now
                && now < self.valid_until
                && self.market.observed_at <= self.calculated_at
                && self.market.instrument_id == b.arguments.instrument_id
                && self.market.snapshot_id == b.arguments.market_snapshot_id
                && b.stop_price > Decimal::ZERO
                && b.entry_price > Decimal::ZERO
                && (b.arguments.position_id.is_some() || b.stop_price < b.entry_price)
                && b.stop_price < b.target_price
                && b.entry_price < b.target_price
                && b.estimated_cost_fraction >= Decimal::ZERO
                && b.estimated_cost_fraction < Decimal::ONE
                && (1..=31_536_000).contains(&b.horizon_seconds)
                && self
                    .calculated_at
                    .checked_add_signed(Duration::seconds(i64::from(b.horizon_seconds)))
                    == Some(b.deadline_at)
                && now < b.deadline_at,
            "invalid, unsupported or stale perpetual scenario",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureLineage {
    pub source_id: String,
    pub source_revision: String,
    pub evidence_id: String,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureValue {
    pub name: String,
    pub value: f64,
    pub lineage: Vec<FeatureLineage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureSnapshot {
    pub snapshot_id: String,
    pub schema_version: String,
    pub as_of: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub features: Vec<FeatureValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentContext {
    pub scenario: ScenarioSnapshot,
    pub features: FeatureSnapshot,
}

impl AssessmentContext {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        self.scenario.validate(now)?;
        let f = &self.features;
        require(
            valid_reference(&f.snapshot_id)
                && valid_reference(&f.schema_version)
                && f.as_of == self.scenario.calculated_at
                && now < f.valid_until
                && !f.features.is_empty()
                && f.features.len() <= 256,
            "invalid or stale perpetual feature snapshot",
        )?;
        let mut names = BTreeSet::new();
        for value in &f.features {
            require(
                valid_reference(&value.name)
                    && names.insert(&value.name)
                    && value.value.is_finite()
                    && !value.lineage.is_empty()
                    && value.lineage.len() <= 32,
                "missing, duplicate or non-finite perpetual feature",
            )?;
            for lineage in &value.lineage {
                require(
                    valid_reference(&lineage.source_id)
                        && valid_reference(&lineage.source_revision)
                        && valid_reference(&lineage.evidence_id)
                        && lineage.observed_at <= lineage.available_at
                        && lineage.available_at <= f.as_of,
                    "perpetual feature lineage is invalid or contains future information",
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureSpec {
    pub name: String,
    pub minimum: f64,
    pub maximum: f64,
    pub reference_mean: f64,
    pub reference_stddev: f64,
    pub max_age_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureSchema {
    pub version: String,
    /// Order is the native model's column order, never sorted at inference time.
    pub features: Vec<FeatureSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelApproval {
    Candidate,
    Validated,
    Shadow,
    Approved,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingLineage {
    pub dataset_id: String,
    pub dataset_revision: String,
    pub training_data_cutoff_at: DateTime<Utc>,
    pub calibration_start_at: DateTime<Utc>,
    pub calibration_end_at: DateTime<Utc>,
    pub validation_start_at: DateTime<Utc>,
    pub validation_end_at: DateTime<Utc>,
    pub purge_seconds: u32,
    pub learning_budget: f64,
    pub random_seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbabilityCalibration {
    pub revision_id: String,
    /// Platt calibration of logit(native predict_proba), fitted offline.
    pub slope: f64,
    pub intercept: f64,
    pub samples: u32,
    pub minimum_samples: u32,
    pub brier_score: f64,
    pub maximum_brier_score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioSupport {
    pub instruments: Vec<InstrumentId>,
    pub minimum_horizon_seconds: u32,
    pub maximum_horizon_seconds: u32,
    pub minimum_target_fraction: Decimal,
    pub maximum_target_fraction: Decimal,
    /// Signed downside distance `(entry_price - stop_price) / entry_price`.
    /// A zero or negative range explicitly admits break-even/profitable holding
    /// stops and requires `supports_positions`. Existing positive ranges never
    /// acquire that support implicitly through an absolute-value conversion.
    pub minimum_stop_fraction: Decimal,
    pub maximum_stop_fraction: Decimal,
    pub maximum_cost_fraction: Decimal,
    pub supports_positions: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelManifest {
    pub model_revision_id: String,
    pub approval: ModelApproval,
    pub artifact_sha256: String,
    pub runtime_version: String,
    pub runtime_source_revision: String,
    pub model_format: String,
    pub feature_schema: FeatureSchema,
    pub label_version: String,
    pub target_definition: String,
    pub training: TrainingLineage,
    pub validation_report_id: String,
    pub calibration: ProbabilityCalibration,
    pub support: ScenarioSupport,
    pub approved_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub max_assessment_age_seconds: u32,
    pub quote_drift_tolerance_fraction: Decimal,
    pub fx_drift_tolerance_fraction: Decimal,
    /// Standard deviations from the training reference; label-free checks only.
    pub drift_warning_z: f64,
    pub drift_rejection_z: f64,
}

impl ModelManifest {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        let t = &self.training;
        let s = &self.support;
        let c = &self.calibration;
        require(
            self.approval == ModelApproval::Approved
                && valid_reference(&self.model_revision_id)
                && valid_reference(&self.feature_schema.version)
                && valid_reference(&self.label_version)
                && valid_reference(&self.validation_report_id)
                && valid_reference(&t.dataset_id)
                && valid_reference(&t.dataset_revision)
                && valid_reference(&c.revision_id)
                && self.artifact_sha256.len() == 64
                && self
                    .artifact_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && self.target_definition == TARGET_DEFINITION
                && self.approved_at <= now
                && now < self.valid_until
                && (1..=86_400).contains(&self.max_assessment_age_seconds),
            "model is unapproved, expired or has invalid provenance",
        )?;
        require(
            !s.instruments.is_empty()
                && s.instruments.len() <= 1000
                && s.instruments.iter().collect::<BTreeSet<_>>().len() == s.instruments.len()
                && s.minimum_horizon_seconds > 0
                && s.minimum_horizon_seconds <= s.maximum_horizon_seconds
                && s.maximum_horizon_seconds <= 31_536_000
                && s.minimum_target_fraction > Decimal::ZERO
                && s.minimum_target_fraction <= s.maximum_target_fraction
                && (s.supports_positions || s.minimum_stop_fraction > Decimal::ZERO)
                && s.minimum_stop_fraction <= s.maximum_stop_fraction
                && s.maximum_stop_fraction < Decimal::ONE
                && s.maximum_cost_fraction >= Decimal::ZERO
                && s.maximum_cost_fraction < Decimal::ONE
                && (Decimal::ZERO..=Decimal::ONE).contains(&self.quote_drift_tolerance_fraction)
                && (Decimal::ZERO..=Decimal::ONE).contains(&self.fx_drift_tolerance_fraction),
            "invalid perpetual scenario support or drift tolerance",
        )?;
        let purge = Duration::seconds(i64::from(t.purge_seconds));
        require(
            t.learning_budget.is_finite()
                && t.learning_budget > 0.0
                && t.purge_seconds >= s.maximum_horizon_seconds
                && t.training_data_cutoff_at
                    .checked_add_signed(purge)
                    .is_some_and(|v| v <= t.calibration_start_at)
                && t.calibration_start_at < t.calibration_end_at
                && t.calibration_end_at
                    .checked_add_signed(purge)
                    .is_some_and(|v| v <= t.validation_start_at)
                && t.validation_start_at < t.validation_end_at
                && t.validation_end_at <= self.approved_at,
            "training, calibration and validation must be chronological with label-horizon purging",
        )?;
        require(
            c.slope.is_finite()
                && c.slope > 0.0
                && c.intercept.is_finite()
                && c.minimum_samples > 0
                && c.samples >= c.minimum_samples
                && c.brier_score.is_finite()
                && (0.0..=1.0).contains(&c.brier_score)
                && c.maximum_brier_score.is_finite()
                && (0.0..=1.0).contains(&c.maximum_brier_score)
                && c.brier_score <= c.maximum_brier_score,
            "insufficient or invalid model calibration",
        )?;
        require(
            self.drift_warning_z.is_finite()
                && self.drift_warning_z > 0.0
                && self.drift_rejection_z.is_finite()
                && self.drift_rejection_z > self.drift_warning_z,
            "invalid model drift bounds",
        )?;
        let mut names = BTreeSet::new();
        require(
            !self.feature_schema.features.is_empty() && self.feature_schema.features.len() <= 256,
            "invalid model feature count",
        )?;
        for f in &self.feature_schema.features {
            require(
                valid_reference(&f.name)
                    && names.insert(&f.name)
                    && f.minimum.is_finite()
                    && f.maximum.is_finite()
                    && f.minimum < f.maximum
                    && f.reference_mean.is_finite()
                    && (f.minimum..=f.maximum).contains(&f.reference_mean)
                    && f.reference_stddev.is_finite()
                    && f.reference_stddev > 0.0
                    && f.max_age_seconds > 0,
                "invalid ordered model feature schema",
            )?;
        }
        Ok(())
    }

    pub fn supports(&self, context: &AssessmentContext) -> bool {
        let b = &context.scenario.binding;
        let s = &self.support;
        let Some(target) = b
            .target_price
            .checked_sub(b.entry_price)
            .and_then(|v| v.checked_div(b.entry_price))
        else {
            return false;
        };
        let Some(stop) = b
            .entry_price
            .checked_sub(b.stop_price)
            .and_then(|v| v.checked_div(b.entry_price))
        else {
            return false;
        };
        b.entry_price > Decimal::ZERO
            && b.stop_price > Decimal::ZERO
            && b.stop_price < b.target_price
            && b.entry_price < b.target_price
            && (b.arguments.position_id.is_some() || stop > Decimal::ZERO)
            && self.feature_schema.version == context.features.schema_version
            && s.instruments.contains(&b.arguments.instrument_id)
            && (s.minimum_horizon_seconds..=s.maximum_horizon_seconds).contains(&b.horizon_seconds)
            && (s.minimum_target_fraction..=s.maximum_target_fraction).contains(&target)
            && (s.minimum_stop_fraction..=s.maximum_stop_fraction).contains(&stop)
            && b.estimated_cost_fraction <= s.maximum_cost_fraction
            && (b.arguments.position_id.is_none() || s.supports_positions)
    }

    pub fn check_features(
        &self,
        context: &AssessmentContext,
        now: DateTime<Utc>,
    ) -> Result<DriftStatus> {
        context.validate(now)?;
        require(
            self.supports(context),
            "model does not support this scenario or feature schema",
        )?;
        require(
            self.feature_schema.features.len() == context.features.features.len(),
            "perpetual feature count changed",
        )?;
        let mut status = DriftStatus::InRange;
        for (spec, value) in self
            .feature_schema
            .features
            .iter()
            .zip(&context.features.features)
        {
            require(spec.name == value.name, "perpetual feature order changed")?;
            require(
                value.lineage.iter().all(|source| {
                    now.signed_duration_since(source.observed_at)
                        <= Duration::seconds(i64::from(spec.max_age_seconds))
                }),
                "perpetual feature source is stale",
            )?;
            let z = ((value.value - spec.reference_mean) / spec.reference_stddev).abs();
            if !(spec.minimum..=spec.maximum).contains(&value.value)
                || !z.is_finite()
                || z >= self.drift_rejection_z
            {
                return Ok(DriftStatus::OutOfRange);
            }
            if z >= self.drift_warning_z {
                status = DriftStatus::Warning;
            }
        }
        Ok(status)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentStatus {
    Ok,
    Abstain,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CalibrationStatus {
    Validated,
    Insufficient,
    NotSupported,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DriftStatus {
    InRange,
    Warning,
    OutOfRange,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LimitsAuthority {
    AdvisoryOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contribution {
    pub feature_name: String,
    pub contribution: Decimal,
    pub interpretation: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnInterval {
    pub low_net_return_fraction: Decimal,
    pub high_net_return_fraction: Decimal,
    pub nominal_coverage: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub assessment_version: u32,
    pub assessment_id: String,
    pub status: AssessmentStatus,
    pub instrument_id: InstrumentId,
    pub scenario_calculation_id: String,
    pub market_snapshot_id: String,
    pub feature_snapshot_id: Option<String>,
    pub model_revision_id: Option<String>,
    pub training_data_cutoff_at: Option<DateTime<Utc>>,
    pub validation_report_id: Option<String>,
    pub as_of: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub target_definition: String,
    pub target_hit_probability: Option<Decimal>,
    pub expected_net_return_fraction: Option<Decimal>,
    pub return_interval: Option<ReturnInterval>,
    pub calibration_status: CalibrationStatus,
    pub drift_status: DriftStatus,
    pub top_contributions: Vec<Contribution>,
    pub limitations: Vec<String>,
    pub limits_authority: LimitsAuthority,
}

impl Assessment {
    pub fn unavailable(
        id: String,
        args: &AssessArguments,
        now: DateTime<Utc>,
        reason: &str,
    ) -> Self {
        Self {
            assessment_version: 1,
            assessment_id: id,
            status: AssessmentStatus::Unavailable,
            instrument_id: args.instrument_id.clone(),
            scenario_calculation_id: args.scenario_calculation_id.clone(),
            market_snapshot_id: args.market_snapshot_id.clone(),
            feature_snapshot_id: None,
            model_revision_id: None,
            training_data_cutoff_at: None,
            validation_report_id: None,
            as_of: now,
            valid_until: now,
            target_definition: TARGET_DEFINITION.into(),
            target_hit_probability: None,
            expected_net_return_fraction: None,
            return_interval: None,
            calibration_status: CalibrationStatus::Unknown,
            drift_status: DriftStatus::Unknown,
            top_contributions: vec![],
            limitations: vec![reason.into()],
            limits_authority: LimitsAuthority::AdvisoryOnly,
        }
    }

    pub fn validate(&self) -> Result<()> {
        require(
            self.assessment_version == 1
                && valid_reference(&self.assessment_id)
                && valid_reference(&self.scenario_calculation_id)
                && valid_reference(&self.market_snapshot_id)
                && self
                    .feature_snapshot_id
                    .as_deref()
                    .is_none_or(valid_reference)
                && self
                    .model_revision_id
                    .as_deref()
                    .is_none_or(valid_reference)
                && self
                    .validation_report_id
                    .as_deref()
                    .is_none_or(valid_reference)
                && self.as_of <= self.valid_until
                && self.target_definition == TARGET_DEFINITION
                && self.limits_authority == LimitsAuthority::AdvisoryOnly
                && self.top_contributions.len() <= 8
                && self.limitations.len() <= 16
                && self
                    .limitations
                    .iter()
                    .all(|s| !s.trim().is_empty() && s.len() <= 2000)
                && self
                    .target_hit_probability
                    .is_none_or(|p| (Decimal::ZERO..=Decimal::ONE).contains(&p)),
            "invalid perpetual assessment",
        )?;
        if let Some(interval) = &self.return_interval {
            require(
                interval.low_net_return_fraction <= interval.high_net_return_fraction
                    && (Decimal::ZERO..=Decimal::ONE).contains(&interval.nominal_coverage),
                "invalid perpetual return interval",
            )?;
        }
        if self.status == AssessmentStatus::Ok {
            require(
                self.feature_snapshot_id.is_some()
                    && self.model_revision_id.is_some()
                    && self.training_data_cutoff_at.is_some_and(|t| t < self.as_of)
                    && self.validation_report_id.is_some()
                    && self.target_hit_probability.is_some()
                    && self.as_of < self.valid_until
                    && self.calibration_status == CalibrationStatus::Validated
                    && self.drift_status == DriftStatus::InRange,
                "OK assessment requires current validated, in-range model advice",
            )?;
        } else {
            require(
                self.target_hit_probability.is_none()
                    && self.expected_net_return_fraction.is_none()
                    && self.return_interval.is_none()
                    && self.top_contributions.is_empty()
                    && !self.limitations.is_empty(),
                "unavailable or abstaining advice cannot carry predictions",
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentRecord {
    pub assessment: Assessment,
    /// Full immutable input retained under the repository's evidence retention rules.
    pub context: AssessmentContext,
    pub model: Option<ModelManifest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentInfluence {
    Used,
    Disagreed,
    Abstained,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerpetualReview {
    pub assessment_id: String,
    pub influence: AssessmentInfluence,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-31T12:00:00Z".parse().unwrap()
    }
    fn args() -> AssessArguments {
        AssessArguments {
            instrument_id: InstrumentId::new("US_TEST").unwrap(),
            scenario_calculation_id: "scenario_1".into(),
            market_snapshot_id: "market_1".into(),
            profile_review_id: "profile_1".into(),
            screening_artifact_id: "screening_1".into(),
            position_id: None,
        }
    }

    #[test]
    fn reference_only_tool_arguments_reject_runtime_model_paths() {
        let mut value = serde_json::to_value(args()).unwrap();
        value["model_path"] = "/tmp/untrusted.json".into();
        assert!(serde_json::from_value::<AssessArguments>(value).is_err());
        let mut a = args();
        a.scenario_calculation_id = "../model.json".into();
        assert!(a.validate().is_err());
    }

    #[test]
    fn only_position_scenarios_admit_break_even_or_profitable_stops() {
        let at = now();
        let mut scenario = ScenarioSnapshot {
            binding: ScenarioBinding {
                arguments: args(),
                currency: Currency::new("USD").unwrap(),
                entry_price: Decimal::new(9804, 2),
                stop_price: Decimal::from(99),
                target_price: Decimal::from(100),
                estimated_cost_fraction: Decimal::ZERO,
                horizon_seconds: 3600,
                deadline_at: at + Duration::hours(1),
            },
            calculated_at: at,
            valid_until: at + Duration::seconds(30),
            market: MarketObservation {
                instrument_id: args().instrument_id,
                snapshot_id: "market_1".into(),
                observed_at: at,
                valid_until: at + Duration::seconds(30),
                executable_price: Decimal::new(9901, 2),
                fx_rate: Decimal::new(102, 2),
            },
        };
        assert!(
            scenario.validate(at).is_err(),
            "entry stop still must be below entry"
        );
        scenario.binding.arguments.position_id = Some("test_only_holding".into());
        scenario.validate(at).unwrap();
        scenario.binding.stop_price = scenario.binding.entry_price;
        scenario.validate(at).unwrap();
        for stop in [Decimal::ZERO, Decimal::from(100), Decimal::from(101)] {
            scenario.binding.stop_price = stop;
            assert!(
                scenario.validate(at).is_err(),
                "invalid holding stop {stop}"
            );
        }
        scenario.binding.stop_price = Decimal::from(99);
        scenario.binding.entry_price = Decimal::ZERO;
        assert!(scenario.validate(at).is_err());
    }

    #[test]
    fn unavailable_assessment_has_no_actionable_numbers() {
        let mut a =
            Assessment::unavailable("assessment_1".into(), &args(), now(), "No approved model");
        a.validate().unwrap();
        a.target_hit_probability = Some(Decimal::new(9, 1));
        assert!(a.validate().is_err());
        a.status = AssessmentStatus::Ok;
        assert!(
            a.validate().is_err(),
            "numeric advice alone cannot produce OK status"
        );
    }

    #[test]
    fn invalid_probability_intervals_and_future_cutoffs_are_rejected() {
        let mut a = Assessment::unavailable("assessment_1".into(), &args(), now(), "fixture");
        a.status = AssessmentStatus::Ok;
        a.feature_snapshot_id = Some("features_1".into());
        a.model_revision_id = Some("model_1".into());
        a.training_data_cutoff_at = Some(now() - Duration::days(10));
        a.validation_report_id = Some("validation_1".into());
        a.target_hit_probability = Some(Decimal::new(5, 1));
        a.calibration_status = CalibrationStatus::Validated;
        a.drift_status = DriftStatus::InRange;
        a.valid_until = now() + Duration::seconds(60);
        a.validate().unwrap();
        a.target_hit_probability = Some(Decimal::new(101, 2));
        assert!(a.validate().is_err());
        a.target_hit_probability = Some(Decimal::new(5, 1));
        a.training_data_cutoff_at = Some(now());
        assert!(a.validate().is_err());
        a.training_data_cutoff_at = Some(now() - Duration::days(10));
        a.return_interval = Some(ReturnInterval {
            low_net_return_fraction: Decimal::ONE,
            high_net_return_fraction: Decimal::ZERO,
            nominal_coverage: Decimal::ONE,
        });
        assert!(a.validate().is_err());
    }
}

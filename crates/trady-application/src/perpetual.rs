//! Provider-neutral advisory use case and persisted final-scenario validation.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use trady_domain::{Error, Result, perpetual::*, require};

/// Implementations join and verify actual scenario, market, profile, screening,
/// position and evidence rows. No numeric features are accepted from a tool call.
pub trait PerpetualRepository {
    fn load_context(&mut self, arguments: &AssessArguments) -> Result<AssessmentContext>;
    /// Insert immutable assessment, input snapshot and model provenance atomically.
    /// Reject duplicate IDs and nonexistent or differently bound references.
    fn save_assessment(&mut self, record: &AssessmentRecord) -> Result<()>;
    fn load_assessment(&mut self, assessment_id: &str) -> Result<Option<AssessmentRecord>>;
    fn save_review(
        &mut self,
        review: &PerpetualReview,
        final_scenario: &ScenarioBinding,
        checked_at: DateTime<Utc>,
    ) -> Result<()>;
}

pub struct AdvisoryOutput {
    pub assessment: Assessment,
    pub model: Option<ModelManifest>,
}

/// Inference only. Implementations have no broker, fit, promotion or runtime-path API.
pub trait PerpetualAdvisor {
    fn assess(
        &self,
        assessment_id: &str,
        context: &AssessmentContext,
        now: DateTime<Utc>,
    ) -> Result<AdvisoryOutput>;
}

/// Return advice only after its immutable input and output have been persisted.
pub fn assess(
    repository: &mut impl PerpetualRepository,
    advisor: &impl PerpetualAdvisor,
    arguments: &AssessArguments,
    now: DateTime<Utc>,
) -> Result<Assessment> {
    arguments.validate()?;
    let context = repository.load_context(arguments)?;
    require(
        &context.scenario.binding.arguments == arguments,
        "perpetual repository returned differently bound references",
    )?;
    context.validate(now)?;
    let id = format!("ml_{}", uuid::Uuid::new_v4().simple());
    let output = advisor.assess(&id, &context, now)?;
    require(
        output.assessment.assessment_id == id && output.assessment.as_of == now,
        "perpetual advisor returned a different assessment identity or time",
    )?;
    let record = AssessmentRecord {
        assessment: output.assessment,
        context,
        model: output.model,
    };
    validate_record(&record)?;
    repository.save_assessment(&record)?;
    Ok(record.assessment)
}

/// Revalidate the persisted model/input/output relationship, independently of the adapter.
pub fn validate_record(record: &AssessmentRecord) -> Result<()> {
    let a = &record.assessment;
    let c = &record.context;
    let args = &c.scenario.binding.arguments;
    a.validate()?;
    args.validate()?;
    require(
        a.instrument_id == args.instrument_id
            && a.scenario_calculation_id == args.scenario_calculation_id
            && a.market_snapshot_id == args.market_snapshot_id
            && a.feature_snapshot_id
                .as_ref()
                .is_none_or(|id| id == &c.features.snapshot_id),
        "assessment references do not match the persisted scenario",
    )?;
    if let Some(model) = &record.model {
        require(
            a.model_revision_id.as_ref() == Some(&model.model_revision_id)
                && a.training_data_cutoff_at == Some(model.training.training_data_cutoff_at)
                && a.validation_report_id.as_ref() == Some(&model.validation_report_id),
            "assessment model provenance does not match the approved manifest",
        )?;
    }
    if a.status == AssessmentStatus::Ok {
        let model = record
            .model
            .as_ref()
            .ok_or_else(|| Error("OK advice has no approved model".into()))?;
        model.validate(a.as_of)?;
        require(
            model.check_features(c, a.as_of)? == DriftStatus::InRange,
            "OK advice uses stale or drifting features",
        )?;
        let maximum_age = a
            .as_of
            .checked_add_signed(Duration::seconds(i64::from(
                model.max_assessment_age_seconds,
            )))
            .ok_or_else(|| Error("perpetual assessment expiry overflow".into()))?;
        require(
            a.feature_snapshot_id.as_ref() == Some(&c.features.snapshot_id)
                && a.valid_until <= maximum_age
                && a.valid_until <= model.valid_until
                && a.valid_until <= c.features.valid_until
                && a.valid_until <= c.scenario.valid_until
                && a.valid_until <= c.scenario.market.valid_until
                && a.valid_until <= c.scenario.binding.deadline_at,
            "perpetual assessment exceeds model or input validity",
        )?;
        // This contract supports one calibrated binary output only.
        require(
            a.expected_net_return_fraction.is_none()
                && a.return_interval.is_none()
                && a.top_contributions.is_empty(),
            "unsupported perpetual regression or explanation output",
        )?;
    }
    Ok(())
}

fn validate_review(review: &PerpetualReview, assessment: &Assessment) -> Result<()> {
    require(
        valid_reference(&review.assessment_id)
            && review.assessment_id == assessment.assessment_id
            && !review.reason.trim().is_empty()
            && review.reason.len() <= 2000,
        "perpetual review requires the actual assessment and a reason",
    )?;
    let matches = match assessment.status {
        AssessmentStatus::Ok => matches!(
            review.influence,
            AssessmentInfluence::Used | AssessmentInfluence::Disagreed
        ),
        AssessmentStatus::Abstain => review.influence == AssessmentInfluence::Abstained,
        AssessmentStatus::Unavailable => review.influence == AssessmentInfluence::Unavailable,
    };
    require(
        matches,
        "perpetual review influence misrepresents the assessment",
    )
}

/// A readiness gate for entries and target raises; it never applies a probability
/// threshold, changes quantities or grants permission to place an order.
pub fn validate_review_for_scenario(
    repository: &mut impl PerpetualRepository,
    review: &PerpetualReview,
    final_scenario: &ScenarioBinding,
    current_market: &MarketObservation,
    now: DateTime<Utc>,
) -> Result<Assessment> {
    let record = repository
        .load_assessment(&review.assessment_id)?
        .ok_or_else(|| Error("perpetual assessment does not exist".into()))?;
    validate_record(&record)?;
    validate_review(review, &record.assessment)?;
    let a = &record.assessment;
    require(
        a.status == AssessmentStatus::Ok && a.as_of <= now && now < a.valid_until,
        "a fresh OK perpetual assessment is required",
    )?;
    require(
        &record.context.scenario.binding == final_scenario,
        "final scenario differs from the assessed stop, target, horizon, costs or references",
    )?;
    let model = record
        .model
        .as_ref()
        .ok_or_else(|| Error("assessment model is missing".into()))?;
    model.validate(now)?;
    require(
        model.check_features(&record.context, now)? == DriftStatus::InRange,
        "assessment features have become stale or drifted",
    )?;
    current_market.validate(now)?;
    let original = &record.context.scenario.market;
    require(
        current_market.instrument_id == final_scenario.arguments.instrument_id
            && current_market.observed_at >= original.observed_at,
        "final perpetual quote has the wrong instrument or moved backwards in time",
    )?;
    require(
        within_drift(
            current_market.executable_price,
            original.executable_price,
            model.quote_drift_tolerance_fraction,
        ) && within_drift(
            current_market.fx_rate,
            original.fx_rate,
            model.fx_drift_tolerance_fraction,
        ),
        "quote or FX moved beyond evaluated model tolerance; reassessment required",
    )?;
    repository.save_review(review, final_scenario, now)?;
    Ok(record.assessment)
}

fn within_drift(current: Decimal, original: Decimal, tolerance: Decimal) -> bool {
    current
        .checked_sub(original)
        .map(|v| v.abs())
        .and_then(|v| v.checked_div(original))
        .is_some_and(|v| v <= tolerance)
}

/// Store research-only abstention/unavailability or the analyst's recorded use.
/// Activation must separately call `validate_review_for_scenario`.
pub fn record_review(
    repository: &mut impl PerpetualRepository,
    review: &PerpetualReview,
    now: DateTime<Utc>,
) -> Result<()> {
    let record = repository
        .load_assessment(&review.assessment_id)?
        .ok_or_else(|| Error("perpetual assessment does not exist".into()))?;
    validate_record(&record)?;
    validate_review(review, &record.assessment)?;
    require(
        record.assessment.as_of <= now,
        "cannot review a future assessment",
    )?;
    repository.save_review(review, &record.context.scenario.binding, now)
}

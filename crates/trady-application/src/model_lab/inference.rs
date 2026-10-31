use super::*;
use crate::optimization::{
    BoundCalculation, FormulaAuthority, ScenarioPredictionInput, calculate_authoritative_limits,
};
use trady_domain::{InstrumentId, prediction::*};
use trady_policy::{
    Policy,
    limits::LimitsInput,
    prediction::{FusionContext, OutcomeEstimates, ReturnNormalization},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalObservation {
    pub model_version: String,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    /// Original extractor/inference instant; serving cannot refresh this timestamp.
    pub generated_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub evidence_lineages: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplementalSignals {
    pub research: SignalObservation,
    pub subjective_confidence: Option<u8>,
    pub material_conflict: bool,
    pub sentiment: SignalObservation,
    pub positive: Decimal,
    pub negative: Decimal,
    pub neutral: Decimal,
    pub quantitative: SignalObservation,
    pub indicators: QuantitativeIndicators,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastObservation {
    pub instrument_id: InstrumentId,
    pub market_snapshot_id: String,
    pub feature_schema_version: String,
    pub feature_names: Vec<String>,
    pub features: Vec<Decimal>,
    pub regime: String,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub generated_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub evidence_lineages: Vec<String>,
    pub signals: SupplementalSignals,
}

fn scenario(input: &LimitsInput, calculation: &BoundCalculation) -> Result<ScenarioFeatures> {
    let limits = &calculation.limits;
    Ok(ScenarioFeatures {
        target_net_fraction: limits.target_net_fraction,
        stop_distance_fraction: trady_policy::div(
            trady_policy::sub(limits.buy_limit, limits.stop_trigger)?,
            limits.buy_limit,
        )?,
        spread_fraction: trady_policy::div(
            trady_policy::sub(input.quote.ask, input.quote.bid)?,
            input.quote.ask,
        )?,
        exchange_sessions: limits.max_holding_sessions,
    })
}

fn metadata(
    signal: &SignalObservation,
    source: PredictionSource,
    target: PredictionTarget,
    context: &FusionContext,
    deadline: DateTime<Utc>,
    payload: &PredictionPayload,
) -> Result<PredictionMetadata> {
    let id = format!("signal_{}", digest(&(signal, source, context, payload))?);
    Ok(PredictionMetadata {
        schema_version: 1,
        prediction_id: id,
        source,
        model_version: signal.model_version.clone(),
        instrument_id: context.instrument_id.clone(),
        market_snapshot_id: context.market_snapshot_id.clone(),
        scenario_hash: Some(context.scenario_hash.clone()),
        observed_at: signal.observed_at,
        available_at: signal.available_at,
        generated_at: signal.generated_at,
        valid_until: deadline.min(signal.valid_until),
        horizon: context.horizon.clone(),
        target,
        target_definition:
            "Verified point-in-time research/financial-text/market observations; no price forecast"
                .into(),
        label_version: "verified-observations-v1".into(),
        uncertainty: None,
        calibration: CalibrationRecord {
            status: CalibrationStatus::Uncalibrated,
            calibrator_version: None,
            report_id: None,
            training_cutoff_at: None,
            validation_start_at: None,
            validation_end_at: None,
            sample_count: 0,
            out_of_sample: false,
            supported_regimes: vec![],
        },
        regime: context.regime.clone(),
        dependence_group_id: format!("observations_{}", digest(&signal.evidence_lineages)?),
        evidence_lineages: signal.evidence_lineages.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn infer_scenario(
    model: &ApprovedModel,
    authority: &FormulaAuthority,
    policy: &Policy,
    input: &LimitsInput,
    calculation: &BoundCalculation,
    observation: &ForecastObservation,
    opening_equity: &Money,
    now: DateTime<Utc>,
) -> Result<ScenarioPredictionInput> {
    model.validate(now)?;
    build(
        model,
        authority,
        policy,
        input,
        calculation,
        observation,
        opening_equity,
        now,
    )
}

/// Explicit offline diagnostic. The caller must preserve this wrapper and must
/// never publish its contents on a production prediction route.
#[derive(Debug, Clone, Serialize)]
pub struct OfflinePrediction {
    pub test_only: bool,
    pub deployment_eligible: bool,
    pub prediction: ScenarioPredictionInput,
}
#[allow(clippy::too_many_arguments)]
pub fn infer_offline(
    model: &ApprovedModel,
    authority: &FormulaAuthority,
    policy: &Policy,
    input: &LimitsInput,
    calculation: &BoundCalculation,
    observation: &ForecastObservation,
    opening_equity: &Money,
    now: DateTime<Utc>,
) -> Result<OfflinePrediction> {
    model.candidate.validate()?;
    require(
        model.evaluation.candidate_sha256 == model.candidate.hash()?,
        "offline evaluation mismatch",
    )?;
    Ok(OfflinePrediction {
        test_only: model.candidate.test_only || model.evaluation.test_only,
        deployment_eligible: false,
        prediction: build(
            model,
            authority,
            policy,
            input,
            calculation,
            observation,
            opening_equity,
            now,
        )?,
    })
}

#[allow(clippy::too_many_arguments)]
fn build(
    model: &ApprovedModel,
    authority: &FormulaAuthority,
    policy: &Policy,
    input: &LimitsInput,
    calculation: &BoundCalculation,
    o: &ForecastObservation,
    opening_equity: &Money,
    now: DateTime<Utc>,
) -> Result<ScenarioPredictionInput> {
    require(
        calculate_authoritative_limits(authority, policy, input, calculation.limits.calculated_at)?
            == *calculation,
        "prediction request differs from approved exact Rust calculation",
    )?;
    let c = &model.candidate;
    let limits = &calculation.limits;
    require(
        now >= limits.calculated_at
            && now < limits.valid_until
            && o.instrument_id == input.quote.instrument_id
            && o.market_snapshot_id == input.market_snapshot_id
            && o.feature_schema_version == c.feature_schema_version
            && o.feature_names == c.feature_names
            && c.supported_regimes.contains(&o.regime)
            && o.observed_at <= o.available_at
            && o.available_at <= o.generated_at
            && o.generated_at <= now
            && now < o.valid_until
            && opening_equity.currency == input.account.spendable_cash.currency
            && opening_equity.amount >= limits.acquisition_cost.amount
            && opening_equity.amount > Decimal::ZERO
            && model.evaluation.evaluation_ended_at < now,
        "unavailable, noncausal, unbound or unsupported forecast observations",
    )?;
    let scenario_hash = calculation
        .calculation_id
        .strip_prefix("limits_")
        .filter(|v| valid_hash(v))
        .ok_or_else(|| Error("invalid calculation identity".into()))?
        .to_owned();
    let context = FusionContext {
        instrument_id: o.instrument_id.clone(),
        market_snapshot_id: o.market_snapshot_id.clone(),
        scenario_hash,
        horizon: ForecastHorizon {
            decision_at: now,
            deadline_at: limits.holding_deadline_at,
            exchange_sessions: limits.max_holding_sessions,
        },
        regime: o.regime.clone(),
    };
    let allocation = trady_policy::div(limits.acquisition_cost.amount, opening_equity.amount)?;
    let (p, r) = fit::predict(c, &o.features, &scenario(input, calculation)?)?;
    // Integer quantization guarantees an exact decimal partition without optimistic renormalization.
    let units = 1_000_000_000_000_i64;
    let mut q = p.map(|p| (p * units as f64).floor() as i64);
    let largest = (0..4)
        .max_by(|a, b| p[*a].total_cmp(&p[*b]))
        .ok_or_else(|| Error("empty probability partition".into()))?;
    q[largest] += units - q.iter().sum::<i64>();
    let probabilities = OutcomeProbabilities {
        target_first: Decimal::new(q[0], 12),
        stop_first: Decimal::new(q[1], 12),
        expiry: Decimal::new(q[2], 12),
        non_fill: Decimal::new(q[3], 12),
    };
    probabilities.validate()?;
    let valid_until = limits
        .valid_until
        .min(o.valid_until)
        .min(model.approval.valid_until);
    let forecast_id = format!("forecast_{}", digest(&(c.hash()?, &context, o))?);
    // The sealed holdout reports error; it never refits or enlarges calibration.
    let radius = c.uncertainty_radius;
    let forecast=Prediction {
        metadata:PredictionMetadata {
            schema_version:1,prediction_id:forecast_id.clone(),
            // v1's complete-forecast slot is named PERPETUAL. This distinct model
            // revision is never a native binary artifact or binary conversion.
            source:PredictionSource::Perpetual,model_version:c.plan.model_id.clone(),instrument_id:o.instrument_id.clone(),
            market_snapshot_id:o.market_snapshot_id.clone(),scenario_hash:Some(context.scenario_hash.clone()),
            observed_at:o.observed_at,available_at:o.available_at,generated_at:now,valid_until,horizon:context.horizon.clone(),
            target:PredictionTarget::ScenarioOutcomes,target_definition:"Executable target before stop; stop before target; filled expiry; unfilled expiry. Reconciled costs once; four-outcome-v1".into(),
            label_version:c.label_version.clone(),uncertainty:Some(PredictionUncertainty {unit:UncertaintyUnit::NetAccountReturn,
                radius:trady_policy::mul(decimal(radius)?,allocation)?}),
            calibration:CalibrationRecord {status:CalibrationStatus::Validated,calibrator_version:Some(model.calibrator_id()?),
                report_id:Some(model.report_id()?),training_cutoff_at:Some(c.plan.train.end),
                validation_start_at:Some(model.evaluation.evaluation_started_at),validation_end_at:Some(model.evaluation.evaluation_ended_at),
                sample_count:model.evaluation.metrics.samples,out_of_sample:true,supported_regimes:c.supported_regimes.clone()},
            regime:o.regime.clone(),dependence_group_id:format!("model_{}",c.hash()?),evidence_lineages:o.evidence_lineages.clone(),
        },payload:PredictionPayload::ScenarioOutcomes {probabilities},
    };
    let s = &o.signals;
    let payloads = [
        (
            &s.research,
            PredictionSource::LlmResearch,
            PredictionTarget::ResearchEvidence,
            PredictionPayload::Research {
                subjective_confidence: s.subjective_confidence,
                material_conflict: s.material_conflict,
            },
        ),
        (
            &s.sentiment,
            PredictionSource::FinBert,
            PredictionTarget::FinancialTextSentiment,
            PredictionPayload::Sentiment {
                positive: s.positive,
                negative: s.negative,
                neutral: s.neutral,
            },
        ),
        (
            &s.quantitative,
            PredictionSource::Quantitative,
            PredictionTarget::MarketIndicators,
            PredictionPayload::Quantitative {
                indicators: s.indicators.clone(),
            },
        ),
    ];
    let mut predictions = vec![forecast];
    for (signal, source, target, payload) in payloads {
        predictions.push(Prediction {
            metadata: metadata(signal, source, target, &context, valid_until, &payload)?,
            payload,
        });
    }
    for prediction in &predictions {
        prediction.validate(now)?;
    }
    let mut returns = [Decimal::ZERO; 4];
    for (slot, value) in returns.iter_mut().zip(r) {
        *slot = trady_policy::mul(decimal(value)?, allocation)?;
    }
    let outcomes = OutcomeEstimates {
        scenario_hash: context.scenario_hash.clone(),
        forecast_prediction_id: forecast_id,
        estimation_report_id: model.estimation_id()?,
        cost_model_hash: c.cost_model_hash.clone(),
        available_at: now,
        normalization: ReturnNormalization {
            account_id: input.account.account_id.clone(),
            account_snapshot_id: input.account.snapshot_id.clone(),
            opening_equity: opening_equity.clone(),
        },
        net_account_returns: returns,
    };
    // The deployment's fusion authority must explicitly approve every model/report.
    let approved = authority
        .prediction
        .as_ref()
        .ok_or_else(|| Error("prediction authority missing".into()))?;
    let fused = trady_policy::prediction::PredictionFusionEngine::new(approved.fusion.clone())?
        .fuse(&context, &predictions, now)?;
    require(
        fused.status() == trady_policy::prediction::FusionStatus::Ready,
        "approved fusion abstained from complete forecast",
    )?;
    require(
        approved.utility.approved_estimation_report_id == outcomes.estimation_report_id
            && approved.utility.cost_model_hash == outcomes.cost_model_hash,
        "payoff estimator is not deployment approved",
    )?;
    Ok(ScenarioPredictionInput {
        context,
        predictions,
        outcomes,
    })
}

//! Offline weekly calibration and deployment-owned formula authority.
//! Research decisions never grant order or release authority.

mod weekly;
pub use weekly::*;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use trady_domain::{Error, Result, require};
use trady_policy::prediction::{
    FusionConfig, FusionContext, OutcomeEstimates, PredictionFusionEngine, UtilityConfig,
    UtilitySelection, expected_utility, select_scenario,
};
use trady_policy::{Policy, limits::*};

pub fn digest<T: Serialize>(value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| Error("optimization artifact encoding failed".into()))?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
}

/// Loaded from the deployment-owned release store/configuration, never an analyst tool.
/// Report references resolve at release time; strings supplied by an agent are insufficient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedFormula {
    pub revision_id: String,
    pub formula_hash: String,
    pub policy_hash: String,
    pub parameters: FormulaParameters,
    pub approved_at: DateTime<Utc>,
    pub effective_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub proof_report_id: String,
    pub parity_report_id: String,
    pub evaluation_report_id: String,
    pub shadow_report_id: String,
    pub owner_approval_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaAuthority {
    pub envelope: FormulaEnvelope,
    pub max_account_risk_fraction: Decimal,
    pub active: ApprovedFormula,
    /// No complete calibrated model is supplied by default or by the analyst.
    #[serde(default)]
    pub prediction: Option<PredictionAuthority>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictionAuthority {
    pub fusion: FusionConfig,
    pub utility: UtilityConfig,
}

impl FormulaAuthority {
    /// Structural checks permit an expired strategy to remain attached to an open
    /// plan. New entries separately require `validate_active` at their current time.
    pub fn validate(&self, policy: &Policy) -> Result<()> {
        self.active.parameters.validate(&self.envelope, policy)?;
        require(
            valid_id(&self.active.revision_id)
                && self.active.formula_hash == digest(&self.active.parameters)?
                && self.active.policy_hash == digest(policy)?
                && self.active.approved_at <= self.active.effective_from
                && self.active.effective_from < self.active.valid_until
                && self.max_account_risk_fraction > Decimal::ZERO
                && self.max_account_risk_fraction <= policy.max_initial_loss_fraction,
            "unapproved, incompatible or corrupt formula authority",
        )?;
        for id in [
            &self.active.proof_report_id,
            &self.active.parity_report_id,
            &self.active.evaluation_report_id,
            &self.active.shadow_report_id,
            &self.active.owner_approval_id,
        ] {
            require(valid_id(id), "formula release evidence is missing")?;
        }
        Ok(())
    }

    pub fn validate_active(&self, policy: &Policy, now: DateTime<Utc>) -> Result<()> {
        self.validate(policy)?;
        require(
            self.active.effective_from <= now && now < self.active.valid_until,
            "approved weekly formula is not active or has expired",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundCalculation {
    pub schema_version: u32,
    pub calculation_id: String,
    pub formula_revision_id: String,
    pub formula_hash: String,
    pub policy_hash: String,
    pub input_hash: String,
    pub authority_hash: String,
    pub limits: CalculatedLimits,
}

pub fn calculate_authoritative_limits(
    authority: &FormulaAuthority,
    policy: &Policy,
    input: &LimitsInput,
    now: DateTime<Utc>,
) -> Result<BoundCalculation> {
    authority.validate_active(policy, now)?;
    let mut limits = calculate_limits(
        policy,
        &authority.envelope,
        &authority.active.parameters,
        authority.max_account_risk_fraction,
        input,
        now,
    )?;
    limits.valid_until = limits.valid_until.min(authority.active.valid_until);
    let mut bound = BoundCalculation {
        schema_version: 1,
        calculation_id: String::new(),
        formula_revision_id: authority.active.revision_id.clone(),
        formula_hash: authority.active.formula_hash.clone(),
        policy_hash: authority.active.policy_hash.clone(),
        input_hash: digest(input)?,
        authority_hash: digest(authority)?,
        limits,
    };
    bound.calculation_id = format!("limits_{}", digest(&bound)?);
    Ok(bound)
}

/// Persist this with the final scenario *before* requesting Perpetual assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaProposal {
    pub input: LimitsInput,
    pub calculation: BoundCalculation,
    #[serde(default)]
    pub predictions: Option<ScenarioPredictionInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPredictionInput {
    pub context: FusionContext,
    pub predictions: Vec<trady_domain::prediction::Prediction>,
    pub outcomes: OutcomeEstimates,
}

impl FormulaProposal {
    pub fn validate(
        &self,
        authority: &FormulaAuthority,
        policy: &Policy,
        current_quote: &trady_domain::Quote,
        now: DateTime<Utc>,
    ) -> Result<&CalculatedLimits> {
        authority.validate_active(policy, now)?;
        let original = &self.calculation;
        require(
            original.limits.calculated_at <= now && original.limits.valid_until > now,
            "formula calculation is future-dated or expired",
        )?;
        let checked = calculate_authoritative_limits(
            authority,
            policy,
            &self.input,
            original.limits.calculated_at,
        )?;
        require(
            checked == *original,
            "formula calculation differs from the approved Rust result",
        )?;
        let mut current_input = self.input.clone();
        current_input.quote = current_quote.clone();
        let mut current =
            calculate_authoritative_limits(authority, policy, &current_input, now)?.limits;
        current.calculated_at = original.limits.calculated_at;
        // A refreshed quote cannot extend the original assessment's lifetime.
        current.valid_until = current.valid_until.min(original.limits.valid_until);
        require(
            current == original.limits,
            "market changed formula limits; calculate and assess the final scenario again",
        )?;
        let prediction_authority = authority
            .prediction
            .as_ref()
            .ok_or_else(|| Error("approved prediction/fusion authority is unavailable".into()))?;
        let predictions = self.predictions.as_ref().ok_or_else(|| {
            Error("complete scenario-bound calibrated predictions are unavailable".into())
        })?;
        let scenario_hash = original
            .calculation_id
            .strip_prefix("limits_")
            .filter(|h| valid_hash(h))
            .ok_or_else(|| Error("invalid financial scenario identity".into()))?;
        require(
            predictions.context.scenario_hash == scenario_hash
                && predictions.context.instrument_id == self.input.quote.instrument_id
                && predictions.context.market_snapshot_id == self.input.market_snapshot_id
                && predictions.context.horizon.decision_at >= original.limits.calculated_at
                && predictions.context.horizon.deadline_at == original.limits.holding_deadline_at
                && predictions.context.horizon.exchange_sessions
                    == original.limits.max_holding_sessions,
            "prediction fusion differs from the frozen financial scenario or horizon",
        )?;
        let fusion = PredictionFusionEngine::new(prediction_authority.fusion.clone())?.fuse(
            &predictions.context,
            &predictions.predictions,
            now,
        )?;
        let negative = fusion
            .maximum_negative_sentiment()
            .ok_or_else(|| Error("required FinBERT sentiment is unavailable".into()))?;
        let expected_score = if negative >= Decimal::new(60, 2) {
            self.input.evidence.base_confidence_score.saturating_sub(10)
        } else {
            self.input.evidence.base_confidence_score
        };
        require(
            self.input.evidence.confidence_score == expected_score
                && negative < Decimal::new(80, 2),
            "formula confidence does not preserve the FinBERT overlay",
        )?;
        require(
            predictions.outcomes.normalization.account_id == self.input.account.account_id
                && predictions.outcomes.normalization.account_snapshot_id
                    == self.input.account.snapshot_id
                && predictions.outcomes.normalization.opening_equity.currency
                    == self.input.account.spendable_cash.currency,
            "outcome returns use a different account, snapshot or currency",
        )?;
        let utility = expected_utility(
            &fusion,
            &predictions.outcomes,
            &prediction_authority.utility,
            now,
        )?;
        require(
            matches!(
                select_scenario(&[utility], &prediction_authority.utility, now)?,
                UtilitySelection::Candidate { .. }
            ),
            "NO_TRADE: scenario utility does not exceed cash",
        )?;
        Ok(&original.limits)
    }
}

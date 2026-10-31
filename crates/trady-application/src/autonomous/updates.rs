//! Target updates require persisted native advice and fresh research; old exits win.

use super::*;
use crate::perpetual::{PerpetualRepository, validate_review_for_scenario};
use trady_domain::perpetual::*;
use trady_policy::economics::{CostBasis, ExitCosts, liquidation_proceeds, target_price};
use trady_policy::holding::{
    HoldingContext, HoldingPlan, PlanActivation, PlanUpdate, activate_plan,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingUpdate {
    pub id: String,
    pub position_id: String,
    pub expected_plan_version: u64,
    pub proposed: HoldingPlan,
    pub created_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub research: EntryCandidate,
    pub assessment: AssessmentRecord,
    pub review: PerpetualReview,
}

struct PersistedAdvice(AssessmentRecord);

/// Holding advice normalizes the remaining exit charges by actual acquisition
/// cost. Entry charges are already included in that cost and are not counted twice.
pub fn holding_advisory_cost_fraction(
    basis: &CostBasis,
    bid: Decimal,
    costs: &ExitCosts,
    now: DateTime<Utc>,
) -> Result<Decimal> {
    let proceeds = liquidation_proceeds(basis, bid, costs, now)?;
    let fx = if basis.instrument_currency == basis.acquisition_cost.currency {
        Decimal::ONE
    } else {
        costs
            .fx
            .as_ref()
            .ok_or_else(|| Error("holding advice requires current FX".into()))?
            .rate
    };
    let gross = trady_policy::mul(trady_policy::mul(basis.quantity, bid)?, fx)?;
    div(sub(gross, proceeds.amount)?, basis.acquisition_cost.amount)
}
impl PerpetualRepository for PersistedAdvice {
    fn load_context(&mut self, _: &AssessArguments) -> Result<AssessmentContext> {
        Ok(self.0.context.clone())
    }
    fn save_assessment(&mut self, _: &AssessmentRecord) -> Result<()> {
        Err(Error(
            "holding dispatch cannot replace native advice".into(),
        ))
    }
    fn load_assessment(&mut self, id: &str) -> Result<Option<AssessmentRecord>> {
        Ok((id == self.0.assessment.assessment_id).then(|| self.0.clone()))
    }
    fn save_review(
        &mut self,
        _: &PerpetualReview,
        _: &ScenarioBinding,
        _: DateTime<Utc>,
    ) -> Result<()> {
        // The journal has resolved the immutable persisted review. The caller
        // persists the resulting plan under its current fence and revision.
        Ok(())
    }
}

/// Validation never changes broker protection. A target update cannot change
/// the native stop; changing protection uses the separate cancellation/fill path.
#[allow(clippy::too_many_arguments)]
pub fn activate_reviewed_holding_update(
    config: &TradingConfig,
    plan: &ActivePlan,
    update: &HoldingUpdate,
    basis: &CostBasis,
    quote: &Quote,
    costs: &ExitCosts,
    context: &HoldingContext,
) -> Result<PlanActivation> {
    let now = context.now;
    require(
        valid_reference(&update.id)
            && update.position_id == plan.candidate.proposal_id
            && update.created_at <= now
            && update.valid_until > now
            && update.valid_until <= update.created_at + Duration::minutes(5)
            && update.research.account_id == config.account_id
            && update.research.instrument_id == plan.candidate.instrument_id
            && update.research.currency == basis.instrument_currency
            && update.research.policy_version == config.policy.version
            && update.research.created_at <= now
            && update.research.valid_until > now
            && update.research.valid_until <= update.research.created_at + Duration::minutes(5)
            && basis.instrument_id == plan.candidate.instrument_id
            && basis.quantity == plan.acquired_quantity
            && basis.acquisition_cost.currency == config.base_currency
            && update.proposed.stop_price == plan.holding_state.plan().stop_price,
        "holding update is stale, differently scoped or changes broker protection",
    )?;
    update.research.validate_dossier(config, now)?;
    let base_score = trady_policy::confidence(&update.research.confidence_inputs).total();
    let adverse_sentiment = update
        .research
        .sentiment_reference
        .as_ref()
        .is_some_and(|reference| reference.maximum_negative_probability >= Decimal::new(60, 2));
    let confidence_score = if adverse_sentiment {
        base_score.saturating_sub(10)
    } else {
        base_score
    };
    let record = &update.assessment;
    let scenario = &record.context.scenario.binding;
    let fx = if basis.instrument_currency == basis.acquisition_cost.currency {
        Decimal::ONE
    } else {
        costs.fx.as_ref().map_or(Decimal::ONE, |f| f.rate)
    };
    require(
        fx > Decimal::ZERO,
        "holding update requires positive current FX",
    )?;
    require(
        scenario.arguments.position_id.as_deref() == Some(update.position_id.as_str())
            && scenario.arguments.instrument_id == basis.instrument_id
            && scenario.currency == basis.instrument_currency
            && scenario.entry_price
                == div(div(basis.acquisition_cost.amount, basis.quantity)?, fx)?
            && scenario.stop_price == update.proposed.stop_price
            && scenario.deadline_at == update.proposed.deadline
            && scenario.target_price
                == target_price(
                    basis,
                    update.proposed.target_net_fraction,
                    costs,
                    plan.candidate.instrument_rules.price_tick,
                    now,
                )?
            && record.context.scenario.market.fx_rate == fx,
        "holding update changes assessed position economics",
    )?;
    require(
        scenario.estimated_cost_fraction
            == holding_advisory_cost_fraction(
                basis,
                record.context.scenario.market.executable_price,
                costs,
                now,
            )?,
        "holding advice exit charges changed; reassessment required",
    )?;
    let mut repository = PersistedAdvice(record.clone());
    validate_review_for_scenario(
        &mut repository,
        &update.review,
        scenario,
        &MarketObservation {
            instrument_id: quote.instrument_id.clone(),
            snapshot_id: record.context.scenario.market.snapshot_id.clone(),
            observed_at: quote.observed_at,
            valid_until: quote.observed_at + Duration::seconds(10),
            executable_price: quote.bid,
            fx_rate: fx,
        },
        now,
    )?;
    let mut checked_context = context.clone();
    checked_context.research_available = true;
    activate_plan(
        &plan.holding_state,
        &PlanUpdate {
            expected_version: update.expected_plan_version,
            proposed: update.proposed.clone(),
            // This overlay only restricts authority. Before persisting an activated
            // revision, the fenced journal must verify the exact persisted sentiment
            // dependency; a locally supplied reference cannot grant that authority.
            confidence_score,
            exceptional_verified: update.research.exceptional_verified,
            target_increase_allowed: !adverse_sentiment,
        },
        basis,
        Some(quote),
        costs,
        &checked_context,
        &config.policy,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn holding_advice_uses_actual_cost_including_entry_fees_and_current_exit_charges() {
        let currency = Currency::new("EUR").unwrap();
        let basis = CostBasis {
            instrument_id: InstrumentId::new("fixture_stock").unwrap(),
            quantity: 2.into(),
            instrument_currency: currency.clone(),
            acquisition_cost: Money {
                amount: 205.into(),
                currency: currency.clone(),
            },
        };
        let mut costs = ExitCosts {
            fixed: Money {
                amount: 1.into(),
                currency: currency.clone(),
            },
            rate: Decimal::new(1, 2),
            uncertainty: Money {
                amount: Decimal::ZERO,
                currency,
            },
            fx: None,
        };
        let now = Utc::now();
        assert_eq!(
            holding_advisory_cost_fraction(&basis, 100.into(), &costs, now).unwrap(),
            div(3.into(), 205.into()).unwrap()
        );
        costs.fixed.amount = 2.into();
        assert_eq!(
            holding_advisory_cost_fraction(&basis, 100.into(), &costs, now).unwrap(),
            div(4.into(), 205.into()).unwrap()
        );
        costs.rate = Decimal::ONE;
        assert!(holding_advisory_cost_fraction(&basis, 100.into(), &costs, now).is_err());
    }
}

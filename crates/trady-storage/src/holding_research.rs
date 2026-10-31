//! Resolve immutable advice terms against a current executor-attested holding.

use super::*;
use trady_application::autonomous::{ActivePlan, ExecutionObservation};
use trady_policy::economics::{CostBasis, ExitCosts, net_executable_return, target_price};
use trady_policy::{EntryCosts, Policy, ceil_to, div, fresh};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedHolding {
    pub owner_id: String,
    pub account_id: AccountId,
    /// Stable entry proposal identity; it cannot be reassigned to another holding.
    pub position_id: String,
    pub revision: u64,
    pub policy: Policy,
    pub plan: ActivePlan,
    pub observation: ExecutionObservation,
    pub exit_costs: EntryCosts,
}

impl VerifiedHolding {
    pub fn basis(&self, now: DateTime<Utc>) -> Result<CostBasis> {
        self.policy.validate()?;
        self.observation.snapshot.validate()?;
        let candidate = &self.plan.candidate;
        require(
            valid_reference(&self.owner_id)
                && self.position_id == candidate.proposal_id
                && self.account_id == candidate.account_id
                && self.observation.snapshot.account_id == self.account_id
                && fresh(self.observation.snapshot.observed_at, now, 60)
                && self.plan.entered_at <= now
                && self.plan.exit_latch.is_none()
                && self.plan.holding_state.exit_latch().is_none()
                && self.plan.holding_state.plan().deadline > now,
            "holding is stale, differently scoped or already exiting",
        )?;
        let positions = &self.observation.snapshot.positions;
        require(
            positions.len() == 1,
            "holding requires exactly one reconciled stock",
        )?;
        let position = &positions[0];
        let cost = self
            .observation
            .position_costs
            .get(&candidate.instrument_id)
            .ok_or_else(|| Error("actual holding acquisition cost is unavailable".into()))?;
        require(
            position.instrument_id == candidate.instrument_id
                && position.quantity > 0.into()
                && position.quantity == self.plan.acquired_quantity
                && cost.amount > 0.into()
                && cost.currency == self.observation.snapshot.spendable_cash.currency,
            "holding quantity, acquisition cost or instrument differs",
        )?;
        let basis = CostBasis {
            instrument_id: position.instrument_id.clone(),
            quantity: position.quantity,
            instrument_currency: candidate.currency.clone(),
            acquisition_cost: cost.clone(),
        };
        basis.validate()?;
        Ok(basis)
    }

    fn costs(&self) -> ExitCosts {
        ExitCosts {
            fixed: self.exit_costs.fixed.clone(),
            rate: self.exit_costs.rate,
            uncertainty: self.exit_costs.uncertainty.clone(),
            fx: self.exit_costs.fx.clone(),
        }
    }

    /// Content identity includes actual position economics and active thresholds.
    /// A later observation with unchanged quantities/rates does not invent a new plan.
    pub fn calculation_id(&self, scenario: &ScenarioBinding, now: DateTime<Utc>) -> Result<String> {
        let basis = self.basis(now)?;
        let mut terms = scenario.clone();
        terms.arguments.scenario_calculation_id.clear();
        let mut costs = self.exit_costs.clone();
        if let Some(fx) = &mut costs.fx {
            fx.observed_at = DateTime::<Utc>::UNIX_EPOCH;
        }
        Ok(format!(
            "holding_{}",
            digest(&(
                &self.owner_id,
                &self.account_id,
                &self.position_id,
                &self.policy,
                self.plan.holding_state.plan(),
                &basis,
                costs,
                terms,
            ))?
        ))
    }

    /// Advice acquisition admits the owner's target domain. Activation separately
    /// checks the explicit nominal fraction against the fresh research confidence.
    pub fn validate_context(&self, context: &AssessmentContext, now: DateTime<Utc>) -> Result<()> {
        context.validate(now)?;
        let basis = self.basis(now)?;
        let binding = &context.scenario.binding;
        let active = self.plan.holding_state.plan();
        let fx = if basis.instrument_currency == basis.acquisition_cost.currency {
            1.into()
        } else {
            self.exit_costs.fx.as_ref().map_or(1.into(), |f| f.rate)
        };
        require(fx > 0.into(), "holding FX must be positive")?;
        let entry = div(div(basis.acquisition_cost.amount, basis.quantity)?, fx)?;
        require(
            binding.arguments.position_id.as_deref() == Some(self.position_id.as_str())
                && binding.arguments.instrument_id == basis.instrument_id
                && binding.arguments.scenario_calculation_id
                    == self.calculation_id(binding, now)?
                && binding.currency == basis.instrument_currency
                && binding.entry_price == entry
                && binding.stop_price == active.stop_price
                && binding.estimated_cost_fraction
                    == trady_application::autonomous::holding_advisory_cost_fraction(
                        &basis,
                        context.scenario.market.executable_price,
                        &self.costs(),
                        now,
                    )?
                && binding.deadline_at <= active.deadline
                && context.scenario.market.fx_rate == fx
                && fresh(context.scenario.calculated_at, now, 60)
                && context.scenario.valid_until
                    <= context.scenario.calculated_at + Duration::minutes(5),
            "holding advice differs from verified position economics or active plan",
        )?;
        let target_quote = Quote {
            instrument_id: basis.instrument_id.clone(),
            currency: basis.instrument_currency.clone(),
            bid: binding.target_price,
            ask: binding.target_price,
            observed_at: now,
            received_at: now,
            tradable: true,
        };
        let target = net_executable_return(&basis, &target_quote, &self.costs(), now)?;
        // The achieved return at an upward-rounded limit can exceed its nominal
        // target by a fraction of a tick. Compare reachable prices, not that
        // achieved return against the nominal policy fraction.
        let tick = self.plan.candidate.instrument_rules.price_tick;
        let maximum_target_price = target_price(
            &basis,
            self.policy.max_target_fraction,
            &self.costs(),
            tick,
            now,
        )?;
        require(
            target.fraction > 0.into()
                && ceil_to(binding.target_price, tick)? == binding.target_price
                && binding.target_price <= maximum_target_price,
            "holding advice target is not tick-aligned or exceeds the owner policy domain",
        )
    }
}

impl PgTradingResearch {
    pub fn queue_holding_update(
        &mut self,
        update: &trady_application::autonomous::HoldingUpdate,
    ) -> Result<()> {
        let holding = self.load_verified_holding(&update.position_id)?;
        holding.validate_context(&update.assessment.context, Utc::now())?;
        let stored = self
            .load_assessment(&update.assessment.assessment.assessment_id)?
            .ok_or_else(|| Error("holding update references missing native assessment".into()))?;
        require(
            stored == update.assessment,
            "holding update changed native assessment",
        )?;
        let document = serde_json::to_value(update)
            .map_err(|_| Error("cannot encode holding update".into()))?;
        self.client
            .query_one(
                "SELECT research.queue_holding_update($1,$2,$3)",
                &[&self.owner_id, &self.account_id.as_str(), &document],
            )
            .map_err(db)?;
        Ok(())
    }
    pub fn load_verified_holding(&mut self, position_id: &str) -> Result<VerifiedHolding> {
        require(
            valid_reference(position_id),
            "invalid holding position reference",
        )?;
        let row = self
            .client
            .query_one(
                "SELECT research.verified_holding($1,$2,$3)",
                &[&self.owner_id, &self.account_id.as_str(), &position_id],
            )
            .map_err(db)?;
        let holding: VerifiedHolding = serde_json::from_value(row.get(0))
            .map_err(|_| Error("invalid executor-attested holding document".into()))?;
        require(
            holding.owner_id == self.owner_id
                && holding.account_id == self.account_id
                && holding.position_id == position_id,
            "holding repository scope mismatch",
        )?;
        holding.basis(Utc::now())?;
        Ok(holding)
    }
}

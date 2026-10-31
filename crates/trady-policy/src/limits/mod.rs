//! FORMULA_V1: the shared production and research calculator. No model price hints.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trady_domain::{AccountSnapshot, Money, Quote, Result, require};

use crate::{
    EntryCosts, EntryRange, InstrumentRules, Policy, add, ceil_to, div,
    economics::{CostBasis, ExitCosts, fx_rate, liquidation_proceeds, target_price},
    floor_to, fresh, mul, size_entry, sub, target_ceiling, validate_risk,
};

pub mod proof_bridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FormulaFamily {
    Trend,
    Reversal,
}

/// Only these parameters are calibrated. Risk and allocation remain owner policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaParameters {
    pub version: u32,
    pub family: FormulaFamily,
    /// Bounded allowance relative to the observed ask, applied once at calculation.
    pub entry_allowance_fraction: Decimal,
    pub stop_atr_multiple: Decimal,
    pub target_net_fraction: Decimal,
    pub max_holding_sessions: u8,
    pub atr_lookback: u16,
    pub bar_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaEnvelope {
    pub max_entry_allowance_fraction: Decimal,
    pub min_stop_atr_multiple: Decimal,
    pub max_stop_atr_multiple: Decimal,
    pub min_target_net_fraction: Decimal,
    pub max_target_net_fraction: Decimal,
}

impl FormulaEnvelope {
    pub fn validate(&self, policy: &Policy) -> Result<()> {
        policy.validate()?;
        require(
            self.max_entry_allowance_fraction >= Decimal::ZERO
                && self.max_entry_allowance_fraction <= Decimal::new(1, 2)
                && self.min_stop_atr_multiple > Decimal::ZERO
                && self.max_stop_atr_multiple >= self.min_stop_atr_multiple
                && self.max_stop_atr_multiple <= Decimal::from(10)
                && self.min_target_net_fraction > Decimal::ZERO
                && self.max_target_net_fraction >= self.min_target_net_fraction
                && self.max_target_net_fraction <= policy.max_target_fraction,
            "invalid owner-approved formula envelope",
        )
    }
}

impl FormulaParameters {
    pub fn validate(&self, envelope: &FormulaEnvelope, policy: &Policy) -> Result<()> {
        envelope.validate(policy)?;
        require(
            self.version == 1
                && self.entry_allowance_fraction >= Decimal::ZERO
                && self.entry_allowance_fraction <= envelope.max_entry_allowance_fraction
                && self.stop_atr_multiple >= envelope.min_stop_atr_multiple
                && self.stop_atr_multiple <= envelope.max_stop_atr_multiple
                && self.target_net_fraction >= envelope.min_target_net_fraction
                && self.target_net_fraction <= envelope.max_target_net_fraction
                && (1..=policy.max_holding_sessions).contains(&self.max_holding_sessions)
                && (2..=252).contains(&self.atr_lookback)
                && (60..=86_400).contains(&self.bar_seconds),
            "formula parameters exceed the approved domain",
        )
    }
}

/// The trusted feature collector computes ATR from completed observations only.
/// A family signal is a verified research constraint, never a model instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaEvidence {
    pub snapshot_id: String,
    pub family: FormulaFamily,
    pub signal_eligible: bool,
    pub atr: Decimal,
    pub atr_lookback: u16,
    pub bar_seconds: u32,
    pub completed_bars: u32,
    pub last_completed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub minimum_stop_distance: Decimal,
    pub noise_distance: Decimal,
    /// Final confidence after required overlays; a raw LLM confidence is invalid.
    pub base_confidence_score: u8,
    pub confidence_score: u8,
    pub sentiment_entry_allowed: bool,
    pub sentiment_target_ceiling: Decimal,
    pub exceptional_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsInput {
    pub account: AccountSnapshot,
    pub market_snapshot_id: String,
    pub quote: Quote,
    pub entry_range: EntryRange,
    pub entry_costs: EntryCosts,
    pub exit_costs: ExitCosts,
    pub rules: InstrumentRules,
    pub evidence: FormulaEvidence,
    pub holding_deadline_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalculatedLimits {
    pub authority: String,
    pub account_snapshot_id: String,
    pub market_snapshot_id: String,
    pub evidence_snapshot_id: String,
    pub calculated_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub buy_limit: Decimal,
    pub quantity: Decimal,
    pub stop_trigger: Decimal,
    pub target_sell_limit: Decimal,
    pub target_net_fraction: Decimal,
    pub target_rounding_overshoot: Decimal,
    pub max_holding_sessions: u8,
    pub holding_deadline_at: DateTime<Utc>,
    pub budget: Money,
    pub acquisition_cost: Money,
    pub proceeds_at_stop: Money,
    pub proceeds_at_target: Money,
    pub planned_loss_fraction: Decimal,
    pub account_risk: Money,
    pub reward_to_risk: Decimal,
}

/// A pure calculation, not a readiness certificate or permission to place orders.
/// The owner risk fraction is deliberately outside the optimizable parameter set.
pub fn calculate_limits(
    policy: &Policy,
    envelope: &FormulaEnvelope,
    parameters: &FormulaParameters,
    max_account_risk_fraction: Decimal,
    input: &LimitsInput,
    now: DateTime<Utc>,
) -> Result<CalculatedLimits> {
    parameters.validate(envelope, policy)?;
    input.account.validate()?;
    require(
        max_account_risk_fraction > Decimal::ZERO
            && max_account_risk_fraction <= policy.max_initial_loss_fraction,
        "owner account-risk budget is missing or exceeds policy",
    )?;
    require(
        !input.account.snapshot_id.is_empty()
            && !input.market_snapshot_id.is_empty()
            && fresh(input.account.observed_at, now, 60)
            && now < input.valid_until
            && input.valid_until <= input.holding_deadline_at,
        "missing, stale or expired formula inputs",
    )?;
    let evidence = &input.evidence;
    require(
        !evidence.snapshot_id.is_empty()
            && evidence.family == parameters.family
            && evidence.signal_eligible
            && evidence.atr > Decimal::ZERO
            && evidence.atr_lookback == parameters.atr_lookback
            && evidence.bar_seconds == parameters.bar_seconds
            && evidence.completed_bars > u32::from(parameters.atr_lookback)
            && evidence.last_completed_at <= evidence.available_at
            && fresh(
                evidence.last_completed_at,
                now,
                i64::from(parameters.bar_seconds),
            )
            && evidence.available_at <= now
            && evidence.valid_until > now
            && evidence.minimum_stop_distance >= Decimal::ZERO
            && evidence.noise_distance >= Decimal::ZERO,
        "formula requires current, warmed, causal volatility and an eligible signal",
    )?;
    let ceiling = target_ceiling(
        evidence.confidence_score,
        evidence.exceptional_verified,
        policy,
    )?;
    require(
        evidence.base_confidence_score <= 100
            && evidence.confidence_score <= evidence.base_confidence_score
            && evidence.sentiment_entry_allowed
            && evidence.sentiment_target_ceiling >= Decimal::ZERO
            && evidence.sentiment_target_ceiling <= ceiling
            && parameters.target_net_fraction <= evidence.sentiment_target_ceiling,
        "formula target or entry violates the confidence/sentiment gate",
    )?;

    // Rounding downward must still leave a marketable price inside the evidence range.
    let buy_limit = floor_to(
        input.entry_range.maximum.min(mul(
            input.quote.ask,
            add(Decimal::ONE, parameters.entry_allowance_fraction)?,
        )?),
        input.rules.price_tick,
    )?;
    require(
        buy_limit >= input.quote.ask,
        "no marketable formula tick inside range",
    )?;
    let mut priced_quote = input.quote.clone();
    priced_quote.ask = buy_limit;
    // Validate the actual quote first; replacing ask must not hide an out-of-range input.
    size_entry(
        &input.account.spendable_cash,
        &input.quote,
        &input.entry_range,
        &input.entry_costs,
        &input.rules,
        now,
        policy,
    )?;
    let upper = size_entry(
        &input.account.spendable_cash,
        &priced_quote,
        &input.entry_range,
        &input.entry_costs,
        &input.rules,
        now,
        policy,
    )?;
    let stop_trigger = ceil_to(
        sub(buy_limit, mul(parameters.stop_atr_multiple, evidence.atr)?)?,
        input.rules.price_tick,
    )?;
    let stop_distance = sub(input.quote.bid, stop_trigger)?;
    require(
        stop_trigger > Decimal::ZERO
            && stop_trigger < buy_limit
            && stop_distance > evidence.noise_distance
            && stop_distance >= evidence.minimum_stop_distance,
        "formula stop is nonpositive or inside spread, noise or venue minimum",
    )?;

    let cash = &input.account.spendable_cash;
    let entry_fx = fx_rate(
        &input.quote.currency,
        &cash.currency,
        input.entry_costs.fx.as_ref(),
        now,
    )?;
    let exit_fx = fx_rate(
        &input.quote.currency,
        &cash.currency,
        input.exit_costs.fx.as_ref(),
        now,
    )?;
    require(
        input.exit_costs.fixed.currency == cash.currency
            && input.exit_costs.uncertainty.currency == cash.currency
            && input.exit_costs.fixed.amount >= Decimal::ZERO
            && input.exit_costs.uncertainty.amount >= Decimal::ZERO
            && input.exit_costs.rate >= Decimal::ZERO
            && input.exit_costs.rate < Decimal::ONE,
        "invalid formula liquidation costs",
    )?;
    let acquisition_unit = mul(
        mul(buy_limit, entry_fx)?,
        add(Decimal::ONE, input.entry_costs.rate)?,
    )?;
    let entry_fixed = add(
        input.entry_costs.fixed.amount,
        input.entry_costs.uncertainty.amount,
    )?;
    let exit_fixed = add(
        input.exit_costs.fixed.amount,
        input.exit_costs.uncertainty.amount,
    )?;
    let loss_unit = sub(
        acquisition_unit,
        mul(
            mul(stop_trigger, exit_fx)?,
            sub(Decimal::ONE, input.exit_costs.rate)?,
        )?,
    )?;
    let fixed_loss = add(entry_fixed, exit_fixed)?;
    let account_risk_cap = mul(cash.amount, max_account_risk_fraction)?;
    require(
        loss_unit > Decimal::ZERO && account_risk_cap > fixed_loss,
        "no feasible positive modeled downside",
    )?;
    let mut quantity = floor_to(
        upper
            .quantity
            .min(div(sub(account_risk_cap, fixed_loss)?, loss_unit)?),
        input.rules.quantity_step,
    )?;
    // Decimal division can round at its precision boundary. Correct one lot and
    // recheck the original inequalities rather than trusting an approximate root.
    for _ in 0..2 {
        let cost = add(mul(quantity, acquisition_unit)?, entry_fixed)?;
        let loss = add(mul(quantity, loss_unit)?, fixed_loss)?;
        if cost <= upper.budget.amount && loss <= account_risk_cap {
            break;
        }
        quantity = sub(quantity, input.rules.quantity_step)?;
    }
    require(
        quantity > Decimal::ZERO && quantity >= input.rules.minimum_quantity,
        "no supported quantity within account risk",
    )?;
    let next = add(quantity, input.rules.quantity_step)?;
    require(
        next > upper.quantity
            || add(mul(next, acquisition_unit)?, entry_fixed)? > upper.budget.amount
            || add(mul(next, loss_unit)?, fixed_loss)? > account_risk_cap,
        "quantity solver precision cannot certify the largest lot",
    )?;
    let acquisition_cost = Money {
        amount: add(mul(quantity, acquisition_unit)?, entry_fixed)?,
        currency: cash.currency.clone(),
    };
    let basis = CostBasis {
        instrument_id: input.quote.instrument_id.clone(),
        quantity,
        instrument_currency: input.quote.currency.clone(),
        acquisition_cost: acquisition_cost.clone(),
    };
    let proceeds_at_stop = liquidation_proceeds(&basis, stop_trigger, &input.exit_costs, now)?;
    let loss = sub(acquisition_cost.amount, proceeds_at_stop.amount)?;
    require(
        acquisition_cost.amount <= upper.budget.amount
            && upper.budget.amount <= cash.amount
            && acquisition_cost.amount <= cash.amount
            && loss > Decimal::ZERO
            && loss <= account_risk_cap
            && loss <= mul(acquisition_cost.amount, policy.max_initial_loss_fraction)?,
        "final formula quantity fails allocation or account-risk budget",
    )?;
    let planned_loss_fraction = div(loss, acquisition_cost.amount)?;
    validate_risk(
        parameters.target_net_fraction,
        planned_loss_fraction,
        evidence.confidence_score,
        evidence.exceptional_verified,
        policy,
    )?;
    let target_sell_limit = target_price(
        &basis,
        parameters.target_net_fraction,
        &input.exit_costs,
        input.rules.price_tick,
        now,
    )?;
    let proceeds_at_target =
        liquidation_proceeds(&basis, target_sell_limit, &input.exit_costs, now)?;
    let reward = sub(proceeds_at_target.amount, acquisition_cost.amount)?;
    let reward_to_risk = div(reward, loss)?;
    require(
        target_sell_limit > buy_limit
            && reward >= mul(loss, policy.min_reward_to_risk)?
            && reward_to_risk >= policy.min_reward_to_risk,
        "formula target ordering or reward/risk failed",
    )?;
    require(
        [buy_limit, stop_trigger, target_sell_limit]
            .iter()
            .all(|p| p.checked_rem(input.rules.price_tick) == Some(Decimal::ZERO))
            && quantity.checked_rem(input.rules.quantity_step) == Some(Decimal::ZERO),
        "final formula prices or quantity lost grid precision",
    )?;
    let mut valid_until = input.valid_until.min(evidence.valid_until);
    for (observed_at, age) in [
        (input.account.observed_at, 60),
        (input.quote.observed_at, 10),
        (input.quote.received_at, 10),
        (
            evidence.last_completed_at,
            i64::from(parameters.bar_seconds),
        ),
    ] {
        valid_until = valid_until.min(
            observed_at
                .checked_add_signed(Duration::seconds(age))
                .ok_or_else(|| trady_domain::Error("formula validity timestamp overflow".into()))?,
        );
    }
    if input.quote.currency != cash.currency {
        for fx in [input.entry_costs.fx.as_ref(), input.exit_costs.fx.as_ref()]
            .into_iter()
            .flatten()
        {
            valid_until = valid_until.min(
                fx.observed_at
                    .checked_add_signed(Duration::seconds(60))
                    .ok_or_else(|| {
                        trady_domain::Error("formula FX validity timestamp overflow".into())
                    })?,
            );
        }
    }
    require(
        now < valid_until,
        "no remaining valid formula input lifetime",
    )?;
    Ok(CalculatedLimits {
        authority: "FORMULA_V1".into(),
        account_snapshot_id: input.account.snapshot_id.clone(),
        market_snapshot_id: input.market_snapshot_id.clone(),
        evidence_snapshot_id: evidence.snapshot_id.clone(),
        calculated_at: now,
        valid_until,
        buy_limit,
        quantity,
        stop_trigger,
        target_sell_limit,
        target_net_fraction: parameters.target_net_fraction,
        target_rounding_overshoot: sub(
            div(reward, acquisition_cost.amount)?,
            parameters.target_net_fraction,
        )?,
        max_holding_sessions: parameters.max_holding_sessions,
        holding_deadline_at: input.holding_deadline_at,
        budget: upper.budget,
        acquisition_cost,
        proceeds_at_stop,
        proceeds_at_target,
        planned_loss_fraction,
        account_risk: Money {
            amount: loss,
            currency: cash.currency.clone(),
        },
        reward_to_risk,
    })
}

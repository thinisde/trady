//! Linear trading charges only. Unsupported/nonlinear charges require abstention.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trady_domain::{Currency, Error, InstrumentId, Money, Quote, Result, require};

use crate::{FxObservation, Policy, add, ceil_to, div, fresh, mul, sub, validate_risk};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionFill {
    pub instrument_id: InstrumentId,
    pub quantity: Decimal,
    pub price: Money,
    /// All actual attributable charges, already converted into account currency.
    pub charges: Money,
    pub fx: Option<FxObservation>,
    pub filled_at: DateTime<Utc>,
}

/// Actual filled cost for holding valuation, or conservative sized acquisition
/// cost for entry prechecks. Replace entry estimates with actual costs after fills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostBasis {
    pub instrument_id: InstrumentId,
    pub quantity: Decimal,
    pub instrument_currency: Currency,
    pub acquisition_cost: Money,
}

impl CostBasis {
    pub fn validate(&self) -> Result<()> {
        require(
            self.quantity > Decimal::ZERO && self.acquisition_cost.amount > Decimal::ZERO,
            "cost basis requires positive filled quantity and acquisition cost",
        )
    }
}

pub(crate) fn fx_rate(
    from: &Currency,
    to: &Currency,
    fx: Option<&FxObservation>,
    now: DateTime<Utc>,
) -> Result<Decimal> {
    if from == to {
        return Ok(Decimal::ONE);
    }
    let fx = fx.ok_or_else(|| Error("FX observation required".into()))?;
    require(
        &fx.from == from
            && &fx.to == to
            && fx.rate > Decimal::ZERO
            && fresh(fx.observed_at, now, 60),
        "invalid or stale FX",
    )?;
    Ok(fx.rate)
}

/// Caller deduplicates actual fills and supplies actual entry FX/charges, not a
/// proposal's estimates. Recompute after each partial fill before deriving limits.
pub fn cost_basis(fills: &[AcquisitionFill]) -> Result<CostBasis> {
    let first = fills
        .first()
        .ok_or_else(|| Error("at least one actual fill is required".into()))?;
    let mut quantity = Decimal::ZERO;
    let mut cost = Decimal::ZERO;
    for fill in fills {
        require(
            fill.instrument_id == first.instrument_id
                && fill.price.currency == first.price.currency
                && fill.charges.currency == first.charges.currency
                && fill.quantity > Decimal::ZERO
                && fill.price.amount > Decimal::ZERO
                && fill.charges.amount >= Decimal::ZERO,
            "inconsistent actual fill or negative charge",
        )?;
        let fx = fx_rate(
            &fill.price.currency,
            &fill.charges.currency,
            fill.fx.as_ref(),
            fill.filled_at,
        )?;
        quantity = add(quantity, fill.quantity)?;
        cost = add(
            cost,
            add(
                mul(mul(fill.quantity, fill.price.amount)?, fx)?,
                fill.charges.amount,
            )?,
        )?;
    }
    let basis = CostBasis {
        instrument_id: first.instrument_id.clone(),
        quantity,
        instrument_currency: first.price.currency.clone(),
        acquisition_cost: Money {
            amount: cost,
            currency: first.charges.currency.clone(),
        },
    };
    basis.validate()?;
    Ok(basis)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExitCosts {
    /// Fixed charges and uncertainty are in account currency. Rate applies once
    /// to gross account-currency sale consideration, after FX conversion.
    pub fixed: Money,
    pub rate: Decimal,
    pub uncertainty: Money,
    pub fx: Option<FxObservation>,
}

impl ExitCosts {
    fn validate(&self, basis: &CostBasis, now: DateTime<Utc>) -> Result<Decimal> {
        basis.validate()?;
        require(
            self.fixed.currency == basis.acquisition_cost.currency
                && self.uncertainty.currency == basis.acquisition_cost.currency
                && self.fixed.amount >= Decimal::ZERO
                && self.uncertainty.amount >= Decimal::ZERO
                && self.rate >= Decimal::ZERO
                && self.rate < Decimal::ONE,
            "invalid exit charges or account currency",
        )?;
        fx_rate(
            &basis.instrument_currency,
            &basis.acquisition_cost.currency,
            self.fx.as_ref(),
            now,
        )
    }
}

/// Can be negative if charges exceed the liquidation value; never invent zero.
pub fn liquidation_proceeds(
    basis: &CostBasis,
    bid: Decimal,
    costs: &ExitCosts,
    now: DateTime<Utc>,
) -> Result<Money> {
    require(bid > Decimal::ZERO, "liquidation price must be positive")?;
    let fx = costs.validate(basis, now)?;
    let gross = mul(mul(basis.quantity, bid)?, fx)?;
    Ok(Money {
        amount: sub(
            mul(gross, sub(Decimal::ONE, costs.rate)?)?,
            add(costs.fixed.amount, costs.uncertainty.amount)?,
        )?,
        currency: basis.acquisition_cost.currency.clone(),
    })
}

/// Estimated downside at a supplied instrument-currency trigger. A negative
/// result means a profitable stop; initial-entry validation requires positive risk.
pub fn planned_loss_fraction(
    basis: &CostBasis,
    stop_price: Decimal,
    costs: &ExitCosts,
    now: DateTime<Utc>,
) -> Result<Decimal> {
    let proceeds = liquidation_proceeds(basis, stop_price, costs, now)?;
    div(
        sub(basis.acquisition_cost.amount, proceeds.amount)?,
        basis.acquisition_cost.amount,
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetReturn {
    pub proceeds: Money,
    pub pnl: Money,
    pub fraction: Decimal,
    pub fx_rate: Decimal,
}

/// Price-only protection can still evaluate a fresh quote when FX is unavailable.
pub fn validate_executable_quote(
    basis: &CostBasis,
    quote: &Quote,
    now: DateTime<Utc>,
) -> Result<()> {
    require(
        quote.instrument_id == basis.instrument_id
            && quote.currency == basis.instrument_currency
            && quote.tradable
            && quote.bid > Decimal::ZERO
            && quote.ask >= quote.bid
            && quote.observed_at <= quote.received_at
            && fresh(quote.observed_at, now, 10)
            && fresh(quote.received_at, now, 10),
        "unusable, mismatched, stale or future executable quote",
    )
}

/// Current, executable bid and current FX; ask/last/headline returns cannot stand in.
pub fn net_executable_return(
    basis: &CostBasis,
    quote: &Quote,
    costs: &ExitCosts,
    now: DateTime<Utc>,
) -> Result<NetReturn> {
    validate_executable_quote(basis, quote, now)?;
    let proceeds = liquidation_proceeds(basis, quote.bid, costs, now)?;
    let pnl = Money {
        amount: sub(proceeds.amount, basis.acquisition_cost.amount)?,
        currency: proceeds.currency.clone(),
    };
    Ok(NetReturn {
        fraction: div(pnl.amount, basis.acquisition_cost.amount)?,
        pnl,
        proceeds,
        fx_rate: costs.validate(basis, now)?,
    })
}

/// Solve the supported linear charges and round the sell target upward. The
/// recomputation prevents division precision from rounding below required wealth.
pub fn target_price(
    basis: &CostBasis,
    target_fraction: Decimal,
    costs: &ExitCosts,
    price_tick: Decimal,
    now: DateTime<Utc>,
) -> Result<Decimal> {
    require(
        target_fraction > Decimal::ZERO && target_fraction <= Decimal::new(10, 2),
        "net target must be positive and at most 10%",
    )?;
    let fx = costs.validate(basis, now)?;
    let required = mul(
        basis.acquisition_cost.amount,
        add(Decimal::ONE, target_fraction)?,
    )?;
    let fixed = add(costs.fixed.amount, costs.uncertainty.amount)?;
    let unit_proceeds = mul(mul(basis.quantity, fx)?, sub(Decimal::ONE, costs.rate)?)?;
    require(
        unit_proceeds > Decimal::ZERO,
        "exit proceeds precision exhausted",
    )?;
    let mut price = ceil_to(div(add(required, fixed)?, unit_proceeds)?, price_tick)?;
    if liquidation_proceeds(basis, price, costs, now)?.amount < required {
        price = add(price, price_tick)?;
    }
    require(
        price > Decimal::ZERO
            && price.checked_rem(price_tick) == Some(Decimal::ZERO)
            && liquidation_proceeds(basis, price, costs, now)?.amount >= required,
        "target price cannot conservatively satisfy net proceeds",
    )?;
    Ok(price)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopConstraints {
    pub price_tick: Decimal,
    /// Checked venue minimum and observed ordinary noise, in instrument price units.
    pub minimum_distance: Decimal,
    pub noise_distance: Decimal,
    pub protection_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectiveStop {
    pub price: Decimal,
    pub estimated_proceeds: Money,
    pub planned_loss_fraction: Decimal,
    pub reward_to_risk: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopProposal {
    pub price: Decimal,
    pub target_fraction: Decimal,
    pub confidence_score: u8,
    pub exceptional_verified: bool,
}

/// Round a long stop upward by at most one tick, so rounding cannot increase
/// planned downside. Reject a technical stop that fails economics; never search
/// for an invented tighter stop. Fill slippage/gaps are not bounded by this check.
pub fn validate_protective_stop(
    basis: &CostBasis,
    quote: &Quote,
    costs: &ExitCosts,
    constraints: &StopConstraints,
    proposal: &StopProposal,
    now: DateTime<Utc>,
    policy: &Policy,
) -> Result<ProtectiveStop> {
    net_executable_return(basis, quote, costs, now)?;
    require(
        constraints.protection_verified
            && constraints.minimum_distance >= Decimal::ZERO
            && constraints.noise_distance >= Decimal::ZERO,
        "unknown protection, minimum distance or noise",
    )?;
    require(
        proposal.price > Decimal::ZERO,
        "stop price must be positive",
    )?;
    let price = ceil_to(proposal.price, constraints.price_tick)?;
    let distance = sub(quote.bid, price)?;
    require(
        distance > Decimal::ZERO
            && distance >= constraints.minimum_distance
            && distance > constraints.noise_distance,
        "stop is inside spread/noise or minimum distance",
    )?;
    let estimated_proceeds = liquidation_proceeds(basis, price, costs, now)?;
    let planned_loss_fraction = planned_loss_fraction(basis, price, costs, now)?;
    validate_risk(
        proposal.target_fraction,
        planned_loss_fraction,
        proposal.confidence_score,
        proposal.exceptional_verified,
        policy,
    )?;
    Ok(ProtectiveStop {
        price,
        estimated_proceeds,
        planned_loss_fraction,
        reward_to_risk: div(proposal.target_fraction, planned_loss_fraction)?,
    })
}

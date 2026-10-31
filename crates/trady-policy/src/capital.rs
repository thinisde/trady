//! Reporting denominators and the operational loss metric, never broker balances.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trady_domain::{Money, Result, require};

use crate::{add, div, sub};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPerformance {
    pub adjusted_pnl: Decimal,
    /// Opening equity plus cumulative deposits; withdrawals do not reduce it.
    pub loss_guard_denominator: Decimal,
    /// None means unavailable and must block entries, not a zero loss.
    pub loss_guard_fraction: Option<Decimal>,
}

/// All amounts share an account currency and flows are confirmed, cumulative,
/// nonnegative external flows. This is not a time-weighted investment return.
pub fn session_performance(
    opening_equity: Decimal,
    current_equity: Decimal,
    deposits: Decimal,
    withdrawals: Decimal,
) -> Result<SessionPerformance> {
    require(
        opening_equity >= Decimal::ZERO
            && current_equity >= Decimal::ZERO
            && deposits >= Decimal::ZERO
            && withdrawals >= Decimal::ZERO,
        "session equity and gross cash flows must be nonnegative",
    )?;
    let denominator = add(opening_equity, deposits)?;
    let adjusted_pnl = add(
        sub(sub(current_equity, opening_equity)?, deposits)?,
        withdrawals,
    )?;
    Ok(SessionPerformance {
        adjusted_pnl,
        loss_guard_denominator: denominator,
        loss_guard_fraction: if denominator > Decimal::ZERO {
            Some(div(adjusted_pnl, denominator)?)
        } else {
            None
        },
    })
}

/// A position outcome's contribution to account growth has a separate denominator.
/// Project/hosting costs and outside cash flows belong in separate reports.
pub fn account_return_contribution(pnl: &Money, opening_equity: &Money) -> Result<Decimal> {
    require(
        pnl.currency == opening_equity.currency && opening_equity.amount > Decimal::ZERO,
        "invalid account return denominator or currency",
    )?;
    div(pnl.amount, opening_equity.amount)
}

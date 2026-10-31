//! Offline proof fixtures execute the production calculator and checked helpers.
//! This bridge grants no order authority and accepts no caller verdict or witness.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{CalculatedLimits, FormulaEnvelope, FormulaParameters, LimitsInput, calculate_limits};
use crate::{Policy, add, ceil_to, div, floor_to, mul, sub};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum Request {
    Formula {
        policy: Box<Policy>,
        envelope: Box<FormulaEnvelope>,
        parameters: Box<FormulaParameters>,
        max_account_risk_fraction: Decimal,
        input: Box<LimitsInput>,
        as_of: DateTime<Utc>,
    },
    Decimal {
        operation: Operation,
        a: Decimal,
        b: Decimal,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Operation {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Floor,
    Ceil,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum Reply {
    Accepted { limits: Box<CalculatedLimits> },
    Decimal { value: Decimal },
    Rejected { error: String },
}

/// Canonical exact base-ten values make insignificant scale bytes irrelevant to
/// the wire comparison; no rounding or conversion through floats occurs.
fn canonical(mut limits: CalculatedLimits) -> CalculatedLimits {
    for value in [
        &mut limits.buy_limit,
        &mut limits.quantity,
        &mut limits.stop_trigger,
        &mut limits.target_sell_limit,
        &mut limits.target_net_fraction,
        &mut limits.target_rounding_overshoot,
        &mut limits.budget.amount,
        &mut limits.acquisition_cost.amount,
        &mut limits.proceeds_at_stop.amount,
        &mut limits.proceeds_at_target.amount,
        &mut limits.planned_loss_fraction,
        &mut limits.account_risk.amount,
        &mut limits.reward_to_risk,
    ] {
        *value = value.normalize();
    }
    limits
}

pub fn evaluate(request: &Request) -> Reply {
    match request {
        Request::Formula {
            policy,
            envelope,
            parameters,
            max_account_risk_fraction,
            input,
            as_of,
        } => match calculate_limits(
            policy,
            envelope,
            parameters,
            *max_account_risk_fraction,
            input,
            *as_of,
        ) {
            Ok(limits) => Reply::Accepted {
                limits: Box::new(canonical(limits)),
            },
            Err(error) => Reply::Rejected {
                error: error.to_string(),
            },
        },
        Request::Decimal { operation, a, b } => {
            let result = match operation {
                Operation::Add => add(*a, *b),
                Operation::Sub => sub(*a, *b),
                Operation::Mul => mul(*a, *b),
                Operation::Div => div(*a, *b),
                Operation::Rem => a
                    .checked_rem(*b)
                    .ok_or_else(|| trady_domain::Error("decimal remainder overflow".into())),
                Operation::Floor => floor_to(*a, *b),
                Operation::Ceil => ceil_to(*a, *b),
            };
            match result {
                Ok(value) => Reply::Decimal {
                    value: value.normalize(),
                },
                Err(error) => Reply::Rejected {
                    error: error.to_string(),
                },
            }
        }
    }
}

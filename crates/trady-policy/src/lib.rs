//! Pure, checked financial calculations. Inputs must be verified observations.

pub mod capital;
pub mod economics;
pub mod holding;
pub mod intelligence;
pub mod limits;
pub mod prediction;
pub mod sentiment;

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trady_domain::{AccountSnapshot, Capabilities, Currency, Error, Money, Quote, Result, require};

pub fn add(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_add(b)
        .ok_or_else(|| Error("decimal addition overflow".into()))
}
pub fn sub(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_sub(b)
        .ok_or_else(|| Error("decimal subtraction overflow".into()))
}
pub fn mul(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_mul(b)
        .ok_or_else(|| Error("decimal multiplication overflow".into()))
}
pub fn div(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_div(b)
        .ok_or_else(|| Error("decimal division overflow or zero denominator".into()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: u32,
    pub entry_cash_fraction: Decimal,
    pub confidence_entry_min: u8,
    pub max_target_fraction: Decimal,
    pub exceptional_targets_enabled: bool,
    pub max_initial_loss_fraction: Decimal,
    pub min_reward_to_risk: Decimal,
    pub max_holding_sessions: u8,
    pub leverage: bool,
    pub add_to_position: bool,
    pub rotation_enabled: bool,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            version: 1,
            entry_cash_fraction: Decimal::new(98, 2),
            confidence_entry_min: 90,
            max_target_fraction: Decimal::new(10, 2),
            exceptional_targets_enabled: false,
            max_initial_loss_fraction: Decimal::new(2, 2),
            min_reward_to_risk: Decimal::new(15, 1),
            max_holding_sessions: 5,
            leverage: false,
            add_to_position: false,
            rotation_enabled: false,
        }
    }
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        require(self.version > 0, "policy version must be positive")?;
        require(
            self.entry_cash_fraction > Decimal::ZERO
                && self.entry_cash_fraction <= Decimal::new(98, 2),
            "entry allocation exceeds 98% or is nonpositive",
        )?;
        require(
            (90..=100).contains(&self.confidence_entry_min),
            "entry confidence must be 90 through 100",
        )?;
        require(
            self.max_target_fraction > Decimal::ZERO
                && self.max_target_fraction <= Decimal::new(10, 2),
            "target ceiling exceeds 10% or is nonpositive",
        )?;
        require(
            self.max_initial_loss_fraction > Decimal::ZERO
                && self.max_initial_loss_fraction <= Decimal::new(2, 2),
            "initial loss exceeds 2% or is nonpositive",
        )?;
        require(
            self.min_reward_to_risk >= Decimal::new(15, 1),
            "reward/risk must be at least 1.5",
        )?;
        require(
            (1..=5).contains(&self.max_holding_sessions),
            "holding limit must be one through five sessions",
        )?;
        require(
            !self.leverage && !self.add_to_position && !self.rotation_enabled,
            "leverage, additions and rotation are unsupported by the foundation",
        )
    }
}

/// Values are application-verified features, never a model-authored score.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConfidenceInputs {
    pub primary_verified: bool,
    pub catalyst: Catalyst,
    pub independent_lineages: u8,
    pub positive_15m_return: bool,
    pub above_vwap: bool,
    pub relative_volume: Option<Decimal>,
    pub spread_fraction: Option<Decimal>,
    pub round_trip_cost_fraction: Option<Decimal>,
    pub catalyst_age_hours: Option<u32>,
    pub sources_current: bool,
    pub continuing_relevance: bool,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Catalyst {
    #[default]
    Generic,
    Indirect,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConfidenceComponents")]
pub struct ConfidenceScore {
    primary: u8,
    catalyst: u8,
    independent: u8,
    price: u8,
    economics: u8,
    freshness: u8,
}
impl ConfidenceScore {
    /// Components come from `confidence` or a checked journal snapshot, never a raw total.
    pub fn total(&self) -> u8 {
        self.primary
            + self.catalyst
            + self.independent
            + self.price
            + self.economics
            + self.freshness
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfidenceComponents {
    primary: u8,
    catalyst: u8,
    independent: u8,
    price: u8,
    economics: u8,
    freshness: u8,
}

impl TryFrom<ConfidenceComponents> for ConfidenceScore {
    type Error = Error;

    fn try_from(value: ConfidenceComponents) -> Result<Self> {
        require(
            [0, 25].contains(&value.primary)
                && [0, 10, 20].contains(&value.catalyst)
                && [0, 8, 15].contains(&value.independent)
                && [0, 10, 20].contains(&value.price)
                && [0, 5, 10].contains(&value.economics)
                && [0, 5, 10].contains(&value.freshness),
            "invalid confidence components",
        )?;
        Ok(Self {
            primary: value.primary,
            catalyst: value.catalyst,
            independent: value.independent,
            price: value.price,
            economics: value.economics,
            freshness: value.freshness,
        })
    }
}
pub fn confidence(inputs: &ConfidenceInputs) -> ConfidenceScore {
    let price_count = u8::from(inputs.positive_15m_return)
        + u8::from(inputs.above_vwap)
        + u8::from(
            inputs
                .relative_volume
                .is_some_and(|v| v >= Decimal::new(15, 1)),
        );
    let economics = match (inputs.spread_fraction, inputs.round_trip_cost_fraction) {
        (Some(s), Some(c))
            if s >= Decimal::ZERO
                && c >= Decimal::ZERO
                && s <= Decimal::new(20, 4)
                && c <= Decimal::new(40, 4) =>
        {
            10
        }
        (Some(s), Some(c))
            if s >= Decimal::ZERO
                && c >= Decimal::ZERO
                && s <= Decimal::new(35, 4)
                && c <= Decimal::new(60, 4) =>
        {
            5
        }
        _ => 0,
    };
    ConfidenceScore {
        primary: if inputs.primary_verified { 25 } else { 0 },
        catalyst: match inputs.catalyst {
            Catalyst::Direct => 20,
            Catalyst::Indirect => 10,
            Catalyst::Generic => 0,
        },
        independent: match inputs.independent_lineages {
            0 => 0,
            1 => 8,
            _ => 15,
        },
        price: match price_count {
            3 => 20,
            2 => 10,
            _ => 0,
        },
        economics,
        freshness: match inputs.catalyst_age_hours {
            Some(0..=24) if inputs.sources_current => 10,
            Some(0..=72) if inputs.continuing_relevance => 5,
            _ => 0,
        },
    }
}

pub fn target_ceiling(score: u8, exceptional_verified: bool, policy: &Policy) -> Result<Decimal> {
    policy.validate()?;
    require(score <= 100, "confidence score exceeds 100")?;
    let band = match score {
        0..=69 => Decimal::ZERO,
        70..=79 => Decimal::new(1, 2),
        80..=89 => Decimal::new(2, 2),
        95..=100 if exceptional_verified && policy.exceptional_targets_enabled => {
            Decimal::new(10, 2)
        }
        _ => Decimal::new(3, 2),
    };
    Ok(band.min(policy.max_target_fraction))
}

pub fn validate_risk(
    target: Decimal,
    planned_loss: Decimal,
    score: u8,
    exceptional_verified: bool,
    policy: &Policy,
) -> Result<()> {
    let ceiling = target_ceiling(score, exceptional_verified, policy)?;
    require(
        score >= policy.confidence_entry_min,
        "entry confidence below policy floor",
    )?;
    require(
        target > Decimal::ZERO && target <= ceiling,
        "target exceeds confidence or policy ceiling",
    )?;
    require(
        planned_loss > Decimal::ZERO && planned_loss <= policy.max_initial_loss_fraction,
        "invalid initial loss",
    )?;
    require(
        target >= mul(planned_loss, policy.min_reward_to_risk)?,
        "insufficient reward/risk",
    )
}

pub fn fresh(observed_at: DateTime<Utc>, now: DateTime<Utc>, max_age_seconds: i64) -> bool {
    max_age_seconds >= 0
        && observed_at <= now
        && now.signed_duration_since(observed_at)
            <= Duration::seconds(max_age_seconds.min(i64::MAX / 1000))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryRange {
    pub minimum: Decimal,
    pub maximum: Decimal,
    pub currency: Currency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FxObservation {
    pub from: Currency,
    pub to: Currency,
    pub rate: Decimal,
    pub observed_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryCosts {
    pub fixed: Money,
    pub rate: Decimal,
    pub uncertainty: Money,
    pub fx: Option<FxObservation>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentRules {
    pub price_tick: Decimal,
    pub quantity_step: Decimal,
    pub minimum_quantity: Decimal,
    pub currency_unit: Decimal,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizedEntry {
    pub limit_price: Decimal,
    pub quantity: Decimal,
    pub budget: Money,
    pub estimated_cost: Money,
    pub remaining_cash: Money,
}

pub fn floor_to(value: Decimal, step: Decimal) -> Result<Decimal> {
    require(
        step > Decimal::ZERO && value >= Decimal::ZERO,
        "invalid rounding step or value",
    )?;
    let remainder = value
        .checked_rem(step)
        .ok_or_else(|| Error("decimal remainder overflow".into()))?;
    let rounded = sub(value, remainder)?;
    require(
        rounded <= value && rounded.checked_rem(step) == Some(Decimal::ZERO),
        "rounding precision exhausted",
    )?;
    Ok(rounded)
}

/// Round upward without division losing a tiny remainder at a large magnitude.
pub fn ceil_to(value: Decimal, step: Decimal) -> Result<Decimal> {
    let floor = floor_to(value, step)?;
    if floor == value {
        Ok(value)
    } else {
        let rounded = add(floor, step)?;
        require(
            rounded >= value && rounded > floor && rounded.checked_rem(step) == Some(Decimal::ZERO),
            "rounding precision exhausted",
        )?;
        Ok(rounded)
    }
}

/// A linear, conservative cost model only. Unsupported fee models must abstain.
pub fn size_entry(
    cash: &Money,
    quote: &Quote,
    range: &EntryRange,
    costs: &EntryCosts,
    rules: &InstrumentRules,
    now: DateTime<Utc>,
    policy: &Policy,
) -> Result<SizedEntry> {
    policy.validate()?;
    require(cash.amount >= Decimal::ZERO, "negative cash")?;
    require(
        quote.tradable && quote.bid > Decimal::ZERO && quote.ask >= quote.bid,
        "unusable bid/ask",
    )?;
    require(
        quote.observed_at <= quote.received_at
            && fresh(quote.observed_at, now, 10)
            && fresh(quote.received_at, now, 10),
        "stale or future quote",
    )?;
    require(
        range.currency == quote.currency
            && range.minimum > Decimal::ZERO
            && range.minimum <= range.maximum,
        "invalid entry range or currency",
    )?;
    require(
        quote.ask >= range.minimum && quote.ask <= range.maximum,
        "ask outside inclusive entry range",
    )?;
    require(
        rules.price_tick > Decimal::ZERO
            && rules.quantity_step > Decimal::ZERO
            && rules.minimum_quantity > Decimal::ZERO
            && rules.currency_unit > Decimal::ZERO,
        "invalid instrument increments",
    )?;
    require(
        costs.fixed.currency == cash.currency && costs.uncertainty.currency == cash.currency,
        "cost currency mismatch",
    )?;
    require(
        costs.fixed.amount >= Decimal::ZERO
            && costs.uncertainty.amount >= Decimal::ZERO
            && costs.rate >= Decimal::ZERO,
        "negative costs",
    )?;
    let fx = if quote.currency == cash.currency {
        Decimal::ONE
    } else {
        let fx = costs
            .fx
            .as_ref()
            .ok_or_else(|| Error("FX observation required".into()))?;
        require(
            fx.from == quote.currency
                && fx.to == cash.currency
                && fx.rate > Decimal::ZERO
                && fresh(fx.observed_at, now, 60),
            "invalid or stale FX",
        )?;
        fx.rate
    };
    let limit_price = ceil_to(quote.ask, rules.price_tick)?;
    require(
        limit_price >= range.minimum && limit_price <= range.maximum,
        "no valid entry tick inside range",
    )?;
    let budget = floor_to(
        mul(cash.amount, policy.entry_cash_fraction)?,
        rules.currency_unit,
    )?;
    let fixed = add(costs.fixed.amount, costs.uncertainty.amount)?;
    require(budget > fixed, "cash cannot cover costs")?;
    let unit_cost = mul(mul(limit_price, fx)?, add(Decimal::ONE, costs.rate)?)?;
    let quantity = floor_to(div(sub(budget, fixed)?, unit_cost)?, rules.quantity_step)?;
    require(quantity >= rules.minimum_quantity, "below minimum quantity")?;
    let estimated_cost = add(mul(quantity, unit_cost)?, fixed)?;
    require(estimated_cost <= budget, "rounded cost exceeds allocation")?;
    Ok(SizedEntry {
        limit_price,
        quantity,
        budget: Money {
            amount: budget,
            currency: cash.currency.clone(),
        },
        estimated_cost: Money {
            amount: estimated_cost,
            currency: cash.currency.clone(),
        },
        remaining_cash: Money {
            amount: sub(cash.amount, estimated_cost)?,
            currency: cash.currency.clone(),
        },
    })
}

/// This is only the account/capability portion of readiness; it grants no authority.
pub fn validate_account_entry(
    snapshot: &AccountSnapshot,
    capabilities: &Capabilities,
    now: DateTime<Utc>,
) -> Result<()> {
    snapshot.validate()?;
    require(
        fresh(snapshot.observed_at, now, 60),
        "stale or future account snapshot",
    )?;
    require(
        snapshot.positions.is_empty() && snapshot.orders.iter().all(|o| o.status.terminal()),
        "holding or unresolved order blocks new entry",
    )?;
    require(
        [
            &capabilities.cash_account,
            &capabilities.instrument,
            &capabilities.protective_orders,
            &capabilities.quotes,
        ]
        .iter()
        .all(|c| c.usable(now)),
        "unknown, unsupported or expired entry capability",
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionLossGuard {
    pub session_id: String,
    pub paused: bool,
}
impl SessionLossGuard {
    pub fn observe(
        &mut self,
        opening_equity: Decimal,
        current_equity: Decimal,
        net_deposits: Decimal,
    ) -> Result<()> {
        // Compatibility for a single net flow. Use observe_cash_flows when both
        // deposits and withdrawals occurred; netting them loses the denominator.
        let deposits = net_deposits.max(Decimal::ZERO);
        let withdrawals = sub(Decimal::ZERO, net_deposits.min(Decimal::ZERO))?;
        self.observe_cash_flows(opening_equity, current_equity, deposits, withdrawals)
            .map(|_| ())
    }

    /// Cumulative confirmed flows, not pending transfers. Never clears the latch.
    pub fn observe_cash_flows(
        &mut self,
        opening_equity: Decimal,
        current_equity: Decimal,
        deposits: Decimal,
        withdrawals: Decimal,
    ) -> Result<capital::SessionPerformance> {
        let result =
            capital::session_performance(opening_equity, current_equity, deposits, withdrawals);
        match result {
            Ok(performance) => {
                if performance
                    .loss_guard_fraction
                    .is_none_or(|fraction| fraction <= Decimal::new(-2, 2))
                {
                    self.paused = true;
                }
                Ok(performance)
            }
            Err(error) => {
                self.paused = true;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    fn d(v: &str) -> Decimal {
        Decimal::from_str(v).unwrap()
    }
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
            .unwrap()
            .to_utc()
    }
    fn sizing(cash: &str, ask: &str, fee: &str) -> Result<SizedEntry> {
        let eur = Currency::new("EUR")?;
        size_entry(
            &Money {
                amount: d(cash),
                currency: eur.clone(),
            },
            &Quote {
                instrument_id: trady_domain::InstrumentId::new("stock")?,
                currency: eur.clone(),
                bid: d(ask),
                ask: d(ask),
                observed_at: now(),
                received_at: now(),
                tradable: true,
            },
            &EntryRange {
                minimum: d("1"),
                maximum: d("200"),
                currency: eur.clone(),
            },
            &EntryCosts {
                fixed: Money {
                    amount: d(fee),
                    currency: eur.clone(),
                },
                rate: d("0.0015"),
                uncertainty: Money {
                    amount: d("0.02"),
                    currency: eur,
                },
                fx: None,
            },
            &InstrumentRules {
                price_tick: d("0.01"),
                quantity_step: d("0.001"),
                minimum_quantity: d("0.001"),
                currency_unit: d("0.01"),
            },
            now(),
            &Policy::default(),
        )
    }
    #[test]
    fn cap_01_02_costs_and_deposits_use_total_budget() {
        let small = sizing("50", "10", "0.10").unwrap();
        assert_eq!(small.budget.amount, d("49"));
        assert!(small.estimated_cost.amount <= d("49"));
        assert!(small.remaining_cash.amount >= d("1"));
        assert_eq!(small.quantity, d("4.880"));
        let large = sizing("150", "10", "0.10").unwrap();
        assert_eq!(large.budget.amount, d("147"));
        assert!(large.quantity > small.quantity);
        assert!(sizing("0.01", "100", "0.10").is_err());
    }
    #[test]
    fn pol_01_02_03_confidence_and_hard_ceilings() {
        for score in [69, 70, 79, 80, 89] {
            assert!(
                validate_risk(d("0.01"), d("0.005"), score, false, &Policy::default()).is_err()
            );
        }
        for score in [90, 94, 95, 100] {
            assert!(validate_risk(d("0.01"), d("0.005"), score, false, &Policy::default()).is_ok());
        }
        assert!(validate_risk(d("0.01"), d("0.02"), 100, false, &Policy::default()).is_err());
        assert!(validate_risk(d("0.11"), d("0.01"), 100, true, &Policy::default()).is_err());
        for p in [
            Policy {
                entry_cash_fraction: d("0.99"),
                ..Policy::default()
            },
            Policy {
                max_target_fraction: d("0.11"),
                ..Policy::default()
            },
            Policy {
                leverage: true,
                ..Policy::default()
            },
            Policy {
                confidence_entry_min: 89,
                ..Policy::default()
            },
        ] {
            assert!(p.validate().is_err());
        }
        assert_eq!(
            target_ceiling(69, false, &Policy::default()).unwrap(),
            Decimal::ZERO
        );
        assert_eq!(
            target_ceiling(70, false, &Policy::default()).unwrap(),
            d("0.01")
        );
        assert_eq!(
            target_ceiling(80, false, &Policy::default()).unwrap(),
            d("0.02")
        );
    }
    #[test]
    fn confidence_is_recomputed_and_missing_data_adds_nothing() {
        assert_eq!(confidence(&ConfidenceInputs::default()).total(), 0);
        let inputs = ConfidenceInputs {
            primary_verified: true,
            catalyst: Catalyst::Direct,
            independent_lineages: 2,
            positive_15m_return: true,
            above_vwap: true,
            relative_volume: Some(d("1.5")),
            spread_fraction: Some(d("0.002")),
            round_trip_cost_fraction: Some(d("0.004")),
            catalyst_age_hours: Some(24),
            sources_current: true,
            continuing_relevance: false,
        };
        assert_eq!(confidence(&inputs).total(), 100);
        assert_eq!(
            confidence(&ConfidenceInputs {
                independent_lineages: 0,
                ..inputs
            })
            .total(),
            85
        );
    }
    #[test]
    fn pol_06_loss_pause_latches_despite_rebound_and_deposit() {
        let mut guard = SessionLossGuard {
            session_id: "xnys_2026_10_30".into(),
            paused: false,
        };
        guard.observe(d("100"), d("98"), Decimal::ZERO).unwrap();
        guard.observe(d("100"), d("210"), d("100")).unwrap();
        assert!(guard.paused);
    }
    #[test]
    fn overflow_and_invalid_steps_fail_without_panicking() {
        assert!(mul(Decimal::MAX, d("2")).is_err());
        assert!(div(Decimal::ONE, Decimal::ZERO).is_err());
        assert!(floor_to(Decimal::ONE, Decimal::ZERO).is_err());
        assert!(!fresh(now() + Duration::seconds(1), now(), 10));
    }
    #[test]
    fn dat_01_stale_quotes_fx_and_out_of_range_asks_abstain() {
        let eur = Currency::new("EUR").unwrap();
        let usd = Currency::new("USD").unwrap();
        let cash = Money {
            amount: d("50"),
            currency: eur.clone(),
        };
        let mut quote = Quote {
            instrument_id: trady_domain::InstrumentId::new("stock").unwrap(),
            currency: usd.clone(),
            bid: d("9.99"),
            ask: d("10"),
            observed_at: now(),
            received_at: now(),
            tradable: true,
        };
        let range = EntryRange {
            minimum: d("9.95"),
            maximum: d("10.05"),
            currency: usd.clone(),
        };
        let rules = InstrumentRules {
            price_tick: d("0.01"),
            quantity_step: d("0.001"),
            minimum_quantity: d("0.001"),
            currency_unit: d("0.01"),
        };
        let mut costs = EntryCosts {
            fixed: Money {
                amount: d("0.10"),
                currency: eur.clone(),
            },
            rate: d("0.0015"),
            uncertainty: Money {
                amount: d("0.02"),
                currency: eur.clone(),
            },
            fx: None,
        };
        let size = |quote: &Quote, costs: &EntryCosts| {
            size_entry(
                &cash,
                quote,
                &range,
                costs,
                &rules,
                now(),
                &Policy::default(),
            )
        };
        assert!(size(&quote, &costs).is_err());
        costs.fx = Some(FxObservation {
            from: usd,
            to: eur,
            rate: d("0.9"),
            observed_at: now(),
        });
        let accepted = size(&quote, &costs).unwrap();
        assert!(accepted.estimated_cost.amount <= d("49"));
        costs.fx.as_mut().unwrap().observed_at = now() - Duration::seconds(61);
        assert!(size(&quote, &costs).is_err());
        costs.fx.as_mut().unwrap().observed_at = now();
        quote.observed_at = now() - Duration::seconds(11);
        assert!(size(&quote, &costs).is_err());
        quote.observed_at = now();
        quote.ask = d("9.94");
        quote.bid = d("9.93");
        assert!(size(&quote, &costs).is_err());
        quote.ask = d("10.06");
        assert!(size(&quote, &costs).is_err());
    }
}

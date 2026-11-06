//! EUR same-day policy and deterministic entry/exit economics.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use trady_domain::{
    AccountSnapshot, Currency, Error, InstrumentId, Money, Quote, Result, SignalSnapshot, require,
};

use crate::economics::{
    CostBasis, ExitCosts, liquidation_proceeds, planned_loss_fraction, target_price,
};
use crate::{
    EntryCosts, EntryRange, InstrumentRules, Policy, add, ceil_to, div, fresh, mul, size_entry, sub,
};

pub const INTRADAY_POLICY_VERSION: &str = "eur_intraday_v1";
pub const THIRTY_ENTRY_POLICY_VERSION: &str = "eur_intraday_30_entries_v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntradayRiskPolicy {
    pub revision: String,
    pub desired_target_min_fraction: Decimal,
    pub desired_target_max_fraction: Decimal,
    pub max_initial_loss_fraction: Decimal,
    pub min_reward_to_risk: Decimal,
    pub max_hold_seconds: i64,
    pub minimum_seconds_to_flatten: i64,
    /// Conservative entry attempts per UTC day. STOPs and exits are separate.
    pub max_daily_entries: u8,
    pub max_daily_exits: Option<u64>,
    pub max_open_positions: u8,
    pub rotation_enabled: bool,
    pub minimum_rotation_net_improvement_fraction: Decimal,
    pub no_overnight: bool,
    pub sentiment_may_widen_stop: bool,
}

impl Default for IntradayRiskPolicy {
    fn default() -> Self {
        Self {
            revision: INTRADAY_POLICY_VERSION.into(),
            desired_target_min_fraction: Decimal::new(1, 2),
            desired_target_max_fraction: Decimal::new(3, 2),
            max_initial_loss_fraction: Decimal::new(2, 2),
            min_reward_to_risk: Decimal::new(15, 1),
            max_hold_seconds: 28_800,
            minimum_seconds_to_flatten: 1_800,
            // The first-run authority has one entry slot. A later release may
            // explicitly raise this after rotation evidence is approved.
            max_daily_entries: 1,
            max_daily_exits: Some(2),
            max_open_positions: 1,
            rotation_enabled: true,
            minimum_rotation_net_improvement_fraction: Decimal::new(2, 3),
            no_overnight: true,
            sentiment_may_widen_stop: false,
        }
    }
}

impl IntradayRiskPolicy {
    pub fn validate(&self) -> Result<()> {
        require(
            ((self.revision == INTRADAY_POLICY_VERSION
                && (1..=2).contains(&self.max_daily_entries)
                && self.max_daily_exits.is_some_and(|v| (1..=2).contains(&v)))
                || (self.revision == THIRTY_ENTRY_POLICY_VERSION
                    && (1..=30).contains(&self.max_daily_entries)
                    && self.max_daily_exits.is_none()))
                && self.desired_target_min_fraction >= Decimal::new(1, 2)
                && self.desired_target_max_fraction >= self.desired_target_min_fraction
                && self.desired_target_max_fraction <= Decimal::new(3, 2)
                && self.max_initial_loss_fraction >= Decimal::new(1, 2)
                && self.max_initial_loss_fraction <= Decimal::new(2, 2)
                && self.min_reward_to_risk >= Decimal::new(15, 1)
                && (300..=86_400).contains(&self.max_hold_seconds)
                && (60..=86_400).contains(&self.minimum_seconds_to_flatten)
                && self.max_open_positions == 1
                && self.minimum_rotation_net_improvement_fraction > Decimal::ZERO
                && self.minimum_rotation_net_improvement_fraction <= Decimal::new(1, 2)
                && self.no_overnight
                && !self.sentiment_may_widen_stop,
            "invalid EUR intraday risk policy",
        )
    }

    fn sizing_policy(&self) -> Policy {
        Policy {
            entry_cash_fraction: Decimal::new(98, 2),
            max_order_cash_fraction: Decimal::new(98, 2),
            max_target_fraction: self.desired_target_max_fraction,
            max_initial_loss_fraction: self.max_initial_loss_fraction,
            min_reward_to_risk: self.min_reward_to_risk,
            max_holding_sessions: 1,
            ..Policy::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntradayTradePlan {
    pub plan_id: String,
    pub policy_revision: String,
    pub account_id: String,
    pub instrument_id: InstrumentId,
    pub currency: Currency,
    pub entry_price: Decimal,
    pub quantity: Decimal,
    pub acquisition_cost: Money,
    pub stop_price: Decimal,
    pub target_price: Decimal,
    pub target_net_fraction: Decimal,
    pub planned_loss_fraction: Decimal,
    pub reward_to_risk: Decimal,
    pub created_at: DateTime<Utc>,
    pub flatten_deadline: DateTime<Utc>,
    pub evidence_snapshot_ids: Vec<String>,
    pub stop_revision: u64,
    /// Retained calculator inputs. Legacy plans can be read for recovery, but
    /// cannot establish new entry readiness without reproducible economics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_model: Option<IntradayCostModel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntradayCostModel {
    pub entry: EntryCosts,
    pub exit: ExitCosts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationAssessment {
    pub session_id: String,
    pub current_plan_id: String,
    pub replacement_instrument_id: InstrumentId,
    pub open_position_count: u8,
    pub current_exit_verified: bool,
    pub replacement_quote_and_risk_checked: bool,
    pub current_evidence_score: Decimal,
    pub replacement_evidence_score: Decimal,
    pub current_expected_net_fraction: Decimal,
    pub replacement_expected_net_fraction: Decimal,
    pub evaluated_at: DateTime<Utc>,
}

impl RotationAssessment {
    pub fn validate(&self, policy: &IntradayRiskPolicy, now: DateTime<Utc>) -> Result<()> {
        policy.validate()?;
        require(
            policy.rotation_enabled
                && valid_id(&self.session_id)
                && valid_id(&self.current_plan_id)
                && self.replacement_instrument_id.as_str().ends_with("_EQ")
                && self.open_position_count == policy.max_open_positions
                && self.current_exit_verified
                && self.replacement_quote_and_risk_checked
                && self.current_evidence_score >= Decimal::ZERO
                && self.replacement_evidence_score > self.current_evidence_score
                && self.current_expected_net_fraction >= Decimal::ZERO
                && self.replacement_expected_net_fraction
                    >= self.current_expected_net_fraction
                        + policy.minimum_rotation_net_improvement_fraction
                && self.evaluated_at <= now,
            "replacement does not justify a same-day rotation",
        )
    }
}

pub fn approve_rotation(
    assessment: &RotationAssessment,
    policy: &IntradayRiskPolicy,
    now: DateTime<Utc>,
) -> Result<()> {
    assessment.validate(policy, now)
}

impl IntradayTradePlan {
    pub fn validate(&self, now: DateTime<Utc>, policy: &IntradayRiskPolicy) -> Result<()> {
        policy.validate()?;
        require(
            valid_id(&self.plan_id)
                && self.policy_revision == policy.revision
                && valid_id(&self.account_id)
                && self.instrument_id.as_str().ends_with("_EQ")
                && self.currency.as_str() == "EUR"
                && self.acquisition_cost.currency == self.currency
                && self.entry_price > Decimal::ZERO
                && self.quantity > Decimal::ZERO
                && self.acquisition_cost.amount > Decimal::ZERO
                && self.stop_price > Decimal::ZERO
                && self.stop_price < self.entry_price
                && self.target_price > self.entry_price
                && self.target_net_fraction >= policy.desired_target_min_fraction
                && self.target_net_fraction <= policy.desired_target_max_fraction
                && self.planned_loss_fraction > Decimal::ZERO
                && self.planned_loss_fraction <= policy.max_initial_loss_fraction
                && self.reward_to_risk >= policy.min_reward_to_risk
                && self.created_at <= now
                && self.flatten_deadline > now
                && self.evidence_snapshot_ids.len() <= 128
                && self.evidence_snapshot_ids.iter().all(|id| valid_id(id)),
            "invalid EUR intraday trade plan",
        )?;
        let costs = self
            .cost_model
            .as_ref()
            .ok_or_else(|| Error("intraday entry plan lacks reproducible cost inputs".into()))?;
        require(
            costs.entry.fixed.currency == self.currency
                && costs.entry.uncertainty.currency == self.currency
                && costs.entry.fixed.amount >= Decimal::ZERO
                && costs.entry.uncertainty.amount >= Decimal::ZERO
                && costs.entry.rate >= Decimal::ZERO
                && costs.entry.rate < Decimal::ONE
                && costs.entry.fx.is_none()
                && costs.exit.fx.is_none(),
            "invalid EUR intraday entry costs",
        )?;
        let acquisition = add(
            mul(
                self.quantity,
                mul(self.entry_price, add(Decimal::ONE, costs.entry.rate)?)?,
            )?,
            add(costs.entry.fixed.amount, costs.entry.uncertainty.amount)?,
        )?;
        require(
            acquisition == self.acquisition_cost.amount,
            "intraday acquisition cost differs from calculator inputs",
        )?;
        let basis = CostBasis {
            instrument_id: self.instrument_id.clone(),
            quantity: self.quantity,
            instrument_currency: self.currency.clone(),
            acquisition_cost: self.acquisition_cost.clone(),
        };
        let loss = planned_loss_fraction(&basis, self.stop_price, &costs.exit, now)?;
        let proceeds = liquidation_proceeds(&basis, self.target_price, &costs.exit, now)?;
        let target = div(sub(proceeds.amount, acquisition)?, acquisition)?;
        require(
            loss == self.planned_loss_fraction
                && target == self.target_net_fraction
                && self.reward_to_risk == div(target, loss)?,
            "intraday risk economics differ from calculator inputs",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExitReason {
    Stop,
    Target,
    ProfitProtection,
    AdverseEvidence,
    StaleData,
    BrokerDisconnect,
    Halt,
    Deadline,
    OwnerClose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DynamicExitDecision {
    pub plan_id: String,
    pub reason: ExitReason,
    pub observed_bid: Option<Decimal>,
    pub proposed_stop: Decimal,
    pub proposed_target: Decimal,
    pub decided_at: DateTime<Utc>,
    pub evidence_ids: Vec<String>,
}

impl DynamicExitDecision {
    pub fn validate_against(&self, plan: &IntradayTradePlan, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_id(&self.plan_id)
                && self.plan_id == plan.plan_id
                && self.decided_at <= now
                && self.proposed_stop >= plan.stop_price
                && self.proposed_stop <= plan.entry_price
                && self.proposed_target >= plan.target_price
                && self.evidence_ids.len() <= 64
                && self.evidence_ids.iter().all(|id| valid_id(id)),
            "dynamic exit widens protection or is invalid",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlattenDecision {
    Continue,
    Flatten(ExitReason),
}

pub fn flatten_decision(
    now: DateTime<Utc>,
    flatten_deadline: DateTime<Utc>,
    quote_fresh: bool,
    market_open: bool,
) -> Result<FlattenDecision> {
    require(
        flatten_deadline > DateTime::<Utc>::UNIX_EPOCH,
        "invalid flatten deadline",
    )?;
    if now >= flatten_deadline {
        return Ok(FlattenDecision::Flatten(ExitReason::Deadline));
    }
    if !market_open {
        return Ok(FlattenDecision::Flatten(ExitReason::Halt));
    }
    if !quote_fresh {
        return Ok(FlattenDecision::Flatten(ExitReason::StaleData));
    }
    Ok(FlattenDecision::Continue)
}

/// Build the executable entry/stop/target/quantity proposal. The signal only
/// supplies bounded context; all financial values are recomputed here.
#[allow(clippy::too_many_arguments)]
pub fn calculate_intraday_plan(
    account: &AccountSnapshot,
    quote: &Quote,
    signal: &SignalSnapshot,
    entry_costs: &EntryCosts,
    exit_costs: &ExitCosts,
    rules: &InstrumentRules,
    flatten_deadline: DateTime<Utc>,
    policy: &IntradayRiskPolicy,
    plan_id: String,
    evidence_snapshot_ids: Vec<String>,
    now: DateTime<Utc>,
) -> Result<IntradayTradePlan> {
    policy.validate()?;
    account.validate()?;
    require(
        account.spendable_cash.currency.as_str() == "EUR"
            && quote.currency.as_str() == "EUR"
            && quote.instrument_id == signal.instrument_id
            && quote.tradable
            && quote.bid > Decimal::ZERO
            && quote.ask >= quote.bid
            && quote.observed_at <= quote.received_at
            && fresh(quote.observed_at, now, 10)
            && fresh(quote.received_at, now, 10),
        "intraday plan requires a fresh EUR executable quote",
    )?;
    signal.validate(now)?;
    require(
        flatten_deadline > now
            && flatten_deadline <= signal.max_hold_deadline
            && flatten_deadline - now <= chrono::Duration::seconds(policy.max_hold_seconds),
        "intraday plan exceeds the same-day holding deadline",
    )?;
    let size = size_entry(
        &account.spendable_cash,
        quote,
        &EntryRange {
            minimum: signal.entry_zone.minimum,
            maximum: signal.entry_zone.maximum,
            currency: quote.currency.clone(),
        },
        entry_costs,
        rules,
        now,
        &policy.sizing_policy(),
    )?;
    let basis = CostBasis {
        instrument_id: quote.instrument_id.clone(),
        quantity: size.quantity,
        instrument_currency: quote.currency.clone(),
        acquisition_cost: size.estimated_cost.clone(),
    };
    let stop_price = ceil_to(signal.invalidation, rules.price_tick)?;
    require(
        stop_price > Decimal::ZERO && stop_price < size.limit_price,
        "signal stop is not below the executable entry",
    )?;
    let planned_loss = planned_loss_fraction(&basis, stop_price, exit_costs, now)?;
    require(
        planned_loss > Decimal::ZERO && planned_loss <= policy.max_initial_loss_fraction,
        "planned loss exceeds the EUR intraday risk envelope",
    )?;
    let minimum_target = target_price(
        &basis,
        policy.desired_target_min_fraction,
        exit_costs,
        rules.price_tick,
        now,
    )?;
    let maximum_target = target_price(
        &basis,
        policy.desired_target_max_fraction,
        exit_costs,
        rules.price_tick,
        now,
    )?;
    let target_price = ceil_to(
        signal
            .expected_target
            .max(minimum_target)
            .min(maximum_target),
        rules.price_tick,
    )?;
    require(target_price > size.limit_price, "target is not above entry")?;
    let proceeds = liquidation_proceeds(&basis, target_price, exit_costs, now)?;
    let target_net_fraction = div(
        sub(proceeds.amount, basis.acquisition_cost.amount)?,
        basis.acquisition_cost.amount,
    )?;
    require(
        target_net_fraction >= policy.desired_target_min_fraction
            && target_net_fraction <= policy.desired_target_max_fraction,
        "target economics are outside the 1-3% net envelope",
    )?;
    let reward_to_risk = div(target_net_fraction, planned_loss)?;
    require(
        reward_to_risk >= policy.min_reward_to_risk,
        "target does not satisfy minimum reward to risk",
    )?;
    let plan = IntradayTradePlan {
        plan_id,
        policy_revision: policy.revision.clone(),
        account_id: account.account_id.to_string(),
        instrument_id: quote.instrument_id.clone(),
        currency: quote.currency.clone(),
        entry_price: size.limit_price,
        quantity: size.quantity,
        acquisition_cost: size.estimated_cost,
        stop_price,
        target_price,
        target_net_fraction,
        planned_loss_fraction: planned_loss,
        reward_to_risk,
        created_at: now,
        flatten_deadline,
        evidence_snapshot_ids,
        stop_revision: 0,
        cost_model: Some(IntradayCostModel {
            entry: entry_costs.clone(),
            exit: exit_costs.clone(),
        }),
    };
    plan.validate(now, policy)?;
    Ok(plan)
}

pub fn tighten_stop(
    plan: &mut IntradayTradePlan,
    proposed_stop: Decimal,
    now: DateTime<Utc>,
) -> Result<()> {
    require(
        proposed_stop >= plan.stop_price
            && proposed_stop < plan.entry_price
            && proposed_stop > Decimal::ZERO
            && now < plan.flatten_deadline,
        "protective stop may only tighten before the deadline",
    )?;
    let revision = plan
        .stop_revision
        .checked_add(1)
        .ok_or_else(|| Error("stop revision overflow".into()))?;
    let economics = plan
        .cost_model
        .as_ref()
        .map(|costs| {
            let basis = CostBasis {
                instrument_id: plan.instrument_id.clone(),
                quantity: plan.quantity,
                instrument_currency: plan.currency.clone(),
                acquisition_cost: plan.acquisition_cost.clone(),
            };
            let loss = planned_loss_fraction(&basis, proposed_stop, &costs.exit, now)?;
            Ok::<_, Error>((loss, div(plan.target_net_fraction, loss)?))
        })
        .transpose()?;
    plan.stop_price = proposed_stop;
    plan.stop_revision = revision;
    if let Some((loss, reward_to_risk)) = economics {
        plan.planned_loss_fraction = loss;
        plan.reward_to_risk = reward_to_risk;
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::str::FromStr;
    use trady_domain::SignalDirection;

    #[test]
    fn thirty_stock_entries_never_apply_an_exit_quota() {
        let mut policy = IntradayRiskPolicy {
            revision: THIRTY_ENTRY_POLICY_VERSION.into(),
            max_daily_entries: 30,
            max_daily_exits: None,
            ..IntradayRiskPolicy::default()
        };
        policy.validate().unwrap();
        assert_eq!(
            serde_json::to_value(&policy).unwrap()["max_daily_exits"],
            serde_json::Value::Null
        );
        policy.max_daily_entries = 31;
        assert!(policy.validate().is_err());
        policy.max_daily_entries = 30;
        policy.max_daily_exits = Some(255);
        assert!(policy.validate().is_err());
        IntradayRiskPolicy::default().validate().unwrap();
    }
    fn d(value: &str) -> Decimal {
        Decimal::from_str(value).expect("decimal")
    }
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }
    fn account() -> AccountSnapshot {
        AccountSnapshot {
            snapshot_id: "account_1".into(),
            account_id: trady_domain::AccountId::new("paper_eur").expect("account"),
            mode: trady_domain::Mode::Paper,
            spendable_cash: Money {
                amount: d("10000"),
                currency: Currency::new("EUR").expect("currency"),
            },
            positions: vec![],
            orders: vec![],
            observed_at: now(),
        }
    }
    fn quote() -> Quote {
        Quote {
            instrument_id: InstrumentId::new("AIR_PA_EQ").expect("id"),
            currency: Currency::new("EUR").expect("currency"),
            bid: d("100"),
            ask: d("100.01"),
            observed_at: now(),
            received_at: now(),
            tradable: true,
        }
    }
    fn costs() -> (EntryCosts, ExitCosts) {
        let eur = Currency::new("EUR").expect("currency");
        (
            EntryCosts {
                fixed: Money {
                    amount: d("1"),
                    currency: eur.clone(),
                },
                rate: d("0.001"),
                uncertainty: Money {
                    amount: d("0.1"),
                    currency: eur.clone(),
                },
                fx: None,
            },
            ExitCosts {
                fixed: Money {
                    amount: d("1"),
                    currency: eur.clone(),
                },
                rate: d("0.001"),
                uncertainty: Money {
                    amount: d("0.1"),
                    currency: eur,
                },
                fx: None,
            },
        )
    }
    fn signal() -> SignalSnapshot {
        SignalSnapshot {
            formula_id: "donchian_atr_intraday_v1".into(),
            parameter_revision: "donchian_atr_intraday_v1".into(),
            instrument_id: InstrumentId::new("AIR_PA_EQ").expect("id"),
            bar_snapshot_id: "bars_1".into(),
            direction: SignalDirection::Long,
            signal_at: now(),
            entry_zone: trady_domain::strategy::PriceZone {
                minimum: d("100"),
                maximum: d("100.50"),
            },
            invalidation: d("99"),
            stop_distance: d("1"),
            expected_target: d("103"),
            max_hold_deadline: now() + Duration::hours(4),
            reward_to_risk: d("2"),
            confidence_inputs: Default::default(),
        }
    }

    #[test]
    fn default_policy_is_bounded_to_one_to_three_percent_and_same_day() {
        let policy = IntradayRiskPolicy::default();
        assert!(policy.validate().is_ok());
        assert!(!policy.sentiment_may_widen_stop);
        assert!(policy.no_overnight);
        assert_eq!(policy.max_open_positions, 1);
        assert!(policy.rotation_enabled);
    }

    #[test]
    fn plan_includes_cost_aware_target_and_stop_never_widens() {
        let (entry, exit) = costs();
        let plan = calculate_intraday_plan(
            &account(),
            &quote(),
            &signal(),
            &entry,
            &exit,
            &InstrumentRules {
                price_tick: d("0.01"),
                quantity_step: Decimal::ONE,
                minimum_quantity: Decimal::ONE,
                currency_unit: d("0.01"),
            },
            now() + Duration::hours(4),
            &IntradayRiskPolicy::default(),
            "plan_1".into(),
            vec!["evidence_1".into()],
            now(),
        )
        .expect("plan");
        assert!(plan.target_net_fraction >= d("0.01"));
        assert!(plan.target_net_fraction <= d("0.03"));
        assert!(plan.planned_loss_fraction <= d("0.02"));
        let mut updated = plan.clone();
        assert!(tighten_stop(&mut updated, d("99.50"), now()).is_ok());
        assert!(
            updated
                .validate(now(), &IntradayRiskPolicy::default())
                .is_ok()
        );
        assert!(tighten_stop(&mut updated, d("99.25"), now()).is_err());
        updated.stop_revision = u64::MAX;
        let unchanged = updated.clone();
        assert!(tighten_stop(&mut updated, d("99.75"), now()).is_err());
        assert_eq!(updated, unchanged);
    }

    #[test]
    fn rotation_requires_flattened_current_position_and_stronger_verified_economics() {
        let assessment = RotationAssessment {
            session_id: "xpar_2026_11_03".into(),
            current_plan_id: "plan_1".into(),
            replacement_instrument_id: InstrumentId::new("ASML_AS_EQ").expect("id"),
            open_position_count: 1,
            current_exit_verified: true,
            replacement_quote_and_risk_checked: true,
            current_evidence_score: d("0.70"),
            replacement_evidence_score: d("0.90"),
            current_expected_net_fraction: d("0.010"),
            replacement_expected_net_fraction: d("0.013"),
            evaluated_at: now(),
        };
        assert!(approve_rotation(&assessment, &IntradayRiskPolicy::default(), now()).is_ok());
        let mut blocked = assessment;
        blocked.current_exit_verified = false;
        assert!(approve_rotation(&blocked, &IntradayRiskPolicy::default(), now()).is_err());
    }
}

//! Caller-serialized, deterministic holding decisions. Results do not submit,
//! cancel or authorize orders; the application persists state and coordinates protection.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trady_domain::{Error, InstrumentId, Quote, Result, require};

use crate::economics::{
    CostBasis, ExitCosts, NetReturn, net_executable_return, validate_executable_quote,
};
use crate::{Policy, add, ceil_to, mul, sub, target_ceiling};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionWindow {
    /// Actual exchange timestamps supplied by a verified calendar, including early closes.
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
}

impl SessionWindow {
    pub fn validate(&self) -> Result<()> {
        require(self.opens_at < self.closes_at, "invalid exchange session")
    }

    pub fn permits_entry(&self, now: DateTime<Utc>) -> bool {
        self.opens_at < self.closes_at
            && now >= self.opens_at
            && self.closes_at.signed_duration_since(now) > Duration::minutes(30)
    }

    /// Exclusive expiry: the five-minute proposal/order window never exceeds
    /// the actual session cutoff. The application still cancels/reconciles orders.
    pub fn entry_expires_at(&self, proposed_at: DateTime<Utc>) -> Result<DateTime<Utc>> {
        self.validate()?;
        require(
            self.permits_entry(proposed_at),
            "proposal outside entry session",
        )?;
        let proposal_expiry = proposed_at
            .checked_add_signed(Duration::minutes(5))
            .ok_or_else(|| Error("proposal expiry overflow".into()))?;
        let cutoff = self
            .closes_at
            .checked_sub_signed(Duration::minutes(30))
            .ok_or_else(|| Error("entry cutoff overflow".into()))?;
        Ok(proposal_expiry.min(cutoff))
    }

    pub fn validate_entry_time(
        &self,
        proposed_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<DateTime<Utc>> {
        let expires_at = self.entry_expires_at(proposed_at)?;
        require(
            proposed_at <= now && now < expires_at && self.permits_entry(now),
            "future or expired entry proposal/window",
        )?;
        Ok(expires_at)
    }

    fn liquidation_due(&self, now: DateTime<Utc>) -> bool {
        self.closes_at.signed_duration_since(now) <= Duration::minutes(15)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrailingSettings {
    pub activation_net_fraction: Decimal,
    pub distance_fraction: Decimal,
}

impl Default for TrailingSettings {
    fn default() -> Self {
        Self {
            activation_net_fraction: Decimal::new(5, 3),
            distance_fraction: Decimal::new(5, 3),
        }
    }
}

impl TrailingSettings {
    fn validate(&self) -> Result<()> {
        require(
            self.activation_net_fraction >= Decimal::ZERO && self.distance_fraction > Decimal::ZERO,
            "invalid trailing activation or distance",
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrailingState {
    pub peak_net_fraction: Option<Decimal>,
    pub floor_net_fraction: Option<Decimal>,
}

impl TrailingState {
    fn validate(&self) -> Result<()> {
        require(
            self.floor_net_fraction
                .is_none_or(|floor| self.peak_net_fraction.is_some_and(|peak| floor <= peak)),
            "trailing floor requires a known peak at least as high",
        )
    }

    fn observe(&mut self, value: Decimal, settings: &TrailingSettings) -> Result<()> {
        self.validate()?;
        let peak = self
            .peak_net_fraction
            .map_or(value, |previous| previous.max(value));
        self.peak_net_fraction = Some(peak);
        if peak >= settings.activation_net_fraction {
            let candidate = sub(peak, settings.distance_fraction)?;
            self.floor_net_fraction = Some(
                self.floor_net_fraction
                    .map_or(candidate, |previous| previous.max(candidate)),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingPlan {
    pub version: u64,
    /// Instrument-currency long stop, distinct from the FX-dependent net target.
    pub stop_price: Decimal,
    pub target_net_fraction: Decimal,
    pub deadline: DateTime<Utc>,
    pub trailing: TrailingSettings,
}

impl HoldingPlan {
    fn validate(&self) -> Result<()> {
        require(
            self.version > 0
                && self.stop_price > Decimal::ZERO
                && self.target_net_fraction > Decimal::ZERO
                && self.target_net_fraction <= Decimal::new(10, 2),
            "invalid holding plan version, stop or target",
        )?;
        self.trailing.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExitReason {
    HardStop,
    TrailingFloor,
    ProfitTarget,
    PlanDeadline,
    MaximumHoldingSessions,
    MarketClose,
    OwnerClose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExitLatch {
    pub reason: ExitReason,
    pub plan_version: u64,
    pub latched_at: DateTime<Utc>,
    pub quote_observed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingState {
    instrument_id: InstrumentId,
    price_tick: Decimal,
    original_deadline: DateTime<Utc>,
    plan: HoldingPlan,
    trailing: TrailingState,
    exit_latch: Option<ExitLatch>,
    latest_quote: Option<Quote>,
}

impl HoldingState {
    /// The caller separately validates entry readiness/risk and resolves the
    /// original deadline to at most the fifth actual holding session.
    pub fn new(
        instrument_id: InstrumentId,
        price_tick: Decimal,
        mut plan: HoldingPlan,
        policy: &Policy,
    ) -> Result<Self> {
        policy.validate()?;
        plan.stop_price = ceil_to(plan.stop_price, price_tick)?;
        plan.validate()?;
        require(
            plan.target_net_fraction <= policy.max_target_fraction,
            "holding target exceeds policy ceiling",
        )?;
        Ok(Self {
            instrument_id,
            price_tick,
            original_deadline: plan.deadline,
            plan,
            trailing: TrailingState::default(),
            exit_latch: None,
            latest_quote: None,
        })
    }

    pub fn plan(&self) -> &HoldingPlan {
        &self.plan
    }
    pub fn trailing(&self) -> &TrailingState {
        &self.trailing
    }
    pub fn exit_latch(&self) -> Option<&ExitLatch> {
        self.exit_latch.as_ref()
    }
    pub fn latest_quote(&self) -> Option<&Quote> {
        self.latest_quote.as_ref()
    }
    pub fn original_deadline(&self) -> DateTime<Utc> {
        self.original_deadline
    }

    fn validate(&self, basis: &CostBasis, policy: &Policy) -> Result<()> {
        policy.validate()?;
        self.plan.validate()?;
        self.trailing.validate()?;
        basis.validate()?;
        require(
            self.instrument_id == basis.instrument_id
                && self.plan.deadline <= self.original_deadline
                && ceil_to(self.plan.stop_price, self.price_tick)? == self.plan.stop_price,
            "mismatched holding, extended deadline or invalid stop tick",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingContext {
    pub now: DateTime<Utc>,
    pub session: Option<SessionWindow>,
    /// Entry session is 1. Count actual exchange sessions, never calendar days.
    pub holding_session_number: Option<u32>,
    pub overnight_protection_verified: bool,
    pub research_available: bool,
    /// An unknown request/quantity requires reconciliation; it is never a retry.
    pub order_outcome_known: bool,
    pub owner_close_authenticated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HoldingAction {
    Monitor,
    ExitRequired,
    Reconcile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HoldingDegradation {
    ResearchUnavailable,
    QuoteUnavailable,
    QuoteRejected,
    ValuationUnavailable,
    CalendarUnavailable,
    OrderOutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingEvaluation {
    /// Persist this snapshot under the caller's account/position serialization.
    pub state: HoldingState,
    pub action: HoldingAction,
    pub net_return: Option<NetReturn>,
    pub degradation: Vec<HoldingDegradation>,
}

fn decision(
    state: HoldingState,
    context: &HoldingContext,
    net_return: Option<NetReturn>,
    degradation: Vec<HoldingDegradation>,
) -> HoldingEvaluation {
    let action = if !context.order_outcome_known {
        HoldingAction::Reconcile
    } else if state.exit_latch.is_some() {
        HoldingAction::ExitRequired
    } else {
        HoldingAction::Monitor
    };
    HoldingEvaluation {
        state,
        action,
        net_return,
        degradation,
    }
}

/// Existing latches win. For simultaneous newly observed conditions the stable
/// priority is hard stop, trailing floor, target, deadline, maximum sessions,
/// unprotected close, owner close. This does not claim an unobserved price path.
pub fn evaluate_holding(
    state: &HoldingState,
    basis: &CostBasis,
    quote: Option<&Quote>,
    costs: &ExitCosts,
    context: &HoldingContext,
    policy: &Policy,
) -> Result<HoldingEvaluation> {
    let mut degradation = Vec::new();
    if !context.research_available {
        degradation.push(HoldingDegradation::ResearchUnavailable);
    }
    if !context.order_outcome_known {
        degradation.push(HoldingDegradation::OrderOutcomeUnknown);
    }
    // No subsequent proposal, outage or invalid current observation can undo it.
    if state.exit_latch.is_some() {
        return Ok(decision(state.clone(), context, None, degradation));
    }
    state.validate(basis, policy)?;
    let mut next = state.clone();
    if let Some(quote) = quote {
        let ordered = state
            .latest_quote
            .as_ref()
            .is_none_or(|last| quote.observed_at > last.observed_at || quote == last);
        if ordered {
            if validate_executable_quote(basis, quote, context.now).is_ok() {
                next.latest_quote = Some(quote.clone());
            } else {
                degradation.push(HoldingDegradation::QuoteRejected);
            }
        } else {
            degradation.push(HoldingDegradation::QuoteRejected);
        }
    }
    // A still-fresh latest accepted quote is also checked before plan activation.
    let accepted_quote = next
        .latest_quote
        .as_ref()
        .filter(|last| validate_executable_quote(basis, last, context.now).is_ok())
        .cloned();
    let accepted = accepted_quote
        .as_ref()
        .and_then(|quote| net_executable_return(basis, quote, costs, context.now).ok());
    if accepted_quote.is_none() {
        degradation.push(HoldingDegradation::QuoteUnavailable);
    } else if accepted.is_none() {
        degradation.push(HoldingDegradation::ValuationUnavailable);
    }
    let valid_session = context
        .session
        .as_ref()
        .filter(|session| session.validate().is_ok());
    if valid_session.is_none()
        || context
            .holding_session_number
            .is_none_or(|number| number == 0)
    {
        degradation.push(HoldingDegradation::CalendarUnavailable);
    }
    let mut reason = None;
    if let Some(value) = &accepted {
        next.trailing.observe(value.fraction, &next.plan.trailing)?;
    }
    if accepted_quote
        .as_ref()
        .is_some_and(|quote| quote.bid <= next.plan.stop_price)
    {
        reason = Some(ExitReason::HardStop);
    } else if let Some(value) = &accepted {
        if next
            .trailing
            .floor_net_fraction
            .is_some_and(|floor| value.fraction <= floor)
        {
            reason = Some(ExitReason::TrailingFloor);
        } else if value.proceeds.amount
            >= mul(
                basis.acquisition_cost.amount,
                add(Decimal::ONE, next.plan.target_net_fraction)?,
            )?
        {
            reason = Some(ExitReason::ProfitTarget);
        }
    }
    if reason.is_none() {
        if context.now >= next.plan.deadline {
            reason = Some(ExitReason::PlanDeadline);
        } else if context.holding_session_number.is_some_and(|number| {
            number > u32::from(policy.max_holding_sessions)
                || (number == u32::from(policy.max_holding_sessions)
                    && valid_session.is_some_and(|session| session.liquidation_due(context.now)))
        }) {
            reason = Some(ExitReason::MaximumHoldingSessions);
        } else if !context.overnight_protection_verified
            && valid_session.is_some_and(|session| session.liquidation_due(context.now))
        {
            reason = Some(ExitReason::MarketClose);
        } else if context.owner_close_authenticated {
            reason = Some(ExitReason::OwnerClose);
        }
    }
    if let Some(reason) = reason {
        next.exit_latch = Some(ExitLatch {
            reason,
            plan_version: next.plan.version,
            latched_at: context.now,
            quote_observed_at: accepted_quote.as_ref().map(|quote| quote.observed_at),
        });
    }
    Ok(decision(next, context, accepted, degradation))
}

/// Below 70 an existing target can remain or decrease; low confidence alone
/// grants no market sell and cannot increase the target.
pub fn holding_target_ceiling(
    score: u8,
    active_target: Decimal,
    exceptional_verified: bool,
    policy: &Policy,
) -> Result<Decimal> {
    require(
        active_target > Decimal::ZERO && active_target <= Decimal::new(10, 2),
        "invalid active holding target",
    )?;
    let band = target_ceiling(score, exceptional_verified, policy)?;
    Ok(if score < 70 {
        active_target.min(policy.max_target_fraction)
    } else {
        band
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanUpdate {
    pub expected_version: u64,
    pub proposed: HoldingPlan,
    /// Application-computed score, possibly reduced by an applicable overlay.
    pub confidence_score: u8,
    pub exceptional_verified: bool,
    /// Caller verifies material evidence, current research/ML and applicable
    /// sentiment permissions; this flag cannot override any numeric invariant.
    pub target_increase_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanActivation {
    pub activated: bool,
    /// Rejection still returns old-plan observations/latches for persistence.
    pub rejection: Option<String>,
    pub evaluation: HoldingEvaluation,
}

fn checked_update(
    state: &HoldingState,
    update: &PlanUpdate,
    context: &HoldingContext,
    policy: &Policy,
) -> Result<HoldingPlan> {
    require(
        context.order_outcome_known,
        "unknown order outcome blocks plan activation",
    )?;
    require(
        context.research_available,
        "research unavailable; retain active thresholds",
    )?;
    require(
        update.expected_version == state.plan.version,
        "stale expected plan version",
    )?;
    let next_version = state
        .plan
        .version
        .checked_add(1)
        .ok_or_else(|| Error("plan version overflow".into()))?;
    let mut proposed = update.proposed.clone();
    proposed.stop_price = ceil_to(proposed.stop_price, state.price_tick)?;
    proposed.validate()?;
    require(
        proposed.version == next_version,
        "plan version must advance exactly once",
    )?;
    require(
        proposed.stop_price >= state.plan.stop_price,
        "long stop cannot move downward",
    )?;
    require(
        proposed.deadline <= state.plan.deadline && proposed.deadline <= state.original_deadline,
        "holding deadline cannot extend",
    )?;
    require(
        proposed.trailing.activation_net_fraction <= state.plan.trailing.activation_net_fraction
            && proposed.trailing.distance_fraction <= state.plan.trailing.distance_fraction,
        "trailing activation/distance cannot loosen",
    )?;
    let ceiling = holding_target_ceiling(
        update.confidence_score,
        state.plan.target_net_fraction,
        update.exceptional_verified,
        policy,
    )?;
    require(
        proposed.target_net_fraction <= ceiling,
        "holding target exceeds permitted ceiling",
    )?;
    if proposed.target_net_fraction > state.plan.target_net_fraction {
        require(
            update.target_increase_allowed && update.confidence_score >= 70,
            "target increase lacks current checked evidence/permissions",
        )?;
    }
    Ok(proposed)
}

/// Always evaluates the old plan before validating the replacement. Persist the
/// returned state even on rejection. The caller supplies serialization and CAS;
/// this pure function does not make concurrent storage updates atomic.
pub fn activate_plan(
    state: &HoldingState,
    update: &PlanUpdate,
    basis: &CostBasis,
    quote: Option<&Quote>,
    costs: &ExitCosts,
    context: &HoldingContext,
    policy: &Policy,
) -> Result<PlanActivation> {
    let evaluation = evaluate_holding(state, basis, quote, costs, context, policy)?;
    let rejection = if evaluation.state.exit_latch.is_some() {
        Some("exit already latched".into())
    } else if evaluation.net_return.is_none() {
        Some("fresh executable quote/FX required for plan activation".into())
    } else if evaluation
        .degradation
        .contains(&HoldingDegradation::CalendarUnavailable)
    {
        Some("verified holding session/calendar required for plan activation".into())
    } else {
        match checked_update(&evaluation.state, update, context, policy) {
            Ok(proposed) => {
                let mut next = evaluation.state.clone();
                next.plan = proposed;
                match evaluate_holding(&next, basis, None, costs, context, policy) {
                    Ok(next_evaluation) => {
                        return Ok(PlanActivation {
                            activated: true,
                            rejection: None,
                            evaluation: next_evaluation,
                        });
                    }
                    Err(error) => Some(error.to_string()),
                }
            }
            Err(error) => Some(error.to_string()),
        }
    };
    Ok(PlanActivation {
        activated: false,
        rejection,
        evaluation,
    })
}

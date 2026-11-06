//! EUR research, order-readiness, coordinator, and same-day holding services.
//!
//! This module is the target authority. Legacy IEX30 replay use cases remain
//! available in their original modules, but no target operation can bypass the
//! checks below.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::analyst::{AnalystRequest, EurResearchAnalyst};
use trady_domain::coordinator::{
    Coordinator as DomainCoordinator, CoordinatorEvent, CoordinatorEventKind, CoordinatorSnapshot,
    CoordinatorState,
};
use trady_domain::selection::{
    FallbackAttempt, RankedCandidate, SelectionFailure, SelectionProposal, SelectionRun,
    rank_candidates,
};
use trady_domain::{
    AccountId, AccountSnapshot, Capabilities, Error, EurEligibilityPolicy, EurInstrumentBinding,
    EurUniverseSnapshot, EvidenceItem, InstrumentId, MarketState, QuoteQuality, Result,
    SessionWindow, VenueQuote, require,
};
use trady_policy::economics::ExitCosts;
use trady_policy::intraday::{
    DynamicExitDecision, ExitReason, IntradayRiskPolicy, IntradayTradePlan,
    calculate_intraday_plan, flatten_decision, tighten_stop,
};
use trady_policy::{EntryCosts, InstrumentRules};

/// Broker environment is deliberately separate from the four runtime modes.
/// `SHADOW` can read a demo account while remaining mutation-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EurBrokerEnvironment {
    Demo,
    Live,
}

impl EurBrokerEnvironment {
    pub fn is_demo(self) -> bool {
        matches!(self, Self::Demo)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EurBrokerConfig {
    pub environment: EurBrokerEnvironment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EurRuntimeConfig {
    pub config_version: u32,
    pub enabled: bool,
    pub execution_stage: trady_domain::ExecutionStage,
    /// Structural kill switch. This must remain false in every target build;
    /// a live release is intentionally unavailable.
    pub live_enabled: bool,
    pub broker: EurBrokerConfig,
    pub account_id: AccountId,
    pub base_currency: trady_domain::Currency,
    pub owner_id: String,
    pub policy: IntradayRiskPolicy,
    pub eligibility: EurEligibilityPolicy,
    pub approved_universe_snapshot_id: String,
    #[serde(default)]
    pub universe_config: Option<String>,
    pub opencode_model: String,
    pub trading212_only: bool,
    pub telegram_enabled: bool,
    #[serde(default)]
    pub session_deadlines: Vec<SessionDeadlineConfig>,
    #[serde(default)]
    pub yfinance_config: Option<String>,
    #[serde(default)]
    pub research_config: Option<String>,
    #[serde(default)]
    pub trading212_config: Option<String>,
    #[serde(default)]
    pub opencode_config: Option<String>,
    #[serde(default)]
    pub shadow: Option<crate::eur_shadow::ShadowSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_config: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionDeadlineConfig {
    pub exchange_mic: String,
    pub timezone: String,
    pub flatten_minutes_before_close: u16,
}

impl SessionDeadlineConfig {
    pub fn flatten_at(&self, closes_at: DateTime<Utc>) -> Result<DateTime<Utc>> {
        closes_at
            .checked_sub_signed(chrono::Duration::minutes(i64::from(
                self.flatten_minutes_before_close,
            )))
            .ok_or_else(|| Error("exchange flatten deadline underflow".into()))
    }
}

impl EurRuntimeConfig {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.config_version == 1
                && self.base_currency.as_str() == "EUR"
                && self.execution_stage.is_target_mode()
                && self.account_id_namespace_is_valid()
                && !self.owner_id.is_empty()
                && self.owner_id.len() <= 160
                && valid_id(&self.approved_universe_snapshot_id)
                && self.opencode_model == "deepseek-v4.1-flash"
                && self.trading212_only
                && self.broker.environment.is_demo()
                && !self.live_enabled,
            "invalid EUR runtime authority configuration",
        )?;
        require(
            !matches!(self.execution_stage, trady_domain::ExecutionStage::Live),
            "LIVE mode is structurally blocked for the EUR runtime",
        )?;
        require(
            !self.contains_legacy_authority_reference(),
            "mixed IEX30/Alpaca and EUR authorities are not permitted",
        )?;
        self.policy.validate()?;
        self.eligibility.validate()?;
        if let Some(settings) = &self.shadow {
            settings.validate()?;
        }
        require(
            !self.session_deadlines.is_empty()
                && self.session_deadlines.iter().all(|deadline| {
                    deadline.exchange_mic.len() == 4
                        && deadline
                            .exchange_mic
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                        && !deadline.timezone.is_empty()
                        && (1..=120).contains(&deadline.flatten_minutes_before_close)
                })
                && self
                    .session_deadlines
                    .iter()
                    .map(|deadline| deadline.exchange_mic.as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.session_deadlines.len(),
            "EUR runtime requires reviewed exchange-local flatten deadlines",
        )?;
        require(
            (1..=30).contains(&self.policy.max_daily_entries)
                && self.policy.max_open_positions == 1
                && self.policy.no_overnight
                && self.policy.max_initial_loss_fraction <= rust_decimal::Decimal::new(2, 2)
                && self.policy.min_reward_to_risk >= rust_decimal::Decimal::new(15, 1),
            "EUR runtime policy exceeds the loss, reward-to-risk, or30entry daily limits",
        )?;
        let _ = now;
        Ok(())
    }

    fn account_id_namespace_is_valid(&self) -> bool {
        match self.execution_stage {
            trady_domain::ExecutionStage::Replay => self.account_id.as_str().starts_with("replay_"),
            trady_domain::ExecutionStage::Shadow | trady_domain::ExecutionStage::Demo => {
                self.account_id.as_str().starts_with("paper_")
            }
            trady_domain::ExecutionStage::Live => false,
            // `is_target_mode` is checked above. Keep this branch explicit so
            // a future enum extension cannot silently inherit demo authority.
            _ => false,
        }
    }

    fn contains_legacy_authority_reference(&self) -> bool {
        let references = [
            self.universe_config.as_deref(),
            self.yfinance_config.as_deref(),
            self.research_config.as_deref(),
            self.trading212_config.as_deref(),
            self.opencode_config.as_deref(),
        ];
        references.into_iter().flatten().any(|value| {
            let lower = value.to_ascii_lowercase();
            lower.contains("iex30") || lower.contains("alpaca")
        })
    }

    pub fn permits_broker_mutation(&self) -> bool {
        // Configuration alone never proves release, owner or sequential-stop
        // acceptance. The explicit protocol driver verifies those separately;
        // mode selection and metadata flags cannot enable it.
        false
    }

    pub fn requires_real_inputs(&self) -> bool {
        self.execution_stage.uses_real_inputs()
    }

    /// Bind a reviewed universe snapshot to this runtime authority before any
    /// provider or broker call. This is the canonical mixed-authority check for
    /// callers that already loaded the snapshot from PostgreSQL or a replay.
    pub fn validate_universe(
        &self,
        universe: &EurUniverseSnapshot,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.validate(now)?;
        universe.validate(now)?;
        require(
            universe.snapshot_id == self.approved_universe_snapshot_id
                && universe
                    .members
                    .iter()
                    .all(|member| member.currency.as_str() == "EUR"),
            "EUR runtime and universe authority differ",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReadinessBlocker {
    RuntimeDisabled,
    ExecutionStageBlocked,
    AccountMismatch,
    AccountCurrencyMismatch,
    AccountStateStale,
    HoldingOrPendingOrder,
    UniverseExpired,
    InstrumentNotInUniverse,
    InstrumentCrosswalkMismatch,
    QuoteUnavailable,
    QuoteNotExecutable,
    QuoteStale,
    QuoteQuality,
    MarketClosed,
    SessionMismatch,
    TooLittleTimeToFlatten,
    CapabilityUnknown,
    CapabilityExpired,
    ProtectionUnsupported,
    SelectionNotFinal,
    SelectionInstrumentMismatch,
    RiskPlanInvalid,
    DailyEntryLimit,
    DeadlineInvalid,
    IdempotencyConflict,
    ReleaseEvidenceMissing,
    OwnerApprovalMissing,
    ProtectionProtocolUnavailable,
    SnapshotUnavailable,
    QuantityPrecision,
    PricePrecision,
    OrderValueOrPriceRange,
    FractionalUnsupported,
    BuyingPower,
    SpreadTooWide,
}

/// Shared configuration portion of the gate. Diagnostics without an order
/// snapshot remain blocked and say so explicitly.
pub fn configuration_readiness(
    config: &EurRuntimeConfig,
    now: DateTime<Utc>,
) -> Result<Vec<ReadinessBlocker>> {
    config.validate(now)?;
    let mut blockers = vec![
        ReadinessBlocker::ReleaseEvidenceMissing,
        ReadinessBlocker::OwnerApprovalMissing,
        ReadinessBlocker::ProtectionProtocolUnavailable,
    ];
    if !config.enabled {
        blockers.push(ReadinessBlocker::RuntimeDisabled);
    }
    if !config.execution_stage.permits_target_mutation() {
        blockers.push(ReadinessBlocker::ExecutionStageBlocked);
    }
    Ok(blockers)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderReadinessReceipt {
    pub gate_revision: String,
    pub checked_at: DateTime<Utc>,
    pub account_id: AccountId,
    pub instrument_id: InstrumentId,
    pub quote_id: String,
    pub session_id: String,
    pub policy_revision: String,
    pub evidence_snapshot_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "decision",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum OrderReadiness {
    Approved { receipt: OrderReadinessReceipt },
    Blocked { blockers: Vec<ReadinessBlocker> },
}

/// Immutable inputs shared by diagnostics and offline gate replay. The caller
/// supplies a clock; recorded timestamps never refresh provider observations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderGateSnapshot {
    pub account: AccountSnapshot,
    pub universe: EurUniverseSnapshot,
    pub binding: EurInstrumentBinding,
    pub quote: VenueQuote,
    pub session: SessionWindow,
    pub capabilities: Capabilities,
    pub selection: SelectionRun,
    pub plan: IntradayTradePlan,
    pub daily_entry_count: u8,
}

impl OrderGateSnapshot {
    pub fn evaluate(
        &self,
        config: &EurRuntimeConfig,
        now: DateTime<Utc>,
    ) -> Result<OrderReadiness> {
        evaluate_order_readiness(
            config,
            &self.account,
            &self.universe,
            &self.binding,
            &self.quote,
            &self.session,
            &self.capabilities,
            &self.selection,
            &self.plan,
            self.daily_entry_count,
            now,
        )
    }
}

/// The one application-owned target order gate. CLI diagnostics and execution
/// both call this function; no adapter or model gets a second authority path.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_order_readiness(
    config: &EurRuntimeConfig,
    account: &AccountSnapshot,
    universe: &EurUniverseSnapshot,
    binding: &EurInstrumentBinding,
    quote: &VenueQuote,
    session: &SessionWindow,
    capabilities: &Capabilities,
    selection: &SelectionRun,
    plan: &IntradayTradePlan,
    daily_entry_count: u8,
    now: DateTime<Utc>,
) -> Result<OrderReadiness> {
    config.validate(now)?;
    let mut blockers = configuration_readiness(config, now)?;
    if account.account_id != config.account_id {
        blockers.push(ReadinessBlocker::AccountMismatch);
    }
    if account.spendable_cash.currency != config.base_currency {
        blockers.push(ReadinessBlocker::AccountCurrencyMismatch);
    }
    if account.observed_at > now
        || now.signed_duration_since(account.observed_at).num_seconds() > 60
    {
        blockers.push(ReadinessBlocker::AccountStateStale);
    }
    if !account.positions.is_empty() || account.orders.iter().any(|order| !order.status.terminal())
    {
        blockers.push(ReadinessBlocker::HoldingOrPendingOrder);
    }
    if universe.validate(now).is_err() {
        blockers.push(ReadinessBlocker::UniverseExpired);
    }
    match universe.member(&binding.instrument_id) {
        None => blockers.push(ReadinessBlocker::InstrumentNotInUniverse),
        Some(member) if member != binding => {
            blockers.push(ReadinessBlocker::InstrumentCrosswalkMismatch)
        }
        _ => {}
    }
    if binding.validate(now).is_err() {
        blockers.push(ReadinessBlocker::InstrumentCrosswalkMismatch);
    }
    if plan.quantity < binding.minimum_quantity
        || plan.quantity.checked_rem(binding.quantity_step) != Some(rust_decimal::Decimal::ZERO)
    {
        blockers.push(ReadinessBlocker::QuantityPrecision);
    }
    if binding
        .entry_minimum_quantity(plan.entry_price)
        .map_or(true, |minimum| plan.quantity < minimum)
        || binding.value_rules.as_ref().is_some_and(|rules| {
            [plan.entry_price, plan.stop_price]
                .iter()
                .any(|price| *price < rules.price_floor || *price > rules.price_ceiling)
        })
    {
        blockers.push(ReadinessBlocker::OrderValueOrPriceRange);
    }
    if plan.quantity.fract() != rust_decimal::Decimal::ZERO
        && (!binding.fractional_orders || !capabilities.fractional_orders.usable(now))
    {
        blockers.push(ReadinessBlocker::FractionalUnsupported);
    }
    if [plan.entry_price, plan.stop_price, plan.target_price]
        .iter()
        .any(|price| price.checked_rem(binding.price_tick) != Some(rust_decimal::Decimal::ZERO))
    {
        blockers.push(ReadinessBlocker::PricePrecision);
    }
    if plan.acquisition_cost.currency != account.spendable_cash.currency
        || account
            .spendable_cash
            .amount
            .checked_mul(rust_decimal::Decimal::new(98, 2))
            .is_none_or(|budget| plan.acquisition_cost.amount > budget)
    {
        blockers.push(ReadinessBlocker::BuyingPower);
    }
    if quote
        .spread_bps()
        .is_ok_and(|spread| spread > binding.liquidity_threshold.maximum_spread_bps)
    {
        blockers.push(ReadinessBlocker::SpreadTooWide);
    }
    if !binding.maps_quote(quote) {
        blockers.push(ReadinessBlocker::InstrumentCrosswalkMismatch);
    }
    if quote
        .validate(now, config.eligibility.quote_max_age_seconds)
        .is_err()
    {
        blockers.push(ReadinessBlocker::QuoteUnavailable);
    }
    if now.signed_duration_since(quote.observed_at).num_seconds()
        > config.eligibility.quote_max_age_seconds
    {
        blockers.push(ReadinessBlocker::QuoteStale);
    }
    if quote.quality != QuoteQuality::Executable {
        blockers.push(ReadinessBlocker::QuoteQuality);
    }
    if quote.quality == QuoteQuality::Executable
        && (!quote.market_state.entry_allowed() || quote.bid.is_none() || quote.ask.is_none())
    {
        blockers.push(
            if matches!(
                quote.market_state,
                MarketState::Closed | MarketState::Halted
            ) {
                ReadinessBlocker::MarketClosed
            } else {
                ReadinessBlocker::QuoteNotExecutable
            },
        );
    }
    if session.validate(now).is_err() {
        blockers.push(ReadinessBlocker::SessionMismatch);
    } else {
        if session.exchange_mic != binding.exchange_mic
            || session.timezone != binding.session_timezone
            || selection.started_at < session.opens_at
            || selection.completed_at > now
            || plan.created_at < selection.completed_at
        {
            blockers.push(ReadinessBlocker::SessionMismatch);
        }
        if let Some(deadline) = config
            .session_deadlines
            .iter()
            .find(|deadline| deadline.exchange_mic == session.exchange_mic)
        {
            match deadline.flatten_at(session.closes_at) {
                Ok(expected) if expected == session.flatten_at => {}
                _ => blockers.push(ReadinessBlocker::DeadlineInvalid),
            }
        } else {
            blockers.push(ReadinessBlocker::SessionMismatch);
        }
        if !session.permits_entry(now, config.eligibility.minimum_seconds_to_flatten) {
            blockers.push(ReadinessBlocker::TooLittleTimeToFlatten);
        }
    }
    for capability in [
        &capabilities.cash_account,
        &capabilities.instrument,
        &capabilities.quotes,
    ] {
        if capability.support == trady_domain::Support::Unknown {
            blockers.push(ReadinessBlocker::CapabilityUnknown);
        } else if !capability.usable(now) {
            blockers.push(ReadinessBlocker::CapabilityExpired);
        }
    }
    if !capabilities.protective_orders.usable(now) {
        blockers.push(
            if capabilities.protective_orders.support == trady_domain::Support::Unsupported {
                ReadinessBlocker::ProtectionUnsupported
            } else {
                ReadinessBlocker::CapabilityUnknown
            },
        );
    }
    if selection.validate(now).is_err()
        || selection.account_id != config.account_id.as_str()
        || selection.universe_snapshot_id != config.approved_universe_snapshot_id
        || selection.selected_instrument_id.as_ref() != Some(&binding.instrument_id)
        || selection.failure.is_some()
    {
        blockers.push(ReadinessBlocker::SelectionNotFinal);
    }
    if plan.instrument_id != binding.instrument_id
        || plan.account_id != config.account_id.to_string()
    {
        blockers.push(ReadinessBlocker::SelectionInstrumentMismatch);
    }
    if plan.validate(now, &config.policy).is_err() {
        blockers.push(ReadinessBlocker::RiskPlanInvalid);
    }
    if daily_entry_count >= config.policy.max_daily_entries {
        blockers.push(ReadinessBlocker::DailyEntryLimit);
    }
    if plan.flatten_deadline > session.flatten_at || plan.flatten_deadline <= now {
        blockers.push(ReadinessBlocker::DeadlineInvalid);
    }
    blockers.sort_by_key(|blocker| format!("{blocker:?}"));
    blockers.dedup();
    if !blockers.is_empty() {
        return Ok(OrderReadiness::Blocked { blockers });
    }
    Ok(OrderReadiness::Approved {
        receipt: OrderReadinessReceipt {
            gate_revision: "eur_order_gate_v1".into(),
            checked_at: now,
            account_id: config.account_id.clone(),
            instrument_id: binding.instrument_id.clone(),
            quote_id: quote.quote_id.clone(),
            session_id: session.session_id.clone(),
            policy_revision: config.policy.revision.clone(),
            evidence_snapshot_ids: selection.evidence_snapshot_ids.clone(),
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionContext {
    pub run: SelectionRun,
    pub universe_snapshot_id: String,
    pub fallback_attempts: Vec<FallbackAttempt>,
}

/// Research-only selector. It ranks deterministic eligibility survivors and
/// returns an operational failure if every fallback source is unavailable.
pub fn select_candidate(
    mut run: SelectionRun,
    candidates: Vec<RankedCandidate>,
    fallback_attempts: Vec<FallbackAttempt>,
    proposal: Option<SelectionProposal>,
    now: DateTime<Utc>,
) -> Result<SelectionRun> {
    let ranked = rank_candidates(candidates)?;
    run.candidates = ranked;
    run.fallback_attempts = fallback_attempts;
    let selected = run
        .candidates
        .iter()
        .find(|candidate| candidate.eligible)
        .map(|candidate| candidate.instrument_id.clone());
    let Some(selected) = selected else {
        let failure = run
            .fallback_attempts
            .iter()
            .find(|attempt| {
                matches!(
                    attempt.status,
                    trady_domain::selection::SelectionAttemptStatus::Unavailable
                        | trady_domain::selection::SelectionAttemptStatus::RateLimited
                        | trady_domain::selection::SelectionAttemptStatus::Stale
                )
            })
            .map(|attempt| SelectionFailure::OperationalInputsUnavailable {
                stage: attempt.stage,
                code: attempt
                    .error_code
                    .clone()
                    .unwrap_or_else(|| "provider_unavailable".into()),
                message: "approved EUR candidate inputs are unavailable".into(),
            })
            .unwrap_or(SelectionFailure::NoEligibleInstrument {
                message: "all approved EUR instruments were deterministically excluded".into(),
            });
        return run.operational_failure(failure, now);
    };
    let proposal =
        proposal.ok_or_else(|| Error("successful selection requires a bounded proposal".into()))?;
    run.successful(selected, proposal, now)
}

/// Invoke the pinned research analyst after deterministic eligibility has
/// produced the allowed instrument IDs. The response is converted into the
/// durable selection envelope; it cannot enlarge the supplied universe.
pub fn analyst_select_candidate<A: EurResearchAnalyst>(
    analyst: &mut A,
    request: &AnalystRequest,
    fallback_attempts: Vec<FallbackAttempt>,
    now: DateTime<Utc>,
) -> Result<SelectionRun> {
    request.validate(now)?;
    require(
        !request.eligible_instrument_ids.is_empty(),
        "EUR analyst selection requires deterministic eligibility survivors",
    )?;
    let response = analyst.select(request)?;
    response.validate(request, Utc::now().max(now))?;
    response.into_selection_run(request.run.clone(), fallback_attempts)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HoldingAction {
    Hold,
    TightenStop,
    TakeProfit,
    EmergencyExit,
    Flatten,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingObservation {
    pub plan: IntradayTradePlan,
    pub quote: Option<VenueQuote>,
    pub fresh_evidence: Vec<EvidenceItem>,
    pub session: SessionWindow,
    pub broker_connected: bool,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingDecision {
    pub action: HoldingAction,
    pub reason: Option<ExitReason>,
    pub exit: Option<DynamicExitDecision>,
}

/// Apply a dynamic stop update only after validating it against the currently
/// persisted plan. A stale or wider proposal is rejected before it can reach a
/// broker cancel/replace path.
pub fn apply_dynamic_exit_update(
    plan: &mut IntradayTradePlan,
    decision: &DynamicExitDecision,
    now: DateTime<Utc>,
) -> Result<()> {
    decision.validate_against(plan, now)?;
    tighten_stop(plan, decision.proposed_stop, now)
}

/// Deterministic holding behavior remains available when the LLM/news/Telegram
/// providers are down. New evidence can tighten or exit, never widen a stop.
pub fn evaluate_holding(observation: &HoldingObservation) -> Result<HoldingDecision> {
    let plan = &observation.plan;
    require(
        observation.session.flatten_at >= plan.flatten_deadline,
        "holding deadline is outside the verified session",
    )?;
    if observation.now >= plan.flatten_deadline {
        return Ok(HoldingDecision {
            action: HoldingAction::Flatten,
            reason: Some(ExitReason::Deadline),
            exit: None,
        });
    }
    if !observation.broker_connected {
        return Ok(HoldingDecision {
            action: HoldingAction::EmergencyExit,
            reason: Some(ExitReason::BrokerDisconnect),
            exit: None,
        });
    }
    let Some(quote) = &observation.quote else {
        return Ok(HoldingDecision {
            action: HoldingAction::EmergencyExit,
            reason: Some(ExitReason::StaleData),
            exit: None,
        });
    };
    let quote_fresh =
        quote.validate(observation.now, 10).is_ok() && quote.quality == QuoteQuality::Executable;
    match flatten_decision(
        observation.now,
        plan.flatten_deadline,
        quote_fresh,
        quote.market_state.session_open(),
    )? {
        trady_policy::intraday::FlattenDecision::Flatten(reason) => {
            return Ok(HoldingDecision {
                action: HoldingAction::EmergencyExit,
                reason: Some(reason),
                exit: None,
            });
        }
        trady_policy::intraday::FlattenDecision::Continue => {}
    }
    let bid = quote
        .bid
        .ok_or_else(|| Error("holding quote bid unavailable".into()))?;
    if bid <= plan.stop_price {
        return Ok(HoldingDecision {
            action: HoldingAction::EmergencyExit,
            reason: Some(ExitReason::Stop),
            exit: None,
        });
    }
    if bid >= plan.target_price {
        return Ok(HoldingDecision {
            action: HoldingAction::TakeProfit,
            reason: Some(ExitReason::Target),
            exit: None,
        });
    }
    if observation
        .fresh_evidence
        .iter()
        .any(|item| item.trusted_for_selection() && !item.contradictions.is_empty())
    {
        return Ok(HoldingDecision {
            action: HoldingAction::EmergencyExit,
            reason: Some(ExitReason::AdverseEvidence),
            exit: None,
        });
    }
    // Lock in part of a favorable move using a deterministic, monotonic stop
    // update. This is advisory to the broker path; the caller must persist the
    // returned decision and apply it through the same protected plan journal.
    let protection_trigger =
        plan.entry_price * (rust_decimal::Decimal::ONE + rust_decimal::Decimal::new(5, 3));
    if bid >= protection_trigger && plan.stop_price < plan.entry_price {
        let proposed_stop = (plan.stop_price + plan.entry_price)
            .checked_div(rust_decimal::Decimal::from(2u32))
            .ok_or_else(|| Error("protective stop calculation overflow".into()))?;
        if proposed_stop > plan.stop_price && proposed_stop < plan.entry_price {
            return Ok(HoldingDecision {
                action: HoldingAction::TightenStop,
                reason: Some(ExitReason::ProfitProtection),
                exit: Some(DynamicExitDecision {
                    plan_id: plan.plan_id.clone(),
                    reason: ExitReason::ProfitProtection,
                    observed_bid: Some(bid),
                    proposed_stop,
                    proposed_target: plan.target_price,
                    decided_at: observation.now,
                    evidence_ids: observation
                        .fresh_evidence
                        .iter()
                        .map(|item| item.id.clone())
                        .take(64)
                        .collect(),
                }),
            });
        }
    }
    Ok(HoldingDecision {
        action: HoldingAction::Hold,
        reason: None,
        exit: None,
    })
}

#[derive(Debug, Clone)]
pub struct EurCoordinator {
    machine: DomainCoordinator,
    events: Vec<CoordinatorEvent>,
}

impl EurCoordinator {
    pub fn new(
        account_id: AccountId,
        session_id: String,
        policy_revision: String,
        flatten_deadline: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        Ok(Self {
            machine: DomainCoordinator::new(
                account_id.to_string(),
                session_id,
                policy_revision,
                flatten_deadline,
                now,
            )?,
            events: vec![],
        })
    }

    pub fn snapshot(&self) -> &CoordinatorSnapshot {
        self.machine.snapshot()
    }

    pub fn events(&self) -> &[CoordinatorEvent] {
        &self.events
    }

    #[allow(clippy::too_many_arguments)]
    pub fn transition(
        &mut self,
        event_id: String,
        kind: CoordinatorEventKind,
        to: CoordinatorState,
        instrument_id: Option<InstrumentId>,
        evidence_snapshot_ids: Vec<String>,
        reason: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let snapshot = self.machine.snapshot().clone();
        let event = CoordinatorEvent {
            event_id,
            kind,
            from: snapshot.state,
            to,
            account_id: snapshot.account_id,
            instrument_id,
            policy_revision: snapshot.policy_revision,
            evidence_snapshot_ids,
            occurred_at: now,
            reason,
        };
        self.machine.apply(event.clone())?;
        self.events.push(event);
        Ok(())
    }

    pub fn resume(
        account_id: AccountId,
        session_id: String,
        policy_revision: String,
        flatten_deadline: DateTime<Utc>,
        events: &[CoordinatorEvent],
        now: DateTime<Utc>,
    ) -> Result<Self> {
        let initial_at = events.first().map(|event| event.occurred_at).unwrap_or(now);
        let mut coordinator = Self::new(
            account_id,
            session_id,
            policy_revision,
            flatten_deadline,
            initial_at,
        )?;
        for event in events {
            coordinator.machine.apply(event.clone())?;
            coordinator.events.push(event.clone());
        }
        Ok(coordinator)
    }
}

/// Helper for the application layer to compute a bounded target plan after an
/// analyst has returned a signal. The analyst cannot supply quantity or risk.
#[allow(clippy::too_many_arguments)]
pub fn build_risk_plan(
    account: &AccountSnapshot,
    quote: &trady_domain::Quote,
    signal: &trady_domain::SignalSnapshot,
    entry_costs: &EntryCosts,
    exit_costs: &ExitCosts,
    rules: &InstrumentRules,
    flatten_deadline: DateTime<Utc>,
    policy: &IntradayRiskPolicy,
    plan_id: String,
    evidence_snapshot_ids: Vec<String>,
    now: DateTime<Utc>,
) -> Result<IntradayTradePlan> {
    calculate_intraday_plan(
        account,
        quote,
        signal,
        entry_costs,
        exit_costs,
        rules,
        flatten_deadline,
        policy,
        plan_id,
        evidence_snapshot_ids,
        now,
    )
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
    use rust_decimal::Decimal;
    use trady_domain::selection::{
        ModelFingerprint, SelectionAttemptStatus, SelectionFallbackStage,
    };

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }

    fn config() -> EurRuntimeConfig {
        EurRuntimeConfig {
            config_version: 1,
            enabled: false,
            execution_stage: trady_domain::ExecutionStage::Shadow,
            live_enabled: false,
            broker: EurBrokerConfig {
                environment: EurBrokerEnvironment::Demo,
            },
            account_id: AccountId::new("paper_eur").expect("account"),
            base_currency: trady_domain::Currency::new("EUR").expect("currency"),
            owner_id: "owner".into(),
            policy: IntradayRiskPolicy::default(),
            eligibility: EurEligibilityPolicy::default(),
            approved_universe_snapshot_id: "eur_universe_1".into(),
            universe_config: None,
            opencode_model: "deepseek-v4.1-flash".into(),
            trading212_only: true,
            telegram_enabled: true,
            session_deadlines: vec![SessionDeadlineConfig {
                exchange_mic: "XPAR".into(),
                timezone: "Europe/Paris".into(),
                flatten_minutes_before_close: 15,
            }],
            yfinance_config: Some("config/yfinance.euronext.example.json".into()),
            research_config: Some("config/research.eur.json".into()),
            trading212_config: Some("config/trading212.demo.example.json".into()),
            opencode_config: Some("config/opencode/llm.json".into()),
            shadow: None,
            operation_config: None,
            notes: vec![],
        }
    }

    fn selection() -> SelectionRun {
        SelectionRun {
            schema_version: 1,
            run_id: "selection_1".into(),
            account_id: "paper_eur".into(),
            session_id: "xpar_2026_11_03".into(),
            universe_snapshot_id: "eur_universe_1".into(),
            started_at: now() - Duration::minutes(1),
            completed_at: now(),
            model: ModelFingerprint {
                run_id: "selection_1".into(),
                session_id: "xpar_2026_11_03".into(),
                provider: "opencode-go".into(),
                model: "deepseek-v4.1-flash".into(),
                prompt_hash: "a".repeat(64),
                tool_manifest_hash: "b".repeat(64),
                model_revision: "c".repeat(64),
            },
            fallback_attempts: vec![FallbackAttempt {
                stage: SelectionFallbackStage::ReviewedUniverse,
                status: SelectionAttemptStatus::Complete,
                provider_reference: "universe_1".into(),
                candidate_count: 1,
                completed_at: now(),
                error_code: None,
            }],
            candidates: vec![RankedCandidate {
                instrument_id: InstrumentId::new("AIR_PA_EQ").expect("id"),
                currency: trady_domain::Currency::new("EUR").expect("currency"),
                eligibility_snapshot_id: "elig_1".into(),
                eligible: true,
                ranking_score: Decimal::new(90, 2),
                sentiment_score: Some(Decimal::ZERO),
                confidence: Some(Decimal::ONE),
                evidence_ids: vec!["evidence_1".into()],
                exclusion_reasons: vec![],
                catalyst: Some("earnings".into()),
                contradictions: vec![],
                strategy_snapshot_ids: vec!["signal_1".into()],
            }],
            selected_instrument_id: Some(InstrumentId::new("AIR_PA_EQ").expect("id")),
            proposal: Some(SelectionProposal {
                entry_minimum: Decimal::new(100, 0),
                entry_maximum: Decimal::new(101, 0),
                stop_price: Decimal::new(99, 0),
                target_price: Decimal::new(103, 0),
                maximum_hold_until: now() + Duration::hours(3),
                rationale: "bounded".into(),
            }),
            evidence_snapshot_ids: vec!["evidence_1".into()],
            failure: None,
        }
    }

    #[test]
    fn selector_never_returns_an_empty_success() {
        let mut run = selection();
        run.candidates[0].eligible = false;
        run.candidates[0].exclusion_reasons = vec!["stale".into()];
        run.selected_instrument_id = None;
        run.proposal = None;
        let mut ineligible = selection().candidates[0].clone();
        ineligible.eligible = false;
        ineligible.exclusion_reasons = vec!["stale".into()];
        let result = select_candidate(
            run,
            vec![ineligible],
            vec![FallbackAttempt {
                stage: SelectionFallbackStage::YfinanceNews,
                status: SelectionAttemptStatus::Unavailable,
                provider_reference: "yf".into(),
                candidate_count: 0,
                completed_at: now(),
                error_code: Some("provider_unavailable".into()),
            }],
            None,
            now(),
        )
        .expect("failure run");
        assert!(result.selected_instrument_id.is_none());
        assert!(result.failure.is_some());
    }

    #[test]
    fn runtime_configuration_is_demo_first_and_pinned_to_deepseek() {
        assert!(config().validate(now()).is_ok());
    }

    #[test]
    fn runtime_modes_are_explicit_and_live_is_structurally_blocked() {
        let mut replay = config();
        replay.execution_stage = trady_domain::ExecutionStage::Replay;
        replay.account_id = AccountId::new("replay_eur").expect("account");
        assert!(replay.validate(now()).is_ok());

        let mut demo = config();
        demo.enabled = true;
        demo.execution_stage = trady_domain::ExecutionStage::Demo;
        assert!(!demo.permits_broker_mutation());
        assert!(demo.validate(now()).is_ok());

        let mut live = config();
        live.execution_stage = trady_domain::ExecutionStage::Live;
        live.live_enabled = true;
        assert!(live.validate(now()).is_err());
    }

    #[test]
    fn mixed_iex_authority_is_rejected_before_provider_access() {
        let mut mixed = config();
        mixed.universe_config = Some("config/iex30.universe.json".into());
        assert!(mixed.validate(now()).is_err());
    }

    #[test]
    fn demo_toggle_cannot_clear_the_shared_gate_and_stale_or_rebound_quotes_are_explained() {
        let mut config = config();
        config.enabled = true;
        config.execution_stage = trady_domain::ExecutionStage::Demo;
        let mut universe: EurUniverseSnapshot = serde_json::from_str(include_str!(
            "../../../config/eur_equity_universe.example.json"
        ))
        .unwrap();
        universe.snapshot_id = config.approved_universe_snapshot_id.clone();
        let binding = universe.members[0].clone();
        let quote:VenueQuote=serde_json::from_value(serde_json::json!({
            "quote_id":"fixture_quote","instrument_id":binding.instrument_id,
            "broker_instrument_id":binding.broker_instrument_id,"isin":binding.isin,
            "exchange_mic":"XPAR","currency":"EUR","yahoo_symbol":"AIR.PA",
            "bid":"100","ask":"100.01","last":"100","bid_size":"100","ask_size":"100","volume":"100000",
            "observed_at":now()-Duration::seconds(61),"received_at":now(),
            "market_state":"REGULAR","source":"recorded_yfinance","quality":"EXECUTABLE"
        })).unwrap();
        let plan = IntradayTradePlan {
            plan_id: "plan_fixture".into(),
            cost_model: None,
            policy_revision: config.policy.revision.clone(),
            account_id: config.account_id.to_string(),
            instrument_id: binding.instrument_id.clone(),
            currency: trady_domain::Currency::new("EUR").unwrap(),
            entry_price: Decimal::new(10001, 2),
            quantity: Decimal::new(15, 1),
            acquisition_cost: trady_domain::Money {
                amount: Decimal::from(150),
                currency: trady_domain::Currency::new("EUR").unwrap(),
            },
            stop_price: Decimal::from(99),
            target_price: Decimal::from(103),
            target_net_fraction: Decimal::new(3, 2),
            planned_loss_fraction: Decimal::new(1, 2),
            reward_to_risk: Decimal::from(2),
            created_at: now(),
            flatten_deadline: now() + Duration::hours(3),
            evidence_snapshot_ids: vec!["fixture_evidence".into()],
            stop_revision: 0,
        };
        let snapshot = OrderGateSnapshot {
            account: AccountSnapshot {
                snapshot_id: "fixture_account".into(),
                account_id: config.account_id.clone(),
                mode: trady_domain::Mode::Paper,
                spendable_cash: trady_domain::Money {
                    amount: Decimal::from(10000),
                    currency: config.base_currency.clone(),
                },
                positions: vec![],
                orders: vec![],
                observed_at: now(),
            },
            universe,
            binding,
            quote,
            plan,
            capabilities: Capabilities::default(),
            selection: selection(),
            daily_entry_count: 0,
            session: SessionWindow {
                session_id: selection().session_id,
                exchange_mic: "XPAR".into(),
                timezone: "Europe/Paris".into(),
                opens_at: now() - Duration::hours(1),
                closes_at: now() + Duration::hours(4),
                flatten_at: now() + Duration::hours(4) - Duration::minutes(15),
                observed_at: now() - Duration::hours(2),
                source: "reviewed_session".into(),
            },
        };
        let diagnostic = snapshot.evaluate(&config, now()).unwrap();
        let executor = evaluate_order_readiness(
            &config,
            &snapshot.account,
            &snapshot.universe,
            &snapshot.binding,
            &snapshot.quote,
            &snapshot.session,
            &snapshot.capabilities,
            &snapshot.selection,
            &snapshot.plan,
            0,
            now(),
        )
        .unwrap();
        assert_eq!(diagnostic, executor);
        let OrderReadiness::Blocked { blockers } = diagnostic else {
            panic!("demo must stay blocked")
        };
        for reason in [
            ReadinessBlocker::QuoteStale,
            ReadinessBlocker::FractionalUnsupported,
            ReadinessBlocker::QuantityPrecision,
            ReadinessBlocker::ReleaseEvidenceMissing,
            ReadinessBlocker::OwnerApprovalMissing,
            ReadinessBlocker::ProtectionProtocolUnavailable,
        ] {
            assert!(blockers.contains(&reason), "{reason:?}: {blockers:?}");
        }
        let mut boundary = snapshot.clone();
        // Other release/source blockers are intentional in this fixture. A
        // consistent plan must not add RiskPlanInvalid, and each economic
        // mutation must add it independently of those unrelated blockers.
        let mut economics = snapshot.clone();
        economics.plan.entry_price = Decimal::from(100);
        economics.plan.quantity = Decimal::ONE;
        economics.plan.acquisition_cost.amount = Decimal::from(100);
        economics.plan.stop_price = Decimal::from(99);
        economics.plan.target_price = Decimal::from(102);
        economics.plan.target_net_fraction = Decimal::new(2, 2);
        economics.plan.planned_loss_fraction = Decimal::new(1, 2);
        economics.plan.reward_to_risk = Decimal::from(2);
        let zero = trady_domain::Money {
            amount: Decimal::ZERO,
            currency: config.base_currency.clone(),
        };
        economics.plan.cost_model = Some(trady_policy::intraday::IntradayCostModel {
            entry: EntryCosts {
                fixed: zero.clone(),
                rate: Decimal::ZERO,
                uncertainty: zero.clone(),
                fx: None,
            },
            exit: ExitCosts {
                fixed: zero.clone(),
                rate: Decimal::ZERO,
                uncertainty: zero,
                fx: None,
            },
        });
        assert!(
            matches!(economics.evaluate(&config,now()).unwrap(),OrderReadiness::Blocked{blockers}
            if !blockers.contains(&ReadinessBlocker::RiskPlanInvalid))
        );
        for field in [
            "entry_price",
            "stop_price",
            "target_price",
            "quantity",
            "acquisition_cost",
            "target_net_fraction",
            "planned_loss_fraction",
            "reward_to_risk",
            "policy_revision",
            "cost_currency",
            "cost_inputs",
        ] {
            let mut invalid = economics.clone();
            match field {
                "entry_price" => invalid.plan.entry_price += Decimal::ONE,
                "stop_price" => invalid.plan.stop_price = Decimal::from(50),
                "target_price" => invalid.plan.target_price += Decimal::ONE,
                "quantity" => invalid.plan.quantity += Decimal::ONE,
                "acquisition_cost" => invalid.plan.acquisition_cost.amount += Decimal::ONE,
                "target_net_fraction" => invalid.plan.target_net_fraction += Decimal::new(1, 3),
                "planned_loss_fraction" => invalid.plan.planned_loss_fraction += Decimal::new(1, 3),
                "reward_to_risk" => invalid.plan.reward_to_risk += Decimal::ONE,
                "policy_revision" => invalid.plan.policy_revision = "different_policy".into(),
                "cost_currency" => {
                    invalid.plan.acquisition_cost.currency =
                        trady_domain::Currency::new("USD").unwrap()
                }
                _ => invalid.plan.cost_model = None,
            }
            assert!(
                matches!(invalid.evaluate(&config,now()).unwrap(),OrderReadiness::Blocked{blockers}
                if blockers.contains(&ReadinessBlocker::RiskPlanInvalid)),
                "{field}"
            );
        }
        for (quantity, blocked) in [(98, false), (100, true)] {
            let mut allocation = economics.clone();
            allocation.plan.quantity = Decimal::from(quantity);
            allocation.plan.acquisition_cost.amount = Decimal::from(quantity * 100);
            assert!(allocation.plan.validate(now(), &config.policy).is_ok());
            let OrderReadiness::Blocked { blockers } = allocation.evaluate(&config, now()).unwrap()
            else {
                panic!("fixture still lacks release authority")
            };
            assert_eq!(blockers.contains(&ReadinessBlocker::BuyingPower), blocked);
        }
        boundary.session.session_id = "XPAR_physical_session".into();
        assert!(
            matches!(boundary.evaluate(&config,now()).unwrap(),OrderReadiness::Blocked{blockers}
            if !blockers.contains(&ReadinessBlocker::SessionMismatch))
        );
        for case in ["prior_session", "future_selection", "plan_before_selection"] {
            let mut invalid = boundary.clone();
            match case {
                "prior_session" => {
                    invalid.selection.started_at = invalid.session.opens_at - Duration::seconds(1)
                }
                "future_selection" => invalid.selection.completed_at = now() + Duration::seconds(1),
                _ => {
                    invalid.plan.created_at = invalid.selection.completed_at - Duration::seconds(1)
                }
            }
            assert!(
                matches!(invalid.evaluate(&config,now()).unwrap(),OrderReadiness::Blocked{blockers}
                if blockers.contains(&ReadinessBlocker::SessionMismatch)),
                "{case}"
            );
        }
        boundary.binding.value_rules = Some(trady_domain::eur::EurOrderValueRules {
            minimum_trade_value: Decimal::ONE,
            minimum_remaining_position_value: Decimal::ONE,
            price_floor: Decimal::from(100),
            price_ceiling: Decimal::from(110),
        });
        // The entry is within the reviewed band, but its native stop is not.
        assert!(
            matches!(boundary.evaluate(&config, now()).unwrap(), OrderReadiness::Blocked { blockers }
            if blockers.contains(&ReadinessBlocker::OrderValueOrPriceRange))
        );
        boundary.plan.stop_price = Decimal::from(100);
        boundary.binding.minimum_quantity = Decimal::new(1, 4);
        boundary.binding.quantity_step = Decimal::new(1, 4);
        boundary.plan.quantity = Decimal::new(50, 4);
        // Static precision passes; the EUR1 entry-value minimum still fails.
        assert!(
            matches!(boundary.evaluate(&config, now()).unwrap(), OrderReadiness::Blocked { blockers }
            if blockers.contains(&ReadinessBlocker::OrderValueOrPriceRange))
        );
        let mut rebound = snapshot;
        rebound.binding.isin = "NL0010273215".into();
        assert!(
            matches!(rebound.evaluate(&config,now()).unwrap(),OrderReadiness::Blocked{blockers}
            if blockers.contains(&ReadinessBlocker::InstrumentCrosswalkMismatch))
        );
    }

    #[test]
    fn checked_in_eur_example_and_schema_agree_and_never_authorize_mutations() {
        let text = include_str!("../../../config/eur-demo.example.json");
        let document: serde_json::Value = serde_json::from_str(text).unwrap();
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/eur-config.v1.json")).unwrap();
        assert!(
            jsonschema::validator_for(&schema)
                .unwrap()
                .is_valid(&document)
        );
        let config: EurRuntimeConfig = serde_json::from_str(text).unwrap();
        assert!(config.validate(now()).is_ok());
        assert!(!config.enabled && !config.live_enabled && !config.permits_broker_mutation());
        let mut mixed = document.clone();
        mixed["alpaca_market_data"] = serde_json::json!({"enabled":true});
        assert!(serde_json::from_value::<EurRuntimeConfig>(mixed).is_err());
        let mut non_eur = document;
        non_eur["base_currency"] = serde_json::json!("USD");
        assert!(
            serde_json::from_value::<EurRuntimeConfig>(non_eur)
                .unwrap()
                .validate(now())
                .is_err()
        );
    }

    #[test]
    fn coordinator_resume_replays_events_and_holding_deadline_is_hard() {
        let at = now();
        let mut coordinator = EurCoordinator::new(
            AccountId::new("paper_eur").expect("account"),
            "xpar_2026_11_03".into(),
            "eur_intraday_v1".into(),
            at + Duration::hours(8),
            at,
        )
        .expect("coordinator");
        coordinator
            .transition(
                "event_research".into(),
                CoordinatorEventKind::StartResearch,
                CoordinatorState::Researching,
                None,
                vec![],
                None,
                at,
            )
            .expect("transition");
        let resumed = EurCoordinator::resume(
            AccountId::new("paper_eur").expect("account"),
            "xpar_2026_11_03".into(),
            "eur_intraday_v1".into(),
            at + Duration::hours(8),
            coordinator.events(),
            at + Duration::hours(1),
        )
        .expect("resume");
        assert_eq!(resumed.snapshot().state, CoordinatorState::Researching);
        let decision =
            flatten_decision(at + Duration::hours(9), at + Duration::hours(8), true, true)
                .expect("deadline");
        assert_eq!(
            decision,
            trady_policy::intraday::FlattenDecision::Flatten(ExitReason::Deadline)
        );
    }

    #[test]
    fn dynamic_stop_update_is_monotonic_and_replayable() {
        let at = now();
        let eur = trady_domain::Currency::new("EUR").expect("currency");
        let mut plan = IntradayTradePlan {
            plan_id: "plan_1".into(),
            cost_model: None,
            policy_revision: "eur_intraday_v1".into(),
            account_id: "paper_eur".into(),
            instrument_id: InstrumentId::new("AIR_PA_EQ").expect("instrument"),
            currency: eur.clone(),
            entry_price: Decimal::new(100, 0),
            quantity: Decimal::ONE,
            acquisition_cost: trady_domain::Money {
                amount: Decimal::new(100, 0),
                currency: eur,
            },
            stop_price: Decimal::new(98, 0),
            target_price: Decimal::new(102, 0),
            target_net_fraction: Decimal::new(2, 2),
            planned_loss_fraction: Decimal::new(1, 2),
            reward_to_risk: Decimal::new(2, 0),
            created_at: at,
            flatten_deadline: at + Duration::hours(4),
            evidence_snapshot_ids: vec!["evidence_1".into()],
            stop_revision: 0,
        };
        let update = DynamicExitDecision {
            plan_id: "plan_1".into(),
            reason: ExitReason::ProfitProtection,
            observed_bid: Some(Decimal::new(101, 0)),
            proposed_stop: Decimal::new(99, 0),
            proposed_target: Decimal::new(102, 0),
            decided_at: at,
            evidence_ids: vec!["evidence_1".into()],
        };
        assert!(apply_dynamic_exit_update(&mut plan, &update, at).is_ok());
        assert_eq!(plan.stop_price, Decimal::new(99, 0));
        let mut wider = update;
        wider.proposed_stop = Decimal::new(97, 0);
        assert!(apply_dynamic_exit_update(&mut plan, &wider, at).is_err());
    }
}

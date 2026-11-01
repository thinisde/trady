//! Frozen expectation, actual-vintage and observed market-reaction decomposition.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EstimateDispersionDefinition {
    SampleStandardDeviation,
    PopulationStandardDeviation,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventUnit {
    pub label: String,
    pub currency: Option<String>,
    pub adjustment_definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventExpectation {
    pub estimate_id: String,
    pub metric: String,
    pub period: String,
    pub unit: EventUnit,
    pub value: Option<Decimal>,
    pub missing_reason: Option<String>,
    pub provider: String,
    pub vintage_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub analyst_count: Option<u32>,
    pub dispersion: Option<Decimal>,
    pub dispersion_definition: Option<EstimateDispersionDefinition>,
    pub revision_of: Option<String>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActualObservation {
    pub observation_id: String,
    pub metric: String,
    pub period: String,
    pub unit: EventUnit,
    pub value: Option<Decimal>,
    pub missing_reason: Option<String>,
    pub reported_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub revision_of: Option<String>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventPrice {
    pub value: Decimal,
    pub currency: String,
    pub adjustment_basis: String,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketReaction {
    pub window_id: String,
    /// Caller-supplied verified exchange session, including holidays and after-close events.
    pub session_open_at: DateTime<Utc>,
    pub session_close_at: DateTime<Utc>,
    pub session_calendar_reference: String,
    pub reference: EventPrice,
    pub after: EventPrice,
    pub benchmark_reference: Option<EventPrice>,
    pub benchmark_after: Option<EventPrice>,
    pub total_return_adjustments_verified: bool,
    pub corporate_actions_resolved: bool,
    pub fx_resolved: bool,
    pub trading_halt: bool,
    pub missing_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventPricingProxy {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub stock_total_return: Option<Decimal>,
    pub benchmark_total_return: Option<Decimal>,
    pub options_implied_move: Option<Decimal>,
    pub source_refs: Vec<String>,
    pub missing_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventConfounder {
    pub event_id: String,
    pub description: String,
    pub released_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurpriseNormalization {
    pub scale_floor: Decimal,
    /// Near-zero EPS should normally abstain; a floor requires metric-specific justification.
    pub allow_floor_for_metric: bool,
    pub justification: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventInput {
    pub event_id: String,
    pub issuer_id: Option<String>,
    pub event_type: String,
    pub metric: String,
    pub period: String,
    pub unit: EventUnit,
    pub release_at: DateTime<Utc>,
    pub clock_verified_at: DateTime<Utc>,
    pub clock_valid_until: DateTime<Utc>,
    pub frozen_estimate_id: Option<String>,
    pub expectation_frozen_at: Option<DateTime<Utc>>,
    pub drift_baseline_id: Option<String>,
    pub expectations: Vec<EventExpectation>,
    pub actuals: Vec<ActualObservation>,
    pub normalization: Option<SurpriseNormalization>,
    pub reaction: Option<MarketReaction>,
    pub pricing_proxy: Option<EventPricingProxy>,
    pub confounders: Vec<EventConfounder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventOutcome {
    SurpriseUpReactionUp,
    SurpriseUpReactionDown,
    SurpriseDownReactionUp,
    SurpriseDownReactionDown,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventAssessment {
    pub event_id: String,
    pub as_of: DateTime<Utc>,
    pub version: String,
    pub period: String,
    pub unit: EventUnit,
    pub estimate_id: Option<String>,
    pub actual_id: Option<String>,
    pub estimated_value: Option<Decimal>,
    pub reported_value: Option<Decimal>,
    pub raw_surprise: Option<Decimal>,
    pub normalized_surprise: Option<Decimal>,
    pub normalization_reason: Option<String>,
    pub standardized_surprise: Option<Decimal>,
    pub standardization_reason: Option<String>,
    pub expectation_drift: Option<Decimal>,
    pub drift_reason: Option<String>,
    /// Observed stock total return minus matched benchmark total return; not causal effect.
    pub abnormal_return: Option<Decimal>,
    pub pre_event_abnormal_return_proxy: Option<Decimal>,
    pub options_implied_move_proxy: Option<Decimal>,
    pub pricing_proxy_reasons: Vec<String>,
    pub outcome: EventOutcome,
    pub confounded: bool,
    pub confounders: Vec<EventConfounder>,
    pub actual_revisions: Vec<String>,
    pub reasons: Vec<String>,
    pub evidence_refs: Vec<String>,
}

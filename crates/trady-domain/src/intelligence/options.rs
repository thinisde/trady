//! Research-only option expectations. Premiums are per underlying share, not per contract.
//!
//! Pricing and volatility use finite `f64`: Black–Scholes requires transcendental
//! functions. Quoted money, strikes, ticks and multipliers remain exact decimals.
//! Rights and source lineage are supplied by the enclosing intelligence envelope.

use crate::Currency;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OptionsRight {
    Call,
    Put,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OptionsExercise {
    European,
    American,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsUnderlying {
    pub price: Decimal,
    pub adjustment_basis: String,
    pub observed_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

/// Continuous annualized carry, ACT/365 seconds. No implicit zero dividend/rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsPricingAssumptions {
    pub risk_free_rate_annualized: f64,
    pub dividend_yield_annualized: f64,
    pub observed_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsQuote {
    pub contract_id: String,
    pub right: OptionsRight,
    pub exercise: OptionsExercise,
    /// Actual contractual expiration instant; no weekday/holiday inference.
    pub expiry_at: DateTime<Utc>,
    pub strike: Decimal,
    pub contract_multiplier: Decimal,
    /// None means the corporate-action/adjusted-deliverable basis is unknown.
    pub adjustment_basis: Option<String>,
    pub price_tick: Decimal,
    pub bid: Decimal,
    pub ask: Decimal,
    pub bid_size: Option<Decimal>,
    pub ask_size: Option<Decimal>,
    pub volume_contracts: Option<u64>,
    pub open_interest_contracts: Option<u64>,
    pub observed_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

/// A comparable annualized volatility estimate; method and tenor must match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsVolatilityObservation {
    pub value_annualized: f64,
    pub horizon_seconds: u64,
    pub method: String,
    pub observed_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

/// Event variance is attempted only with an explicit non-event variance baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsEvent {
    pub event_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
    pub non_event_variance_rate_annualized: Option<f64>,
    pub baseline_source_ref: Option<String>,
    pub baseline_available_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsInput {
    pub instrument_id: String,
    pub currency: Currency,
    pub venue: String,
    pub underlying: Option<OptionsUnderlying>,
    pub assumptions: Option<OptionsPricingAssumptions>,
    pub contracts: Vec<OptionsQuote>,
    pub horizon_seconds: u64,
    pub max_quote_age_seconds: u64,
    pub max_alignment_seconds: u64,
    pub max_relative_spread_fraction: Decimal,
    pub atm_max_moneyness_fraction: Decimal,
    pub minimum_open_interest: Option<u64>,
    pub skew_absolute_delta: f64,
    pub skew_delta_tolerance: f64,
    pub history_start: DateTime<Utc>,
    pub iv_history: Vec<OptionsVolatilityObservation>,
    pub sector_comparables: Vec<OptionsVolatilityObservation>,
    pub realized_volatility: Option<OptionsVolatilityObservation>,
    pub event: Option<OptionsEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OptionsStatus {
    Available,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsIssue {
    pub field: String,
    pub reason: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsIvInterval {
    pub bid: f64,
    pub midpoint: f64,
    pub ask: f64,
    pub solver_iterations: u32,
    pub max_price_residual: f64,
}

/// Spot × annualized IV × sqrt(ACT/365 horizon); a model scale, not a price target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsMoveScale {
    pub lower_iv_scale: f64,
    pub midpoint_iv_scale: f64,
    pub upper_iv_scale: f64,
    pub horizon_years: f64,
    pub distribution_assumption: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsSkew {
    pub put_minus_call_iv: f64,
    pub put_absolute_delta: f64,
    pub call_absolute_delta: f64,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsExpiryAssessment {
    pub expiry_at: DateTime<Utc>,
    pub atm_iv: Option<OptionsIvInterval>,
    pub one_standard_deviation_scale: Option<OptionsMoveScale>,
    pub skew: Option<OptionsSkew>,
    pub used_contract_ids: Vec<String>,
    pub source_refs: Vec<String>,
    pub oldest_quote_at: Option<DateTime<Utc>>,
    pub eligible_contracts: usize,
    pub issues: Vec<OptionsIssue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsTermStructure {
    pub near_expiry: DateTime<Utc>,
    pub far_expiry: DateTime<Utc>,
    pub far_minus_near_iv: f64,
    pub forward_variance_rate_annualized: Option<f64>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsAssessment {
    pub at: DateTime<Utc>,
    pub status: OptionsStatus,
    pub model: String,
    pub expiries: Vec<OptionsExpiryAssessment>,
    pub term_structure: Vec<OptionsTermStructure>,
    pub reference_expiry: Option<DateTime<Utc>>,
    pub iv_history_rank: Option<f64>,
    pub sector_iv_percentile: Option<f64>,
    pub iv_to_realized_ratio: Option<f64>,
    pub event_variance: Option<f64>,
    pub event_variance_interval: Option<[f64; 2]>,
    pub comparison_horizon_seconds: Option<u64>,
    pub history_start: DateTime<Utc>,
    pub comparison_method: String,
    pub realized_volatility_method: Option<String>,
    pub comparison_source_refs: Vec<String>,
    pub issues: Vec<OptionsIssue>,
}

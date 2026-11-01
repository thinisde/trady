//! Sparse, publication-aware cross-market context. Secondary trading is never fund flow.
//!
//! Exact returns, flows and changes use Decimal. Only sample statistics and their
//! uncertainty use finite f64, because square roots and Fisher transforms are required.

use crate::Currency;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrossAssetKind {
    PeriodTotalReturn,
    YieldFraction,
    YieldSpreadFraction,
    FxSpot,
    LiquidityCostFraction,
    VolatilityAnnualized,
    PrimaryNetFlow,
    SecondaryTradingVolume,
    AssetsUnderManagement,
    SharesOutstanding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrossAssetUnit {
    Fraction,
    CurrencyAmount,
    CurrencyPairRate,
    Shares,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetObservation {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub value: Option<Decimal>,
    pub missing_reason: Option<String>,
    pub vintage: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetSeries {
    pub id: String,
    /// Stable fund, index or benchmark identity; flow and AUM must name the same fund.
    pub entity_id: String,
    pub kind: CrossAssetKind,
    pub unit: CrossAssetUnit,
    pub currency: Option<Currency>,
    pub currency_pair: Option<[Currency; 2]>,
    /// IANA/local exchange calendar identifier, including the session timezone.
    pub calendar_id: String,
    pub venue: String,
    pub adjustment_basis: String,
    pub max_age_seconds: u64,
    pub required: bool,
    pub observations: Vec<CrossAssetObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetPair {
    pub asset_series_id: String,
    pub benchmark_series_id: String,
    pub lookback_start: DateTime<Utc>,
    pub minimum_samples: usize,
    pub short_window_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetFlow {
    pub flow_series_id: String,
    pub assets_series_id: String,
    pub performance_series_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrossAssetStressMethod {
    Difference,
    RelativeChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrossAssetStressDirection {
    Rising,
    Falling,
    Either,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetStress {
    pub series_id: String,
    pub horizon_seconds: u64,
    pub method: CrossAssetStressMethod,
    pub direction: CrossAssetStressDirection,
    pub threshold: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetBreadthMember {
    pub instrument_id: String,
    pub market_cap_weight: Decimal,
    pub total_return_fraction: Decimal,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetBreadth {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub weights_effective_at: DateTime<Utc>,
    pub weights_available_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub weights_version: String,
    pub source_ref: String,
    pub members: Vec<CrossAssetBreadthMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetInput {
    pub region: String,
    pub effective_horizon_seconds: u64,
    pub series: Vec<CrossAssetSeries>,
    pub pairs: Vec<CrossAssetPair>,
    pub flows: Vec<CrossAssetFlow>,
    pub stress: Vec<CrossAssetStress>,
    pub breadth: Option<CrossAssetBreadth>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrossAssetStatus {
    Available,
    Partial,
    Abstain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetAlignedValue {
    pub series_id: String,
    pub entity_id: String,
    pub kind: CrossAssetKind,
    pub unit: CrossAssetUnit,
    pub currency: Option<Currency>,
    pub currency_pair: Option<[Currency; 2]>,
    pub calendar_id: String,
    pub venue: String,
    pub adjustment_basis: String,
    pub value: Option<Decimal>,
    pub period_end: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub available_at: Option<DateTime<Utc>>,
    pub age_seconds: Option<i64>,
    pub vintage: Option<String>,
    pub source_ref: Option<String>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetInterval {
    pub lower: f64,
    pub upper: f64,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetPairAssessment {
    pub asset_series_id: String,
    pub benchmark_series_id: String,
    pub samples: usize,
    pub beta: Option<f64>,
    pub beta_standard_error: Option<f64>,
    pub correlation: Option<f64>,
    pub correlation_interval_95: Option<CrossAssetInterval>,
    pub short_window_correlation: Option<f64>,
    pub correlation_change: Option<f64>,
    pub relative_compounded_return: Option<Decimal>,
    pub source_refs: Vec<String>,
    pub unavailable_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetFlowAssessment {
    pub flow_series_id: String,
    pub net_primary_flow: Option<Decimal>,
    pub flow_to_opening_assets_fraction: Option<Decimal>,
    pub performance_driven_assets_change: Option<Decimal>,
    pub unexplained_assets_change: Option<Decimal>,
    pub source_refs: Vec<String>,
    pub unavailable_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetStressAssessment {
    pub series_id: String,
    pub change: Option<Decimal>,
    pub threshold_exceeded: Option<bool>,
    pub source_refs: Vec<String>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetBreadthAssessment {
    pub positive_constituent_fraction: Decimal,
    pub positive_market_cap_weight_fraction: Decimal,
    pub market_cap_weighted_return_fraction: Decimal,
    pub constituents: usize,
    pub weights_version: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossAssetAssessment {
    pub at: DateTime<Utc>,
    pub region: String,
    pub status: CrossAssetStatus,
    pub effective_horizon_seconds: u64,
    pub aligned: Vec<CrossAssetAlignedValue>,
    pub sparse_missing_series: Vec<String>,
    pub pairs: Vec<CrossAssetPairAssessment>,
    pub flows: Vec<CrossAssetFlowAssessment>,
    pub stress: Vec<CrossAssetStressAssessment>,
    pub breadth: Option<CrossAssetBreadthAssessment>,
    pub issues: Vec<String>,
}

//! Publication-aware reported short positions and crowding context, never a squeeze forecast.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ShortPositionMetric {
    ReportedOutstandingPositions,
    DailyShortSaleVolume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortPositionReport {
    pub settlement_date: NaiveDate,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub shares_short: Decimal,
    pub metric: ShortPositionMetric,
    pub adjustment_basis: Option<String>,
    pub source_ref: String,
    pub release_vintage: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortFloatObservation {
    pub shares: Decimal,
    pub issued_shares: Option<Decimal>,
    pub effective_date: NaiveDate,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub adjustment_basis: Option<String>,
    pub methodology: String,
    pub exclusions: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ShortVolumeScope {
    Consolidated,
    SingleVenue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortAverageVolume {
    pub daily_shares: Decimal,
    pub window_start: NaiveDate,
    pub window_end: NaiveDate,
    pub completed_sessions: u32,
    pub scope: ShortVolumeScope,
    pub venue: Option<String>,
    pub adjustment_basis: Option<String>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortBorrowObservation {
    pub fee_annualized_fraction: Option<Decimal>,
    pub utilization_fraction: Option<Decimal>,
    pub shares_on_loan: Option<Decimal>,
    pub hard_to_borrow: Option<bool>,
    pub observed_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub methodology: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortMarketContext {
    pub session_open: Decimal,
    pub previous_close: Decimal,
    pub recent_volume_shares: Decimal,
    pub comparable_baseline_volume_shares: Decimal,
    pub realized_volatility_annualized: Option<Decimal>,
    pub volatility_method: Option<String>,
    pub adjustment_basis: Option<String>,
    pub observed_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

/// Research thresholds supplied by a frozen experiment, not execution limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortCrowdingThresholds {
    pub short_to_float_fraction: Decimal,
    pub days_to_cover: Decimal,
    pub absolute_gap_fraction: Decimal,
    pub relative_volume: Decimal,
    pub realized_volatility_annualized: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortInterestInput {
    pub issuer_id: String,
    pub adjustment_basis: Option<String>,
    pub reports: Vec<ShortPositionReport>,
    pub float: Option<ShortFloatObservation>,
    pub average_volume: Option<ShortAverageVolume>,
    pub borrow: Option<ShortBorrowObservation>,
    pub borrow_history: Vec<ShortBorrowObservation>,
    pub market_context: Option<ShortMarketContext>,
    pub max_report_age_seconds: u64,
    pub max_context_age_seconds: u64,
    pub history_start: DateTime<Utc>,
    pub thresholds: ShortCrowdingThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ShortInterestStatus {
    Available,
    Partial,
    Abstain,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ShortCrowdingFlag {
    HighReportedShortToFloat,
    ElevatedVolumeBasedDaysToCover,
    LargeOpeningGap,
    ElevatedRelativeVolume,
    HighRealizedVolatility,
    HardToBorrow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortInterestIssue {
    pub field: String,
    pub reason: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortInterestAssessment {
    pub at: DateTime<Utc>,
    pub status: ShortInterestStatus,
    pub settlement_date: Option<NaiveDate>,
    pub report_published_at: Option<DateTime<Utc>>,
    pub report_available_at: Option<DateTime<Utc>>,
    pub release_vintage: Option<String>,
    pub report_age_seconds: Option<i64>,
    /// Date-only settlement is compared by calendar date, not a fabricated instant.
    pub publication_lag_calendar_days: Option<i64>,
    pub reported_shares_short: Option<Decimal>,
    pub verified_float_shares: Option<Decimal>,
    pub float_methodology: Option<String>,
    pub float_effective_date: Option<NaiveDate>,
    pub short_to_float_fraction: Option<Decimal>,
    /// A volume heuristic, never a time deadline to cover.
    pub volume_based_days_to_cover: Option<Decimal>,
    pub average_daily_volume_shares: Option<Decimal>,
    pub average_volume_window_start: Option<NaiveDate>,
    pub average_volume_window_end: Option<NaiveDate>,
    pub average_volume_completed_sessions: Option<u32>,
    pub average_volume_scope: Option<ShortVolumeScope>,
    pub average_volume_venue: Option<String>,
    pub reported_position_change_shares: Option<Decimal>,
    pub reported_position_change_fraction: Option<Decimal>,
    pub borrow_fee_annualized_fraction: Option<Decimal>,
    pub borrow_fee_historical_percentile: Option<Decimal>,
    /// Max minus min within the same provider methodology; not cross-broker dispersion.
    pub borrow_fee_historical_range: Option<Decimal>,
    pub borrow_utilization_fraction: Option<Decimal>,
    pub borrow_shares_on_loan: Option<Decimal>,
    pub borrow_hard_to_borrow: Option<bool>,
    pub borrow_methodology: Option<String>,
    pub opening_gap_fraction: Option<Decimal>,
    pub relative_volume: Option<Decimal>,
    pub realized_volatility_annualized: Option<Decimal>,
    pub realized_volatility_method: Option<String>,
    pub flags: Vec<ShortCrowdingFlag>,
    pub source_refs: Vec<String>,
    pub issues: Vec<ShortInterestIssue>,
}

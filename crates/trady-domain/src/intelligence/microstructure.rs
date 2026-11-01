//! Venue-scoped displayed-depth research; these types confer no fill or order authority.
//!
//! A SequenceBookBuilder must supply a fully materialized, sorted book and explicit
//! state. Never send deltas as levels. Independent snapshots cannot establish queue
//! position or identify cancellations. All sizes are shares; all prices are currency/share.

use crate::Currency;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MicrostructureBookState {
    ValidCompleteDepth,
    ValidPartialDepth,
    SequenceGap,
    Invalidated,
    Auction,
    Halted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MicrostructureFeedKind {
    IndependentSnapshot,
    SequencedBook,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MicrostructureLevel {
    pub price: Decimal,
    pub size_shares: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MicrostructureBook {
    pub instrument_id: String,
    pub venue: String,
    /// Monotonic snapshot/version sequence, including for independent snapshots.
    pub sequence: u64,
    /// Changes on a feed reset; history from a different epoch cannot be compared.
    pub epoch: String,
    pub book_version: String,
    pub feed_kind: MicrostructureFeedKind,
    pub state: MicrostructureBookState,
    pub exchange_time: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
    pub bids: Vec<MicrostructureLevel>,
    pub asks: Vec<MicrostructureLevel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MicrostructureRouting {
    Unknown,
    VerifiedSelectedVenue,
    OtherVenue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MicrostructureAuction {
    pub indicative_price: Decimal,
    /// Signed shares: positive buy imbalance, negative sell imbalance.
    pub imbalance_shares: Decimal,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MicrostructureMarkoutSide {
    Buy,
    Sell,
}

/// Registered post-signal measurement, available only after its full window elapsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MicrostructureMarkout {
    pub side: MicrostructureMarkoutSide,
    pub signal_at: DateTime<Utc>,
    pub signal_price: Decimal,
    pub signal_source_ref: String,
    pub mark_at: DateTime<Utc>,
    pub mark_mid: Decimal,
    pub mark_source_ref: String,
    pub available_at: DateTime<Utc>,
    pub registered_window_milliseconds: u64,
    /// False means a quote-based markout, not a measured execution outcome.
    pub signal_price_is_confirmed_fill: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MicrostructureInput {
    pub instrument_id: String,
    pub venue: String,
    pub currency: Currency,
    pub tick_size: Decimal,
    pub size_increment: Decimal,
    pub max_age_milliseconds: u64,
    pub max_clock_skew_milliseconds: u64,
    pub depth_limit: usize,
    pub capacity_band_ticks: u32,
    pub routing: MicrostructureRouting,
    pub book: Option<MicrostructureBook>,
    /// Strictly ordered snapshots preceding `book`, from the same feed/epoch.
    pub history: Vec<MicrostructureBook>,
    pub auction: Option<MicrostructureAuction>,
    pub markout: Option<MicrostructureMarkout>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MicrostructureStatus {
    Available,
    Partial,
    Unavailable,
    Invalidated,
    Auction,
    Halted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MicrostructureMetrics {
    pub mid_price: Decimal,
    pub spread: Decimal,
    pub spread_ticks: Decimal,
    pub spread_basis_points: Decimal,
    pub displayed_imbalance_fraction: Decimal,
    pub microprice: Decimal,
    pub bid_depth_shares: Decimal,
    pub ask_depth_shares: Decimal,
    pub bid_notional: Decimal,
    pub ask_notional: Decimal,
    /// Capacity within N ticks of each side's own best price, for selected levels only.
    pub bid_capacity_notional: Decimal,
    pub ask_capacity_notional: Decimal,
    pub bid_capacity_truncated: bool,
    pub ask_capacity_truncated: bool,
    /// Additional displayed shares per tick between best and last selected price.
    pub bid_slope_shares_per_tick: Option<Decimal>,
    pub ask_slope_shares_per_tick: Option<Decimal>,
    pub bid_levels: usize,
    pub ask_levels: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MicrostructureDynamics {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub snapshots: usize,
    pub spread_change: Decimal,
    pub imbalance_change: Decimal,
    /// Minimum total displayed depth / initial total depth, capped at one.
    /// This measures aggregate persistence, never survival of individual orders.
    pub displayed_depth_persistence_fraction: Decimal,
    pub top_quote_updates_per_second: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MicrostructureAssessment {
    pub at: DateTime<Utc>,
    pub instrument_id: String,
    pub venue: String,
    pub currency: Currency,
    pub routing: MicrostructureRouting,
    pub derived_cost_model: String,
    pub depth_limit: usize,
    pub capacity_band_ticks: u32,
    pub status: MicrostructureStatus,
    pub book_state: Option<MicrostructureBookState>,
    pub sequence: Option<u64>,
    pub book_version: Option<String>,
    pub exchange_time: Option<DateTime<Utc>>,
    pub received_at: Option<DateTime<Utc>>,
    pub receive_minus_exchange_milliseconds: Option<i64>,
    pub metrics: Option<MicrostructureMetrics>,
    pub dynamics: Option<MicrostructureDynamics>,
    pub auction: Option<MicrostructureAuction>,
    /// Positive means adverse movement for the supplied side. No fill probability.
    pub post_signal_adverse_selection_basis_points: Option<Decimal>,
    pub markout_is_confirmed_fill: Option<bool>,
    pub source_refs: Vec<String>,
    pub issues: Vec<String>,
}

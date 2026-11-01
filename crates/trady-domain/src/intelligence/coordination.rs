//! Behavioral reliability diagnostics, never allegations about people or trading signals.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PromotionKind {
    Original,
    Repost,
    Quotation,
    VerifiedDisclosure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionEvent {
    pub event_id: String,
    pub source_id: String,
    /// Opaque, source-scoped identifier; no account names or raw message text.
    pub author_hash: String,
    pub kind: PromotionKind,
    pub parent_event_id: Option<String>,
    /// Must resolve to the actual terminal ancestor, not a claimed new origin.
    pub origin_event_id: String,
    pub lexical_cluster: String,
    pub outbound_url_hash: Option<String>,
    /// Trusted verifier's link to a VerifiedDisclosure event in this input.
    pub verified_disclosure_id: Option<String>,
    pub published_at: DateTime<Utc>,
    pub first_available_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub reference: String,
    pub invalidated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostingBaseline {
    pub source_id: String,
    pub source_window_key: String,
    pub event_category: String,
    pub bucket_seconds: u32,
    /// Expected number of collected posts, for comparable collection coverage.
    pub expected_posts: Decimal,
    pub collection_coverage: Decimal,
    pub historical_windows: u32,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinationCoverage {
    pub source_id: String,
    pub covered_records: u64,
    pub expected_records: Option<u64>,
    pub unavailable_reason: Option<String>,
    pub censored: bool,
    pub baseline: Option<PostingBaseline>,
    pub baseline_unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinationThresholds {
    pub version: String,
    pub minimum_coverage: Decimal,
    pub maximum_baseline_coverage_difference: Decimal,
    pub minimum_baseline_windows: u32,
    pub minimum_behavioral_posts: u64,
    pub burst_ratio: Decimal,
    pub concentration_fraction: Decimal,
    pub synchronization_seconds: u32,
    pub elevated_weight: Decimal,
    pub review_weight: Decimal,
    pub calibration_reference: Option<String>,
    pub calibration_unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinationMarketDeviation {
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub turnover_ratio: Decimal,
    pub market_adjusted_return: Decimal,
    pub sector_adjusted_return: Decimal,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinationInput {
    pub issuer_id: String,
    pub claim_cluster: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub source_window_key: String,
    pub event_category: String,
    pub events: Vec<PromotionEvent>,
    pub coverage: Vec<CoordinationCoverage>,
    /// Non-content tombstones cascade through every dependent lineage.
    pub invalidated_origin_ids: Vec<String>,
    pub market_deviation: Option<CoordinationMarketDeviation>,
    pub market_deviation_unavailable_reason: Option<String>,
    pub thresholds: CoordinationThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoordinationTier {
    Normal,
    Elevated,
    ReviewRequired,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBurst {
    pub source_id: String,
    pub ratio: Option<Decimal>,
    pub unavailable_reason: Option<String>,
    pub baseline_reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinationAssessment {
    pub issuer_id: String,
    pub claim_cluster: String,
    pub assessed_at: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub threshold_version: String,
    pub calibration_reference: Option<String>,
    pub source_coverage: Vec<CoordinationCoverage>,
    pub risk_of_unreliable_attention: CoordinationTier,
    /// Suppression only, in [0,1]. Unavailable never becomes a neutral multiplier.
    pub evidence_weight_multiplier: Option<Decimal>,
    pub unique_posts: u64,
    pub reposts: u64,
    /// Effective components after lineage, lexical, URL, author and disclosure dedup.
    pub independent_origin_count: Option<u64>,
    /// Distinct representative source-scoped authors of those components.
    pub independent_author_count: Option<u64>,
    /// Largest lexical-cluster count divided by non-disclosure post count.
    pub near_duplicate_fraction: Option<Decimal>,
    /// Largest observed URL-cluster count divided by non-disclosure post count.
    pub shared_url_fraction: Option<Decimal>,
    pub synchronized_fraction: Option<Decimal>,
    pub source_bursts: Vec<SourceBurst>,
    pub supporting_origin_ids: Vec<String>,
    pub preserved_disclosure_references: Vec<String>,
    pub contributions: Vec<String>,
    pub counterexamples: Vec<String>,
    pub feature_unavailable_reasons: Vec<String>,
    pub unavailable_reasons: Vec<String>,
}

//! Public interest observations retain their query scale and cannot represent sentiment.
//!
//! f64 is used only for robust statistical indices/variance, never money. Policy
//! validation rejects nonfinite inputs and arithmetic before constructing assessments.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttentionSemantics {
    RelativeNormalizedIndex0To100,
    ConsistentScaleIndex,
    AbsolutePageviews,
    AggregatedForumCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttentionEntityKind {
    Issuer,
    Brand,
    Product,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionResolution {
    pub issuer_id: String,
    pub canonical_topic_id: String,
    pub entity_kind: AttentionEntityKind,
    pub audit_version: String,
    pub audit_reference: String,
    pub audited_at: DateTime<Utc>,
    pub owner_verified: bool,
    pub ambiguous: bool,
    pub generic_or_acronym_only: bool,
    pub redirect_or_name_change_verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttentionAgentCoverage {
    HumanOnly,
    KnownBotsExcluded,
    IncludesAutomated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionObservation {
    pub query_id: String,
    pub region: String,
    pub language: String,
    pub scale_id: String,
    pub semantics: AttentionSemantics,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub sampled_at: DateTime<Utc>,
    pub value: Option<f64>,
    pub unavailable_reason: Option<String>,
    pub collection_coverage: Option<f64>,
    pub censored_or_low_count: bool,
    pub zero_is_measured: bool,
    pub invalidated: bool,
    pub superseded_by_reference: Option<String>,
    pub agent_coverage: AttentionAgentCoverage,
    pub revision: String,
    pub reference: String,
    pub origin_id: String,
    /// Permitted independent repeated samples of this exact query/scale/bucket.
    pub independent_sample_values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionSeries {
    pub source_id: String,
    pub query_id: String,
    pub resolution: AttentionResolution,
    pub region: String,
    pub language: String,
    pub scale_id: String,
    pub semantics: AttentionSemantics,
    /// Defines the common normalization scope, including reviewed UI imports.
    pub normalization_start: DateTime<Utc>,
    pub normalization_end: DateTime<Utc>,
    /// First availability of this exact scale/vintage; UI rescaling cannot backdate it.
    pub scale_available_at: DateTime<Utc>,
    pub bucket_seconds: u32,
    pub observations: Vec<AttentionObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedAttentionEvent {
    pub event_id: String,
    pub first_public_at: DateTime<Utc>,
    pub verified_available_at: DateTime<Utc>,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionThresholds {
    pub version: String,
    pub minimum_history: u32,
    pub maximum_history: u32,
    pub minimum_coverage: f64,
    pub spike_robust_z: f64,
    pub sustained_buckets: u32,
    pub maximum_age_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionInput {
    pub issuer_id: String,
    pub series: Vec<AttentionSeries>,
    pub verified_events: Vec<VerifiedAttentionEvent>,
    pub coordination_warning_reference: Option<String>,
    pub thresholds: AttentionThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttentionStatus {
    Available,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttentionPattern {
    Ordinary,
    Spike,
    Sustained,
    Decay,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttentionEventTiming {
    AvailableBeforeEvent,
    EventAlreadyPublic,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionEventComparison {
    pub event_id: String,
    pub timing: AttentionEventTiming,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionQueryAssessment {
    pub source_id: String,
    pub query_id: String,
    pub entity_kind: AttentionEntityKind,
    pub region: String,
    pub language: String,
    pub semantics: AttentionSemantics,
    pub scale_id: String,
    pub canonical_topic_id: String,
    pub resolution_audit_reference: String,
    pub resolution_audit_version: String,
    pub normalization_start: DateTime<Utc>,
    pub normalization_end: DateTime<Utc>,
    pub scale_available_at: DateTime<Utc>,
    pub status: AttentionStatus,
    pub pattern: AttentionPattern,
    pub historical_median: Option<f64>,
    pub historical_mad: Option<f64>,
    pub robust_z: Option<f64>,
    pub independent_sample_variance: Option<f64>,
    pub history_count: u32,
    pub current_value: Option<f64>,
    pub current_collection_coverage: Option<f64>,
    pub available_at: Option<DateTime<Utc>>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub origin_ids: Vec<String>,
    pub references: Vec<String>,
    pub event_comparisons: Vec<AttentionEventComparison>,
    pub unavailable_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionConcordance {
    pub query_ids: Vec<String>,
    /// Pattern agreement only; magnitudes on different scales are never summed.
    pub agreeing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionAssessment {
    pub issuer_id: String,
    pub assessed_at: DateTime<Utc>,
    pub feature_version: String,
    pub status: AttentionStatus,
    pub queries: Vec<AttentionQueryAssessment>,
    pub concordance: Vec<AttentionConcordance>,
    pub deduplicated_origin_ids: Vec<String>,
    pub possible_publicity_manipulation: bool,
    pub coordination_warning_reference: Option<String>,
    pub limitations: Vec<String>,
}

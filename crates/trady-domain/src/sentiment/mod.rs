//! Provider-neutral, non-authoritative sentiment observations.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub const SENTIMENT_VERSION: u32 = 1;
pub const FINBERT_MODEL: &str = "ProsusAI/finbert";
pub const FINBERT_REVISION: &str = "0bc2ac10dda6929bcb1f77ccc43c7ec5c424f031";
pub const MAX_SENTIMENT_ITEMS: usize = 16;
pub const MAX_SENTIMENT_CHARS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentimentProbabilities {
    pub positive: Decimal,
    pub negative: Decimal,
    pub neutral: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentimentDocument {
    pub item_id: String,
    pub text_sha256: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentimentPrediction {
    pub item_id: String,
    pub text_sha256: String,
    pub probabilities: SentimentProbabilities,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentimentBatch {
    pub schema_version: u32,
    pub model_id: String,
    pub model_revision: String,
    pub predictions: Vec<SentimentPrediction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentimentEvidence {
    pub item_id: String,
    pub lineage_id: String,
    pub text_sha256: String,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub probabilities: SentimentProbabilities,
}

/// Constructed by application correlation, not accepted as model-authored authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SentimentAssessment {
    pub schema_version: u32,
    pub model_id: String,
    pub model_revision: String,
    pub scored_at: DateTime<Utc>,
    pub evidence: Vec<SentimentEvidence>,
}

//! Source observations are untrusted research data and carry no execution authority.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceKind {
    XTimeline,
    XLookup,
    XRecentSearch,
    GoogleNewsRss,
    PublisherRss,
    AlphaVantage,
    Finnhub,
    Gdelt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRights {
    pub store_content: bool,
    pub llm_processing: bool,
    pub retention_hours: u32,
    pub terms_reference: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContentStatus {
    MetadataOnly,
    Snippet,
    Available,
    Superseded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SocialAction {
    Unknown,
    Original,
    Reply,
    Quote,
    Repost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocialObservation {
    pub action: SocialAction,
    pub actor_user_id: String,
    pub original_author_id: Option<String>,
    pub conversation_id: Option<String>,
    pub reply_to_id: Option<String>,
    pub quoted_id: Option<String>,
    pub reposted_id: Option<String>,
    pub edit_history_ids: Vec<String>,
    pub missing_context: bool,
    pub context: Vec<ReferencedPost>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferencedPost {
    pub post_id: String,
    pub author_id: String,
    pub text: Option<String>,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceItem {
    pub item_id: String,
    pub source_id: String,
    pub external_id: String,
    pub canonical_url: String,
    pub lineage_id: String,
    pub content_hash: String,
    pub parser_version: String,
    pub publisher: String,
    pub feed_title: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub text: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub event_at: Option<DateTime<Utc>>,
    pub first_seen_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub language: Option<String>,
    pub rights: SourceRights,
    pub content_status: ContentStatus,
    pub social: Option<SocialObservation>,
    pub related_urls: Vec<String>,
    pub gaps: Vec<String>,
}

impl SourceItem {
    /// Every reader applies this as well as the database's physical content purge.
    pub fn redact(&mut self, now: DateTime<Utc>, for_model: bool) {
        if !self.rights.store_content
            || self.expires_at <= now
            || (for_model && !self.rights.llm_processing)
            || matches!(
                self.content_status,
                ContentStatus::Superseded | ContentStatus::Unavailable
            )
        {
            self.title = None;
            self.feed_title = None;
            self.summary = None;
            self.text = None;
            if let Some(social) = &mut self.social {
                for context in &mut social.context {
                    context.text = None;
                }
            }
            if !matches!(
                self.content_status,
                ContentStatus::Superseded | ContentStatus::Unavailable
            ) {
                self.content_status = ContentStatus::MetadataOnly;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CollectionStatus {
    Complete,
    Partial,
    Unavailable,
    RateLimited,
    Disabled,
}

/// A continuation retains its original interval across bounded collection runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SourceState {
    pub revision: i64,
    pub registry_hash: Option<String>,
    pub rights: Option<SourceRights>,
    pub request_day: Option<NaiveDate>,
    pub requests_today: u32,
    pub watermark: Option<DateTime<Utc>>,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub next_token: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub retry_at: Option<DateTime<Utc>>,
    pub last_status: Option<CollectionStatus>,
    pub gaps: Vec<String>,
    #[serde(default)]
    pub coverage_revision: u32,
    #[serde(default)]
    pub x_counts: Option<XCountsMeasurement>,
    #[serde(default)]
    pub coverage: Option<SourceCoverage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCoverage {
    pub transport: SourceKind,
    pub revision: u32,
    pub rule_version: Option<u32>,
    pub rule_hash: Option<String>,
    pub query: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approval_reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XCountBucket {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    #[serde(alias = "tweet_count")]
    pub post_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XCountsMeasurement {
    pub rule_hash: String,
    pub query: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub measured_at: DateTime<Utc>,
    pub complete: bool,
    pub total_posts: u64,
    pub buckets: Vec<XCountBucket>,
    pub next_token: Option<String>,
    pub gaps: Vec<String>,
}

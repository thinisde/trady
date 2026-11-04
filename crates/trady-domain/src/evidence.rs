//! Normalized, rights-aware evidence and untrusted user URL ingestion.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{Error, Result, require};

pub const EVIDENCE_SCHEMA_VERSION: u32 = 1;
pub const MAX_EVIDENCE_BODY_CHARS: usize = 32_000;
pub const MAX_USER_URL_QUEUE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceSourceKind {
    YfinanceNews,
    AlphaVantage,
    Finnhub,
    Gdelt,
    XPost,
    GoogleNewsRss,
    PublisherRss,
    BrowserPage,
    UserSubmittedUrl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RightsStatus {
    Reviewed,
    MetadataOnly,
    Restricted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceItem {
    pub schema_version: u32,
    pub id: String,
    pub canonical_url: String,
    pub source_kind: EvidenceSourceKind,
    pub publisher: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub body_or_excerpt: Option<String>,
    pub symbol_mentions: Vec<String>,
    pub entity_ids: Vec<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub content_hash: String,
    pub rights_status: RightsStatus,
    pub primary_or_secondary: EvidenceLineage,
    pub contradictions: Vec<String>,
    pub raw_ref: Option<String>,
    pub untrusted_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceLineage {
    Primary,
    Secondary,
    Unknown,
}

impl EvidenceItem {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.schema_version == EVIDENCE_SCHEMA_VERSION
                && valid_id(&self.id)
                && valid_https_url(&self.canonical_url)
                && self.fetched_at <= now
                && self
                    .published_at
                    .is_none_or(|published| published <= self.fetched_at)
                && is_hash(&self.content_hash)
                && self.symbol_mentions.len() <= 64
                && self
                    .symbol_mentions
                    .iter()
                    .all(|symbol| valid_symbol(symbol))
                && self.entity_ids.len() <= 64
                && self.entity_ids.iter().all(|id| valid_id(id))
                && self.contradictions.len() <= 32
                && self.contradictions.iter().all(|id| valid_id(id))
                && self.raw_ref.as_ref().is_none_or(|id| valid_id(id)),
            "invalid normalized evidence item",
        )?;
        for text in [
            self.publisher.as_deref(),
            self.author.as_deref(),
            self.title.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            require(valid_text(text, 512), "invalid evidence metadata")?;
        }
        if let Some(body) = &self.body_or_excerpt {
            require(
                body.chars().count() <= MAX_EVIDENCE_BODY_CHARS && !body.contains('\0'),
                "evidence body exceeds the bounded content limit",
            )?;
        }
        if matches!(
            self.rights_status,
            RightsStatus::MetadataOnly | RightsStatus::Restricted
        ) {
            require(
                self.body_or_excerpt.is_none(),
                "restricted evidence cannot retain body text",
            )?;
        }
        if self.source_kind == EvidenceSourceKind::UserSubmittedUrl {
            require(
                self.untrusted_input,
                "user submitted evidence must remain explicitly untrusted",
            )?;
        }
        Ok(())
    }

    pub fn fresh_for_live_sentiment(&self, now: DateTime<Utc>, max_age_minutes: i64) -> bool {
        max_age_minutes >= 0
            && self.published_at.is_some_and(|published| {
                published <= now
                    && now.signed_duration_since(published) <= Duration::minutes(max_age_minutes)
            })
            && !matches!(
                self.rights_status,
                RightsStatus::Unknown | RightsStatus::Restricted
            )
    }

    pub fn trusted_for_selection(&self) -> bool {
        !self.untrusted_input
            && !matches!(
                self.rights_status,
                RightsStatus::Unknown | RightsStatus::Restricted
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyndicationCluster {
    pub cluster_id: String,
    pub representative_id: String,
    pub member_ids: Vec<String>,
    pub conflicting_item_ids: Vec<String>,
    pub content_hashes: Vec<String>,
}

impl SyndicationCluster {
    pub fn validate(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        require(
            valid_id(&self.cluster_id)
                && valid_id(&self.representative_id)
                && !self.member_ids.is_empty()
                && self
                    .member_ids
                    .iter()
                    .all(|id| ids.insert(id) && valid_id(id))
                && ids.contains(&self.representative_id)
                && self.conflicting_item_ids.iter().all(|id| valid_id(id))
                && self.content_hashes.iter().all(|hash| is_hash(hash)),
            "invalid evidence syndication cluster",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserUrlStatus {
    Queued,
    Fetched,
    Unavailable,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserSubmittedUrl {
    pub id: String,
    pub canonical_url: String,
    pub submitted_at: DateTime<Utc>,
    pub submitted_by: String,
    pub status: UserUrlStatus,
    pub evidence_id: Option<String>,
    pub error_code: Option<String>,
}

impl UserSubmittedUrl {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_id(&self.id)
                && valid_https_url(&self.canonical_url)
                && valid_id(&self.submitted_by)
                && self.submitted_at <= now
                && self.evidence_id.as_ref().is_none_or(|id| valid_id(id))
                && self.error_code.as_ref().is_none_or(|code| valid_id(code)),
            "invalid user submitted URL",
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct UserUrlQueue {
    rows: Vec<UserSubmittedUrl>,
}

impl UserUrlQueue {
    pub fn enqueue(
        &mut self,
        url: impl Into<String>,
        submitted_by: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<UserSubmittedUrl> {
        require(
            self.rows.len() < MAX_USER_URL_QUEUE,
            "user URL queue is full",
        )?;
        let canonical_url = canonicalize_url(&url.into())?;
        require(
            !self
                .rows
                .iter()
                .any(|row| row.canonical_url == canonical_url),
            "user URL is already queued",
        )?;
        let submitted_by = submitted_by.into();
        let id = format!("url_{}", stable_short_hash(&canonical_url));
        let row = UserSubmittedUrl {
            id,
            canonical_url,
            submitted_at: now,
            submitted_by,
            status: UserUrlStatus::Queued,
            evidence_id: None,
            error_code: None,
        };
        row.validate(now)?;
        self.rows.push(row.clone());
        Ok(row)
    }

    pub fn pending(&self, limit: usize) -> Vec<UserSubmittedUrl> {
        self.rows
            .iter()
            .filter(|row| row.status == UserUrlStatus::Queued)
            .take(limit.min(32))
            .cloned()
            .collect()
    }

    pub fn mark_fetched(&mut self, id: &str, evidence_id: &str, now: DateTime<Utc>) -> Result<()> {
        let row = self
            .rows
            .iter_mut()
            .find(|row| row.id == id)
            .ok_or_else(|| Error("user URL is not queued".into()))?;
        require(
            row.status == UserUrlStatus::Queued,
            "user URL is not pending",
        )?;
        require(valid_id(evidence_id), "invalid user URL evidence reference")?;
        row.status = UserUrlStatus::Fetched;
        row.evidence_id = Some(evidence_id.into());
        row.validate(now)
    }

    pub fn mark_unavailable(
        &mut self,
        id: &str,
        error_code: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let row = self
            .rows
            .iter_mut()
            .find(|row| row.id == id)
            .ok_or_else(|| Error("user URL is not queued".into()))?;
        require(
            row.status == UserUrlStatus::Queued,
            "user URL is not pending",
        )?;
        require(valid_id(error_code), "invalid user URL failure code")?;
        row.status = UserUrlStatus::Unavailable;
        row.error_code = Some(error_code.into());
        row.validate(now)
    }

    pub fn rows(&self) -> &[UserSubmittedUrl] {
        &self.rows
    }
}

pub fn canonicalize_url(value: &str) -> Result<String> {
    let value = value.trim();
    require(
        valid_https_url(value),
        "user URL must be a bounded HTTPS URL",
    )?;
    let without_fragment = value.split('#').next().unwrap_or(value);
    require(without_fragment.len() <= 2048, "user URL is too long")?;
    Ok(without_fragment.to_owned())
}

fn valid_https_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?']).next().unwrap_or_default();
    !host.is_empty()
        && !host.contains(['@', ':', '\\'])
        && host != "localhost"
        && !host.ends_with('.')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
}

fn stable_short_hash(value: &str) -> String {
    // A deterministic queue key without pulling hashing or network code into
    // the domain crate. The full content hash is still required on evidence.
    let mut hash: u64 = 1469598103934665603;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains(['\0', '\r', '\n'])
}

fn valid_symbol(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b".-".contains(&b))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }

    fn item(source_kind: EvidenceSourceKind) -> EvidenceItem {
        EvidenceItem {
            schema_version: 1,
            id: "evidence_1".into(),
            canonical_url: "https://example.com/article".into(),
            source_kind,
            publisher: Some("Example".into()),
            author: None,
            title: Some("Headline".into()),
            body_or_excerpt: Some("Text".into()),
            symbol_mentions: vec!["AIR".into()],
            entity_ids: vec!["AIR_PA_EQ".into()],
            published_at: Some(now() - Duration::minutes(5)),
            fetched_at: now(),
            content_hash: "a".repeat(64),
            rights_status: RightsStatus::Reviewed,
            primary_or_secondary: EvidenceLineage::Secondary,
            contradictions: vec![],
            raw_ref: None,
            untrusted_input: false,
        }
    }

    #[test]
    fn live_sentiment_uses_publication_age_and_rights() {
        let evidence = item(EvidenceSourceKind::PublisherRss);
        assert!(evidence.validate(now()).is_ok());
        assert!(evidence.fresh_for_live_sentiment(now(), 30));
        let mut old = evidence.clone();
        old.published_at = Some(now() - Duration::minutes(31));
        assert!(!old.fresh_for_live_sentiment(now(), 30));
        let mut restricted = evidence;
        restricted.rights_status = RightsStatus::Restricted;
        restricted.body_or_excerpt = None;
        assert!(restricted.validate(now()).is_ok());
        assert!(!restricted.trusted_for_selection());
    }

    #[test]
    fn user_urls_are_untrusted_and_credentials_or_private_hosts_are_rejected() {
        let mut queue = UserUrlQueue::default();
        assert!(
            queue
                .enqueue("https://example.com/news", "owner", now())
                .is_ok()
        );
        assert!(
            queue
                .enqueue("https://user:pass@example.com/news", "owner", now())
                .is_err()
        );
        assert!(
            queue
                .enqueue("https://localhost/news", "owner", now())
                .is_err()
        );
        let row = &queue.rows()[0];
        assert_eq!(row.status, UserUrlStatus::Queued);
        let mut from_user = item(EvidenceSourceKind::UserSubmittedUrl);
        from_user.untrusted_input = true;
        assert!(from_user.validate(now()).is_ok());
        assert!(!from_user.trusted_for_selection());
    }
}

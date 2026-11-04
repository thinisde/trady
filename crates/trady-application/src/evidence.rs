//! Shared evidence ledger for yfinance, Alpha Vantage, X, RSS, browser, and
//! owner-submitted URLs.

use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

use crate::market_tools::EurEvidenceReadPort;
use trady_domain::evidence::{
    EvidenceLineage, EvidenceSourceKind, RightsStatus, SyndicationCluster, UserSubmittedUrl,
};
use trady_domain::research::{ContentStatus, SourceItem};
use trady_domain::{Error, EvidenceItem, InstrumentId, Result, require};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserPageInput {
    pub id: String,
    pub canonical_url: String,
    pub publisher: Option<String>,
    pub title: Option<String>,
    pub body_or_excerpt: Option<String>,
    pub entity_ids: Vec<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub rights_status: RightsStatus,
}

/// Normalize content returned by a reviewed browser/page reader. The reader
/// owns allowlisted hosts, redirects, size and timeout checks; this function
/// ensures the resulting evidence is bounded and content-addressed.
pub fn normalize_browser_page(input: BrowserPageInput, now: DateTime<Utc>) -> Result<EvidenceItem> {
    let content = input
        .body_or_excerpt
        .as_deref()
        .unwrap_or_default()
        .to_owned();
    let evidence = EvidenceItem {
        schema_version: 1,
        id: input.id,
        canonical_url: input.canonical_url,
        source_kind: EvidenceSourceKind::BrowserPage,
        publisher: input.publisher,
        author: None,
        title: input.title,
        body_or_excerpt: input.body_or_excerpt,
        symbol_mentions: vec![],
        entity_ids: input.entity_ids,
        published_at: input.published_at,
        fetched_at: input.fetched_at,
        content_hash: crate::harness::content_hash(&content),
        rights_status: input.rights_status,
        primary_or_secondary: EvidenceLineage::Secondary,
        contradictions: vec![],
        raw_ref: None,
        untrusted_input: false,
    };
    evidence.validate(now)?;
    Ok(evidence)
}

/// Owner-submitted pages are normalized through the same ledger but remain
/// explicitly untrusted. They can inform research and never grant tool or
/// order authority.
pub fn normalize_user_submitted_page(
    queued: &UserSubmittedUrl,
    title: Option<String>,
    excerpt: Option<String>,
    fetched_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<EvidenceItem> {
    require(
        queued.canonical_url.starts_with("https://"),
        "user page must use the queued HTTPS URL",
    )?;
    let evidence = EvidenceItem {
        schema_version: 1,
        id: queued
            .evidence_id
            .clone()
            .unwrap_or_else(|| queued.id.clone()),
        canonical_url: queued.canonical_url.clone(),
        source_kind: EvidenceSourceKind::UserSubmittedUrl,
        publisher: None,
        author: None,
        title,
        // User-submitted content remains metadata-only until a separate rights
        // review approves retaining text.
        body_or_excerpt: None,
        symbol_mentions: vec![],
        entity_ids: vec![],
        published_at: None,
        fetched_at,
        content_hash: crate::harness::content_hash(
            excerpt.as_deref().unwrap_or(queued.canonical_url.as_str()),
        ),
        rights_status: RightsStatus::MetadataOnly,
        primary_or_secondary: EvidenceLineage::Unknown,
        contradictions: vec![],
        raw_ref: Some(queued.id.clone()),
        untrusted_input: true,
    };
    evidence.validate(now)?;
    Ok(evidence)
}

#[derive(Debug, Clone, Default)]
pub struct EvidenceLedger {
    items: BTreeMap<String, EvidenceItem>,
    clusters: BTreeMap<String, SyndicationCluster>,
}

impl EvidenceLedger {
    pub fn ingest(&mut self, item: EvidenceItem, now: DateTime<Utc>) -> Result<bool> {
        item.validate(now)?;
        if let Some(existing) = self.items.get(&item.id) {
            require(
                existing == &item,
                "evidence ID was reused with different content",
            )?;
            return Ok(false);
        }
        let same_hash: Vec<String> = self
            .items
            .values()
            .filter(|existing| existing.content_hash == item.content_hash)
            .map(|existing| existing.id.clone())
            .collect();
        self.items.insert(item.id.clone(), item.clone());
        let cluster_id = format!("cluster_{}", &item.content_hash[..16]);
        let cluster =
            self.clusters
                .entry(cluster_id.clone())
                .or_insert_with(|| SyndicationCluster {
                    cluster_id,
                    representative_id: item.id.clone(),
                    member_ids: vec![],
                    conflicting_item_ids: vec![],
                    content_hashes: vec![],
                });
        if !cluster.member_ids.contains(&item.id) {
            cluster.member_ids.push(item.id.clone());
        }
        if !cluster.content_hashes.contains(&item.content_hash) {
            cluster.content_hashes.push(item.content_hash.clone());
        }
        for id in same_hash {
            if id != item.id && !cluster.member_ids.contains(&id) {
                cluster.member_ids.push(id);
            }
        }
        cluster.validate()?;
        Ok(true)
    }

    pub fn ingest_source_item(&mut self, item: &SourceItem, now: DateTime<Utc>) -> Result<bool> {
        let source_kind = match item_source_kind(item.source_id.as_str()) {
            Some(kind) => kind,
            None => return Err(Error("source is not enabled for target evidence".into())),
        };
        let canonical_url = item.canonical_url.trim();
        require(
            canonical_url.starts_with("https://"),
            "source item lacks an approved HTTPS canonical URL",
        )?;
        let rights_status = if item.rights.store_content && item.rights.llm_processing {
            RightsStatus::Reviewed
        } else if item.rights.store_content {
            RightsStatus::MetadataOnly
        } else {
            RightsStatus::Restricted
        };
        let body = if matches!(rights_status, RightsStatus::Reviewed)
            && !matches!(
                item.content_status,
                ContentStatus::MetadataOnly | ContentStatus::Unavailable
            ) {
            item.text.clone().or_else(|| item.summary.clone())
        } else {
            None
        };
        let entity_ids: Vec<String> = item
            .related_urls
            .iter()
            .filter(|value| value.len() <= 160)
            .cloned()
            .collect();
        let evidence = EvidenceItem {
            schema_version: 1,
            id: item.item_id.clone(),
            canonical_url: canonical_url.to_owned(),
            source_kind,
            publisher: (!item.publisher.is_empty()).then(|| item.publisher.clone()),
            author: None,
            title: item.title.clone(),
            body_or_excerpt: body,
            symbol_mentions: vec![],
            entity_ids,
            published_at: item.published_at,
            fetched_at: item.fetched_at,
            content_hash: item.content_hash.clone(),
            rights_status,
            primary_or_secondary: if item.social.is_none() {
                EvidenceLineage::Secondary
            } else {
                EvidenceLineage::Unknown
            },
            contradictions: vec![],
            raw_ref: Some(item.lineage_id.clone()),
            untrusted_input: false,
        };
        self.ingest(evidence, now)
    }

    pub fn get(&self, id: &str) -> Option<&EvidenceItem> {
        self.items.get(id)
    }

    pub fn fresh_for(
        &self,
        instrument_id: &InstrumentId,
        now: DateTime<Utc>,
        minutes: i64,
    ) -> Vec<&EvidenceItem> {
        self.items
            .values()
            .filter(|item| {
                item.entity_ids
                    .iter()
                    .any(|id| id == instrument_id.as_str())
            })
            .filter(|item| item.fresh_for_live_sentiment(now, minutes))
            .collect()
    }

    pub fn source_counts(&self) -> BTreeMap<EvidenceSourceKind, usize> {
        let mut counts = BTreeMap::new();
        for item in self.items.values() {
            *counts.entry(item.source_kind).or_insert(0) += 1;
        }
        counts
    }

    pub fn clusters(&self) -> impl Iterator<Item = &SyndicationCluster> {
        self.clusters.values()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl EurEvidenceReadPort for EvidenceLedger {
    fn search(
        &mut self,
        instrument_id: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<Vec<EvidenceItem>> {
        Ok(self
            .fresh_for(instrument_id, now, 30)
            .into_iter()
            .cloned()
            .collect())
    }

    fn read(&mut self, evidence_id: &str, now: DateTime<Utc>) -> Result<EvidenceItem> {
        let item = self
            .get(evidence_id)
            .ok_or_else(|| Error("evidence item is unavailable".into()))?;
        item.validate(now)?;
        Ok(item.clone())
    }
}

fn item_source_kind(source_id: &str) -> Option<EvidenceSourceKind> {
    let normalized = source_id.to_ascii_lowercase();
    if normalized.contains("alpha") || normalized.contains("avantage") {
        Some(EvidenceSourceKind::AlphaVantage)
    } else if normalized.contains("finnhub") {
        Some(EvidenceSourceKind::Finnhub)
    } else if normalized.contains("gdelt") {
        Some(EvidenceSourceKind::Gdelt)
    } else if normalized.contains("x") {
        Some(EvidenceSourceKind::XPost)
    } else if normalized.contains("google") {
        Some(EvidenceSourceKind::GoogleNewsRss)
    } else if normalized.contains("rss") || normalized.contains("publisher") {
        Some(EvidenceSourceKind::PublisherRss)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use trady_domain::research::SourceRights;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }

    fn source_item(source_id: &str) -> SourceItem {
        SourceItem {
            item_id: format!("item_{source_id}"),
            source_id: source_id.into(),
            external_id: "external_1".into(),
            canonical_url: "https://example.com/article".into(),
            lineage_id: "lineage_1".into(),
            content_hash: "a".repeat(64),
            parser_version: "parser_1".into(),
            publisher: "Example".into(),
            feed_title: None,
            title: Some("Headline".into()),
            summary: Some("Summary".into()),
            text: Some("Body".into()),
            published_at: Some(now() - Duration::minutes(5)),
            event_at: None,
            first_seen_at: now(),
            fetched_at: now(),
            expires_at: now() + Duration::hours(1),
            language: Some("en".into()),
            rights: SourceRights {
                store_content: true,
                llm_processing: true,
                retention_hours: 24,
                terms_reference: "review_1".into(),
            },
            content_status: ContentStatus::Available,
            social: None,
            related_urls: vec![],
            gaps: vec![],
        }
    }

    #[test]
    fn alpha_vantage_items_enter_the_same_evidence_ledger_as_other_sources() {
        let mut ledger = EvidenceLedger::default();
        assert!(
            ledger
                .ingest_source_item(&source_item("alpha_vantage_news"), now())
                .expect("ingest")
        );
        assert_eq!(ledger.len(), 1);
        assert_eq!(
            ledger
                .source_counts()
                .get(&EvidenceSourceKind::AlphaVantage),
            Some(&1)
        );
        assert!(
            !ledger
                .ingest_source_item(&source_item("alpha_vantage_news"), now())
                .expect("idempotent ingest")
        );
    }

    #[test]
    fn browser_and_user_pages_keep_bounded_provenance_and_untrusted_user_state() {
        let browser = normalize_browser_page(
            BrowserPageInput {
                id: "browser_1".into(),
                canonical_url: "https://publisher.example/article".into(),
                publisher: Some("Publisher".into()),
                title: Some("Headline".into()),
                body_or_excerpt: Some("bounded body".into()),
                entity_ids: vec!["AIR_PA_EQ".into()],
                published_at: Some(now() - Duration::minutes(2)),
                fetched_at: now(),
                rights_status: RightsStatus::Reviewed,
            },
            now(),
        )
        .expect("browser evidence");
        assert_eq!(browser.source_kind, EvidenceSourceKind::BrowserPage);
        let mut queue = trady_domain::evidence::UserUrlQueue::default();
        let queued = queue
            .enqueue("https://owner.example/article", "owner", now())
            .expect("queue");
        let user = normalize_user_submitted_page(
            &queued,
            Some("Owner link".into()),
            Some("untrusted excerpt".into()),
            now(),
            now(),
        )
        .expect("user evidence");
        assert!(user.untrusted_input);
        assert!(user.body_or_excerpt.is_none());
    }
}

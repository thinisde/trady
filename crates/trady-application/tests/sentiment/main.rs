use std::cell::Cell;

use chrono::{Duration, TimeZone, Utc};
use rust_decimal::Decimal;
use trady_application::sentiment::*;
use trady_domain::{Result, research::*, sentiment::*};

struct Provider {
    calls: Cell<usize>,
    fault: &'static str,
}
impl SentimentProvider for Provider {
    fn model_revision(&self) -> &str {
        FINBERT_REVISION
    }
    fn predict(&self, documents: &[SentimentDocument]) -> Result<SentimentBatch> {
        self.calls.set(self.calls.get() + 1);
        let mut predictions: Vec<_> = documents
            .iter()
            .map(|d| SentimentPrediction {
                item_id: d.item_id.clone(),
                text_sha256: d.text_sha256.clone(),
                probabilities: SentimentProbabilities {
                    positive: Decimal::new(8, 1),
                    negative: Decimal::new(1, 1),
                    neutral: Decimal::new(1, 1),
                },
                truncated: self.fault == "truncated",
            })
            .collect();
        match self.fault {
            "hash" => predictions[0].text_sha256 = "b".repeat(64),
            "item" => predictions[0].item_id = "unrelated".into(),
            "probabilities" => predictions[0].probabilities.negative = Decimal::ONE,
            "missing" => predictions.clear(),
            _ => {}
        }
        Ok(SentimentBatch {
            schema_version: 1,
            model_id: FINBERT_MODEL.into(),
            model_revision: if self.fault == "model" {
                "a".repeat(40)
            } else {
                FINBERT_REVISION.into()
            },
            predictions,
        })
    }
}

fn item() -> SourceItem {
    let now = Utc.with_ymd_and_hms(2026, 10, 31, 12, 0, 0).unwrap();
    SourceItem {
        item_id: "item-1".into(),
        source_id: "rss".into(),
        external_id: "1".into(),
        canonical_url: "https://example.org/news/1".into(),
        lineage_id: "lineage-1".into(),
        content_hash: "original-hash".into(),
        parser_version: "fixture-v1".into(),
        publisher: "Example".into(),
        feed_title: None,
        title: Some("Results".into()),
        summary: None,
        text: Some("The earnings outlook improved.".into()),
        published_at: Some(now - Duration::minutes(5)),
        event_at: None,
        first_seen_at: now,
        fetched_at: now,
        expires_at: now + Duration::hours(1),
        language: Some("en".into()),
        rights: SourceRights {
            store_content: true,
            llm_processing: true,
            retention_hours: 1,
            terms_reference: "fixture".into(),
        },
        content_status: ContentStatus::Available,
        social: None,
        related_urls: vec![],
        gaps: vec![],
    }
}

#[test]
fn correlates_exact_sent_text_and_source_provenance() {
    let source = item();
    let provider = Provider {
        calls: Cell::new(0),
        fault: "",
    };
    let value =
        assess_sentiment(&provider, std::slice::from_ref(&source), source.fetched_at).unwrap();
    assert_eq!(
        value.evidence[0].text_sha256,
        text_sha256(source.text.as_ref().unwrap())
    );
    assert_eq!(value.evidence[0].lineage_id, source.lineage_id);
    assert_eq!(value.evidence[0].expires_at, source.expires_at);
    assert_eq!(provider.calls.get(), 1);
}

#[test]
fn denied_expired_missing_non_english_and_copied_sources_never_reach_provider() {
    let source = item();
    let mut cases = vec![];
    let mut value = source.clone();
    value.rights.llm_processing = false;
    cases.push(vec![value]);
    let mut value = source.clone();
    value.rights.store_content = false;
    cases.push(vec![value]);
    let mut value = source.clone();
    value.expires_at = source.fetched_at;
    cases.push(vec![value]);
    let mut value = source.clone();
    value.language = Some("de".into());
    cases.push(vec![value]);
    let mut value = source.clone();
    value.content_status = ContentStatus::Superseded;
    cases.push(vec![value]);
    let mut value = source.clone();
    value.text = Some("x".repeat(4097));
    cases.push(vec![value]);
    let mut value = source.clone();
    value.item_id = "copy".into();
    cases.push(vec![source.clone(), value]);
    for items in cases {
        let provider = Provider {
            calls: Cell::new(0),
            fault: "",
        };
        assert!(assess_sentiment(&provider, &items, source.fetched_at).is_err());
        assert_eq!(provider.calls.get(), 0);
    }
}

#[test]
fn missing_mismatched_invalid_or_truncated_predictions_fail_closed() {
    let source = item();
    for fault in [
        "model",
        "hash",
        "item",
        "probabilities",
        "missing",
        "truncated",
    ] {
        let provider = Provider {
            calls: Cell::new(0),
            fault,
        };
        assert!(
            assess_sentiment(&provider, std::slice::from_ref(&source), source.fetched_at).is_err(),
            "{fault}"
        );
    }
}

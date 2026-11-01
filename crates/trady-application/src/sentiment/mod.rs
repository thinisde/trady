//! Rights-aware source projection and exact correlation of sentiment predictions.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use trady_domain::{Error, Result, require, research::SourceItem, sentiment::*};

pub use trady_policy::sentiment::validate_probabilities;

pub trait SentimentProvider {
    fn predict(&self, documents: &[SentimentDocument]) -> Result<SentimentBatch>;
    fn model_revision(&self) -> &str;
}

pub fn text_sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Caller selects instrument-relevant evidence and one representative per lineage.
/// Any unusable item fails the batch; partial coverage is never a neutral result.
pub fn assess_sentiment(
    provider: &impl SentimentProvider,
    items: &[SourceItem],
    now: DateTime<Utc>,
) -> Result<SentimentAssessment> {
    require(
        (1..=MAX_SENTIMENT_ITEMS).contains(&items.len()),
        "invalid sentiment batch size",
    )?;
    let mut documents = Vec::new();
    let mut observations = Vec::new();
    let mut ids = BTreeSet::new();
    let mut lineages = BTreeSet::new();
    for original in items {
        let mut item = original.clone();
        item.redact(now, true);
        let observed_at = item
            .event_at
            .or(item.published_at)
            .ok_or_else(|| Error("sentiment requires a known source event time".into()))?;
        require(
            item.language.as_deref().is_some_and(|language| {
                language.eq_ignore_ascii_case("en")
                    || language.to_ascii_lowercase().starts_with("en-")
                        && language.split('-').skip(1).all(|part| {
                            !part.is_empty()
                                && part.len() <= 8
                                && part.bytes().all(|b| b.is_ascii_alphanumeric())
                        })
            }) && !item.item_id.is_empty()
                && item.item_id.len() <= 256
                && !item.lineage_id.is_empty()
                && item.lineage_id.len() <= 256
                && ids.insert(item.item_id.clone())
                && lineages.insert(item.lineage_id.clone())
                && trady_policy::fresh(observed_at, now, 86400)
                && item.expires_at > now
                && item.social.as_ref().is_none_or(|s| !s.missing_context),
            "sentiment requires distinct, current English evidence with available context",
        )?;
        let text = item
            .text
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .or(item.summary.as_deref().filter(|t| !t.trim().is_empty()))
            .or(item.title.as_deref().filter(|t| !t.trim().is_empty()))
            .ok_or_else(|| {
                Error("source text unavailable or not permitted for sentiment processing".into())
            })?;
        require(
            text.chars().count() <= MAX_SENTIMENT_CHARS,
            "sentiment text exceeds size bound",
        )?;
        documents.push(SentimentDocument {
            item_id: item.item_id.clone(),
            text_sha256: text_sha256(text),
            text: text.into(),
        });
        observations.push((item.lineage_id, observed_at, item.expires_at));
    }
    let batch = provider.predict(&documents)?;
    require(
        batch.schema_version == SENTIMENT_VERSION
            && batch.model_id == FINBERT_MODEL
            && batch.model_revision == provider.model_revision()
            && batch.predictions.len() == documents.len(),
        "sentiment response model, version or batch mismatch",
    )?;
    let mut predictions = BTreeMap::new();
    for prediction in batch.predictions {
        require(
            !prediction.truncated,
            "truncated sentiment cannot drive threshold decisions",
        )?;
        validate_probabilities(&prediction.probabilities)?;
        require(
            predictions
                .insert(prediction.item_id.clone(), prediction)
                .is_none(),
            "duplicate sentiment prediction",
        )?;
    }
    let mut evidence = Vec::new();
    for (document, (lineage_id, observed_at, expires_at)) in documents.into_iter().zip(observations)
    {
        let prediction = predictions
            .remove(&document.item_id)
            .ok_or_else(|| Error("sentiment response item mismatch".into()))?;
        require(
            prediction.text_sha256 == document.text_sha256,
            "sentiment response text hash mismatch",
        )?;
        evidence.push(SentimentEvidence {
            item_id: document.item_id,
            lineage_id,
            text_sha256: document.text_sha256,
            observed_at,
            expires_at,
            probabilities: prediction.probabilities,
        });
    }
    Ok(SentimentAssessment {
        schema_version: SENTIMENT_VERSION,
        model_id: batch.model_id,
        model_revision: batch.model_revision,
        scored_at: now,
        evidence,
    })
}

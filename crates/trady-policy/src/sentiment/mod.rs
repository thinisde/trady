//! Conservative v1 overlay; original financial limits and readiness remain required.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use trady_domain::{Result, require, sentiment::*};

use crate::{ConfidenceScore, Policy, add, div, fresh, sub, target_ceiling};

pub fn validate_probabilities(p: &SentimentProbabilities) -> Result<()> {
    require(
        [&p.positive, &p.negative, &p.neutral]
            .into_iter()
            .all(|v| *v >= Decimal::ZERO && *v <= Decimal::ONE),
        "sentiment probabilities must be within zero and one",
    )?;
    require(
        sub(add(add(p.positive, p.negative)?, p.neutral)?, Decimal::ONE)?.abs()
            <= Decimal::new(1, 6),
        "sentiment probabilities must sum to one",
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SentimentThresholds {
    pub calculation_version: u32,
    pub base_score: u8,
    pub adjusted_score: u8,
    pub negative_probability_max: Decimal,
    pub mean_net_sentiment: Decimal,
    pub target_ceiling_fraction: Decimal,
    pub entry_score_passes: bool,
    pub target_increase_allowed: bool,
    pub review_required: bool,
}

/// No mutation or execution authority. Call again with current time before use.
pub fn sentiment_thresholds(
    base: &ConfidenceScore,
    assessment: &SentimentAssessment,
    policy: &Policy,
    exceptional_verified: bool,
    now: DateTime<Utc>,
) -> Result<SentimentThresholds> {
    policy.validate()?;
    require(
        assessment.schema_version == SENTIMENT_VERSION
            && assessment.model_id == FINBERT_MODEL
            && assessment.model_revision.len() == 40
            && assessment
                .model_revision
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            && fresh(assessment.scored_at, now, 900)
            && (1..=MAX_SENTIMENT_ITEMS).contains(&assessment.evidence.len()),
        "invalid or stale sentiment assessment",
    )?;
    let mut ids = BTreeSet::new();
    let mut lineages = BTreeSet::new();
    let mut negative_max = Decimal::ZERO;
    let mut net = Decimal::ZERO;
    for evidence in &assessment.evidence {
        require(
            !evidence.item_id.is_empty()
                && !evidence.lineage_id.is_empty()
                && ids.insert(&evidence.item_id)
                && lineages.insert(&evidence.lineage_id)
                && evidence.text_sha256.len() == 64
                && evidence.text_sha256.bytes().all(|b| b.is_ascii_hexdigit())
                && evidence.observed_at <= assessment.scored_at
                && fresh(evidence.observed_at, now, 86400)
                && evidence.expires_at > now,
            "invalid, duplicated or expired sentiment evidence",
        )?;
        validate_probabilities(&evidence.probabilities)?;
        negative_max = negative_max.max(evidence.probabilities.negative);
        net = add(
            net,
            sub(
                evidence.probabilities.positive,
                evidence.probabilities.negative,
            )?,
        )?;
    }
    let review_required = negative_max >= Decimal::new(60, 2);
    let adjusted_score = base
        .total()
        .saturating_sub(if review_required { 10 } else { 0 });
    Ok(SentimentThresholds {
        calculation_version: 1,
        base_score: base.total(),
        adjusted_score,
        negative_probability_max: negative_max,
        mean_net_sentiment: div(net, Decimal::from(assessment.evidence.len()))?,
        target_ceiling_fraction: target_ceiling(adjusted_score, exceptional_verified, policy)?,
        entry_score_passes: adjusted_score >= policy.confidence_entry_min
            && negative_max < Decimal::new(80, 2),
        target_increase_allowed: !review_required && adjusted_score >= 70,
        review_required,
    })
}

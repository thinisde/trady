use chrono::{Duration, TimeZone, Utc};
use rust_decimal::Decimal;
use trady_domain::sentiment::*;
use trady_policy::{Catalyst, ConfidenceInputs, Policy, confidence, sentiment::*};

fn base() -> trady_policy::ConfidenceScore {
    confidence(&ConfidenceInputs {
        primary_verified: true,
        catalyst: Catalyst::Direct,
        independent_lineages: 2,
        positive_15m_return: true,
        above_vwap: true,
        relative_volume: Some(Decimal::new(15, 1)),
        spread_fraction: Some(Decimal::new(1, 3)),
        round_trip_cost_fraction: Some(Decimal::new(2, 3)),
        catalyst_age_hours: Some(25),
        continuing_relevance: true,
        sources_current: true,
    })
}

fn assessment(negative: i64) -> SentimentAssessment {
    let now = Utc.with_ymd_and_hms(2026, 10, 31, 12, 0, 0).unwrap();
    SentimentAssessment {
        schema_version: 1,
        model_id: FINBERT_MODEL.into(),
        model_revision: FINBERT_REVISION.into(),
        scored_at: now,
        evidence: vec![SentimentEvidence {
            item_id: "item-1".into(),
            lineage_id: "lineage-1".into(),
            text_sha256: "a".repeat(64),
            observed_at: now - Duration::minutes(5),
            expires_at: now + Duration::hours(1),
            probabilities: SentimentProbabilities {
                positive: Decimal::new(100 - negative, 2),
                negative: Decimal::new(negative, 2),
                neutral: Decimal::ZERO,
            },
        }],
    }
}

#[test]
fn adverse_sentiment_changes_entry_and_target_band_without_raising_risk() {
    let positive = assessment(10);
    let unchanged = sentiment_thresholds(
        &base(),
        &positive,
        &Policy::default(),
        false,
        positive.scored_at,
    )
    .unwrap();
    assert_eq!(unchanged.base_score, 95);
    assert_eq!(unchanged.adjusted_score, 95);
    assert_eq!(unchanged.target_ceiling_fraction, Decimal::new(3, 2));
    assert!(unchanged.entry_score_passes && unchanged.target_increase_allowed);

    let negative = assessment(60);
    let adjusted = sentiment_thresholds(
        &base(),
        &negative,
        &Policy::default(),
        false,
        negative.scored_at,
    )
    .unwrap();
    assert_eq!(adjusted.adjusted_score, 85);
    assert_eq!(adjusted.target_ceiling_fraction, Decimal::new(2, 2));
    assert!(!adjusted.entry_score_passes && !adjusted.target_increase_allowed);
    assert!(adjusted.review_required);
    assert_eq!(adjusted.mean_net_sentiment, Decimal::new(-20, 2));
}

#[test]
fn exact_negative_veto_and_weak_evidence_cannot_gain_points() {
    let full = confidence(&ConfidenceInputs {
        catalyst_age_hours: Some(0),
        ..serde_json::from_value(
            serde_json::json!({"primary_verified":true,"catalyst":"DIRECT","independent_lineages":2,
            "positive_15m_return":true,"above_vwap":true,"relative_volume":"1.5",
            "spread_fraction":"0.001","round_trip_cost_fraction":"0.002","sources_current":true}),
        )
        .unwrap()
    });
    for (negative, allowed) in [(59, true), (60, true), (79, true), (80, false)] {
        let value = assessment(negative);
        let result =
            sentiment_thresholds(&full, &value, &Policy::default(), false, value.scored_at)
                .unwrap();
        assert_eq!(result.entry_score_passes, allowed);
        assert!(result.adjusted_score <= result.base_score);
        assert!(result.target_ceiling_fraction <= Decimal::new(3, 2));
    }
    let positive = assessment(0);
    let weak = confidence(&ConfidenceInputs::default());
    let result = sentiment_thresholds(
        &weak,
        &positive,
        &Policy::default(),
        true,
        positive.scored_at,
    )
    .unwrap();
    assert_eq!(result.adjusted_score, 0);
    assert!(!result.entry_score_passes);
}

#[test]
fn missing_stale_future_expired_or_duplicate_evidence_is_unusable() {
    let original = assessment(10);
    let now = original.scored_at;
    let mut cases = vec![];
    let mut value = original.clone();
    value.evidence.clear();
    cases.push(value);
    let mut value = original.clone();
    value.scored_at = now - Duration::seconds(901);
    cases.push(value);
    let mut value = original.clone();
    value.scored_at = now + Duration::seconds(1);
    cases.push(value);
    let mut value = original.clone();
    value.evidence[0].expires_at = now;
    cases.push(value);
    let mut value = original.clone();
    value.evidence[0].observed_at = now - Duration::seconds(86401);
    cases.push(value);
    let mut value = original.clone();
    value.evidence.push(value.evidence[0].clone());
    cases.push(value);
    let mut value = original.clone();
    value.evidence[0].probabilities.negative = Decimal::ONE;
    cases.push(value);
    for value in cases {
        assert!(sentiment_thresholds(&base(), &value, &Policy::default(), false, now).is_err());
    }
    assert!(
        sentiment_thresholds(
            &base(),
            &original,
            &Policy::default(),
            false,
            now + Duration::seconds(900)
        )
        .is_ok()
    );
}

#[test]
fn one_adverse_lineage_cannot_be_diluted_by_positive_copies() {
    let mut value = assessment(80);
    let mut second = value.evidence[0].clone();
    second.item_id = "item-2".into();
    second.lineage_id = "lineage-2".into();
    second.probabilities.negative = Decimal::ZERO;
    second.probabilities.positive = Decimal::ONE;
    value.evidence.push(second);
    let result =
        sentiment_thresholds(&base(), &value, &Policy::default(), false, value.scored_at).unwrap();
    assert_eq!(result.negative_probability_max, Decimal::new(80, 2));
    assert!(!result.entry_score_passes);
    value.evidence[1].lineage_id = "lineage-1".into();
    assert!(
        sentiment_thresholds(&base(), &value, &Policy::default(), false, value.scored_at).is_err()
    );
}

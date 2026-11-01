//! Within-query robust statistics. f64 represents statistical indices only; every
//! supplied value and derived result is checked for finiteness, never used as money.

use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use trady_domain::intelligence::attention::*;
use trady_domain::{Result, require};

fn present(value: &str) -> bool {
    !value.trim().is_empty()
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        sorted[middle - 1] / 2.0 + sorted[middle] / 2.0
    } else {
        sorted[middle]
    }
}

fn valid_value(value: f64, semantics: AttentionSemantics) -> bool {
    value.is_finite()
        && value >= 0.0
        && (semantics != AttentionSemantics::RelativeNormalizedIndex0To100 || value <= 100.0)
}

fn usable(observation: &AttentionObservation, minimum_coverage: f64) -> bool {
    observation
        .value
        .is_some_and(|v| v != 0.0 || observation.zero_is_measured)
        && observation
            .collection_coverage
            .is_some_and(|v| v >= minimum_coverage)
        && !observation.censored_or_low_count
        && matches!(
            observation.agent_coverage,
            AttentionAgentCoverage::HumanOnly | AttentionAgentCoverage::KnownBotsExcluded
        )
}

fn assess_series(
    series: &AttentionSeries,
    input: &AttentionInput,
    at: DateTime<Utc>,
) -> Result<AttentionQueryAssessment> {
    let t = &input.thresholds;
    let mut result = AttentionQueryAssessment {
        source_id: series.source_id.clone(),
        query_id: series.query_id.clone(),
        entity_kind: series.resolution.entity_kind,
        region: series.region.clone(),
        language: series.language.clone(),
        semantics: series.semantics,
        scale_id: series.scale_id.clone(),
        canonical_topic_id: series.resolution.canonical_topic_id.clone(),
        resolution_audit_reference: series.resolution.audit_reference.clone(),
        resolution_audit_version: series.resolution.audit_version.clone(),
        normalization_start: series.normalization_start,
        normalization_end: series.normalization_end,
        scale_available_at: series.scale_available_at,
        status: AttentionStatus::Unknown,
        pattern: AttentionPattern::Unknown,
        historical_median: None,
        historical_mad: None,
        robust_z: None,
        independent_sample_variance: None,
        history_count: 0,
        current_value: None,
        current_collection_coverage: None,
        available_at: None,
        window_start: None,
        window_end: None,
        origin_ids: Vec::new(),
        references: Vec::new(),
        event_comparisons: Vec::new(),
        unavailable_reasons: Vec::new(),
    };
    require(
        present(&series.source_id)
            && present(&series.query_id)
            && present(&series.region)
            && present(&series.language)
            && present(&series.scale_id)
            && series.normalization_start < series.normalization_end
            && series.normalization_end <= at
            && series.scale_available_at <= at
            && series.bucket_seconds > 0
            && (series.semantics != AttentionSemantics::RelativeNormalizedIndex0To100
                || series.normalization_end <= series.scale_available_at),
        "invalid attention series identity/normalization scope",
    )?;
    let resolution = &series.resolution;
    if resolution.issuer_id != input.issuer_id
        || !resolution.owner_verified
        || resolution.ambiguous
        || resolution.generic_or_acronym_only
        || !resolution.redirect_or_name_change_verified
        || !present(&resolution.canonical_topic_id)
        || !present(&resolution.audit_version)
        || !present(&resolution.audit_reference)
        || resolution.audited_at > at
    {
        result
            .unavailable_reasons
            .push("ambiguous/unverified issuer/topic/name/redirect mapping".into());
    }
    let mut observations: Vec<&AttentionObservation> = series.observations.iter().collect();
    observations.sort_by_key(|o| (o.window_start, o.window_end, o.reference.as_str()));
    let mut previous_end = None;
    let mut references = BTreeSet::new();
    for observation in &observations {
        require(
            observation.window_start < observation.window_end
                && observation.window_end <= observation.published_at
                && observation.published_at <= observation.available_at
                && observation.available_at <= observation.ingested_at
                && observation.window_end <= observation.sampled_at
                && observation.sampled_at <= observation.ingested_at
                && observation.available_at >= series.scale_available_at
                && observation.ingested_at <= at
                && present(&observation.reference)
                && present(&observation.origin_id)
                && present(&observation.revision)
                && references.insert(observation.reference.as_str()),
            "invalid attention observation timestamps/reference/revision",
        )?;
        require(
            observation
                .value
                .is_none_or(|v| valid_value(v, observation.semantics))
                && observation
                    .collection_coverage
                    .is_none_or(|v| v.is_finite() && (0.0..=1.0).contains(&v))
                && observation
                    .independent_sample_values
                    .iter()
                    .all(|v| valid_value(*v, observation.semantics)),
            "nonfinite/out-of-range attention value or coverage",
        )?;
        if observation.value.is_none() {
            require(
                observation
                    .unavailable_reason
                    .as_deref()
                    .is_some_and(present),
                "missing attention value requires reason",
            )?;
        }
        if observation.invalidated || observation.superseded_by_reference.is_some() {
            result
                .unavailable_reasons
                .push("deleted/corrected observation invalidates derived series".into());
        }
        let compatible = observation.query_id == series.query_id
            && observation.region == series.region
            && observation.language == series.language
            && observation.scale_id == series.scale_id
            && observation.semantics == series.semantics
            && observation.window_start >= series.normalization_start
            && observation.window_end <= series.normalization_end
            && observation
                .window_end
                .signed_duration_since(observation.window_start)
                .num_seconds()
                == i64::from(series.bucket_seconds)
            && previous_end.is_none_or(|end| end == observation.window_start);
        if !compatible {
            result
                .unavailable_reasons
                .push("incompatible query/region/language/scale/bucket or censored gap".into());
        }
        previous_end = Some(observation.window_end);
    }
    // A missing or invalid record never gets replaced by an invented zero.
    if !result.unavailable_reasons.is_empty() {
        return Ok(result);
    }
    let Some(current) = observations.last().copied() else {
        result
            .unavailable_reasons
            .push("source downtime/no observations".into());
        return Ok(result);
    };
    if at.signed_duration_since(current.window_end).num_seconds() > i64::from(t.maximum_age_seconds)
    {
        result
            .unavailable_reasons
            .push("latest attention bucket is stale".into());
        return Ok(result);
    }
    if !usable(current, t.minimum_coverage) {
        result.unavailable_reasons.push(
            "current value has low/unknown coverage, low-count noise, bots or unmeasured zero"
                .into(),
        );
        return Ok(result);
    }
    let history: Vec<&AttentionObservation> = observations[..observations.len() - 1]
        .iter()
        .rev()
        .take(t.maximum_history as usize)
        .copied()
        .collect();
    let values: Vec<f64> = history
        .iter()
        .filter(|o| usable(o, t.minimum_coverage))
        .filter_map(|o| o.value)
        .collect();
    result.history_count = values.len() as u32;
    if values.len() < t.minimum_history as usize {
        result
            .unavailable_reasons
            .push("insufficient uncensored compatible rolling history".into());
        return Ok(result);
    }
    if values.len() != history.len() {
        result
            .unavailable_reasons
            .push("partial history: low-count, bots, missing coverage or values excluded".into());
    }
    let center = median(&values);
    let deviations: Vec<f64> = values.iter().map(|v| (v - center).abs()).collect();
    require(
        deviations.iter().all(|v| v.is_finite()),
        "attention deviation overflow",
    )?;
    let mad = median(&deviations);
    require(
        center.is_finite() && mad.is_finite(),
        "attention median/MAD overflow",
    )?;
    result.historical_median = Some(center);
    result.historical_mad = Some(mad);
    result.current_value = current.value;
    result.current_collection_coverage = current.collection_coverage;
    result.available_at = Some(current.available_at);
    result.window_start = Some(current.window_start);
    result.window_end = Some(current.window_end);
    let mut origin_ids: BTreeSet<String> = history
        .iter()
        .filter(|o| usable(o, t.minimum_coverage))
        .map(|o| o.origin_id.clone())
        .collect();
    origin_ids.insert(current.origin_id.clone());
    result.origin_ids = origin_ids.into_iter().collect();
    result.references = history
        .iter()
        .filter(|o| usable(o, t.minimum_coverage))
        .map(|o| o.reference.clone())
        .chain(std::iter::once(current.reference.clone()))
        .collect();
    if mad <= 0.0 {
        result
            .unavailable_reasons
            .push("zero historical MAD: normalized surprise is undefined".into());
        result.status = AttentionStatus::Partial;
        return Ok(result);
    }
    let score = |value: f64| (value - center) / mad * 0.674_489_750_196_081_7;
    let z = score(current.value.expect("usable current has a value"));
    require(z.is_finite(), "attention robust score overflow")?;
    result.robust_z = Some(z);
    let sustained = observations
        .iter()
        .rev()
        .take(t.sustained_buckets as usize)
        .count()
        == t.sustained_buckets as usize
        && observations
            .iter()
            .rev()
            .take(t.sustained_buckets as usize)
            .all(|o| {
                usable(o, t.minimum_coverage)
                    && o.value
                        .is_some_and(|v| score(v).is_finite() && score(v) >= t.spike_robust_z)
            });
    let prior_high = observations
        .iter()
        .rev()
        .nth(1)
        .filter(|o| usable(o, t.minimum_coverage))
        .and_then(|o| o.value)
        .is_some_and(|v| score(v).is_finite() && score(v) >= t.spike_robust_z && score(v) > z);
    result.pattern = if sustained {
        AttentionPattern::Sustained
    } else if z >= t.spike_robust_z {
        AttentionPattern::Spike
    } else if prior_high {
        AttentionPattern::Decay
    } else {
        AttentionPattern::Ordinary
    };
    if current.independent_sample_values.len() >= 2 {
        // Welford avoids a large sum; all intermediate values remain checked.
        let mut mean = 0.0;
        let mut squared = 0.0;
        for (index, value) in current.independent_sample_values.iter().enumerate() {
            let delta = value - mean;
            mean += delta / (index + 1) as f64;
            squared += delta * (value - mean);
            require(
                mean.is_finite() && squared.is_finite(),
                "attention repeated-sample variance overflow",
            )?;
        }
        let variance = squared / (current.independent_sample_values.len() - 1) as f64;
        require(
            variance.is_finite() && variance >= 0.0,
            "invalid attention sample variance",
        )?;
        result.independent_sample_variance = Some(variance);
    } else {
        result
            .unavailable_reasons
            .push("independent repeated-sample variance unavailable".into());
    }
    for event in &input.verified_events {
        let timing = if event.first_public_at <= current.window_start {
            AttentionEventTiming::EventAlreadyPublic
        } else if current.window_end < event.first_public_at
            && current.available_at < event.first_public_at
        {
            AttentionEventTiming::AvailableBeforeEvent
        } else {
            AttentionEventTiming::Indeterminate
        };
        result.event_comparisons.push(AttentionEventComparison {
            event_id: event.event_id.clone(),
            timing,
            reference: event.reference.clone(),
        });
    }
    result.status = if result.unavailable_reasons.is_empty() {
        AttentionStatus::Available
    } else {
        AttentionStatus::Partial
    };
    Ok(result)
}

pub fn assess(input: &AttentionInput, at: DateTime<Utc>) -> Result<AttentionAssessment> {
    let t = &input.thresholds;
    require(
        present(&input.issuer_id)
            && present(&t.version)
            && t.minimum_history >= 3
            && t.maximum_history >= t.minimum_history
            && t.sustained_buckets >= 2
            && t.sustained_buckets <= t.maximum_history
            && t.maximum_age_seconds > 0
            && t.minimum_coverage.is_finite()
            && t.minimum_coverage > 0.0
            && t.minimum_coverage <= 1.0
            && t.spike_robust_z.is_finite()
            && t.spike_robust_z > 0.0
            && input
                .coordination_warning_reference
                .as_deref()
                .is_none_or(present),
        "invalid attention identity/thresholds",
    )?;
    let mut event_ids = BTreeSet::new();
    for event in &input.verified_events {
        require(
            present(&event.event_id)
                && present(&event.reference)
                && event_ids.insert(&event.event_id)
                && event.first_public_at <= event.verified_available_at
                && event.verified_available_at <= at,
            "unverified/future/duplicate corporate event",
        )?;
    }
    let mut query_ids = BTreeSet::new();
    let mut queries = Vec::new();
    for series in &input.series {
        require(
            query_ids.insert(&series.query_id),
            "duplicate attention query identity",
        )?;
        queries.push(assess_series(series, input, at)?);
    }
    let known_count = queries.iter().filter(|q| q.robust_z.is_some()).count();
    let status = if known_count == 0 {
        AttentionStatus::Unknown
    } else if queries
        .iter()
        .all(|q| q.status == AttentionStatus::Available)
    {
        AttentionStatus::Available
    } else {
        AttentionStatus::Partial
    };
    let mut concordance = Vec::new();
    for (index, left) in queries.iter().enumerate() {
        for right in &queries[index + 1..] {
            if left.robust_z.is_some()
                && right.robust_z.is_some()
                && left.region == right.region
                && left.entity_kind == right.entity_kind
                && left.window_start == right.window_start
                && left.window_end == right.window_end
            {
                concordance.push(AttentionConcordance {
                    query_ids: vec![left.query_id.clone(), right.query_id.clone()],
                    agreeing: left.pattern == right.pattern,
                });
            }
        }
    }
    let deduplicated_origin_ids = queries
        .iter()
        .flat_map(|q| q.origin_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(AttentionAssessment {
        issuer_id: input.issuer_id.clone(), assessed_at: at, feature_version: t.version.clone(), status,
        queries, concordance, deduplicated_origin_ids,
        possible_publicity_manipulation: input.coordination_warning_reference.is_some(),
        coordination_warning_reference: input.coordination_warning_reference.clone(),
        limitations: vec![
            "attention is neither sentiment, absolute searches (for a relative index), nor expected return".into(),
            "pattern concordance and shared origins are correlated observations, not independent confirmations".into(),
        ],
    })
}

#[cfg(test)]
mod attention_acceptance {
    use super::*;
    use chrono::Duration;

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-31T14:00:00Z")
            .unwrap()
            .to_utc()
    }
    fn fixture() -> AttentionInput {
        let end = at() - Duration::hours(1);
        let start = end - Duration::days(8);
        AttentionInput {
            issuer_id: "issuer".into(),
            thresholds: AttentionThresholds {
                version: "fixture-v1".into(),
                minimum_history: 5,
                maximum_history: 20,
                minimum_coverage: 0.8,
                spike_robust_z: 3.0,
                sustained_buckets: 2,
                maximum_age_seconds: 7200,
            },
            series: vec![AttentionSeries {
                source_id: "reviewed-import".into(),
                query_id: "issuer-topic".into(),
                resolution: AttentionResolution {
                    issuer_id: "issuer".into(),
                    canonical_topic_id: "resolved-topic".into(),
                    entity_kind: AttentionEntityKind::Issuer,
                    audit_version: "map-v1".into(),
                    audit_reference: "entity-review".into(),
                    audited_at: start - Duration::days(1),
                    owner_verified: true,
                    ambiguous: false,
                    generic_or_acronym_only: false,
                    redirect_or_name_change_verified: true,
                },
                region: "US".into(),
                language: "en".into(),
                scale_id: "one-window-vintage".into(),
                semantics: AttentionSemantics::RelativeNormalizedIndex0To100,
                normalization_start: start,
                normalization_end: end,
                scale_available_at: end + Duration::minutes(2),
                bucket_seconds: 86400,
                observations: [10.0, 12.0, 9.0, 11.0, 10.0, 13.0, 8.0, 83.0]
                    .into_iter()
                    .enumerate()
                    .map(|(i, value)| AttentionObservation {
                        query_id: "issuer-topic".into(),
                        region: "US".into(),
                        language: "en".into(),
                        scale_id: "one-window-vintage".into(),
                        semantics: AttentionSemantics::RelativeNormalizedIndex0To100,
                        window_start: start + Duration::days(i as i64),
                        window_end: start + Duration::days(i as i64 + 1),
                        published_at: end + Duration::minutes(1),
                        available_at: end + Duration::minutes(2),
                        ingested_at: end + Duration::minutes(3),
                        sampled_at: end + Duration::minutes(2),
                        value: Some(value),
                        unavailable_reason: None,
                        collection_coverage: Some(1.0),
                        censored_or_low_count: false,
                        zero_is_measured: false,
                        invalidated: false,
                        superseded_by_reference: None,
                        agent_coverage: AttentionAgentCoverage::KnownBotsExcluded,
                        revision: "retrieval-v1".into(),
                        reference: format!("ref{i}"),
                        origin_id: format!("origin{i}"),
                        independent_sample_values: if i == 7 { vec![82.0, 84.0] } else { vec![] },
                    })
                    .collect(),
            }],
            verified_events: vec![],
            coordination_warning_reference: None,
        }
    }

    #[test]
    fn robust_within_query_median_mad_and_repeated_sample_variance() {
        let result = assess(&fixture(), at()).unwrap();
        let query = &result.queries[0];
        assert_eq!(query.historical_median, Some(10.0));
        assert_eq!(query.historical_mad, Some(1.0));
        assert!((query.robust_z.unwrap() - 73.0 * 0.674_489_750_196_081_7).abs() < 1e-10);
        assert_eq!(query.independent_sample_variance, Some(2.0));
        assert_eq!(query.pattern, AttentionPattern::Spike);
        assert_eq!(query.status, AttentionStatus::Available);
        assert_eq!(
            query.semantics,
            AttentionSemantics::RelativeNormalizedIndex0To100
        );
        assert!(result.limitations[0].contains("neither sentiment"));
    }

    #[test]
    fn incompatible_query_scale_region_and_ambiguous_entity_are_unknown() {
        for mode in 0..5 {
            let mut input = fixture();
            match mode {
                0 => input.series[0].observations[0].scale_id = "separately-rescaled".into(),
                1 => input.series[0].observations[0].region = "GB".into(),
                2 => input.series[0].observations[0].query_id = "CAT-generic".into(),
                3 => input.series[0].resolution.ambiguous = true,
                _ => input.series[0].resolution.redirect_or_name_change_verified = false,
            }
            let result = assess(&input, at()).unwrap();
            assert_eq!(result.status, AttentionStatus::Unknown);
            assert!(result.queries[0].robust_z.is_none());
        }
    }

    #[test]
    fn bots_low_count_zero_deletion_and_corrections_never_become_zero_features() {
        for mode in 0..7 {
            let mut input = fixture();
            let current = input.series[0].observations.last_mut().unwrap();
            match mode {
                0 => current.agent_coverage = AttentionAgentCoverage::IncludesAutomated,
                1 => current.censored_or_low_count = true,
                2 => current.value = Some(0.0),
                3 => current.invalidated = true,
                4 => current.superseded_by_reference = Some("revision-v2".into()),
                5 => current.collection_coverage = Some(0.1),
                _ => {
                    current.value = None;
                    current.unavailable_reason = Some("source downtime".into());
                }
            }
            let result = assess(&input, at()).unwrap();
            assert_eq!(result.status, AttentionStatus::Unknown);
            assert_eq!(result.queries[0].robust_z, None);
            assert!(!result.queries[0].unavailable_reasons.is_empty());
        }
    }

    #[test]
    fn nonfinite_values_out_of_scale_and_backdated_rescaling_are_rejected() {
        for value in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
            let mut input = fixture();
            input.series[0].observations[0].value = Some(value);
            assert!(assess(&input, at()).is_err());
        }
        let mut input = fixture();
        input.series[0].observations[0].available_at = input.series[0].observations[0].window_end;
        assert!(assess(&input, at()).is_err());
        input = fixture();
        input.thresholds.spike_robust_z = f64::NAN;
        assert!(assess(&input, at()).is_err());
    }

    #[test]
    fn zero_mad_and_insufficient_history_are_explicit() {
        let mut input = fixture();
        for observation in input.series[0].observations.iter_mut().take(7) {
            observation.value = Some(10.0);
        }
        let query = assess(&input, at()).unwrap().queries.remove(0);
        assert_eq!(query.historical_mad, Some(0.0));
        assert_eq!(query.robust_z, None);
        assert_eq!(query.pattern, AttentionPattern::Unknown);
        assert!(
            query
                .unavailable_reasons
                .iter()
                .any(|r| r.contains("undefined"))
        );
        input = fixture();
        for observation in input.series[0].observations.iter_mut().take(7) {
            observation.collection_coverage = None;
        }
        assert_eq!(
            assess(&input, at()).unwrap().status,
            AttentionStatus::Unknown
        );
    }

    #[test]
    fn verified_event_timing_uses_actual_attention_availability() {
        let mut input = fixture();
        let start = input.series[0].observations.last().unwrap().window_start;
        input.verified_events = vec![
            VerifiedAttentionEvent {
                event_id: "old-news".into(),
                first_public_at: start - Duration::hours(1),
                verified_available_at: start,
                reference: "original-filing".into(),
            },
            VerifiedAttentionEvent {
                event_id: "later-event".into(),
                first_public_at: at() - Duration::minutes(30),
                verified_available_at: at() - Duration::minutes(29),
                reference: "corporate-event".into(),
            },
        ];
        let result = assess(&input, at()).unwrap();
        assert_eq!(
            result.queries[0].event_comparisons[0].timing,
            AttentionEventTiming::EventAlreadyPublic
        );
        assert_eq!(
            result.queries[0].event_comparisons[1].timing,
            AttentionEventTiming::AvailableBeforeEvent
        );
        let current = input.series[0].observations.last_mut().unwrap();
        current.available_at = at() - Duration::minutes(10);
        current.ingested_at = at();
        let result = assess(&input, at()).unwrap();
        assert_eq!(
            result.queries[0].event_comparisons[1].timing,
            AttentionEventTiming::Indeterminate
        );
    }

    #[test]
    fn sustained_decay_and_shared_viral_origins_preserve_semantics() {
        let mut input = fixture();
        input.series[0].observations[6].value = Some(80.0);
        assert_eq!(
            assess(&input, at()).unwrap().queries[0].pattern,
            AttentionPattern::Sustained
        );
        input.series[0].observations[7].value = Some(12.0);
        assert_eq!(
            assess(&input, at()).unwrap().queries[0].pattern,
            AttentionPattern::Decay
        );
        for o in &mut input.series[0].observations {
            o.origin_id = "one-viral-story".into();
        }
        input.coordination_warning_reference = Some("coordination-record".into());
        let result = assess(&input, at()).unwrap();
        assert_eq!(result.deduplicated_origin_ids, ["one-viral-story"]);
        assert!(result.possible_publicity_manipulation);
        let mut json = serde_json::to_value(input).unwrap();
        json["sentiment"] = 1.into();
        assert!(serde_json::from_value::<AttentionInput>(json).is_err());
    }

    #[test]
    fn brand_and_issuer_queries_are_not_collapsed_into_one_confirmation() {
        let mut input = fixture();
        let mut brand = input.series[0].clone();
        brand.query_id = "brand-topic".into();
        brand.resolution.entity_kind = AttentionEntityKind::Brand;
        for observation in &mut brand.observations {
            observation.query_id = "brand-topic".into();
        }
        input.series.push(brand);
        let result = assess(&input, at()).unwrap();
        assert_eq!(result.queries.len(), 2);
        assert!(result.concordance.is_empty());
        assert_eq!(result.deduplicated_origin_ids.len(), 8);
    }
}

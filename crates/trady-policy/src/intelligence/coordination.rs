//! Pure, suppression-only behavioral diagnostics over authorized metadata.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::intelligence::coordination::*;
use trady_domain::{Error, Result, require};

use crate::{div, sub};

fn fraction(numerator: u64, denominator: u64) -> Result<Decimal> {
    div(Decimal::from(numerator), Decimal::from(denominator))
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn representative(parents: &mut [usize], index: usize) -> usize {
    let mut root = index;
    while parents[root] != root {
        root = parents[root];
    }
    let mut cursor = index;
    while parents[cursor] != cursor {
        let next = parents[cursor];
        parents[cursor] = root;
        cursor = next;
    }
    root
}

/// Conservative content/URL/author/disclosure linkage prevents a campaign from
/// claiming independence simply by publishing copies as separate original posts.
fn independent_groups(
    events: &BTreeMap<&str, &PromotionEvent>,
    origins: &BTreeSet<&str>,
) -> (u64, u64) {
    let roots: Vec<_> = origins.iter().map(|id| events[id]).collect();
    let mut parents: Vec<usize> = (0..roots.len()).collect();
    let mut seen = BTreeMap::new();
    for (index, root) in roots.iter().enumerate() {
        let mut keys = vec![
            (0, root.lexical_cluster.as_str(), ""),
            (1, root.source_id.as_str(), root.author_hash.as_str()),
        ];
        if let Some(url) = root.outbound_url_hash.as_deref() {
            keys.push((2, url, ""));
        }
        let disclosure = if root.kind == PromotionKind::VerifiedDisclosure {
            Some(root.event_id.as_str())
        } else {
            root.verified_disclosure_id.as_deref()
        };
        if let Some(id) = disclosure {
            keys.push((3, id, ""));
        }
        for key in keys {
            if let Some(previous) = seen.insert(key, index) {
                let a = representative(&mut parents, index);
                let b = representative(&mut parents, previous);
                parents[a.max(b)] = a.min(b);
            }
        }
    }
    let groups: BTreeSet<_> = (0..roots.len())
        .map(|index| representative(&mut parents, index))
        .collect();
    let authors: BTreeSet<_> = groups
        .iter()
        .map(|index| {
            (
                roots[*index].source_id.as_str(),
                roots[*index].author_hash.as_str(),
            )
        })
        .collect();
    (groups.len() as u64, authors.len() as u64)
}

/// Thresholds are research configuration; a multiplier can only suppress evidence.
pub fn assess(input: &CoordinationInput, at: DateTime<Utc>) -> Result<CoordinationAssessment> {
    let t = &input.thresholds;
    require(
        nonempty(&input.issuer_id)
            && nonempty(&input.claim_cluster)
            && nonempty(&input.source_window_key)
            && nonempty(&input.event_category)
            && nonempty(&t.version)
            && input.window_start < input.window_end
            && input.window_end <= at
            && input.expires_at > input.window_end,
        "invalid coordination identity/window/version",
    )?;
    require(
        t.minimum_coverage > Decimal::ZERO
            && t.minimum_coverage <= Decimal::ONE
            && t.maximum_baseline_coverage_difference >= Decimal::ZERO
            && t.maximum_baseline_coverage_difference <= Decimal::ONE
            && t.minimum_baseline_windows > 0
            && t.minimum_behavioral_posts > 0
            && t.burst_ratio > Decimal::ONE
            && t.concentration_fraction > Decimal::ZERO
            && t.concentration_fraction <= Decimal::ONE
            && t.synchronization_seconds > 0
            && t.review_weight >= Decimal::ZERO
            && t.review_weight <= t.elevated_weight
            && t.elevated_weight <= Decimal::ONE,
        "invalid suppression-only coordination thresholds",
    )?;
    let mut result = CoordinationAssessment {
        issuer_id: input.issuer_id.clone(),
        claim_cluster: input.claim_cluster.clone(),
        assessed_at: at,
        window_start: input.window_start,
        window_end: input.window_end,
        expires_at: input.expires_at,
        threshold_version: t.version.clone(),
        calibration_reference: t.calibration_reference.clone(),
        source_coverage: input.coverage.clone(),
        risk_of_unreliable_attention: CoordinationTier::Unavailable,
        evidence_weight_multiplier: None,
        unique_posts: 0,
        reposts: 0,
        independent_origin_count: None,
        independent_author_count: None,
        near_duplicate_fraction: None,
        shared_url_fraction: None,
        synchronized_fraction: None,
        source_bursts: Vec::new(),
        supporting_origin_ids: Vec::new(),
        preserved_disclosure_references: Vec::new(),
        contributions: Vec::new(),
        counterexamples: Vec::new(),
        feature_unavailable_reasons: Vec::new(),
        unavailable_reasons: Vec::new(),
    };
    if at >= input.expires_at {
        result
            .unavailable_reasons
            .push("assessment window expired".into());
    }
    if !t.calibration_reference.as_deref().is_some_and(nonempty) {
        require(
            t.calibration_unavailable_reason
                .as_deref()
                .is_some_and(nonempty),
            "missing coordination calibration needs a reason",
        )?;
        result
            .unavailable_reasons
            .push("unreviewed calibration/support".into());
    }
    if let Some(market) = &input.market_deviation {
        require(
            market.observed_at >= input.window_start
                && market.observed_at <= input.window_end
                && market.observed_at <= market.available_at
                && market.available_at <= at
                && market.turnover_ratio >= Decimal::ZERO
                && nonempty(&market.reference),
            "invalid controlled market deviation",
        )?;
        result.counterexamples.push(
            "market/sector controlled deviation is contemporaneous correlation, not intent".into(),
        );
    } else {
        require(
            input
                .market_deviation_unavailable_reason
                .as_deref()
                .is_some_and(nonempty),
            "missing market deviation needs a reason",
        )?;
        result.feature_unavailable_reasons.push(format!(
            "market deviation: {}",
            input
                .market_deviation_unavailable_reason
                .as_deref()
                .unwrap_or_default()
        ));
    }

    let mut events = BTreeMap::new();
    for event in &input.events {
        require(
            nonempty(&event.event_id)
                && nonempty(&event.source_id)
                && nonempty(&event.author_hash)
                && nonempty(&event.origin_event_id)
                && nonempty(&event.lexical_cluster)
                && nonempty(&event.reference)
                && event.published_at >= input.window_start
                && event.published_at < input.window_end
                && event.published_at <= event.first_available_at
                && event.first_available_at <= event.ingested_at
                && event.ingested_at <= at
                && event.outbound_url_hash.as_deref().is_none_or(nonempty),
            "invalid promotion event identity/timestamps",
        )?;
        require(
            matches!(event.kind, PromotionKind::Repost | PromotionKind::Quotation)
                == event.parent_event_id.is_some(),
            "repost/quotation requires a parent; primary events cannot claim one",
        )?;
        if let Some(previous) = events.insert(event.event_id.as_str(), event) {
            require(previous == event, "conflicting duplicate promotion event")?;
        }
    }
    result.unique_posts = events.len() as u64;
    if events.is_empty() {
        result
            .unavailable_reasons
            .push("no claim observations; absence is not a normal reliability signal".into());
    }
    result.reposts = events
        .values()
        .filter(|e| e.parent_event_id.is_some())
        .count() as u64;
    let tombstones: BTreeSet<&str> = input
        .invalidated_origin_ids
        .iter()
        .map(String::as_str)
        .collect();
    let mut origins = BTreeSet::new();
    let mut behavioral = Vec::new();
    let mut covered_counts: BTreeMap<&str, u64> = BTreeMap::new();
    for event in events.values() {
        *covered_counts.entry(&event.source_id).or_default() += 1;
        let mut cursor = *event;
        let mut visited = BTreeSet::new();
        let mut invalidated = false;
        let mut incomplete = false;
        loop {
            require(
                visited.insert(cursor.event_id.as_str()),
                "cyclic promotion lineage",
            )?;
            invalidated |= cursor.invalidated || tombstones.contains(cursor.event_id.as_str());
            let Some(parent_id) = cursor.parent_event_id.as_deref() else {
                break;
            };
            let Some(parent) = events.get(parent_id) else {
                incomplete = true;
                break;
            };
            require(
                parent.published_at <= cursor.published_at,
                "repost predates its origin",
            )?;
            cursor = parent;
        }
        if incomplete {
            result
                .unavailable_reasons
                .push("unresolved repost/quotation ancestor".into());
            continue;
        }
        require(
            cursor.event_id == event.origin_event_id,
            "spoofed promotion origin",
        )?;
        if invalidated {
            result
                .unavailable_reasons
                .push("deleted/corrected origin retracts dependent features".into());
            continue;
        }
        origins.insert(cursor.event_id.as_str());
        let disclosure = if let Some(id) = &event.verified_disclosure_id {
            let disclosure = events
                .get(id.as_str())
                .ok_or_else(|| Error("unknown verified disclosure".into()))?;
            require(
                disclosure.kind == PromotionKind::VerifiedDisclosure,
                "unverified disclosure exemption",
            )?;
            require(
                disclosure.published_at <= event.published_at,
                "discussion cannot claim a disclosure published later",
            )?;
            if disclosure.invalidated || tombstones.contains(id.as_str()) {
                result
                    .unavailable_reasons
                    .push("linked disclosure invalidated".into());
                continue;
            }
            Some(*disclosure)
        } else if cursor.kind == PromotionKind::VerifiedDisclosure {
            Some(cursor)
        } else {
            None
        };
        if let Some(disclosure) = disclosure {
            result
                .preserved_disclosure_references
                .push(disclosure.reference.clone());
        } else {
            behavioral.push(*event);
        }
    }
    let (origin_count, author_count) = independent_groups(&events, &origins);
    result.independent_origin_count = Some(origin_count);
    result.independent_author_count = Some(author_count);
    result.supporting_origin_ids = origins.iter().map(|s| (*s).to_owned()).collect();
    result.preserved_disclosure_references.sort();
    result.preserved_disclosure_references.dedup();
    if !result.preserved_disclosure_references.is_empty() {
        result.counterexamples.push(
            "verified primary disclosure and linked organic discussion remain available".into(),
        );
    }

    let mut sources = BTreeSet::new();
    let mut burst_flag = false;
    for coverage in &input.coverage {
        require(
            nonempty(&coverage.source_id) && sources.insert(coverage.source_id.as_str()),
            "duplicate/empty coordination coverage source",
        )?;
        require(
            covered_counts
                .get(coverage.source_id.as_str())
                .copied()
                .unwrap_or(0)
                <= coverage.covered_records,
            "event count exceeds source collection count",
        )?;
        let coverage_fraction = if let Some(expected) = coverage.expected_records {
            require(
                expected > 0 && coverage.covered_records <= expected,
                "invalid source coverage denominator",
            )?;
            Some(fraction(coverage.covered_records, expected)?)
        } else {
            require(
                coverage.unavailable_reason.as_deref().is_some_and(nonempty),
                "unknown coverage needs reason",
            )?;
            None
        };
        if coverage.censored || coverage_fraction.is_none_or(|v| v < t.minimum_coverage) {
            result.unavailable_reasons.push(format!(
                "low/unknown/censored source coverage: {}",
                coverage.source_id
            ));
        }
        let mut burst = SourceBurst {
            source_id: coverage.source_id.clone(),
            ratio: None,
            unavailable_reason: None,
            baseline_reference: None,
        };
        if let Some(baseline) = &coverage.baseline {
            require(
                baseline.source_id == coverage.source_id
                    && baseline.expected_posts > Decimal::ZERO
                    && baseline.collection_coverage > Decimal::ZERO
                    && baseline.collection_coverage <= Decimal::ONE
                    && baseline.published_at <= baseline.available_at
                    && baseline.available_at <= input.window_start
                    && nonempty(&baseline.reference),
                "invalid source-specific posting baseline",
            )?;
            let comparable = baseline.source_window_key == input.source_window_key
                && baseline.event_category == input.event_category
                && i64::from(baseline.bucket_seconds)
                    == input
                        .window_end
                        .signed_duration_since(input.window_start)
                        .num_seconds()
                && baseline.historical_windows >= t.minimum_baseline_windows
                && coverage_fraction.is_some_and(|v| {
                    sub(
                        v.max(baseline.collection_coverage),
                        v.min(baseline.collection_coverage),
                    )
                    .is_ok_and(|d| d <= t.maximum_baseline_coverage_difference)
                });
            if comparable {
                let count = covered_counts
                    .get(coverage.source_id.as_str())
                    .copied()
                    .unwrap_or(0);
                let ratio = div(Decimal::from(count), baseline.expected_posts)?;
                burst_flag |= ratio >= t.burst_ratio;
                burst.ratio = Some(ratio);
                burst.baseline_reference = Some(baseline.reference.clone());
            } else {
                burst.unavailable_reason =
                    Some("incompatible source/time/category/coverage baseline".into());
            }
        } else {
            require(
                coverage
                    .baseline_unavailable_reason
                    .as_deref()
                    .is_some_and(nonempty),
                "missing posting baseline needs reason",
            )?;
            burst.unavailable_reason = coverage.baseline_unavailable_reason.clone();
        }
        result.source_bursts.push(burst);
    }
    if sources.is_empty() || covered_counts.keys().any(|s| !sources.contains(s)) {
        result
            .unavailable_reasons
            .push("source coverage missing".into());
    }
    if result.source_bursts.iter().any(|b| b.ratio.is_none()) {
        result
            .unavailable_reasons
            .push("comparable source-window burst unavailable".into());
    }
    if !behavioral.is_empty() {
        let denominator = behavioral.len() as u64;
        let mut lexical = BTreeMap::new();
        let mut urls = BTreeMap::new();
        let mut times = Vec::new();
        for event in &behavioral {
            *lexical
                .entry(event.lexical_cluster.as_str())
                .or_insert(0_u64) += 1;
            if let Some(url) = &event.outbound_url_hash {
                *urls.entry(url.as_str()).or_insert(0_u64) += 1;
            }
            times.push(event.published_at);
        }
        result.near_duplicate_fraction = Some(fraction(
            lexical.values().copied().max().unwrap_or(0),
            denominator,
        )?);
        result.shared_url_fraction = if urls.is_empty() {
            None
        } else {
            Some(fraction(
                urls.values().copied().max().unwrap_or(0),
                denominator,
            )?)
        };
        times.sort();
        let mut left = 0;
        let mut largest = 0;
        for right in 0..times.len() {
            while times[right]
                .signed_duration_since(times[left])
                .num_seconds()
                >= i64::from(t.synchronization_seconds)
            {
                left += 1;
            }
            largest = largest.max(right - left + 1);
        }
        result.synchronized_fraction = Some(fraction(largest as u64, denominator)?);
        if urls.is_empty() {
            result
                .feature_unavailable_reasons
                .push("outbound URL metadata absent".into());
        }
    } else {
        result
            .feature_unavailable_reasons
            .push("no non-disclosure behavioral posts for concentration statistics".into());
    }
    if !result.unavailable_reasons.is_empty() {
        result.unavailable_reasons.sort();
        result.unavailable_reasons.dedup();
        // Do not expose stale derived values after a deletion or incomplete lineage.
        result.near_duplicate_fraction = None;
        result.shared_url_fraction = None;
        result.synchronized_fraction = None;
        result.supporting_origin_ids.clear();
        result.independent_origin_count = None;
        result.independent_author_count = None;
        for burst in &mut result.source_bursts {
            burst.ratio = None;
            if burst.unavailable_reason.is_none() {
                burst.unavailable_reason = Some("aggregate invalidated/unavailable".into());
            }
        }
        return Ok(result);
    }
    let supported = behavioral.len() as u64 >= t.minimum_behavioral_posts;
    let mut flags = 0_u8;
    for (name, active) in [
        ("within-source posting burst", burst_flag && supported),
        (
            "concentrated repeated claim",
            supported
                && result
                    .near_duplicate_fraction
                    .is_some_and(|v| v >= t.concentration_fraction),
        ),
        (
            "concentrated outbound URL",
            supported
                && result
                    .shared_url_fraction
                    .is_some_and(|v| v >= t.concentration_fraction),
        ),
        (
            "synchronized posting",
            supported
                && result
                    .synchronized_fraction
                    .is_some_and(|v| v >= t.concentration_fraction),
        ),
    ] {
        if active {
            flags += 1;
            result.contributions.push(name.into());
        }
    }
    result.risk_of_unreliable_attention = match flags {
        3.. => CoordinationTier::ReviewRequired,
        2 => CoordinationTier::Elevated,
        _ => CoordinationTier::Normal,
    };
    result.evidence_weight_multiplier = Some(match result.risk_of_unreliable_attention {
        CoordinationTier::ReviewRequired => t.review_weight,
        CoordinationTier::Elevated => t.elevated_weight,
        _ => Decimal::ONE,
    });
    result.counterexamples.push("organic emergencies, syndication and publicity can produce the same behavior; no inference of culpability".into());
    Ok(result)
}

#[cfg(test)]
mod coordination_acceptance {
    use super::*;
    use chrono::Duration;

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-31T14:00:00Z")
            .unwrap()
            .to_utc()
    }
    fn fixture() -> CoordinationInput {
        let start = at() - Duration::minutes(20);
        CoordinationInput {
            issuer_id: "issuer".into(),
            claim_cluster: "claim".into(),
            window_start: start,
            window_end: start + Duration::minutes(10),
            expires_at: at() + Duration::minutes(10),
            source_window_key: "saturday-13".into(),
            event_category: "ordinary".into(),
            events: (0..10)
                .map(|i| PromotionEvent {
                    event_id: format!("p{i}"),
                    source_id: "wire".into(),
                    author_hash: format!("scoped_hash_{i}"),
                    kind: if i == 0 {
                        PromotionKind::Original
                    } else {
                        PromotionKind::Repost
                    },
                    parent_event_id: if i == 0 { None } else { Some("p0".into()) },
                    origin_event_id: "p0".into(),
                    lexical_cluster: "same-claim".into(),
                    outbound_url_hash: Some("same-url".into()),
                    verified_disclosure_id: None,
                    published_at: start + Duration::seconds(i),
                    first_available_at: start + Duration::seconds(20 + i),
                    ingested_at: start + Duration::seconds(30 + i),
                    reference: format!("ref{i}"),
                    invalidated: false,
                })
                .collect(),
            coverage: vec![CoordinationCoverage {
                source_id: "wire".into(),
                covered_records: 10,
                expected_records: Some(10),
                unavailable_reason: None,
                censored: false,
                baseline: Some(PostingBaseline {
                    source_id: "wire".into(),
                    source_window_key: "saturday-13".into(),
                    event_category: "ordinary".into(),
                    bucket_seconds: 600,
                    expected_posts: Decimal::from(2),
                    collection_coverage: Decimal::ONE,
                    historical_windows: 12,
                    published_at: start - Duration::hours(1),
                    available_at: start - Duration::minutes(59),
                    reference: "historical".into(),
                }),
                baseline_unavailable_reason: None,
            }],
            invalidated_origin_ids: vec![],
            market_deviation: None,
            market_deviation_unavailable_reason: Some("no authorized price feed".into()),
            thresholds: CoordinationThresholds {
                version: "fixture-v1".into(),
                minimum_coverage: Decimal::new(8, 1),
                maximum_baseline_coverage_difference: Decimal::new(1, 1),
                minimum_baseline_windows: 5,
                minimum_behavioral_posts: 5,
                burst_ratio: Decimal::from(3),
                concentration_fraction: Decimal::new(7, 1),
                synchronization_seconds: 30,
                elevated_weight: Decimal::new(75, 2),
                review_weight: Decimal::new(5, 1),
                calibration_reference: Some("behavior-label-review".into()),
                calibration_unavailable_reason: None,
            },
        }
    }

    #[test]
    fn reposts_and_repeated_ingestion_never_increase_independent_confirmation() {
        let mut input = fixture();
        input.events.push(input.events[1].clone());
        let result = assess(&input, at()).unwrap();
        assert_eq!(result.unique_posts, 10);
        assert_eq!(result.reposts, 9);
        assert_eq!(result.independent_origin_count, Some(1));
        assert_eq!(result.independent_author_count, Some(1));
        assert_eq!(
            result.risk_of_unreliable_attention,
            CoordinationTier::ReviewRequired
        );
        assert_eq!(result.evidence_weight_multiplier, Some(Decimal::new(5, 1)));
        assert!(!result.contributions.is_empty());
    }

    #[test]
    fn verified_disclosure_preserves_organic_simultaneous_discussion() {
        let mut input = fixture();
        input.events[0].kind = PromotionKind::VerifiedDisclosure;
        for i in 1..input.events.len() {
            input.events[i].kind = PromotionKind::Original;
            input.events[i].parent_event_id = None;
            input.events[i].origin_event_id = input.events[i].event_id.clone();
            input.events[i].verified_disclosure_id = Some("p0".into());
        }
        let result = assess(&input, at()).unwrap();
        assert_eq!(
            result.risk_of_unreliable_attention,
            CoordinationTier::Normal
        );
        assert_eq!(result.evidence_weight_multiplier, Some(Decimal::ONE));
        assert_eq!(result.preserved_disclosure_references, ["ref0"]);
        assert!(result.contributions.is_empty());
    }

    #[test]
    fn low_coverage_censoring_and_incompatible_baseline_are_unavailable() {
        for mode in 0..3 {
            let mut input = fixture();
            match mode {
                0 => input.coverage[0].expected_records = Some(100),
                1 => input.coverage[0].censored = true,
                _ => {
                    input.coverage[0].baseline.as_mut().unwrap().event_category = "earnings".into()
                }
            }
            let result = assess(&input, at()).unwrap();
            assert_eq!(
                result.risk_of_unreliable_attention,
                CoordinationTier::Unavailable
            );
            assert_eq!(result.evidence_weight_multiplier, None);
            assert_eq!(result.independent_origin_count, None);
            assert!(result.source_bursts[0].ratio.is_none());
            assert!(!result.unavailable_reasons.is_empty());
        }
    }

    #[test]
    fn deletion_or_missing_origin_retracts_all_dependent_features() {
        for deleted in [false, true] {
            let mut input = fixture();
            if deleted {
                input.invalidated_origin_ids.push("p0".into());
            } else {
                input.events.remove(0);
            }
            let result = assess(&input, at()).unwrap();
            assert_eq!(
                result.risk_of_unreliable_attention,
                CoordinationTier::Unavailable
            );
            assert!(result.supporting_origin_ids.is_empty());
            assert!(result.near_duplicate_fraction.is_none());
        }
    }

    #[test]
    fn spoofed_origins_cycles_timestamp_disorder_and_positive_weights_are_rejected() {
        let mut input = fixture();
        input.events[1].origin_event_id = "p1".into();
        assert!(assess(&input, at()).is_err());
        input = fixture();
        input.events[1].ingested_at = input.window_start;
        assert!(assess(&input, at()).is_err());
        input = fixture();
        input.thresholds.elevated_weight = Decimal::from(2);
        assert!(assess(&input, at()).is_err());
        input = fixture();
        input.events[0].kind = PromotionKind::Repost;
        input.events[0].parent_event_id = Some("p1".into());
        input.events[1].published_at = input.events[0].published_at;
        assert!(assess(&input, at()).is_err());
    }

    #[test]
    fn unknown_features_and_unreviewed_support_are_explicit() {
        let mut input = fixture();
        for event in &mut input.events {
            event.outbound_url_hash = None;
        }
        let result = assess(&input, at()).unwrap();
        assert!(result.shared_url_fraction.is_none());
        assert_eq!(result.feature_unavailable_reasons.len(), 2);
        input.thresholds.calibration_reference = None;
        input.thresholds.calibration_unavailable_reason =
            Some("not independently evaluated".into());
        let result = assess(&input, at()).unwrap();
        assert_eq!(
            result.risk_of_unreliable_attention,
            CoordinationTier::Unavailable
        );
        let mut json = serde_json::to_value(input).unwrap();
        json["confidence"] = 100.into();
        assert!(serde_json::from_value::<CoordinationInput>(json).is_err());
    }

    #[test]
    fn synchronization_is_independent_of_arbitrary_bucket_edges_and_empty_input_is_missing() {
        let mut input = fixture();
        for (i, event) in input.events.iter_mut().enumerate() {
            event.published_at = input.window_start + Duration::seconds(29 + i as i64);
            event.first_available_at = event.published_at + Duration::seconds(1);
            event.ingested_at = event.published_at + Duration::seconds(2);
        }
        assert_eq!(
            assess(&input, at()).unwrap().synchronized_fraction,
            Some(Decimal::ONE)
        );
        input.events.clear();
        assert_eq!(
            assess(&input, at()).unwrap().risk_of_unreliable_attention,
            CoordinationTier::Unavailable
        );
    }

    #[test]
    fn brigading_cannot_claim_new_confirmations_by_mislabelling_copies_as_originals() {
        let mut input = fixture();
        for event in &mut input.events {
            event.kind = PromotionKind::Original;
            event.parent_event_id = None;
            event.origin_event_id = event.event_id.clone();
        }
        let result = assess(&input, at()).unwrap();
        assert_eq!(result.independent_origin_count, Some(1));
        assert_eq!(result.independent_author_count, Some(1));
        assert_eq!(result.supporting_origin_ids.len(), 10);
        assert_eq!(
            result.risk_of_unreliable_attention,
            CoordinationTier::ReviewRequired
        );
    }
}

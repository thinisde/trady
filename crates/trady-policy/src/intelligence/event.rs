//! A point-in-time decomposition, never a causal or expected-return model.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use trady_domain::intelligence::event::*;
use trady_domain::{Error, Result, require};

pub fn assess(input: &EventInput, at: DateTime<Utc>) -> Result<EventAssessment> {
    require(
        !input.event_id.is_empty()
            && !input.event_type.is_empty()
            && !input.metric.is_empty()
            && !input.period.is_empty()
            && !input.unit.label.is_empty()
            && !input.unit.adjustment_definition.is_empty(),
        "missing event semantics",
    )?;
    require(
        input.clock_verified_at <= input.clock_valid_until,
        "invalid event clock validity",
    )?;
    if let Some(n) = &input.normalization {
        require(
            n.scale_floor > Decimal::ZERO && !n.justification.is_empty(),
            "normalization requires a positive declared scale and justification",
        )?;
    }
    let mut reasons = Vec::new();
    if input.release_at > at {
        reasons.push("event_not_released_as_of_assessment".into());
    }
    if input.clock_verified_at > at || input.clock_valid_until < at {
        reasons.push("stale_or_unavailable_event_clock".into());
    }

    let mut estimates = BTreeMap::new();
    for e in &input.expectations {
        require(
            !e.estimate_id.is_empty()
                && !e.source_ref.is_empty()
                && !e.provider.is_empty()
                && e.vintage_at <= e.available_at
                && (e.value.is_some() || e.missing_reason.as_ref().is_some_and(|s| !s.is_empty()))
                && e.dispersion.is_none_or(|v| v >= Decimal::ZERO),
            "invalid consensus estimate evidence",
        )?;
        if let Some(old) = estimates.insert(e.estimate_id.as_str(), e) {
            require(old == e, "conflicting consensus estimate id")?;
        }
    }
    for e in estimates.values() {
        if let Some(id) = &e.revision_of
            && let Some(original) = estimates.get(id.as_str())
        {
            require(
                original.available_at < e.available_at
                    && original.metric == e.metric
                    && original.period == e.period,
                "invalid estimate revision lineage",
            )?;
        }
    }
    let selected = input
        .frozen_estimate_id
        .as_deref()
        .and_then(|id| estimates.get(id).copied());
    let estimate = selected.filter(|e| e.available_at <= at);
    let frozen_ok = match (estimate, input.expectation_frozen_at) {
        (Some(e), Some(frozen)) => {
            if e.available_at <= frozen
                && e.vintage_at <= frozen
                && frozen < input.release_at
                && frozen <= at
            {
                true
            } else {
                reasons.push("estimate_not_frozen_and_available_strictly_before_release".into());
                false
            }
        }
        _ => {
            reasons.push("missing_frozen_pre_release_consensus".into());
            false
        }
    };
    if let Some(e) = estimate {
        if e.metric != input.metric || e.period != input.period || e.unit != input.unit {
            reasons.push("estimate_metric_period_currency_unit_or_adjustment_mismatch".into());
        }
        if e.value.is_none() {
            reasons.push(
                e.missing_reason
                    .clone()
                    .unwrap_or_else(|| "missing_consensus_value".into()),
            );
        }
    }

    let mut all_actuals = BTreeMap::new();
    for actual in &input.actuals {
        require(
            !actual.observation_id.is_empty()
                && !actual.source_ref.is_empty()
                && actual.reported_at <= actual.available_at
                && (actual.value.is_some()
                    || actual
                        .missing_reason
                        .as_ref()
                        .is_some_and(|s| !s.is_empty())),
            "invalid actual observation evidence",
        )?;
        if let Some(old) = all_actuals.insert(actual.observation_id.as_str(), actual) {
            require(old == actual, "conflicting actual observation id")?;
        }
    }
    let mut actuals: BTreeMap<_, _> = all_actuals
        .iter()
        .filter(|(_, v)| v.available_at <= at)
        .map(|(k, v)| (*k, *v))
        .collect();
    let mut actual_revisions = Vec::new();
    for actual in all_actuals.values().filter(|a| a.available_at <= at) {
        if let Some(id) = &actual.revision_of {
            if let Some(original) = all_actuals.get(id.as_str()) {
                require(
                    original.available_at < actual.available_at
                        && original.metric == actual.metric
                        && original.period == actual.period,
                    "invalid actual revision lineage",
                )?;
                actuals.remove(id.as_str());
            } else {
                reasons.push("missing_original_actual_vintage".into());
            }
            actual_revisions.push(actual.observation_id.clone());
        }
    }
    let actual = if actuals.len() == 1 {
        actuals.values().next().copied()
    } else {
        None
    };
    if actuals.len() > 1 {
        reasons.push("ambiguous_actual_vintages".into());
    }
    if let Some(actual) = actual {
        if actual.reported_at < input.release_at {
            reasons.push("actual_predates_verified_event_release".into());
        }
        if actual.metric != input.metric
            || actual.period != input.period
            || actual.unit != input.unit
        {
            reasons.push("actual_metric_period_currency_unit_or_adjustment_mismatch".into());
        }
        if actual.revision_of.is_some() {
            reasons.push(
                "revised_actual_requires_new_reconstruction_no_original_surprise_signal".into(),
            );
        }
        if actual.value.is_none() {
            reasons.push(
                actual
                    .missing_reason
                    .clone()
                    .unwrap_or_else(|| "missing_actual_value".into()),
            );
        }
    } else {
        reasons.push("missing_unique_available_actual".into());
    }

    let mut evidence_refs = Vec::new();
    if let Some(e) = estimate {
        evidence_refs.push(e.source_ref.clone());
    }
    if let Some(a) = actual {
        evidence_refs.push(a.source_ref.clone());
    }
    let (reaction, reaction_reasons, reaction_refs) = reaction(input, at)?;
    reasons.extend(reaction_reasons);
    evidence_refs.extend(reaction_refs);
    let core_comparable = reasons.is_empty() && frozen_ok;
    let raw = if core_comparable {
        match (actual.and_then(|a| a.value), estimate.and_then(|e| e.value)) {
            (Some(actual), Some(estimate)) => Some(sub(actual, estimate)?),
            _ => None,
        }
    } else {
        None
    };
    let (normalized, normalization_reason) = normalize(
        raw,
        estimate.and_then(|e| e.value),
        input.normalization.as_ref(),
    )?;
    let (standardized, standardization_reason) = match (raw, estimate) {
        (Some(raw), Some(e))
            if e.analyst_count.is_some_and(|n| n >= 2)
                && e.dispersion.is_some_and(|d| d > Decimal::ZERO)
                && matches!(
                    e.dispersion_definition,
                    Some(
                        EstimateDispersionDefinition::SampleStandardDeviation
                            | EstimateDispersionDefinition::PopulationStandardDeviation
                    )
                ) =>
        {
            (
                Some(div(
                    raw,
                    e.dispersion
                        .ok_or_else(|| Error("missing dispersion".into()))?,
                )?),
                None,
            )
        }
        (None, _) => (None, Some("surprise_unavailable".into())),
        _ => (
            None,
            Some("insufficient_analyst_count_or_defined_positive_standard_deviation".into()),
        ),
    };
    let (drift, drift_reason) = drift(input, estimate, &estimates, frozen_ok)?;
    let (pre_proxy, options_proxy, proxy_reasons, proxy_refs) = pricing_proxy(input, at)?;
    evidence_refs.extend(proxy_refs);
    let mut confounders = Vec::new();
    for c in &input.confounders {
        require(
            !c.source_ref.is_empty()
                && !c.event_id.is_empty()
                && !c.description.is_empty()
                && c.released_at <= c.available_at,
            "invalid confounding event evidence",
        )?;
        let overlaps = c.released_at == input.release_at
            || input.reaction.as_ref().is_some_and(|r| {
                c.released_at >= r.reference.observed_at && c.released_at <= r.after.observed_at
            });
        if c.available_at <= at && overlaps {
            confounders.push(c.clone());
            evidence_refs.push(c.source_ref.clone());
        }
    }
    confounders.sort_by(|a, b| (&a.released_at, &a.event_id).cmp(&(&b.released_at, &b.event_id)));
    confounders.dedup();
    let confounded = !confounders.is_empty();
    if confounded {
        reasons.push("overlapping_events_confounded_no_causal_attribution".into());
    }
    let outcome = if confounded {
        EventOutcome::Inconclusive
    } else {
        match (raw, reaction) {
            (Some(s), Some(r)) if s > Decimal::ZERO && r > Decimal::ZERO => {
                EventOutcome::SurpriseUpReactionUp
            }
            (Some(s), Some(r)) if s > Decimal::ZERO && r < Decimal::ZERO => {
                EventOutcome::SurpriseUpReactionDown
            }
            (Some(s), Some(r)) if s < Decimal::ZERO && r > Decimal::ZERO => {
                EventOutcome::SurpriseDownReactionUp
            }
            (Some(s), Some(r)) if s < Decimal::ZERO && r < Decimal::ZERO => {
                EventOutcome::SurpriseDownReactionDown
            }
            _ => EventOutcome::Inconclusive,
        }
    };
    if core_comparable && outcome == EventOutcome::Inconclusive && !confounded {
        reasons.push("zero_surprise_or_reaction_has_no_directional_category".into());
    }
    evidence_refs.sort();
    evidence_refs.dedup();
    actual_revisions.sort();
    Ok(EventAssessment {
        event_id: input.event_id.clone(),
        as_of: at,
        version: "event-expectation-v1".into(),
        period: input.period.clone(),
        unit: input.unit.clone(),
        estimate_id: estimate.map(|e| e.estimate_id.clone()),
        actual_id: actual.map(|a| a.observation_id.clone()),
        estimated_value: estimate.and_then(|e| e.value),
        reported_value: actual.and_then(|a| a.value),
        raw_surprise: raw,
        normalized_surprise: normalized,
        normalization_reason,
        standardized_surprise: standardized,
        standardization_reason,
        expectation_drift: drift,
        drift_reason,
        abnormal_return: reaction,
        pre_event_abnormal_return_proxy: pre_proxy,
        options_implied_move_proxy: options_proxy,
        pricing_proxy_reasons: proxy_reasons,
        outcome,
        confounded,
        confounders,
        actual_revisions,
        reasons,
        evidence_refs,
    })
}

type ReactionResult = (Option<Decimal>, Vec<String>, Vec<String>);

fn reaction(input: &EventInput, at: DateTime<Utc>) -> Result<ReactionResult> {
    let Some(r) = &input.reaction else {
        return Ok((
            None,
            vec!["reaction_and_matched_benchmark_not_yet_observed".into()],
            vec![],
        ));
    };
    let mut reasons = r.missing_reasons.clone();
    if r.window_id.is_empty()
        || r.session_calendar_reference.is_empty()
        || r.session_open_at >= r.session_close_at
        || r.reference.observed_at >= input.release_at
        || r.reference.available_at >= input.release_at
        || r.after.observed_at <= input.release_at
        || r.after.observed_at < r.session_open_at
        || r.after.observed_at > r.session_close_at
    {
        reasons
            .push("reaction_window_does_not_follow_release_and_verified_tradable_session".into());
    }
    if !r.total_return_adjustments_verified
        || !r.corporate_actions_resolved
        || !r.fx_resolved
        || r.trading_halt
    {
        reasons.push("unresolved_total_return_corporate_action_fx_or_halt".into());
    }
    let (Some(b0), Some(b1)) = (&r.benchmark_reference, &r.benchmark_after) else {
        reasons.push("missing_matched_benchmark".into());
        return Ok((None, reasons, vec![]));
    };
    let prices = [&r.reference, &r.after, b0, b1];
    for p in prices {
        require(
            p.observed_at <= p.available_at && !p.source_ref.is_empty(),
            "invalid event price evidence chronology",
        )?;
        if p.available_at > at {
            reasons.push("reaction_price_not_available_as_of_assessment".into());
        }
        if p.value <= Decimal::ZERO || p.currency.is_empty() || p.adjustment_basis.is_empty() {
            reasons.push("nonpositive_or_undefined_reaction_price".into());
        }
    }
    if b0.observed_at != r.reference.observed_at
        || b1.observed_at != r.after.observed_at
        || b0.available_at >= input.release_at
    {
        reasons.push("benchmark_tradable_interval_mismatch".into());
    }
    if prices.iter().any(|p| p.currency != r.reference.currency)
        || r.reference.adjustment_basis != r.after.adjustment_basis
        || b0.adjustment_basis != b1.adjustment_basis
    {
        reasons.push("reaction_currency_or_adjustment_basis_mismatch".into());
    }
    if !reasons.is_empty() {
        return Ok((None, reasons, vec![]));
    }
    let stock_return = sub(div(r.after.value, r.reference.value)?, Decimal::ONE)?;
    let benchmark_return = sub(div(b1.value, b0.value)?, Decimal::ONE)?;
    Ok((
        Some(sub(stock_return, benchmark_return)?),
        reasons,
        prices
            .iter()
            .map(|p| p.source_ref.clone())
            .chain(std::iter::once(r.session_calendar_reference.clone()))
            .collect(),
    ))
}

fn normalize(
    raw: Option<Decimal>,
    consensus: Option<Decimal>,
    n: Option<&SurpriseNormalization>,
) -> Result<(Option<Decimal>, Option<String>)> {
    let (Some(raw), Some(consensus)) = (raw, consensus) else {
        return Ok((None, Some("surprise_unavailable".into())));
    };
    let Some(n) = n else {
        return Ok((None, Some("metric_normalization_not_declared".into())));
    };
    let absolute = if consensus < Decimal::ZERO {
        sub(Decimal::ZERO, consensus)?
    } else {
        consensus
    };
    if absolute <= n.scale_floor && !n.allow_floor_for_metric {
        return Ok((
            None,
            Some("near_zero_consensus_unsuitable_for_relative_surprise".into()),
        ));
    }
    let denominator = absolute.max(n.scale_floor);
    Ok((Some(div(raw, denominator)?), None))
}

fn drift(
    input: &EventInput,
    selected: Option<&EventExpectation>,
    estimates: &BTreeMap<&str, &EventExpectation>,
    frozen_ok: bool,
) -> Result<(Option<Decimal>, Option<String>)> {
    let Some(id) = &input.drift_baseline_id else {
        return Ok((None, Some("earlier_estimate_window_not_supplied".into())));
    };
    let baseline = estimates.get(id.as_str()).copied();
    match (selected, baseline) {
        (Some(e), Some(b))
            if frozen_ok
                && b.available_at < e.available_at
                && b.vintage_at < e.vintage_at
                && b.available_at < input.release_at
                && b.metric == e.metric
                && b.period == e.period
                && b.unit == e.unit
                && b.provider == e.provider
                && e.metric == input.metric
                && e.period == input.period
                && e.unit == input.unit
                && b.value.is_some()
                && e.value.is_some() =>
        {
            Ok((
                Some(sub(
                    e.value
                        .ok_or_else(|| Error("missing drift estimate".into()))?,
                    b.value
                        .ok_or_else(|| Error("missing drift baseline".into()))?,
                )?),
                None,
            ))
        }
        _ => Ok((
            None,
            Some("drift_requires_comparable_strictly_earlier_frozen_vintage".into()),
        )),
    }
}

type ProxyResult = (Option<Decimal>, Option<Decimal>, Vec<String>, Vec<String>);

fn pricing_proxy(input: &EventInput, at: DateTime<Utc>) -> Result<ProxyResult> {
    let Some(p) = &input.pricing_proxy else {
        return Ok((
            None,
            None,
            vec!["pre_release_pricing_proxy_unavailable".into()],
            vec![],
        ));
    };
    let mut reasons = p.missing_reasons.clone();
    if p.window_start >= p.window_end
        || p.window_end >= input.release_at
        || p.available_at >= input.release_at
        || p.available_at < p.window_end
        || p.available_at > at
        || p.source_refs.is_empty()
        || p.source_refs.iter().any(|s| s.is_empty())
    {
        reasons.push("pricing_proxy_not_available_strictly_before_release".into());
    }
    if p.stock_total_return.is_some_and(|v| v < -Decimal::ONE)
        || p.benchmark_total_return.is_some_and(|v| v < -Decimal::ONE)
        || p.options_implied_move.is_some_and(|v| v < Decimal::ZERO)
    {
        reasons.push("invalid_pricing_proxy_measure".into());
    }
    if !reasons.is_empty() {
        return Ok((None, None, reasons, vec![]));
    }
    let abnormal = match (p.stock_total_return, p.benchmark_total_return) {
        (Some(s), Some(b)) => Some(sub(s, b)?),
        _ => {
            reasons.push("missing_pre_event_stock_or_benchmark_return".into());
            None
        }
    };
    if p.options_implied_move.is_none() {
        reasons.push("options_implied_move_unavailable".into());
    }
    Ok((
        abnormal,
        p.options_implied_move,
        reasons,
        p.source_refs.clone(),
    ))
}

fn sub(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_sub(b)
        .ok_or_else(|| Error("event subtraction overflow".into()))
}
fn div(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_div(b)
        .ok_or_else(|| Error("event division overflow or zero denominator".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn release() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T20:10:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn unit() -> EventUnit {
        EventUnit {
            label: "USD/share".into(),
            currency: Some("USD".into()),
            adjustment_definition: "GAAP diluted EPS".into(),
        }
    }
    fn price(value: i64, observed_at: DateTime<Utc>) -> EventPrice {
        EventPrice {
            value: Decimal::from(value),
            currency: "USD".into(),
            adjustment_basis: "total-return-v1".into(),
            observed_at,
            available_at: observed_at,
            source_ref: format!("price:{observed_at}"),
        }
    }
    fn input() -> EventInput {
        let pre = release() - Duration::minutes(11);
        let next_open = DateTime::parse_from_rfc3339("2026-11-02T14:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let after = next_open + Duration::minutes(30);
        EventInput {
            event_id: "earnings:issuer:2026Q3".into(),
            issuer_id: Some("issuer".into()),
            event_type: "earnings".into(),
            metric: "diluted_eps".into(),
            period: "2026Q3".into(),
            unit: unit(),
            release_at: release(),
            clock_verified_at: release() - Duration::days(1),
            clock_valid_until: release() + Duration::days(7),
            frozen_estimate_id: Some("e1".into()),
            expectation_frozen_at: Some(release() - Duration::hours(1)),
            drift_baseline_id: None,
            expectations: vec![EventExpectation {
                estimate_id: "e1".into(),
                metric: "diluted_eps".into(),
                period: "2026Q3".into(),
                unit: unit(),
                value: Some(Decimal::from(1)),
                missing_reason: None,
                provider: "licensed-fixture".into(),
                vintage_at: release() - Duration::days(1),
                available_at: release() - Duration::days(1),
                analyst_count: Some(12),
                dispersion: Some(Decimal::new(2, 1)),
                dispersion_definition: Some(EstimateDispersionDefinition::SampleStandardDeviation),
                revision_of: None,
                source_ref: "consensus:fixture:e1".into(),
            }],
            actuals: vec![ActualObservation {
                observation_id: "a1".into(),
                metric: "diluted_eps".into(),
                period: "2026Q3".into(),
                unit: unit(),
                value: Some(Decimal::new(12, 1)),
                missing_reason: None,
                reported_at: release(),
                available_at: release(),
                revision_of: None,
                source_ref: "sec:release:a1".into(),
            }],
            normalization: Some(SurpriseNormalization {
                scale_floor: Decimal::new(1, 2),
                allow_floor_for_metric: false,
                justification: "relative EPS only above material floor".into(),
            }),
            reaction: Some(MarketReaction {
                window_id: "next-session-30m".into(),
                session_open_at: next_open,
                session_close_at: next_open + Duration::hours(6),
                session_calendar_reference: "calendar:verified-next-session".into(),
                reference: price(100, pre),
                after: price(105, after),
                benchmark_reference: Some(price(100, pre)),
                benchmark_after: Some(price(101, after)),
                total_return_adjustments_verified: true,
                corporate_actions_resolved: true,
                fx_resolved: true,
                trading_halt: false,
                missing_reasons: vec![],
            }),
            pricing_proxy: None,
            confounders: vec![],
        }
    }
    fn at() -> DateTime<Utc> {
        release() + Duration::days(4)
    }

    #[test]
    fn event_postmarket_release_uses_next_session_without_a_causal_claim() {
        let a = assess(&input(), at()).unwrap();
        assert_eq!(a.raw_surprise, Some(Decimal::new(2, 1)));
        assert_eq!(a.normalized_surprise, Some(Decimal::new(2, 1)));
        assert_eq!(a.standardized_surprise, Some(Decimal::ONE));
        assert_eq!(a.abnormal_return, Some(Decimal::new(4, 2)));
        assert_eq!(a.outcome, EventOutcome::SurpriseUpReactionUp);
    }

    #[test]
    fn event_negative_to_positive_eps_uses_absolute_consensus_and_zero_abstains() {
        let mut i = input();
        i.expectations[0].value = Some(-Decimal::ONE);
        i.actuals[0].value = Some(Decimal::new(5, 1));
        assert_eq!(
            assess(&i, at()).unwrap().normalized_surprise,
            Some(Decimal::new(15, 1))
        );
        i.expectations[0].value = Some(Decimal::ZERO);
        let a = assess(&i, at()).unwrap();
        assert!(a.normalized_surprise.is_none());
        assert!(a.raw_surprise.is_some());
        assert!(a.normalization_reason.unwrap().contains("near_zero"));
    }

    #[test]
    fn event_late_consensus_and_guidance_definition_mismatch_have_no_surprise_signal() {
        let mut i = input();
        i.expectations[0].available_at = release() + Duration::seconds(1);
        assert!(assess(&i, at()).unwrap().raw_surprise.is_none());
        let mut i = input();
        i.actuals[0].unit.adjustment_definition = "non-GAAP forward guidance".into();
        let a = assess(&i, at()).unwrap();
        assert!(a.raw_surprise.is_none());
        assert_eq!(a.outcome, EventOutcome::Inconclusive);
    }

    #[test]
    fn event_missing_benchmark_stale_clock_and_unobserved_reaction_are_inconclusive() {
        let mut i = input();
        i.reaction.as_mut().unwrap().benchmark_after = None;
        assert!(assess(&i, at()).unwrap().raw_surprise.is_none());
        let mut i = input();
        i.clock_valid_until = release();
        assert_eq!(
            assess(&i, at()).unwrap().outcome,
            EventOutcome::Inconclusive
        );
        let a = assess(&input(), release()).unwrap();
        assert!(a.raw_surprise.is_none());
        assert!(a.abnormal_return.is_none());
    }

    #[test]
    fn event_revised_macro_actual_preserves_original_vintage_and_then_abstains() {
        let mut i = input();
        let mut correction = i.actuals[0].clone();
        correction.observation_id = "a2".into();
        correction.revision_of = Some("a1".into());
        correction.value = Some(Decimal::new(8, 1));
        correction.reported_at = at() + Duration::days(1);
        correction.available_at = correction.reported_at;
        i.actuals.push(correction);
        assert_eq!(
            assess(&i, at()).unwrap().raw_surprise,
            Some(Decimal::new(2, 1))
        );
        let a = assess(&i, at() + Duration::days(1)).unwrap();
        assert!(a.raw_surprise.is_none());
        assert_eq!(a.reported_value, Some(Decimal::new(8, 1)));
        assert_eq!(a.actual_revisions, vec!["a2"]);
    }

    #[test]
    fn event_simultaneous_product_launch_marks_reaction_confounded() {
        let mut i = input();
        i.confounders.push(EventConfounder {
            event_id: "product".into(),
            description: "simultaneous product release".into(),
            released_at: release(),
            available_at: release(),
            source_ref: "issuer:product-release".into(),
        });
        let a = assess(&i, at()).unwrap();
        assert!(a.confounded);
        assert_eq!(a.outcome, EventOutcome::Inconclusive);
        assert!(a.raw_surprise.is_some());
        assert!(a.abnormal_return.is_some());
    }

    #[test]
    fn event_holiday_session_halting_and_splits_require_explicit_quality() {
        let mut i = input();
        let r = i.reaction.as_mut().unwrap();
        r.session_open_at += Duration::days(1);
        r.session_close_at += Duration::days(1);
        r.after.observed_at += Duration::days(1);
        r.after.available_at = r.after.observed_at;
        r.benchmark_after.as_mut().unwrap().observed_at += Duration::days(1);
        r.benchmark_after.as_mut().unwrap().available_at = r.after.available_at;
        assert!(assess(&i, at()).unwrap().raw_surprise.is_some());
        i.reaction.as_mut().unwrap().trading_halt = true;
        assert!(assess(&i, at()).unwrap().raw_surprise.is_none());
        i.reaction.as_mut().unwrap().trading_halt = false;
        i.reaction.as_mut().unwrap().corporate_actions_resolved = false;
        assert!(assess(&i, at()).unwrap().abnormal_return.is_none());
    }

    #[test]
    fn event_drift_and_positioning_proxies_only_use_earlier_public_evidence() {
        let mut i = input();
        let mut old = i.expectations[0].clone();
        old.estimate_id = "e0".into();
        old.available_at -= Duration::days(2);
        old.vintage_at = old.available_at;
        old.value = Some(Decimal::new(8, 1));
        i.expectations.push(old);
        i.drift_baseline_id = Some("e0".into());
        i.pricing_proxy = Some(EventPricingProxy {
            window_start: release() - Duration::days(5),
            window_end: release() - Duration::hours(1),
            available_at: release() - Duration::minutes(30),
            stock_total_return: Some(Decimal::new(5, 2)),
            benchmark_total_return: Some(Decimal::new(1, 2)),
            options_implied_move: None,
            source_refs: vec!["prices:pre-event".into()],
            missing_reasons: vec![],
        });
        let a = assess(&i, at()).unwrap();
        assert_eq!(a.expectation_drift, Some(Decimal::new(2, 1)));
        assert_eq!(a.pre_event_abnormal_return_proxy, Some(Decimal::new(4, 2)));
        i.pricing_proxy.as_mut().unwrap().available_at = release() + Duration::seconds(1);
        assert!(
            assess(&i, at())
                .unwrap()
                .pre_event_abnormal_return_proxy
                .is_none()
        );
    }

    #[test]
    fn event_undefined_dispersion_and_mismatched_drift_never_gain_numeric_meaning() {
        let mut i = input();
        i.expectations[0].dispersion_definition = Some(EstimateDispersionDefinition::Unknown);
        let a = assess(&i, at()).unwrap();
        assert!(a.standardized_surprise.is_none());
        assert!(a.raw_surprise.is_some());
        let mut old = i.expectations[0].clone();
        old.estimate_id = "old".into();
        old.vintage_at -= Duration::days(1);
        old.available_at = old.vintage_at;
        old.unit.adjustment_definition = "non-GAAP".into();
        i.expectations[0].unit.adjustment_definition = "non-GAAP".into();
        i.expectations.push(old);
        i.drift_baseline_id = Some("old".into());
        let a = assess(&i, at()).unwrap();
        assert!(a.expectation_drift.is_none());
        assert!(a.raw_surprise.is_none());
    }

    #[test]
    fn issuer_events_fixture_event_has_strict_research_serialization() {
        let mut i = input();
        i.release_at -= Duration::hours(5);
        i.expectation_frozen_at = Some(i.release_at - Duration::hours(1));
        i.actuals[0].reported_at = i.release_at;
        i.actuals[0].available_at = i.release_at;
        let r = i.reaction.as_mut().unwrap();
        r.session_open_at = i.release_at - Duration::hours(1);
        r.session_close_at = i.release_at + Duration::hours(4);
        r.reference = price(100, i.release_at - Duration::minutes(1));
        r.after = price(105, i.release_at + Duration::minutes(30));
        r.benchmark_reference = Some(price(100, r.reference.observed_at));
        r.benchmark_after = Some(price(101, r.after.observed_at));
        let mut json = serde_json::to_value(&i).unwrap();
        let decoded: EventInput = serde_json::from_value(json.clone()).unwrap();
        let assessment = assess(&decoded, at()).unwrap();
        assert_eq!(assessment.raw_surprise, Some(Decimal::new(2, 1)));
        json.as_object_mut()
            .unwrap()
            .insert("order_authority".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<EventInput>(json).is_err());
        let mut result = serde_json::to_value(&assessment).unwrap();
        assert_eq!(
            serde_json::from_value::<EventAssessment>(result.clone()).unwrap(),
            assessment
        );
        result
            .as_object_mut()
            .unwrap()
            .insert("expected_return".into(), serde_json::json!("0.5"));
        assert!(serde_json::from_value::<EventAssessment>(result).is_err());
        if std::env::var("TRADY_EMIT_ISSUER_EVENTS_FIXTURES").as_deref() == Ok("1") {
            println!(
                "PUBLIC_INPUT_FIXTURE event {}",
                serde_json::to_string(&i).unwrap()
            );
        }
    }
}

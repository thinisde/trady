//! Sparse as-of joins, primary-flow accounting and trailing descriptive statistics.
//! No source revision, holiday gap, absent series or ETF volume is silently filled.

use crate::{add, div, mul, sub};
use chrono::{DateTime, TimeDelta, Utc};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::intelligence::cross_asset::*;
use trady_domain::{Error, Result, require};

pub fn assess(input: &CrossAssetInput, at: DateTime<Utc>) -> Result<CrossAssetAssessment> {
    validate(input)?;
    let mut out = CrossAssetAssessment {
        at,
        region: input.region.clone(),
        status: CrossAssetStatus::Available,
        effective_horizon_seconds: input.effective_horizon_seconds,
        aligned: vec![],
        sparse_missing_series: vec![],
        pairs: vec![],
        flows: vec![],
        stress: vec![],
        breadth: None,
        issues: vec![],
    };
    let series: BTreeMap<_, _> = input.series.iter().map(|s| (s.id.as_str(), s)).collect();
    for s in &input.series {
        let selected = known(s, at).values().next_back().copied();
        let mut aligned = CrossAssetAlignedValue {
            series_id: s.id.clone(),
            entity_id: s.entity_id.clone(),
            kind: s.kind,
            unit: s.unit,
            currency: s.currency.clone(),
            currency_pair: s.currency_pair.clone(),
            calendar_id: s.calendar_id.clone(),
            venue: s.venue.clone(),
            adjustment_basis: s.adjustment_basis.clone(),
            value: None,
            period_end: None,
            published_at: None,
            available_at: None,
            age_seconds: None,
            vintage: None,
            source_ref: None,
            unavailable_reason: None,
        };
        if let Some(o) = selected {
            let age = (at - o.period_end).num_seconds();
            aligned.period_end = Some(o.period_end);
            aligned.published_at = Some(o.published_at);
            aligned.available_at = Some(o.available_at);
            aligned.age_seconds = Some(age);
            aligned.vintage = Some(o.vintage.clone());
            aligned.source_ref = Some(o.source_ref.clone());
            if !within_age(o.period_end, at, s.max_age_seconds) {
                aligned.unavailable_reason =
                    Some("series exceeds its own freshness/holiday policy".into());
            } else if o.value.is_none() {
                aligned.unavailable_reason = o.missing_reason.clone();
            } else {
                aligned.value = o.value;
            }
        } else {
            aligned.unavailable_reason =
                Some("no publication/ingestion was available by the decision".into());
        }
        if aligned.value.is_none() {
            out.sparse_missing_series.push(s.id.clone());
            if s.required {
                out.status = CrossAssetStatus::Abstain;
            } else if out.status == CrossAssetStatus::Available {
                out.status = CrossAssetStatus::Partial;
            }
        }
        out.aligned.push(aligned);
    }
    for p in &input.pairs {
        out.pairs.push(pair_assessment(
            p,
            lookup(&series, &p.asset_series_id)?,
            lookup(&series, &p.benchmark_series_id)?,
            at,
        )?);
    }
    for f in &input.flows {
        out.flows.push(flow_assessment(f, &series, at)?);
    }
    for s in &input.stress {
        out.stress
            .push(stress_assessment(s, lookup(&series, &s.series_id)?, at)?);
    }
    if let Some(b) = &input.breadth {
        if b.available_at <= at
            && b.period_end <= at
            && within_age(b.period_end, at, input.effective_horizon_seconds)
            && b.members.iter().all(|m| m.available_at <= at)
        {
            let mut positive = 0_u64;
            let mut weight = Decimal::ZERO;
            let mut weighted_return = Decimal::ZERO;
            let mut refs = vec![b.source_ref.clone()];
            for m in &b.members {
                if m.total_return_fraction > Decimal::ZERO {
                    positive += 1;
                    weight = add(weight, m.market_cap_weight)?;
                }
                weighted_return = add(
                    weighted_return,
                    mul(m.market_cap_weight, m.total_return_fraction)?,
                )?;
                refs.push(m.source_ref.clone());
            }
            out.breadth = Some(CrossAssetBreadthAssessment {
                positive_constituent_fraction: div(
                    Decimal::from(positive),
                    Decimal::from(b.members.len() as u64),
                )?,
                positive_market_cap_weight_fraction: weight,
                market_cap_weighted_return_fraction: weighted_return,
                constituents: b.members.len(),
                weights_version: b.weights_version.clone(),
                source_refs: refs,
            });
        } else {
            out.issues
                .push("breadth was unpublished, stale or had future constituent returns".into());
        }
    } else {
        out.issues
            .push("historical constituent weights/breadth absent".into());
    }
    if input.series.is_empty() {
        out.status = CrossAssetStatus::Abstain;
        out.issues.push("no cross-asset series supplied".into());
    }
    let partial = !out.issues.is_empty()
        || out.pairs.iter().any(|p| !p.unavailable_reasons.is_empty())
        || out.flows.iter().any(|f| !f.unavailable_reasons.is_empty())
        || out.stress.iter().any(|s| s.unavailable_reason.is_some());
    if partial && out.status == CrossAssetStatus::Available {
        out.status = CrossAssetStatus::Partial;
    }
    Ok(out)
}

fn lookup<'a>(
    series: &BTreeMap<&str, &'a CrossAssetSeries>,
    id: &str,
) -> Result<&'a CrossAssetSeries> {
    series
        .get(id)
        .copied()
        .ok_or_else(|| Error(format!("unknown cross-asset series {id}")))
}

/// One revision per actual observation interval; never choose by future vintage text.
fn known(
    s: &CrossAssetSeries,
    at: DateTime<Utc>,
) -> BTreeMap<(DateTime<Utc>, DateTime<Utc>), &CrossAssetObservation> {
    let mut result: BTreeMap<_, &CrossAssetObservation> = BTreeMap::new();
    for o in &s.observations {
        if o.period_end <= at && o.available_at <= at && o.ingested_at <= at {
            let key = (o.period_end, o.period_start);
            if result.get(&key).is_none_or(|p| {
                (o.published_at, o.available_at, o.ingested_at)
                    > (p.published_at, p.available_at, p.ingested_at)
            }) {
                result.insert(key, o);
            }
        }
    }
    result
}
fn latest(s: &CrossAssetSeries, at: DateTime<Utc>) -> Option<&CrossAssetObservation> {
    known(s, at)
        .values()
        .next_back()
        .copied()
        .filter(|o| within_age(o.period_end, at, s.max_age_seconds) && o.value.is_some())
}

fn pair_assessment(
    p: &CrossAssetPair,
    a: &CrossAssetSeries,
    b: &CrossAssetSeries,
    at: DateTime<Utc>,
) -> Result<CrossAssetPairAssessment> {
    require(
        a.kind == CrossAssetKind::PeriodTotalReturn && b.kind == CrossAssetKind::PeriodTotalReturn,
        "beta/correlation require actual period total returns",
    )?;
    let mut out = CrossAssetPairAssessment {
        asset_series_id: a.id.clone(),
        benchmark_series_id: b.id.clone(),
        samples: 0,
        beta: None,
        beta_standard_error: None,
        correlation: None,
        correlation_interval_95: None,
        short_window_correlation: None,
        correlation_change: None,
        relative_compounded_return: None,
        source_refs: vec![],
        unavailable_reasons: vec![],
    };
    if a.calendar_id != b.calendar_id
        || a.currency != b.currency
        || a.currency.is_none()
        || a.adjustment_basis != b.adjustment_basis
    {
        out.unavailable_reasons.push(
            "currency/session-calendar/timezone/adjustment mismatch; no implicit conversion".into(),
        );
        return Ok(out);
    }
    if latest(a, at).is_none() || latest(b, at).is_none() {
        out.unavailable_reasons
            .push("pair contains missing/stale latest returns".into());
        return Ok(out);
    }
    let ak = known(a, at);
    let bk = known(b, at);
    let mut rows = vec![];
    let mut last_matched_at = None;
    for (key, ao) in ak {
        if ao.period_start < p.lookback_start {
            continue;
        }
        if let Some(bo) = bk.get(&key)
            && let (Some(av), Some(bv)) = (ao.value, bo.value)
        {
            rows.push((av, bv));
            last_matched_at = Some(ao.period_end);
            out.source_refs.push(ao.source_ref.clone());
            out.source_refs.push(bo.source_ref.clone());
        }
    }
    out.samples = rows.len();
    if last_matched_at
        .is_none_or(|last| !within_age(last, at, a.max_age_seconds.min(b.max_age_seconds)))
    {
        out.unavailable_reasons
            .push("latest common return interval is absent or stale".into());
        return Ok(out);
    }
    if rows.len() < p.minimum_samples {
        out.unavailable_reasons
            .push("insufficient exact-interval trailing samples; no forward fill".into());
        return Ok(out);
    }
    let floats: Vec<_> = rows
        .iter()
        .map(|(a, b)| Ok((number(*a)?, number(*b)?)))
        .collect::<Result<_>>()?;
    if let Some((beta, se, correlation)) = statistics(&floats) {
        out.beta = Some(beta);
        out.beta_standard_error = Some(se);
        out.correlation = Some(correlation);
        if rows.len() > 3 {
            let fisher = correlation.atanh();
            let half = 1.959963984540054 / (rows.len() as f64 - 3.0).sqrt();
            out.correlation_interval_95=Some(CrossAssetInterval{lower:(fisher-half).tanh(),upper:(fisher+half).tanh(),method:"Fisher z 95%, iid approximation; structural breaks and serial dependence are unmodeled".into()});
        } else {
            out.unavailable_reasons
                .push("correlation interval needs more than three samples".into());
        }
        if floats.len() >= p.short_window_samples {
            if let Some((_, _, short)) =
                statistics(&floats[floats.len() - p.short_window_samples..])
            {
                out.short_window_correlation = Some(short);
                out.correlation_change = Some(short - correlation);
            } else {
                out.unavailable_reasons
                    .push("short-window covariance is singular".into());
            }
        } else {
            out.unavailable_reasons
                .push("short-window sample count unavailable".into());
        }
    } else {
        out.unavailable_reasons
            .push("zero or singular covariance; beta/correlation withheld".into());
    }
    let (mut ag, mut bg) = (Decimal::ONE, Decimal::ONE);
    for (av, bv) in rows {
        ag = mul(ag, add(Decimal::ONE, av)?)?;
        bg = mul(bg, add(Decimal::ONE, bv)?)?;
    }
    if bg > Decimal::ZERO {
        out.relative_compounded_return = Some(sub(div(ag, bg)?, Decimal::ONE)?);
    } else {
        out.unavailable_reasons
            .push("benchmark compounded value is zero".into());
    }
    Ok(out)
}

/// OLS with intercept, benchmark as x; centered covariance avoids raw-moment subtraction.
fn statistics(rows: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    if rows.len() < 3 {
        return None;
    }
    let n = rows.len() as f64;
    let (my, mx) = rows
        .iter()
        .fold((0.0, 0.0), |(y, x), (a, b)| (y + a / n, x + b / n));
    let (syy, sxx, sxy) = rows.iter().fold((0.0, 0.0, 0.0), |(yy, xx, xy), (y, x)| {
        (
            yy + (y - my).powi(2),
            xx + (x - mx).powi(2),
            xy + (y - my) * (x - mx),
        )
    });
    if sxx <= 0.0 || syy <= 0.0 || sxx * syy - sxy * sxy <= 1e-12 * sxx * syy {
        return None;
    }
    let beta = sxy / sxx;
    let correlation = sxy / (sxx * syy).sqrt();
    let se = ((syy - sxy * sxy / sxx) / (n - 2.0) / sxx).sqrt();
    [beta, correlation, se]
        .iter()
        .all(|v| v.is_finite())
        .then_some((beta, se, correlation))
}

fn flow_assessment(
    f: &CrossAssetFlow,
    series: &BTreeMap<&str, &CrossAssetSeries>,
    at: DateTime<Utc>,
) -> Result<CrossAssetFlowAssessment> {
    let flow = lookup(series, &f.flow_series_id)?;
    let assets = lookup(series, &f.assets_series_id)?;
    require(
        flow.kind == CrossAssetKind::PrimaryNetFlow,
        "secondary ETF trading volume or AUM change cannot be substituted for primary fund flow",
    )?;
    require(
        assets.kind == CrossAssetKind::AssetsUnderManagement,
        "flow denominator must be assets under management",
    )?;
    let mut out = CrossAssetFlowAssessment {
        flow_series_id: flow.id.clone(),
        net_primary_flow: None,
        flow_to_opening_assets_fraction: None,
        performance_driven_assets_change: None,
        unexplained_assets_change: None,
        source_refs: vec![],
        unavailable_reasons: vec![],
    };
    if flow.entity_id != assets.entity_id {
        out.unavailable_reasons
            .push("primary flow and AUM belong to different funds".into());
        return Ok(out);
    }
    if flow.currency.is_none()
        || flow.currency != assets.currency
        || flow.calendar_id != assets.calendar_id
        || flow.adjustment_basis != assets.adjustment_basis
    {
        out.unavailable_reasons
            .push("flow/AUM currency, calendar or valuation basis mismatch".into());
        return Ok(out);
    }
    let Some(o) = latest(flow, at) else {
        out.unavailable_reasons
            .push("primary flow missing, stale or published after decision".into());
        return Ok(out);
    };
    out.net_primary_flow = o.value;
    out.source_refs.push(o.source_ref.clone());
    let ak = known(assets, at);
    let opening = ak
        .values()
        .rev()
        .copied()
        .find(|v| v.period_end <= o.period_start);
    let Some(opening) = opening.filter(|v| {
        v.value.is_some() && within_age(v.period_end, o.period_start, assets.max_age_seconds)
    }) else {
        out.unavailable_reasons
            .push("opening AUM missing or too old for the flow interval".into());
        return Ok(out);
    };
    let denominator = opening.value.unwrap();
    if denominator <= Decimal::ZERO {
        out.unavailable_reasons
            .push("nonpositive opening AUM denominator".into());
        return Ok(out);
    }
    out.flow_to_opening_assets_fraction = Some(div(o.value.unwrap(), denominator)?);
    out.source_refs.push(opening.source_ref.clone());
    let Some(performance_id) = &f.performance_series_id else {
        out.unavailable_reasons.push(
            "performance series absent; AUM performance/flow decomposition unavailable".into(),
        );
        return Ok(out);
    };
    let performance = lookup(series, performance_id)?;
    require(
        performance.kind == CrossAssetKind::PeriodTotalReturn,
        "AUM performance decomposition requires period total return",
    )?;
    if performance.entity_id != flow.entity_id
        || performance.currency != flow.currency
        || performance.calendar_id != flow.calendar_id
        || performance.adjustment_basis != flow.adjustment_basis
    {
        out.unavailable_reasons
            .push("performance currency/calendar/basis mismatch".into());
        return Ok(out);
    }
    let pk = known(performance, at);
    if let Some(p) = pk
        .get(&(o.period_end, o.period_start))
        .filter(|p| p.value.is_some())
    {
        let price_change = mul(denominator, p.value.unwrap())?;
        out.performance_driven_assets_change = Some(price_change);
        out.source_refs.push(p.source_ref.clone());
        if let Some(end) = ak
            .values()
            .copied()
            .find(|v| v.period_end == o.period_end && v.value.is_some())
        {
            out.unexplained_assets_change = Some(sub(
                sub(sub(end.value.unwrap(), denominator)?, price_change)?,
                o.value.unwrap(),
            )?);
            out.source_refs.push(end.source_ref.clone());
        } else {
            out.unavailable_reasons
                .push("same-interval ending AUM missing".into());
        }
    } else {
        out.unavailable_reasons
            .push("same-interval total return missing; no inference from AUM growth".into());
    }
    Ok(out)
}

fn stress_assessment(
    s: &CrossAssetStress,
    series: &CrossAssetSeries,
    at: DateTime<Utc>,
) -> Result<CrossAssetStressAssessment> {
    require(
        matches!(
            series.kind,
            CrossAssetKind::YieldFraction
                | CrossAssetKind::YieldSpreadFraction
                | CrossAssetKind::FxSpot
                | CrossAssetKind::LiquidityCostFraction
                | CrossAssetKind::VolatilityAnnualized
        ),
        "stress contributor must be a yield/credit/FX/liquidity/volatility series",
    )?;
    let mut out = CrossAssetStressAssessment {
        series_id: series.id.clone(),
        change: None,
        threshold_exceeded: None,
        source_refs: vec![],
        unavailable_reason: None,
    };
    let Some(current) = latest(series, at) else {
        out.unavailable_reason = Some("current stress series missing/stale/unpublished".into());
        return Ok(out);
    };
    let delta = TimeDelta::try_seconds(
        i64::try_from(s.horizon_seconds).map_err(|_| Error("stress horizon overflow".into()))?,
    )
    .ok_or_else(|| Error("stress horizon overflow".into()))?;
    let cutoff = current
        .period_end
        .checked_sub_signed(delta)
        .ok_or_else(|| Error("stress cutoff overflow".into()))?;
    let values = known(series, at);
    let prior = values
        .values()
        .rev()
        .copied()
        .find(|v| v.period_end <= cutoff);
    let Some(prior) = prior
        .filter(|v| v.value.is_some() && within_age(v.period_end, cutoff, series.max_age_seconds))
    else {
        out.unavailable_reason = Some("publication-aware stress baseline missing/stale".into());
        return Ok(out);
    };
    let c = current.value.unwrap();
    let p = prior.value.unwrap();
    let change = match s.method {
        CrossAssetStressMethod::Difference => sub(c, p)?,
        CrossAssetStressMethod::RelativeChange => {
            if p <= Decimal::ZERO {
                out.unavailable_reason = Some("relative stress baseline is nonpositive".into());
                return Ok(out);
            }
            sub(div(c, p)?, Decimal::ONE)?
        }
    };
    let exceeded = match s.direction {
        CrossAssetStressDirection::Rising => change >= s.threshold,
        CrossAssetStressDirection::Falling => change <= sub(Decimal::ZERO, s.threshold)?,
        CrossAssetStressDirection::Either => change.abs() >= s.threshold,
    };
    out.change = Some(change);
    out.threshold_exceeded = Some(exceeded);
    out.source_refs = vec![prior.source_ref.clone(), current.source_ref.clone()];
    Ok(out)
}

fn number(v: Decimal) -> Result<f64> {
    v.to_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| Error("non-finite statistical conversion".into()))
}

fn within_age(observed: DateTime<Utc>, at: DateTime<Utc>, seconds: u64) -> bool {
    observed <= at
        && i64::try_from(seconds)
            .ok()
            .and_then(TimeDelta::try_seconds)
            .is_some_and(|limit| at - observed <= limit)
}
fn validate(input: &CrossAssetInput) -> Result<()> {
    require(
        !input.region.trim().is_empty()
            && input.effective_horizon_seconds > 0
            && input.series.len() <= 128
            && input.pairs.len() <= 256
            && input.flows.len() <= 128
            && input.stress.len() <= 128,
        "invalid cross-asset configuration/bounds",
    )?;
    let mut ids = BTreeSet::new();
    for s in &input.series {
        require(
            !s.id.trim().is_empty()
                && !s.entity_id.trim().is_empty()
                && ids.insert(&s.id)
                && !s.calendar_id.trim().is_empty()
                && !s.venue.trim().is_empty()
                && !s.adjustment_basis.trim().is_empty()
                && s.max_age_seconds > 0
                && s.observations.len() <= 16384,
            "invalid/duplicate cross-asset series",
        )?;
        let expected = match s.kind {
            CrossAssetKind::PrimaryNetFlow | CrossAssetKind::AssetsUnderManagement => {
                CrossAssetUnit::CurrencyAmount
            }
            CrossAssetKind::FxSpot => CrossAssetUnit::CurrencyPairRate,
            CrossAssetKind::SecondaryTradingVolume | CrossAssetKind::SharesOutstanding => {
                CrossAssetUnit::Shares
            }
            _ => CrossAssetUnit::Fraction,
        };
        require(
            s.unit == expected
                && ((s.unit != CrossAssetUnit::CurrencyAmount) || s.currency.is_some())
                && ((s.kind != CrossAssetKind::FxSpot)
                    || s.currency_pair.as_ref().is_some_and(|p| p[0] != p[1])),
            "series kind/unit/currency semantics mismatch",
        )?;
        let mut keys = BTreeSet::new();
        for o in &s.observations {
            require(
                o.period_start <= o.period_end
                    && o.period_end <= o.published_at
                    && o.published_at <= o.available_at
                    && o.available_at <= o.ingested_at
                    && !o.source_ref.trim().is_empty()
                    && !o.vintage.trim().is_empty()
                    && (o.value.is_some() != o.missing_reason.is_some())
                    && o.missing_reason
                        .as_ref()
                        .is_none_or(|r| !r.trim().is_empty())
                    && keys.insert((
                        o.period_start,
                        o.period_end,
                        o.published_at,
                        o.available_at,
                        o.ingested_at,
                    )),
                "invalid/ambiguous cross-asset observation/revision",
            )?;
            if let Some(v) = o.value {
                let valid = match s.kind {
                    CrossAssetKind::PeriodTotalReturn => v >= -Decimal::ONE,
                    CrossAssetKind::FxSpot => v > Decimal::ZERO,
                    CrossAssetKind::AssetsUnderManagement
                    | CrossAssetKind::SecondaryTradingVolume
                    | CrossAssetKind::SharesOutstanding
                    | CrossAssetKind::VolatilityAnnualized
                    | CrossAssetKind::LiquidityCostFraction => v >= Decimal::ZERO,
                    _ => true,
                };
                require(valid, "invalid series value for declared semantics")?;
            }
        }
        if matches!(
            s.kind,
            CrossAssetKind::PeriodTotalReturn | CrossAssetKind::PrimaryNetFlow
        ) {
            let intervals: BTreeSet<_> = s
                .observations
                .iter()
                .map(|o| (o.period_start, o.period_end))
                .collect();
            let mut previous_end = None;
            for (start, end) in intervals {
                require(
                    start < end && previous_end.is_none_or(|p| start >= p),
                    "return/flow series intervals must be nonoverlapping; separate different reporting frequencies",
                )?;
                previous_end = Some(end);
            }
        }
    }
    for p in &input.pairs {
        require(
            p.asset_series_id != p.benchmark_series_id
                && p.minimum_samples >= 3
                && p.minimum_samples <= 16384
                && p.short_window_samples >= 3
                && p.short_window_samples <= 16384,
            "invalid trailing pair sample policy",
        )?;
    }
    for s in &input.stress {
        require(
            s.horizon_seconds > 0
                && s.horizon_seconds <= i64::MAX as u64
                && s.threshold > Decimal::ZERO,
            "invalid registered stress horizon/threshold",
        )?;
    }
    if let Some(b) = &input.breadth {
        require(
            b.period_start < b.period_end
                && b.weights_effective_at <= b.period_start
                && b.weights_available_at <= b.period_start
                && b.period_end <= b.published_at
                && b.published_at <= b.available_at
                && !b.weights_version.trim().is_empty()
                && !b.source_ref.trim().is_empty()
                && !b.members.is_empty()
                && b.members.len() <= 10000,
            "invalid historical breadth contract",
        )?;
        let mut members = BTreeSet::new();
        let mut weights = Decimal::ZERO;
        for m in &b.members {
            require(
                !m.instrument_id.trim().is_empty()
                    && members.insert(&m.instrument_id)
                    && m.market_cap_weight > Decimal::ZERO
                    && m.market_cap_weight <= Decimal::ONE
                    && m.total_return_fraction >= -Decimal::ONE
                    && m.available_at >= b.period_end
                    && !m.source_ref.trim().is_empty(),
                "invalid historical breadth member",
            )?;
            weights = add(weights, m.market_cap_weight)?;
        }
        require(
            weights == Decimal::ONE,
            "historical breadth weights must sum exactly to one",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod cross_asset_acceptance {
    use super::*;
    use chrono::Duration;
    use trady_domain::Currency;
    fn time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T20:00:00Z")
            .unwrap()
            .to_utc()
    }
    fn observation(value: Decimal, end: DateTime<Utc>, name: &str) -> CrossAssetObservation {
        CrossAssetObservation {
            period_start: end - Duration::days(1),
            period_end: end,
            published_at: end,
            available_at: end,
            ingested_at: end,
            value: Some(value),
            missing_reason: None,
            vintage: "original".into(),
            source_ref: name.into(),
        }
    }
    fn series(id: &str, kind: CrossAssetKind, values: &[i64]) -> CrossAssetSeries {
        CrossAssetSeries {
            id: id.into(),
            entity_id: id.into(),
            kind,
            unit: if matches!(
                kind,
                CrossAssetKind::PrimaryNetFlow | CrossAssetKind::AssetsUnderManagement
            ) {
                CrossAssetUnit::CurrencyAmount
            } else {
                CrossAssetUnit::Fraction
            },
            currency: Some(Currency::new("USD").unwrap()),
            currency_pair: None,
            calendar_id: "XNYS-America/New_York".into(),
            venue: "consolidated".into(),
            adjustment_basis: "total-return-v1".into(),
            max_age_seconds: 3 * 86400,
            required: true,
            observations: values
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    observation(
                        Decimal::new(*v, 3),
                        time() - Duration::days((values.len() - 1 - i) as i64),
                        &format!("{id}-{i}"),
                    )
                })
                .collect(),
        }
    }
    fn fixture() -> CrossAssetInput {
        CrossAssetInput {
            region: "US".into(),
            effective_horizon_seconds: 86400,
            series: vec![
                series(
                    "sector",
                    CrossAssetKind::PeriodTotalReturn,
                    &[10, -20, 30, 5, -10, 25],
                ),
                series(
                    "market",
                    CrossAssetKind::PeriodTotalReturn,
                    &[8, -10, 15, 7, -4, 20],
                ),
            ],
            pairs: vec![CrossAssetPair {
                asset_series_id: "sector".into(),
                benchmark_series_id: "market".into(),
                lookback_start: time() - Duration::days(10),
                minimum_samples: 4,
                short_window_samples: 4,
            }],
            flows: vec![],
            stress: vec![],
            breadth: None,
        }
    }
    #[test]
    fn trailing_statistics_have_uncertainty_and_exact_relative_returns() {
        let out = assess(&fixture(), time()).unwrap();
        let p = &out.pairs[0];
        assert_eq!(p.samples, 6);
        assert!(p.beta.unwrap() > 1.0);
        assert!(p.correlation.unwrap() > 0.8);
        assert!(p.correlation_interval_95.as_ref().unwrap().upper <= 1.0);
        assert!(p.relative_compounded_return.is_some());
        assert!(p.correlation_change.is_some());
    }
    #[test]
    fn future_revisions_do_not_rewrite_replay() {
        let mut i = fixture();
        let mut o = i.series[0].observations.last().unwrap().clone();
        o.value = Some(Decimal::new(9, 1));
        o.published_at += Duration::days(1);
        o.available_at = o.published_at;
        o.ingested_at = o.published_at;
        o.source_ref = "future-revision".into();
        i.series[0].observations.push(o);
        let out = assess(&i, time()).unwrap();
        assert_eq!(out.aligned[0].value, Some(Decimal::new(25, 3)));
        assert!(!out.pairs[0].source_refs.contains(&"future-revision".into()));
    }
    #[test]
    fn sparse_optional_missing_degrades_and_required_missing_abstains() {
        let mut i = fixture();
        i.series[1].observations.clear();
        i.series[1].required = false;
        assert_eq!(
            assess(&i, time()).unwrap().status,
            CrossAssetStatus::Partial
        );
        i.series[1].required = true;
        assert_eq!(
            assess(&i, time()).unwrap().status,
            CrossAssetStatus::Abstain
        );
        assert_eq!(
            assess(&i, time()).unwrap().sparse_missing_series,
            vec!["market"]
        );
    }
    #[test]
    fn stale_calendar_and_currency_gaps_are_not_filled() {
        let mut i = fixture();
        i.series[1].calendar_id = "XLON-Europe/London".into();
        assert!(assess(&i, time()).unwrap().pairs[0].beta.is_none());
        i = fixture();
        i.series[1].currency = Some(Currency::new("EUR").unwrap());
        assert!(assess(&i, time()).unwrap().pairs[0].correlation.is_none());
        i = fixture();
        assert_eq!(
            assess(&i, time() + Duration::days(4)).unwrap().status,
            CrossAssetStatus::Abstain
        );
    }
    #[test]
    fn singular_covariance_is_unknown() {
        let mut i = fixture();
        for s in &mut i.series {
            for o in &mut s.observations {
                o.value = Some(Decimal::new(1, 2));
            }
        }
        let p = assess(&i, time()).unwrap().pairs.remove(0);
        assert!(p.beta.is_none());
        assert!(p.unavailable_reasons.iter().any(|r| r.contains("singular")));
    }
    #[test]
    fn etf_secondary_volume_cannot_be_primary_flow() {
        let mut i = fixture();
        let mut volume = series("volume", CrossAssetKind::SecondaryTradingVolume, &[100]);
        volume.unit = CrossAssetUnit::Shares;
        i.series.push(volume);
        i.series.push(series(
            "aum",
            CrossAssetKind::AssetsUnderManagement,
            &[1000],
        ));
        i.flows.push(CrossAssetFlow {
            flow_series_id: "volume".into(),
            assets_series_id: "aum".into(),
            performance_series_id: None,
        });
        assert!(assess(&i, time()).unwrap_err().0.contains("secondary ETF"));
    }
    #[test]
    fn primary_flows_are_separate_from_performance_driven_aum() {
        let mut i = fixture();
        let mut flow = series("flow", CrossAssetKind::PrimaryNetFlow, &[]);
        flow.observations = vec![observation(Decimal::from(10), time(), "flow")];
        flow.entity_id = "fund-42".into();
        let mut aum = series("aum", CrossAssetKind::AssetsUnderManagement, &[]);
        aum.observations = vec![
            observation(Decimal::from(100), time() - Duration::days(1), "aum-open"),
            observation(Decimal::from(112), time(), "aum-close"),
        ];
        aum.entity_id = "fund-42".into();
        let mut performance = series("fund-return", CrossAssetKind::PeriodTotalReturn, &[]);
        performance.observations = vec![observation(Decimal::new(2, 2), time(), "return")];
        performance.entity_id = "fund-42".into();
        i.series.extend([flow, aum, performance]);
        i.flows.push(CrossAssetFlow {
            flow_series_id: "flow".into(),
            assets_series_id: "aum".into(),
            performance_series_id: Some("fund-return".into()),
        });
        let out = assess(&i, time()).unwrap();
        let f = &out.flows[0];
        assert_eq!(f.flow_to_opening_assets_fraction, Some(Decimal::new(1, 1)));
        assert_eq!(f.performance_driven_assets_change, Some(Decimal::from(2)));
        assert_eq!(f.unexplained_assets_change, Some(Decimal::ZERO));
    }
    #[test]
    fn delayed_primary_flow_is_absent_before_publication() {
        let mut i = fixture();
        let mut flow = series("flow", CrossAssetKind::PrimaryNetFlow, &[100]);
        let o = &mut flow.observations[0];
        o.published_at += Duration::hours(12);
        o.available_at = o.published_at;
        o.ingested_at = o.published_at;
        i.series.push(flow);
        i.series.push(series(
            "aum",
            CrossAssetKind::AssetsUnderManagement,
            &[1000],
        ));
        for s in &mut i.series {
            if s.id == "flow" || s.id == "aum" {
                s.entity_id = "same-fund".into();
            }
        }
        i.flows.push(CrossAssetFlow {
            flow_series_id: "flow".into(),
            assets_series_id: "aum".into(),
            performance_series_id: None,
        });
        assert!(
            assess(&i, time()).unwrap().flows[0]
                .net_primary_flow
                .is_none()
        );
        assert_eq!(
            assess(&i, time() + Duration::hours(12)).unwrap().flows[0].net_primary_flow,
            Some(Decimal::new(100, 3))
        );
    }
    #[test]
    fn stress_reports_per_series_movement_without_directional_vote() {
        let mut i = fixture();
        i.series.push(series(
            "credit",
            CrossAssetKind::YieldSpreadFraction,
            &[20, 35],
        ));
        i.stress.push(CrossAssetStress {
            series_id: "credit".into(),
            horizon_seconds: 86400,
            method: CrossAssetStressMethod::Difference,
            direction: CrossAssetStressDirection::Rising,
            threshold: Decimal::new(1, 2),
        });
        let out = assess(&i, time()).unwrap();
        assert_eq!(out.stress[0].change, Some(Decimal::new(15, 3)));
        assert_eq!(out.stress[0].threshold_exceeded, Some(true));
    }
    #[test]
    fn exact_interval_join_omits_missing_days() {
        let mut i = fixture();
        i.series[0].observations.remove(1);
        i.series[1].observations.remove(2);
        assert_eq!(assess(&i, time()).unwrap().pairs[0].samples, 4);
    }
    #[test]
    fn strict_json_and_semantic_units_are_enforced() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value["series"][0]["buy"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CrossAssetInput>(value).is_err());
        let mut i = fixture();
        i.series[0].unit = CrossAssetUnit::Shares;
        assert!(assess(&i, time()).is_err());
    }

    #[test]
    fn primary_flow_and_aum_must_refer_to_the_same_fund() {
        let mut i = fixture();
        i.series
            .push(series("flow", CrossAssetKind::PrimaryNetFlow, &[100]));
        i.series.push(series(
            "other-fund-aum",
            CrossAssetKind::AssetsUnderManagement,
            &[1000],
        ));
        i.flows.push(CrossAssetFlow {
            flow_series_id: "flow".into(),
            assets_series_id: "other-fund-aum".into(),
            performance_series_id: None,
        });
        let out = assess(&i, time()).unwrap();
        assert!(out.flows[0].flow_to_opening_assets_fraction.is_none());
        assert!(
            out.flows[0]
                .unavailable_reasons
                .iter()
                .any(|r| r.contains("different funds"))
        );
    }

    #[test]
    fn breadth_uses_historical_weights_without_future_membership() {
        let mut i = fixture();
        i.breadth = Some(CrossAssetBreadth {
            period_start: time() - Duration::days(1),
            period_end: time(),
            weights_effective_at: time() - Duration::days(2),
            weights_available_at: time() - Duration::days(2),
            published_at: time(),
            available_at: time(),
            weights_version: "historical-v1".into(),
            source_ref: "historical-weights".into(),
            members: vec![
                CrossAssetBreadthMember {
                    instrument_id: "a".into(),
                    market_cap_weight: Decimal::new(8, 1),
                    total_return_fraction: Decimal::new(1, 2),
                    available_at: time(),
                    source_ref: "a-return".into(),
                },
                CrossAssetBreadthMember {
                    instrument_id: "b".into(),
                    market_cap_weight: Decimal::new(2, 1),
                    total_return_fraction: Decimal::new(-2, 2),
                    available_at: time(),
                    source_ref: "b-return".into(),
                },
            ],
        });
        let b = assess(&i, time()).unwrap().breadth.unwrap();
        assert_eq!(b.positive_constituent_fraction, Decimal::new(5, 1));
        assert_eq!(b.positive_market_cap_weight_fraction, Decimal::new(8, 1));
        assert_eq!(b.market_cap_weighted_return_fraction, Decimal::new(4, 3));
        i.breadth.as_mut().unwrap().weights_available_at = time();
        assert!(assess(&i, time()).is_err());
    }

    #[test]
    fn published_missing_revision_invalidates_earlier_value() {
        let mut i = fixture();
        let mut correction = i.series[0].observations.last().unwrap().clone();
        correction.published_at += Duration::hours(1);
        correction.available_at = correction.published_at;
        correction.ingested_at = correction.published_at;
        correction.value = None;
        correction.missing_reason = Some("source withdrew erroneous value".into());
        correction.vintage = "withdrawn".into();
        i.series[0].observations.push(correction);
        let out = assess(&i, time() + Duration::hours(1)).unwrap();
        assert_eq!(out.status, CrossAssetStatus::Abstain);
        assert_eq!(out.aligned[0].value, None);
        assert!(
            out.aligned[0]
                .unavailable_reason
                .as_ref()
                .unwrap()
                .contains("withdrew")
        );
    }

    #[test]
    fn overlapping_return_windows_cannot_be_compounded_as_independent_periods() {
        let mut i = fixture();
        i.series[0].observations[1].period_start -= Duration::days(1);
        assert!(assess(&i, time()).unwrap_err().0.contains("nonoverlapping"));
    }

    #[test]
    fn missing_stress_baseline_is_not_replaced_by_an_older_value() {
        let mut i = fixture();
        let mut credit = series("credit", CrossAssetKind::YieldSpreadFraction, &[10, 20, 35]);
        credit.observations[1].value = None;
        credit.observations[1].missing_reason = Some("withdrawn observation".into());
        i.series.push(credit);
        i.stress.push(CrossAssetStress {
            series_id: "credit".into(),
            horizon_seconds: 86400,
            method: CrossAssetStressMethod::Difference,
            direction: CrossAssetStressDirection::Rising,
            threshold: Decimal::new(1, 2),
        });
        let out = assess(&i, time()).unwrap();
        assert_eq!(out.stress[0].change, None);
        assert_eq!(out.stress[0].threshold_exceeded, None);
    }
}

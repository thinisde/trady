//! Pure reported-position ratios. No daily-volume substitution or implied covering deadline.

use crate::{div, sub};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::intelligence::short_interest::*;
use trady_domain::{Result, require};

pub fn assess(input: &ShortInterestInput, at: DateTime<Utc>) -> Result<ShortInterestAssessment> {
    validate(input)?;
    let mut out = ShortInterestAssessment {
        at,
        status: ShortInterestStatus::Abstain,
        settlement_date: None,
        report_published_at: None,
        report_available_at: None,
        release_vintage: None,
        report_age_seconds: None,
        publication_lag_calendar_days: None,
        reported_shares_short: None,
        verified_float_shares: None,
        float_methodology: None,
        float_effective_date: None,
        short_to_float_fraction: None,
        volume_based_days_to_cover: None,
        average_daily_volume_shares: None,
        average_volume_window_start: None,
        average_volume_window_end: None,
        average_volume_completed_sessions: None,
        average_volume_scope: None,
        average_volume_venue: None,
        reported_position_change_shares: None,
        reported_position_change_fraction: None,
        borrow_fee_annualized_fraction: None,
        borrow_fee_historical_percentile: None,
        borrow_fee_historical_range: None,
        borrow_utilization_fraction: None,
        borrow_shares_on_loan: None,
        borrow_hard_to_borrow: None,
        borrow_methodology: None,
        opening_gap_fraction: None,
        relative_volume: None,
        realized_volatility_annualized: None,
        realized_volatility_method: None,
        flags: vec![],
        source_refs: vec![],
        issues: vec![],
    };
    // Freeze each settlement's latest *available* revision; never interpolate daily positions.
    let mut releases = BTreeMap::new();
    for report in &input.reports {
        if report.available_at <= at && report.ingested_at <= at {
            let previous: Option<&&ShortPositionReport> = releases.get(&report.settlement_date);
            if previous.is_none_or(|r| {
                (report.published_at, report.available_at, report.ingested_at)
                    > (r.published_at, r.available_at, r.ingested_at)
            }) {
                releases.insert(report.settlement_date, report);
            }
        }
    }
    let Some(current) = releases.values().next_back().copied() else {
        issue(
            &mut out,
            "report",
            "no reported positions were available at the decision",
            &[],
        );
        return Ok(out);
    };
    let age = (at - current.published_at).num_seconds();
    out.settlement_date = Some(current.settlement_date);
    out.report_published_at = Some(current.published_at);
    out.report_available_at = Some(current.available_at);
    out.release_vintage = Some(current.release_vintage.clone());
    out.report_age_seconds = Some(age);
    out.publication_lag_calendar_days =
        Some((current.published_at.date_naive() - current.settlement_date).num_days());
    out.source_refs.push(current.source_ref.clone());
    out.reported_shares_short = Some(current.shares_short);
    if !fresh(
        current.published_at,
        current.available_at,
        at,
        input.max_report_age_seconds,
    ) {
        issue(
            &mut out,
            "report",
            "published report exceeds the frozen freshness limit",
            &[&current.source_ref],
        );
        return Ok(out);
    }
    out.status = ShortInterestStatus::Available;
    let basis = input.adjustment_basis.as_deref();
    if basis.is_none() || current.adjustment_basis.as_deref() != basis {
        out.status = ShortInterestStatus::ReviewRequired;
        issue(
            &mut out,
            "adjustment_basis",
            "unknown or incompatible share adjustment; ratios withheld",
            &[&current.source_ref],
        );
        return Ok(out);
    }
    if let Some(float) = &input.float {
        if float.available_at > at || float.effective_date > current.settlement_date {
            issue(
                &mut out,
                "short_to_float_fraction",
                "float was unavailable or effective after settlement",
                &[&float.source_ref],
            );
        } else if float.adjustment_basis.as_deref() != basis || float.shares <= Decimal::ZERO {
            out.status = ShortInterestStatus::ReviewRequired;
            issue(
                &mut out,
                "short_to_float_fraction",
                "nonpositive float or unknown/incompatible adjustment",
                &[&float.source_ref],
            );
        } else if float
            .issued_shares
            .is_some_and(|n| n <= Decimal::ZERO || current.shares_short > n || float.shares > n)
        {
            out.status = ShortInterestStatus::ReviewRequired;
            issue(
                &mut out,
                "shares",
                "reported short positions or float exceed plausible issued shares",
                &[&current.source_ref, &float.source_ref],
            );
        } else {
            out.short_to_float_fraction = Some(div(current.shares_short, float.shares)?);
            out.verified_float_shares = Some(float.shares);
            out.float_methodology = Some(float.methodology.clone());
            out.float_effective_date = Some(float.effective_date);
            out.source_refs.push(float.source_ref.clone());
            if float.issued_shares.is_none() {
                issue(
                    &mut out,
                    "issued_shares",
                    "issued shares absent; plausibility check unavailable",
                    &[&float.source_ref],
                );
            }
        }
    } else {
        issue(
            &mut out,
            "short_to_float_fraction",
            "verified float is missing",
            &[],
        );
    }
    // An inconsistent float/adjustment invalidates share-derived features together.
    if out.status == ShortInterestStatus::ReviewRequired {
        return Ok(out);
    }
    if let Some(volume) = &input.average_volume {
        if volume.available_at > at || volume.window_end > at.date_naive() {
            issue(
                &mut out,
                "volume_based_days_to_cover",
                "volume lookback is incomplete or not yet available",
                &[&volume.source_ref],
            );
        } else if volume.adjustment_basis.as_deref() != basis {
            out.status = ShortInterestStatus::ReviewRequired;
            out.short_to_float_fraction = None;
            issue(
                &mut out,
                "volume_based_days_to_cover",
                "volume share adjustment is unknown or incompatible",
                &[&volume.source_ref],
            );
            return Ok(out);
        } else if !fresh(
            volume.published_at,
            volume.available_at,
            at,
            input.max_context_age_seconds,
        ) {
            issue(
                &mut out,
                "volume_based_days_to_cover",
                "volume lookback is stale",
                &[&volume.source_ref],
            );
        } else if volume.daily_shares == Decimal::ZERO {
            issue(
                &mut out,
                "volume_based_days_to_cover",
                "zero average volume; heuristic is undefined",
                &[&volume.source_ref],
            );
        } else {
            out.volume_based_days_to_cover = Some(div(current.shares_short, volume.daily_shares)?);
            out.average_daily_volume_shares = Some(volume.daily_shares);
            out.average_volume_window_start = Some(volume.window_start);
            out.average_volume_window_end = Some(volume.window_end);
            out.average_volume_completed_sessions = Some(volume.completed_sessions);
            out.average_volume_scope = Some(volume.scope);
            out.average_volume_venue = volume.venue.clone();
            out.source_refs.push(volume.source_ref.clone());
        }
    } else {
        issue(
            &mut out,
            "volume_based_days_to_cover",
            "frozen average-volume lookback is missing",
            &[],
        );
    }
    if let Some(previous) = releases
        .range(..current.settlement_date)
        .next_back()
        .map(|(_, r)| *r)
    {
        if previous.adjustment_basis.as_deref() == basis {
            let change = sub(current.shares_short, previous.shares_short)?;
            out.reported_position_change_shares = Some(change);
            out.source_refs.push(previous.source_ref.clone());
            if previous.shares_short > Decimal::ZERO {
                out.reported_position_change_fraction = Some(div(change, previous.shares_short)?);
            } else {
                issue(
                    &mut out,
                    "reported_position_change_fraction",
                    "previous reported positions were zero",
                    &[&previous.source_ref],
                );
            }
        } else {
            issue(
                &mut out,
                "reported_position_change_shares",
                "previous release uses an incompatible adjustment",
                &[&previous.source_ref],
            );
        }
    } else {
        issue(
            &mut out,
            "reported_position_change_shares",
            "no earlier available settlement release",
            &[],
        );
    }
    borrow_features(input, at, &mut out)?;
    market_features(input, at, &mut out)?;
    let t = &input.thresholds;
    for (present, flag) in [
        (
            out.short_to_float_fraction
                .is_some_and(|v| v >= t.short_to_float_fraction),
            ShortCrowdingFlag::HighReportedShortToFloat,
        ),
        (
            out.volume_based_days_to_cover
                .is_some_and(|v| v >= t.days_to_cover),
            ShortCrowdingFlag::ElevatedVolumeBasedDaysToCover,
        ),
        (
            out.opening_gap_fraction
                .is_some_and(|v| v.abs() >= t.absolute_gap_fraction),
            ShortCrowdingFlag::LargeOpeningGap,
        ),
        (
            out.relative_volume.is_some_and(|v| v >= t.relative_volume),
            ShortCrowdingFlag::ElevatedRelativeVolume,
        ),
        (
            out.realized_volatility_annualized
                .is_some_and(|v| v >= t.realized_volatility_annualized),
            ShortCrowdingFlag::HighRealizedVolatility,
        ),
    ] {
        if present {
            out.flags.push(flag);
        }
    }
    out.source_refs.sort();
    out.source_refs.dedup();
    if !out.issues.is_empty() {
        out.status = ShortInterestStatus::Partial;
    }
    Ok(out)
}

fn borrow_features(
    input: &ShortInterestInput,
    at: DateTime<Utc>,
    out: &mut ShortInterestAssessment,
) -> Result<()> {
    let Some(borrow) = input.borrow.as_ref().filter(|b| {
        fresh(
            b.observed_at,
            b.available_at,
            at,
            input.max_context_age_seconds,
        )
    }) else {
        issue(
            out,
            "borrow",
            "licensed borrow context is missing, stale or unpublished",
            &[],
        );
        return Ok(());
    };
    out.source_refs.push(borrow.source_ref.clone());
    out.borrow_fee_annualized_fraction = borrow.fee_annualized_fraction;
    out.borrow_utilization_fraction = borrow.utilization_fraction;
    out.borrow_shares_on_loan = borrow.shares_on_loan;
    out.borrow_hard_to_borrow = borrow.hard_to_borrow;
    out.borrow_methodology = Some(borrow.methodology.clone());
    if borrow.hard_to_borrow == Some(true) {
        out.flags.push(ShortCrowdingFlag::HardToBorrow);
    }
    if borrow.fee_annualized_fraction.is_none() {
        issue(
            out,
            "borrow_fee_annualized_fraction",
            "borrow fee absent; no share-volume imputation",
            &[&borrow.source_ref],
        );
    }
    if borrow.utilization_fraction.is_none() {
        issue(
            out,
            "borrow_utilization_fraction",
            "borrow utilization absent",
            &[&borrow.source_ref],
        );
    }
    if borrow.shares_on_loan.is_none() {
        issue(
            out,
            "borrow_shares_on_loan",
            "licensed shares-on-loan absent",
            &[&borrow.source_ref],
        );
    }
    if borrow.hard_to_borrow.is_none() {
        issue(
            out,
            "borrow_hard_to_borrow",
            "hard-to-borrow status unknown",
            &[&borrow.source_ref],
        );
    }
    let mut history = BTreeMap::new();
    for h in &input.borrow_history {
        if h.observed_at >= input.history_start
            && h.observed_at < borrow.observed_at
            && h.available_at <= at
            && h.methodology == borrow.methodology
        {
            let previous: Option<&&ShortBorrowObservation> = history.get(&h.observed_at);
            if previous
                .is_none_or(|p| (h.published_at, h.available_at) > (p.published_at, p.available_at))
            {
                history.insert(h.observed_at, h);
            }
        }
    }
    let fees: Vec<_> = history
        .values()
        .filter_map(|h| h.fee_annualized_fraction.map(|v| (v, &h.source_ref)))
        .collect();
    if let (Some(fee), Some(min), Some(max)) = (
        borrow.fee_annualized_fraction,
        fees.iter().map(|(v, _)| *v).min(),
        fees.iter().map(|(v, _)| *v).max(),
    ) {
        out.borrow_fee_historical_percentile = Some(div(
            Decimal::from(fees.iter().filter(|(v, _)| *v <= fee).count() as u64),
            Decimal::from(fees.len() as u64),
        )?);
        out.borrow_fee_historical_range = Some(sub(max, min)?);
        out.source_refs
            .extend(fees.iter().map(|(_, r)| (*r).clone()));
    } else {
        issue(
            out,
            "borrow_fee_historical_percentile",
            "no comparable available borrow-fee history",
            &[],
        );
    }
    Ok(())
}

fn market_features(
    input: &ShortInterestInput,
    at: DateTime<Utc>,
    out: &mut ShortInterestAssessment,
) -> Result<()> {
    let Some(m) = input.market_context.as_ref().filter(|m| {
        fresh(
            m.observed_at,
            m.available_at,
            at,
            input.max_context_age_seconds,
        )
    }) else {
        issue(
            out,
            "market_context",
            "gap/volume/volatility context missing, stale or unpublished",
            &[],
        );
        return Ok(());
    };
    if m.adjustment_basis != input.adjustment_basis {
        issue(
            out,
            "market_context",
            "market price/volume adjustment differs from reported shares",
            &[&m.source_ref],
        );
        return Ok(());
    }
    out.opening_gap_fraction = Some(sub(div(m.session_open, m.previous_close)?, Decimal::ONE)?);
    if m.comparable_baseline_volume_shares > Decimal::ZERO {
        out.relative_volume = Some(div(
            m.recent_volume_shares,
            m.comparable_baseline_volume_shares,
        )?);
    } else {
        issue(
            out,
            "relative_volume",
            "zero comparable volume baseline",
            &[&m.source_ref],
        );
    }
    out.realized_volatility_annualized = m.realized_volatility_annualized;
    out.realized_volatility_method = m.volatility_method.clone();
    if out.realized_volatility_annualized.is_none() {
        issue(
            out,
            "realized_volatility_annualized",
            "historical volatility absent",
            &[&m.source_ref],
        );
    }
    out.source_refs.push(m.source_ref.clone());
    Ok(())
}

fn issue(out: &mut ShortInterestAssessment, field: &str, reason: &str, refs: &[&str]) {
    out.issues.push(ShortInterestIssue {
        field: field.into(),
        reason: reason.into(),
        source_refs: refs.iter().map(|s| (*s).into()).collect(),
    });
}

fn fresh(
    observed: DateTime<Utc>,
    available: DateTime<Utc>,
    at: DateTime<Utc>,
    max_age: u64,
) -> bool {
    observed <= at
        && available <= at
        && i64::try_from(max_age)
            .ok()
            .and_then(chrono::TimeDelta::try_seconds)
            .is_some_and(|limit| at - observed <= limit)
}

fn times(
    observed: DateTime<Utc>,
    published: DateTime<Utc>,
    available: DateTime<Utc>,
    reference: &str,
) -> Result<()> {
    require(
        observed <= published
            && published <= available
            && !reference.trim().is_empty()
            && reference.len() <= 2048,
        "invalid short-interest fact timestamps/reference",
    )
}

fn validate(input: &ShortInterestInput) -> Result<()> {
    require(
        !input.issuer_id.trim().is_empty()
            && input.reports.len() <= 4096
            && input.borrow_history.len() <= 4096
            && input.max_report_age_seconds > 0
            && input.max_context_age_seconds > 0
            && input
                .adjustment_basis
                .as_ref()
                .is_none_or(|b| !b.trim().is_empty()),
        "invalid short-interest input/bounds",
    )?;
    let t = &input.thresholds;
    require(
        [
            t.short_to_float_fraction,
            t.days_to_cover,
            t.absolute_gap_fraction,
            t.relative_volume,
            t.realized_volatility_annualized,
        ]
        .iter()
        .all(|v| *v > Decimal::ZERO),
        "research thresholds must be positive",
    )?;
    let mut revisions = BTreeSet::new();
    for r in &input.reports {
        require(
            r.metric == ShortPositionMetric::ReportedOutstandingPositions,
            "daily short-sale volume is not reported outstanding short interest",
        )?;
        require(
            r.shares_short >= Decimal::ZERO
                && r.settlement_date <= r.published_at.date_naive()
                && r.published_at <= r.available_at
                && r.available_at <= r.ingested_at
                && !r.source_ref.trim().is_empty()
                && !r.release_vintage.trim().is_empty(),
            "invalid short-position release",
        )?;
        require(
            revisions.insert((
                r.settlement_date,
                r.published_at,
                r.available_at,
                r.ingested_at,
            )),
            "ambiguous duplicate short-position revision",
        )?;
    }
    if let Some(f) = &input.float {
        require(
            f.effective_date <= f.published_at.date_naive()
                && f.published_at <= f.available_at
                && !f.methodology.trim().is_empty()
                && !f.source_ref.trim().is_empty(),
            "invalid float methodology/times/reference",
        )?;
    }
    if let Some(v) = &input.average_volume {
        require(
            v.daily_shares >= Decimal::ZERO
                && v.window_start <= v.window_end
                && v.window_end <= v.published_at.date_naive()
                && v.published_at <= v.available_at
                && v.completed_sessions > 0
                && !v.source_ref.trim().is_empty()
                && (v.scope != ShortVolumeScope::SingleVenue
                    || v.venue.as_ref().is_some_and(|s| !s.trim().is_empty())),
            "invalid frozen completed volume lookback",
        )?;
    }
    let mut borrow_revisions = BTreeSet::new();
    for b in input.borrow.iter().chain(&input.borrow_history) {
        times(b.observed_at, b.published_at, b.available_at, &b.source_ref)?;
        require(
            borrow_revisions.insert((
                &b.methodology,
                b.observed_at,
                b.published_at,
                b.available_at,
            )),
            "ambiguous duplicate borrow revision",
        )?;
        require(
            !b.methodology.trim().is_empty()
                && b.fee_annualized_fraction.is_none_or(|v| v >= Decimal::ZERO)
                && b.shares_on_loan.is_none_or(|v| v >= Decimal::ZERO)
                && b.utilization_fraction
                    .is_none_or(|v| v >= Decimal::ZERO && v <= Decimal::ONE),
            "invalid borrow observation",
        )?;
    }
    if let Some(m) = &input.market_context {
        times(m.observed_at, m.published_at, m.available_at, &m.source_ref)?;
        require(
            m.session_open > Decimal::ZERO
                && m.previous_close > Decimal::ZERO
                && m.recent_volume_shares >= Decimal::ZERO
                && m.comparable_baseline_volume_shares >= Decimal::ZERO
                && m.realized_volatility_annualized
                    .is_none_or(|v| v >= Decimal::ZERO)
                && (m.realized_volatility_annualized.is_none()
                    || m.volatility_method
                        .as_ref()
                        .is_some_and(|s| !s.trim().is_empty())),
            "invalid gap/volume/volatility observation",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod short_interest_acceptance {
    use super::*;
    use chrono::Duration;

    fn time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T20:00:00Z")
            .unwrap()
            .to_utc()
    }
    fn fixture() -> ShortInterestInput {
        let at = time();
        let report = ShortPositionReport {
            settlement_date: at.date_naive() - Duration::days(15),
            published_at: at - Duration::days(4),
            available_at: at - Duration::days(4),
            ingested_at: at - Duration::days(4),
            shares_short: Decimal::from(6_000_000),
            metric: ShortPositionMetric::ReportedOutstandingPositions,
            adjustment_basis: Some("split-v1".into()),
            source_ref: "report-current".into(),
            release_vintage: "original".into(),
        };
        ShortInterestInput {
            issuer_id: "issuer-42".into(),
            adjustment_basis: Some("split-v1".into()),
            reports: vec![report],
            float: Some(ShortFloatObservation {
                shares: Decimal::from(30_000_000),
                issued_shares: Some(Decimal::from(40_000_000)),
                effective_date: at.date_naive() - Duration::days(15),
                published_at: at - Duration::days(5),
                available_at: at - Duration::days(5),
                adjustment_basis: Some("split-v1".into()),
                methodology: "verified-free-float".into(),
                exclusions: "restricted-shares".into(),
                source_ref: "float-v1".into(),
            }),
            average_volume: Some(ShortAverageVolume {
                daily_shares: Decimal::from(1_500_000),
                window_start: at.date_naive() - Duration::days(30),
                window_end: at.date_naive() - Duration::days(1),
                completed_sessions: 20,
                scope: ShortVolumeScope::Consolidated,
                venue: None,
                adjustment_basis: Some("split-v1".into()),
                published_at: at - Duration::hours(12),
                available_at: at - Duration::hours(12),
                source_ref: "volume-20d".into(),
            }),
            borrow: None,
            borrow_history: vec![],
            market_context: None,
            max_report_age_seconds: 30 * 86400,
            max_context_age_seconds: 86400,
            history_start: at - Duration::days(365),
            thresholds: ShortCrowdingThresholds {
                short_to_float_fraction: Decimal::new(2, 1),
                days_to_cover: Decimal::from(4),
                absolute_gap_fraction: Decimal::new(5, 2),
                relative_volume: Decimal::from(2),
                realized_volatility_annualized: Decimal::ONE,
            },
        }
    }
    #[test]
    fn exact_decimal_ratios_and_no_borrow_imputation() {
        let out = assess(&fixture(), time()).unwrap();
        assert_eq!(out.short_to_float_fraction, Some(Decimal::new(2, 1)));
        assert_eq!(out.volume_based_days_to_cover, Some(Decimal::from(4)));
        assert_eq!(out.publication_lag_calendar_days, Some(11));
        assert_eq!(out.borrow_fee_annualized_fraction, None);
        assert!(
            out.flags
                .contains(&ShortCrowdingFlag::HighReportedShortToFloat)
        );
        assert!(out.issues.iter().any(|i| i.field == "borrow"));
    }
    #[test]
    fn publication_lag_and_corrections_are_point_in_time() {
        let mut input = fixture();
        let mut future = input.reports[0].clone();
        future.shares_short = Decimal::from(12_000_000);
        future.published_at = time() + Duration::seconds(1);
        future.available_at = future.published_at;
        future.ingested_at = future.published_at;
        future.release_vintage = "correction".into();
        future.source_ref = "future".into();
        input.reports.push(future);
        assert_eq!(
            assess(&input, time()).unwrap().short_to_float_fraction,
            Some(Decimal::new(2, 1))
        );
        assert_eq!(
            assess(&input, time() + Duration::seconds(1))
                .unwrap()
                .short_to_float_fraction,
            Some(Decimal::new(4, 1))
        );
        assert_eq!(
            assess(&input, time() - Duration::days(5)).unwrap().status,
            ShortInterestStatus::Abstain
        );
    }
    #[test]
    fn daily_short_volume_is_a_hard_error() {
        let mut input = fixture();
        input.reports[0].metric = ShortPositionMetric::DailyShortSaleVolume;
        assert!(
            assess(&input, time())
                .unwrap_err()
                .0
                .contains("daily short-sale")
        );
    }
    #[test]
    fn zero_volume_is_unknown_and_share_inconsistency_requires_review() {
        let mut input = fixture();
        input.average_volume.as_mut().unwrap().daily_shares = Decimal::ZERO;
        assert_eq!(
            assess(&input, time()).unwrap().volume_based_days_to_cover,
            None
        );
        input.float.as_mut().unwrap().issued_shares = Some(Decimal::from(1));
        let out = assess(&input, time()).unwrap();
        assert_eq!(out.status, ShortInterestStatus::ReviewRequired);
        assert!(out.short_to_float_fraction.is_none());
    }
    #[test]
    fn split_mismatch_and_unknown_adjustment_are_not_clipped() {
        let mut input = fixture();
        input.adjustment_basis = None;
        assert_eq!(
            assess(&input, time()).unwrap().status,
            ShortInterestStatus::ReviewRequired
        );
        input = fixture();
        input.float.as_mut().unwrap().adjustment_basis = Some("pre-split".into());
        assert_eq!(
            assess(&input, time()).unwrap().status,
            ShortInterestStatus::ReviewRequired
        );
    }
    #[test]
    fn prior_positions_are_distinct_releases_and_signed() {
        let mut input = fixture();
        let mut prior = input.reports[0].clone();
        prior.settlement_date -= Duration::days(15);
        prior.published_at -= Duration::days(15);
        prior.available_at = prior.published_at;
        prior.ingested_at = prior.published_at;
        prior.shares_short = Decimal::from(8_000_000);
        prior.source_ref = "previous-release".into();
        input.reports.push(prior);
        let out = assess(&input, time()).unwrap();
        assert_eq!(
            out.reported_position_change_shares,
            Some(Decimal::from(-2_000_000))
        );
        assert_eq!(
            out.reported_position_change_fraction,
            Some(Decimal::new(-25, 2))
        );
    }
    #[test]
    fn checked_decimal_overflow_is_rejected() {
        let mut input = fixture();
        input.float.as_mut().unwrap().shares = Decimal::new(1, 28);
        input.float.as_mut().unwrap().issued_shares = None;
        input.reports[0].shares_short = Decimal::MAX;
        assert!(assess(&input, time()).is_err());
    }
    #[test]
    fn unknown_fields_and_numeric_financial_json_are_rejected() {
        let input = fixture();
        let mut json = serde_json::to_value(&input).unwrap();
        json["order_authority"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ShortInterestInput>(json).is_err());
        let mut json = serde_json::to_value(input).unwrap();
        json["reports"][0]["shares_short"] = serde_json::json!(6000000);
        assert!(serde_json::from_value::<ShortInterestInput>(json).is_err());
    }

    #[test]
    fn borrow_percentile_requires_same_available_methodology() {
        let mut input = fixture();
        let observation = |fee, days, method: &str| ShortBorrowObservation {
            fee_annualized_fraction: Some(Decimal::new(fee, 2)),
            utilization_fraction: Some(Decimal::new(9, 1)),
            shares_on_loan: None,
            hard_to_borrow: Some(true),
            observed_at: time() - Duration::days(days),
            published_at: time() - Duration::days(days),
            available_at: time() - Duration::days(days),
            methodology: method.into(),
            source_ref: format!("borrow-{fee}-{days}"),
        };
        input.borrow = Some(observation(5, 0, "licensed-v1"));
        input.borrow_history = vec![
            observation(2, 2, "licensed-v1"),
            observation(8, 1, "licensed-v1"),
            observation(1, 1, "other-vendor-v2"),
        ];
        let out = assess(&input, time()).unwrap();
        assert_eq!(
            out.borrow_fee_historical_percentile,
            Some(Decimal::new(5, 1))
        );
        assert_eq!(out.borrow_fee_historical_range, Some(Decimal::new(6, 2)));
        assert!(out.flags.contains(&ShortCrowdingFlag::HardToBorrow));
        assert!(!out.source_refs.contains(&"borrow-1-1".into()));
    }

    #[test]
    fn unavailable_float_remains_absent_and_stale_reports_abstain() {
        let mut input = fixture();
        input.float = None;
        let out = assess(&input, time()).unwrap();
        assert!(out.short_to_float_fraction.is_none());
        assert_eq!(out.volume_based_days_to_cover, Some(Decimal::from(4)));
        input.max_report_age_seconds = 86400;
        assert_eq!(
            assess(&input, time()).unwrap().status,
            ShortInterestStatus::Abstain
        );
    }

    #[test]
    fn gap_volume_and_volatility_are_separate_descriptive_risks() {
        let mut input = fixture();
        input.market_context = Some(ShortMarketContext {
            session_open: Decimal::from(110),
            previous_close: Decimal::from(100),
            recent_volume_shares: Decimal::from(3_000_000),
            comparable_baseline_volume_shares: Decimal::from(1_000_000),
            realized_volatility_annualized: Some(Decimal::new(12, 1)),
            volatility_method: Some("synthetic-20-session-close-to-close".into()),
            adjustment_basis: input.adjustment_basis.clone(),
            observed_at: time(),
            published_at: time(),
            available_at: time(),
            source_ref: "market-context".into(),
        });
        let out = assess(&input, time()).unwrap();
        assert_eq!(out.opening_gap_fraction, Some(Decimal::new(1, 1)));
        assert_eq!(out.relative_volume, Some(Decimal::from(3)));
        assert!(
            out.flags
                .contains(&ShortCrowdingFlag::HighRealizedVolatility)
        );
        assert!(out.flags.contains(&ShortCrowdingFlag::LargeOpeningGap));
    }
}

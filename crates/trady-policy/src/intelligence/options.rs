//! European Black–Scholes with continuous dividend yield and ACT/365 seconds.
//!
//! IV is found by monotonic bisection in [0.000001, 5] annualized volatility,
//! at most 128 iterations, with residual <= max(tick*1e-6, spot*1e-10).
//! The normal CDF uses Abramowitz–Stegun 26.2.17 (absolute error about 7.5e-8).
//! American exercise, ambiguous deliverables and expirations under 60 seconds
//! are explicitly unsupported. Finite floats are confined to this statistical
//! pricing model; quoted amounts and quality checks use checked Decimal math.

use crate::{add, div, sub};
use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::intelligence::options::*;
use trady_domain::{Error, Result, require};

const MODEL: &str = "BLACK_SCHOLES_EUROPEAN_ACT365_BISECTION_V1";
const YEAR_SECONDS: f64 = 365.0 * 86400.0;

struct Solved<'a> {
    quote: &'a OptionsQuote,
    iv: OptionsIvInterval,
    absolute_delta: f64,
    moneyness: Decimal,
}

pub fn assess(input: &OptionsInput, at: DateTime<Utc>) -> Result<OptionsAssessment> {
    validate(input)?;
    let mut out = OptionsAssessment {
        at,
        status: OptionsStatus::Unavailable,
        model: MODEL.into(),
        expiries: vec![],
        term_structure: vec![],
        reference_expiry: None,
        iv_history_rank: None,
        sector_iv_percentile: None,
        iv_to_realized_ratio: None,
        event_variance: None,
        event_variance_interval: None,
        comparison_horizon_seconds: None,
        history_start: input.history_start,
        comparison_method:
            "constant-tenor/method; rank=(IV-min)/(max-min); inclusive empirical sector CDF".into(),
        realized_volatility_method: None,
        comparison_source_refs: vec![],
        issues: vec![],
    };
    let Some(spot) = input.underlying.as_ref() else {
        push(
            &mut out.issues,
            "underlying",
            "underlying quote is missing",
            &[],
        );
        return Ok(out);
    };
    if !fresh(
        spot.observed_at,
        spot.available_at,
        at,
        input.max_quote_age_seconds,
    ) {
        push(
            &mut out.issues,
            "underlying",
            "underlying quote is stale or unavailable",
            &[&spot.source_ref],
        );
        return Ok(out);
    }
    let Some(carry) = input.assumptions.as_ref() else {
        push(
            &mut out.issues,
            "assumptions",
            "risk-free/dividend assumptions are missing; no zero imputation",
            &[],
        );
        return Ok(out);
    };
    if carry.available_at > at || carry.valid_until <= at {
        push(
            &mut out.issues,
            "assumptions",
            "pricing assumptions unavailable or expired",
            &[&carry.source_ref],
        );
        return Ok(out);
    }
    let s = number(spot.price)?;
    let r = carry.risk_free_rate_annualized;
    let q = carry.dividend_yield_annualized;
    let mut groups: BTreeMap<DateTime<Utc>, Vec<Solved<'_>>> = BTreeMap::new();
    for quote in &input.contracts {
        groups.entry(quote.expiry_at).or_default();
        if let Some(reason) = quote_unavailable(input, spot, quote, at)? {
            push(
                &mut out.issues,
                &quote.contract_id,
                reason,
                &[&quote.source_ref],
            );
            continue;
        }
        let years = (quote.expiry_at - at).num_seconds() as f64 / YEAR_SECONDS;
        let k = number(quote.strike)?;
        let tick = number(quote.price_tick)?;
        let bid = solve(s, k, r, q, years, quote.right, number(quote.bid)?, tick);
        let mid_price = div(add(quote.bid, quote.ask)?, Decimal::from(2))?;
        let mid = solve(s, k, r, q, years, quote.right, number(mid_price)?, tick);
        let ask = solve(s, k, r, q, years, quote.right, number(quote.ask)?, tick);
        let (Some((bid, bi, br)), Some((midpoint, mi, mr)), Some((ask, ai, ar))) = (bid, mid, ask)
        else {
            push(
                &mut out.issues,
                &quote.contract_id,
                "IV price bounds/bracket/convergence failed",
                &[&quote.source_ref],
            );
            continue;
        };
        let absolute_delta = delta(s, k, r, q, years, midpoint, quote.right).abs();
        require(absolute_delta.is_finite(), "non-finite option delta")?;
        groups.entry(quote.expiry_at).or_default().push(Solved {
            quote,
            iv: OptionsIvInterval {
                bid,
                midpoint,
                ask,
                solver_iterations: bi.max(mi).max(ai),
                max_price_residual: br.max(mr).max(ar),
            },
            absolute_delta,
            moneyness: sub(div(quote.strike, spot.price)?, Decimal::ONE)?.abs(),
        });
    }
    for (expiry, mut solved) in groups {
        solved.sort_by(|a, b| {
            a.moneyness
                .cmp(&b.moneyness)
                .then(a.quote.contract_id.cmp(&b.quote.contract_id))
        });
        let mut result = OptionsExpiryAssessment {
            expiry_at: expiry,
            atm_iv: None,
            one_standard_deviation_scale: None,
            skew: None,
            used_contract_ids: vec![],
            source_refs: vec![spot.source_ref.clone(), carry.source_ref.clone()],
            oldest_quote_at: None,
            eligible_contracts: solved.len(),
            issues: vec![],
        };
        let atm = solved
            .iter()
            .find(|v| v.moneyness <= input.atm_max_moneyness_fraction);
        if let Some(first) = atm {
            let pair: Vec<_> = solved
                .iter()
                .filter(|v| v.quote.strike == first.quote.strike)
                .collect();
            let years = (expiry - at).num_seconds() as f64 / YEAR_SECONDS;
            let parity_ok = if pair.len() == 2 {
                let call = pair
                    .iter()
                    .find(|v| v.quote.right == OptionsRight::Call)
                    .copied();
                let put = pair
                    .iter()
                    .find(|v| v.quote.right == OptionsRight::Put)
                    .copied();
                if let (Some(c), Some(p)) = (call, put) {
                    let fair =
                        s * (-q * years).exp() - number(c.quote.strike)? * (-r * years).exp();
                    let tolerance = 2.0 * number(c.quote.price_tick.max(p.quote.price_tick))?;
                    let low = number(sub(c.quote.bid, p.quote.ask)?)?;
                    let high = number(sub(c.quote.ask, p.quote.bid)?)?;
                    c.quote.contract_multiplier == p.quote.contract_multiplier
                        && aligned(
                            c.quote.observed_at,
                            p.quote.observed_at,
                            input.max_alignment_seconds,
                        )
                        && fair >= low - tolerance
                        && fair <= high + tolerance
                } else {
                    false
                }
            } else {
                true
            };
            let low = pair.iter().map(|v| v.iv.bid).fold(0.0, f64::max);
            let high = pair.iter().map(|v| v.iv.ask).fold(f64::INFINITY, f64::min);
            if parity_ok && low <= high {
                let iv = OptionsIvInterval {
                    bid: low,
                    midpoint: if pair.len() == 1 {
                        first.iv.midpoint
                    } else {
                        (low + high) / 2.0
                    },
                    ask: high,
                    solver_iterations: pair
                        .iter()
                        .map(|v| v.iv.solver_iterations)
                        .max()
                        .unwrap_or(0),
                    max_price_residual: pair
                        .iter()
                        .map(|v| v.iv.max_price_residual)
                        .fold(0.0, f64::max),
                };
                if input.horizon_seconds as f64 <= years * YEAR_SECONDS {
                    let horizon = input.horizon_seconds as f64 / YEAR_SECONDS;
                    let scale = s * horizon.sqrt();
                    let movement=OptionsMoveScale { lower_iv_scale:scale*iv.bid,midpoint_iv_scale:scale*iv.midpoint,upper_iv_scale:scale*iv.ask,horizon_years:horizon,
                        distribution_assumption:"lognormal constant-volatility one-standard-deviation scale; neither guaranteed range nor straddle break-even".into() };
                    require(
                        [
                            movement.lower_iv_scale,
                            movement.midpoint_iv_scale,
                            movement.upper_iv_scale,
                        ]
                        .iter()
                        .all(|v| v.is_finite()),
                        "non-finite option move scale",
                    )?;
                    result.one_standard_deviation_scale = Some(movement);
                } else {
                    push(
                        &mut result.issues,
                        "one_standard_deviation_scale",
                        "requested horizon extends beyond this contract expiry",
                        &[],
                    );
                }
                result.atm_iv = Some(iv);
                result.oldest_quote_at = pair.iter().map(|v| v.quote.observed_at).min();
                for p in &pair {
                    result.used_contract_ids.push(p.quote.contract_id.clone());
                    result.source_refs.push(p.quote.source_ref.clone());
                }
                if pair.len() == 1 {
                    push(
                        &mut result.issues,
                        "parity",
                        "matching put/call absent; parity could not be checked",
                        &[],
                    );
                }
            } else {
                push(
                    &mut result.issues,
                    "atm_iv",
                    "put/call parity, carry, multiplier or IV intervals are inconsistent",
                    &[],
                );
            }
        } else {
            push(
                &mut result.issues,
                "atm_iv",
                "no supported liquid contract sufficiently close to spot",
                &[],
            );
        }
        let select = |right| {
            solved
                .iter()
                .filter(|v| {
                    v.quote.right == right
                        && ((right == OptionsRight::Put && v.quote.strike <= spot.price)
                            || (right == OptionsRight::Call && v.quote.strike >= spot.price))
                        && (v.absolute_delta - input.skew_absolute_delta).abs()
                            <= input.skew_delta_tolerance
                })
                .min_by(|a, b| {
                    (a.absolute_delta - input.skew_absolute_delta)
                        .abs()
                        .total_cmp(&(b.absolute_delta - input.skew_absolute_delta).abs())
                        .then(a.quote.contract_id.cmp(&b.quote.contract_id))
                })
        };
        if let (Some(put), Some(call)) = (select(OptionsRight::Put), select(OptionsRight::Call))
            && (put.absolute_delta - call.absolute_delta).abs() <= input.skew_delta_tolerance
            && aligned(
                put.quote.observed_at,
                call.quote.observed_at,
                input.max_alignment_seconds,
            )
        {
            result.skew = Some(OptionsSkew {
                put_minus_call_iv: put.iv.midpoint - call.iv.midpoint,
                put_absolute_delta: put.absolute_delta,
                call_absolute_delta: call.absolute_delta,
                source_refs: vec![put.quote.source_ref.clone(), call.quote.source_ref.clone()],
            });
        }
        if result.skew.is_none() {
            push(
                &mut result.issues,
                "skew",
                "no same-expiry OTM put/call at comparable supported delta; no interpolation",
                &[],
            );
        }
        result.source_refs.sort();
        result.source_refs.dedup();
        out.expiries.push(result);
    }
    if input.contracts.is_empty() {
        push(
            &mut out.issues,
            "contracts",
            "chain is empty; no synthetic neutral IV",
            &[],
        );
    }
    let valid: Vec<_> = out.expiries.iter().filter(|e| e.atm_iv.is_some()).collect();
    for adjacent in valid.windows(2) {
        let near = adjacent[0];
        let far = adjacent[1];
        let n = near.atm_iv.as_ref().unwrap();
        let f = far.atm_iv.as_ref().unwrap();
        let nt = (near.expiry_at - at).num_seconds() as f64 / YEAR_SECONDS;
        let ft = (far.expiry_at - at).num_seconds() as f64 / YEAR_SECONDS;
        let variance = (f.midpoint.powi(2) * ft - n.midpoint.powi(2) * nt) / (ft - nt);
        out.term_structure.push(OptionsTermStructure {
            near_expiry: near.expiry_at,
            far_expiry: far.expiry_at,
            far_minus_near_iv: f.midpoint - n.midpoint,
            forward_variance_rate_annualized: (variance >= 0.0 && variance.is_finite())
                .then_some(variance),
            unavailable_reason: (!(variance >= 0.0 && variance.is_finite()))
                .then(|| "negative or non-finite forward variance; no clipping".into()),
        });
    }
    let reference = valid
        .iter()
        .copied()
        .filter(|e| (e.expiry_at - at).num_seconds() as u64 >= input.horizon_seconds)
        .min_by_key(|e| e.expiry_at);
    if let Some(reference) = reference {
        let iv = reference.atm_iv.as_ref().unwrap().midpoint;
        let tenor = (reference.expiry_at - at).num_seconds() as u64;
        out.reference_expiry = Some(reference.expiry_at);
        out.comparison_horizon_seconds = Some(tenor);
        comparisons(input, at, iv, tenor, &mut out)?;
    } else {
        push(
            &mut out.issues,
            "reference_expiry",
            "no valid expiry covers the requested horizon",
            &[],
        );
    }
    event_variance(input, at, &mut out)?;
    out.status = if out.expiries.iter().all(|e| e.atm_iv.is_none()) {
        OptionsStatus::Unavailable
    } else if !out.issues.is_empty() || out.expiries.iter().any(|e| !e.issues.is_empty()) {
        OptionsStatus::Partial
    } else {
        OptionsStatus::Available
    };
    out.comparison_source_refs.sort();
    out.comparison_source_refs.dedup();
    Ok(out)
}

fn comparisons(
    input: &OptionsInput,
    at: DateTime<Utc>,
    iv: f64,
    tenor: u64,
    out: &mut OptionsAssessment,
) -> Result<()> {
    let comparable = |v: &&OptionsVolatilityObservation| {
        v.horizon_seconds == tenor && v.method == MODEL && v.available_at <= at
    };
    let history: Vec<_> = input
        .iv_history
        .iter()
        .filter(comparable)
        .filter(|v| v.observed_at >= input.history_start && v.observed_at < at)
        .collect();
    let min = history
        .iter()
        .map(|v| v.value_annualized)
        .min_by(f64::total_cmp);
    let max = history
        .iter()
        .map(|v| v.value_annualized)
        .max_by(f64::total_cmp);
    if let (Some(min), Some(max)) = (min, max)
        && max > min
    {
        let rank = (iv - min) / (max - min);
        require(rank.is_finite(), "non-finite historical IV rank")?;
        out.iv_history_rank = Some(rank);
        out.comparison_source_refs
            .extend(history.iter().map(|v| v.source_ref.clone()));
    }
    if out.iv_history_rank.is_none() {
        push(
            &mut out.issues,
            "iv_history_rank",
            "insufficient nonconstant available history at identical tenor/method",
            &[],
        );
    }
    let sector: Vec<_> = input
        .sector_comparables
        .iter()
        .filter(comparable)
        .filter(|v| {
            fresh(
                v.observed_at,
                v.available_at,
                at,
                input.max_quote_age_seconds,
            )
        })
        .collect();
    if !sector.is_empty() {
        out.sector_iv_percentile = Some(
            sector.iter().filter(|v| v.value_annualized <= iv).count() as f64 / sector.len() as f64,
        );
        out.comparison_source_refs
            .extend(sector.iter().map(|v| v.source_ref.clone()));
    } else {
        push(
            &mut out.issues,
            "sector_iv_percentile",
            "same-tenor/method sector observations missing or stale",
            &[],
        );
    }
    if let Some(rv) = input.realized_volatility.as_ref().filter(|v| {
        v.horizon_seconds == tenor
            && fresh(
                v.observed_at,
                v.available_at,
                at,
                input.max_quote_age_seconds,
            )
            && v.value_annualized > 0.0
    }) {
        let ratio = iv / rv.value_annualized;
        require(ratio.is_finite(), "non-finite IV/realized ratio")?;
        out.iv_to_realized_ratio = Some(ratio);
        out.realized_volatility_method = Some(rv.method.clone());
        out.comparison_source_refs.push(rv.source_ref.clone());
    } else {
        push(
            &mut out.issues,
            "iv_to_realized_ratio",
            "positive realized volatility at matching tenor unavailable",
            &[],
        );
    }
    Ok(())
}

fn event_variance(
    input: &OptionsInput,
    at: DateTime<Utc>,
    out: &mut OptionsAssessment,
) -> Result<()> {
    let Some(event) = &input.event else {
        push(
            &mut out.issues,
            "event_variance",
            "no explicit event window and non-event variance baseline",
            &[],
        );
        return Ok(());
    };
    let Some(baseline) = event.non_event_variance_rate_annualized else {
        push(
            &mut out.issues,
            "event_variance",
            "non-event variance baseline absent; adjacent expiries alone are insufficient",
            &[&event.source_ref],
        );
        return Ok(());
    };
    if event.available_at > at
        || event.event_at <= at
        || event.baseline_available_at.is_none_or(|t| t > at)
    {
        push(
            &mut out.issues,
            "event_variance",
            "event/baseline was unavailable or event already elapsed",
            &[&event.source_ref],
        );
        return Ok(());
    }
    let near = out
        .expiries
        .iter()
        .rev()
        .find(|e| e.expiry_at < event.event_at && e.atm_iv.is_some());
    let far = out
        .expiries
        .iter()
        .find(|e| e.expiry_at >= event.event_at && e.atm_iv.is_some());
    if let (Some(n), Some(f)) = (near, far) {
        let ni = n.atm_iv.as_ref().unwrap();
        let fi = f.atm_iv.as_ref().unwrap();
        let nt = (n.expiry_at - at).num_seconds() as f64 / YEAR_SECONDS;
        let ft = (f.expiry_at - at).num_seconds() as f64 / YEAR_SECONDS;
        let lower = fi.bid.powi(2) * ft - ni.ask.powi(2) * nt - baseline * (ft - nt);
        let upper = fi.ask.powi(2) * ft - ni.bid.powi(2) * nt - baseline * (ft - nt);
        let value = fi.midpoint.powi(2) * ft - ni.midpoint.powi(2) * nt - baseline * (ft - nt);
        if lower >= 0.0 && upper >= lower && [lower, value, upper].iter().all(|v| v.is_finite()) {
            out.event_variance = Some(value);
            out.event_variance_interval = Some([lower, upper]);
            out.comparison_source_refs.push(event.source_ref.clone());
            out.comparison_source_refs
                .push(event.baseline_source_ref.clone().unwrap());
        } else {
            push(
                &mut out.issues,
                "event_variance",
                "bid/ask uncertainty includes negative event variance; fragile inference withheld",
                &[&event.source_ref],
            );
        }
    } else {
        push(
            &mut out.issues,
            "event_variance",
            "valid expiries do not bracket the explicit event",
            &[&event.source_ref],
        );
    }
    Ok(())
}

fn quote_unavailable(
    input: &OptionsInput,
    spot: &OptionsUnderlying,
    c: &OptionsQuote,
    at: DateTime<Utc>,
) -> Result<Option<&'static str>> {
    if c.exercise != OptionsExercise::European {
        return Ok(Some(
            "American exercise requires a separately validated model",
        ));
    }
    if c.adjustment_basis.as_deref() != Some(spot.adjustment_basis.as_str()) {
        return Ok(Some(
            "unknown/incompatible corporate-action deliverable basis",
        ));
    }
    if !fresh(
        c.observed_at,
        c.available_at,
        at,
        input.max_quote_age_seconds,
    ) || !aligned(c.observed_at, spot.observed_at, input.max_alignment_seconds)
    {
        return Ok(Some(
            "chain and underlying timestamps unavailable/stale/misaligned",
        ));
    }
    let seconds = (c.expiry_at - at).num_seconds();
    if !(60..=10 * 365 * 86400).contains(&seconds) {
        return Ok(Some(
            "expiry outside supported [60 seconds, 10 years] model range",
        ));
    }
    if c.bid <= Decimal::ZERO
        || c.ask <= c.bid
        || c.bid_size.is_some_and(|v| v <= Decimal::ZERO)
        || c.ask_size.is_some_and(|v| v <= Decimal::ZERO)
    {
        return Ok(Some("crossed, zero-width, zero-size or nonpositive quote"));
    }
    if c.bid.checked_rem(c.price_tick) != Some(Decimal::ZERO)
        || c.ask.checked_rem(c.price_tick) != Some(Decimal::ZERO)
    {
        return Ok(Some("premium is off the contract tick grid"));
    }
    let mid = div(add(c.bid, c.ask)?, Decimal::from(2))?;
    if div(sub(c.ask, c.bid)?, mid)? > input.max_relative_spread_fraction {
        return Ok(Some(
            "relative premium spread exceeds the registered liquidity limit",
        ));
    }
    if input
        .minimum_open_interest
        .is_some_and(|min| c.open_interest_contracts.is_none_or(|oi| oi < min))
    {
        return Ok(Some(
            "open interest missing/below registered liquidity limit",
        ));
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn solve(
    s: f64,
    k: f64,
    r: f64,
    q: f64,
    t: f64,
    right: OptionsRight,
    target: f64,
    tick: f64,
) -> Option<(f64, u32, f64)> {
    let (mut low, mut high) = (0.000001, 5.0);
    let tolerance = (tick * 1e-6).max(s * 1e-10);
    let a = price(s, k, r, q, t, low, right);
    let b = price(s, k, r, q, t, high, right);
    if !a.is_finite() || !b.is_finite() || target <= a || target >= b {
        return None;
    }
    for iteration in 1..=128 {
        let mid = (low + high) / 2.0;
        let value = price(s, k, r, q, t, mid, right);
        let residual = (value - target).abs();
        if !value.is_finite() {
            return None;
        }
        if residual <= tolerance {
            return Some((mid, iteration, residual));
        }
        if value < target {
            low = mid;
        } else {
            high = mid;
        }
    }
    None
}

fn cdf(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.2316419 * z);
    let tail = 0.3989422804014327
        * (-0.5 * z * z).exp()
        * t
        * (0.319381530
            + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    if x >= 0.0 { 1.0 - tail } else { tail }
}

#[allow(clippy::too_many_arguments)]
fn price(s: f64, k: f64, r: f64, q: f64, t: f64, v: f64, right: OptionsRight) -> f64 {
    let d1 = ((s / k).ln() + (r - q + 0.5 * v * v) * t) / (v * t.sqrt());
    let d2 = d1 - v * t.sqrt();
    match right {
        OptionsRight::Call => s * (-q * t).exp() * cdf(d1) - k * (-r * t).exp() * cdf(d2),
        OptionsRight::Put => k * (-r * t).exp() * cdf(-d2) - s * (-q * t).exp() * cdf(-d1),
    }
}

#[allow(clippy::too_many_arguments)]
fn delta(s: f64, k: f64, r: f64, q: f64, t: f64, v: f64, right: OptionsRight) -> f64 {
    let d1 = ((s / k).ln() + (r - q + 0.5 * v * v) * t) / (v * t.sqrt());
    (-q * t).exp() * (cdf(d1) - if right == OptionsRight::Put { 1.0 } else { 0.0 })
}

fn number(v: Decimal) -> Result<f64> {
    v.to_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| Error("decimal cannot be represented as finite model input".into()))
}
fn fresh(observed: DateTime<Utc>, available: DateTime<Utc>, at: DateTime<Utc>, max: u64) -> bool {
    observed <= at && available <= at && aligned(observed, at, max)
}
fn aligned(a: DateTime<Utc>, b: DateTime<Utc>, seconds: u64) -> bool {
    i64::try_from(seconds)
        .ok()
        .and_then(chrono::TimeDelta::try_seconds)
        .is_some_and(|limit| (a - b).abs() <= limit)
}
fn push(issues: &mut Vec<OptionsIssue>, field: &str, reason: &str, refs: &[&str]) {
    issues.push(OptionsIssue {
        field: field.into(),
        reason: reason.into(),
        source_refs: refs.iter().map(|r| (*r).into()).collect(),
    });
}
fn fact(
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
        "invalid option fact timestamps/reference",
    )
}

fn validate(input: &OptionsInput) -> Result<()> {
    require(
        !input.instrument_id.trim().is_empty()
            && !input.venue.trim().is_empty()
            && input.contracts.len() <= 4096
            && input.iv_history.len() <= 8192
            && input.sector_comparables.len() <= 4096
            && input.horizon_seconds > 0
            && input.horizon_seconds <= 10 * 365 * 86400
            && input.max_quote_age_seconds > 0
            && input.max_relative_spread_fraction > Decimal::ZERO
            && input.max_relative_spread_fraction <= Decimal::ONE
            && input.atm_max_moneyness_fraction > Decimal::ZERO
            && input.atm_max_moneyness_fraction <= Decimal::ONE
            && input.skew_absolute_delta.is_finite()
            && input.skew_absolute_delta > 0.0
            && input.skew_absolute_delta < 0.5
            && input.skew_delta_tolerance.is_finite()
            && input.skew_delta_tolerance > 0.0
            && input.skew_delta_tolerance < 0.5,
        "invalid bounded options configuration",
    )?;
    if let Some(s) = &input.underlying {
        fact(s.observed_at, s.published_at, s.available_at, &s.source_ref)?;
        require(
            s.price > Decimal::ZERO && !s.adjustment_basis.trim().is_empty(),
            "invalid underlying price/adjustment",
        )?;
    }
    if let Some(c) = &input.assumptions {
        fact(c.observed_at, c.published_at, c.available_at, &c.source_ref)?;
        require(
            c.valid_until > c.available_at
                && [c.risk_free_rate_annualized, c.dividend_yield_annualized]
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1.0),
            "invalid finite carry assumptions",
        )?;
    }
    let mut keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for c in &input.contracts {
        fact(c.observed_at, c.published_at, c.available_at, &c.source_ref)?;
        require(
            c.strike > Decimal::ZERO
                && c.contract_multiplier > Decimal::ZERO
                && c.price_tick > Decimal::ZERO
                && !c.contract_id.trim().is_empty()
                && ids.insert(&c.contract_id)
                && keys.insert((c.expiry_at, c.strike, c.right == OptionsRight::Call)),
            "invalid or duplicate option contract",
        )?;
    }
    let mut history_keys = BTreeSet::new();
    for v in input
        .iv_history
        .iter()
        .chain(&input.sector_comparables)
        .chain(input.realized_volatility.iter())
    {
        fact(v.observed_at, v.published_at, v.available_at, &v.source_ref)?;
        require(
            v.value_annualized.is_finite()
                && v.value_annualized >= 0.0
                && v.horizon_seconds > 0
                && !v.method.trim().is_empty(),
            "invalid volatility estimate",
        )?;
        require(
            history_keys.insert((&v.source_ref, v.observed_at, &v.method, v.horizon_seconds)),
            "duplicate volatility evidence",
        )?;
    }
    if let Some(e) = &input.event {
        require(
            e.published_at <= e.available_at
                && !e.source_ref.trim().is_empty()
                && e.non_event_variance_rate_annualized
                    .is_none_or(|v| v.is_finite() && v >= 0.0)
                && (e.non_event_variance_rate_annualized.is_none()
                    || (e
                        .baseline_source_ref
                        .as_ref()
                        .is_some_and(|s| !s.trim().is_empty())
                        && e.baseline_available_at.is_some())),
            "invalid event variance baseline/reference",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod options_acceptance {
    use super::*;
    use chrono::Duration;
    use trady_domain::Currency;
    fn time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T19:00:00Z")
            .unwrap()
            .to_utc()
    }
    fn fixture() -> OptionsInput {
        let at = time();
        let q = OptionsQuote {
            contract_id: "call-100".into(),
            right: OptionsRight::Call,
            exercise: OptionsExercise::European,
            expiry_at: at + Duration::days(365),
            strike: Decimal::from(100),
            contract_multiplier: Decimal::from(100),
            adjustment_basis: Some("standard".into()),
            price_tick: Decimal::new(1, 2),
            bid: Decimal::new(790, 2),
            ask: Decimal::new(803, 2),
            bid_size: Some(Decimal::from(10)),
            ask_size: Some(Decimal::from(10)),
            volume_contracts: Some(80),
            open_interest_contracts: Some(500),
            observed_at: at,
            published_at: at,
            available_at: at,
            source_ref: "chain-call".into(),
        };
        let mut p = q.clone();
        p.contract_id = "put-100".into();
        p.right = OptionsRight::Put;
        p.source_ref = "chain-put".into();
        OptionsInput {
            instrument_id: "equity-42".into(),
            currency: Currency::new("USD").unwrap(),
            venue: "XNAS".into(),
            underlying: Some(OptionsUnderlying {
                price: Decimal::from(100),
                adjustment_basis: "standard".into(),
                observed_at: at,
                published_at: at,
                available_at: at,
                source_ref: "spot".into(),
            }),
            assumptions: Some(OptionsPricingAssumptions {
                risk_free_rate_annualized: 0.0,
                dividend_yield_annualized: 0.0,
                observed_at: at,
                published_at: at,
                available_at: at,
                valid_until: at + Duration::days(2),
                source_ref: "carry".into(),
            }),
            contracts: vec![q, p],
            horizon_seconds: 365 * 86400,
            max_quote_age_seconds: 60,
            max_alignment_seconds: 2,
            max_relative_spread_fraction: Decimal::new(1, 1),
            atm_max_moneyness_fraction: Decimal::new(5, 2),
            minimum_open_interest: Some(100),
            skew_absolute_delta: 0.25,
            skew_delta_tolerance: 0.05,
            history_start: at - Duration::days(365),
            iv_history: vec![],
            sector_comparables: vec![],
            realized_volatility: None,
            event: None,
        }
    }
    #[test]
    fn known_atm_price_recovers_iv_interval_and_model_scale() {
        let out = assess(&fixture(), time()).unwrap();
        let iv = out.expiries[0].atm_iv.as_ref().unwrap();
        assert!(iv.bid < 0.2 && iv.ask > 0.2);
        assert!((iv.midpoint - 0.2).abs() < 0.0001);
        let scale = out.expiries[0]
            .one_standard_deviation_scale
            .as_ref()
            .unwrap();
        assert!((scale.midpoint_iv_scale - 20.0).abs() < 0.02);
        assert!(iv.max_price_residual < 1e-7);
        assert!(out.iv_to_realized_ratio.is_none());
    }
    #[test]
    fn solver_matches_known_reference_price_and_carry_parity() {
        let call = price(100.0, 100.0, 0.0, 0.0, 1.0, 0.2, OptionsRight::Call);
        assert!((call - 7.9655674554).abs() < 0.00002);
        let c = price(100.0, 103.0, 0.04, 0.02, 0.3, 0.35, OptionsRight::Call);
        let p = price(100.0, 103.0, 0.04, 0.02, 0.3, 0.35, OptionsRight::Put);
        assert!(
            (c - p - (100.0 * (-0.02_f64 * 0.3).exp() - 103.0 * (-0.04_f64 * 0.3).exp())).abs()
                < 1e-10
        );
    }
    #[test]
    fn empty_crossed_illiquid_and_american_chains_abstain() {
        for mode in 0..4 {
            let mut i = fixture();
            match mode {
                0 => i.contracts.clear(),
                1 => {
                    for c in &mut i.contracts {
                        c.bid = c.ask;
                    }
                }
                2 => {
                    for c in &mut i.contracts {
                        c.open_interest_contracts = None;
                    }
                }
                _ => {
                    for c in &mut i.contracts {
                        c.exercise = OptionsExercise::American;
                    }
                }
            }
            assert_eq!(
                assess(&i, time()).unwrap().status,
                OptionsStatus::Unavailable
            );
        }
    }
    #[test]
    fn inconsistent_parity_withholds_atm() {
        let mut i = fixture();
        i.contracts[1].bid = Decimal::from(10);
        i.contracts[1].ask = Decimal::new(1010, 2);
        assert!(assess(&i, time()).unwrap().expiries[0].atm_iv.is_none());
    }
    #[test]
    fn expiration_alignment_and_split_adjustments_are_explicit() {
        let mut i = fixture();
        for c in &mut i.contracts {
            c.expiry_at = time() + Duration::seconds(1);
        }
        assert_eq!(
            assess(&i, time()).unwrap().status,
            OptionsStatus::Unavailable
        );
        i = fixture();
        for c in &mut i.contracts {
            c.observed_at -= Duration::seconds(5);
        }
        assert_eq!(
            assess(&i, time()).unwrap().status,
            OptionsStatus::Unavailable
        );
        i = fixture();
        for c in &mut i.contracts {
            c.contract_multiplier = Decimal::from(150);
            c.adjustment_basis = Some("split-adjusted".into());
        }
        assert_eq!(
            assess(&i, time()).unwrap().status,
            OptionsStatus::Unavailable
        );
        i.underlying.as_mut().unwrap().adjustment_basis = "split-adjusted".into();
        assert!(assess(&i, time()).unwrap().expiries[0].atm_iv.is_some());
    }
    #[test]
    fn future_publication_and_stale_quotes_are_unavailable() {
        let i = fixture();
        assert_eq!(
            assess(&i, time() - Duration::seconds(1)).unwrap().status,
            OptionsStatus::Unavailable
        );
        assert_eq!(
            assess(&i, time() + Duration::seconds(61)).unwrap().status,
            OptionsStatus::Unavailable
        );
    }
    #[test]
    fn matching_tenor_comparisons_exclude_future_history() {
        let mut i = fixture();
        let make = |value, reference: &str, observed| OptionsVolatilityObservation {
            value_annualized: value,
            horizon_seconds: 365 * 86400,
            method: MODEL.into(),
            observed_at: observed,
            published_at: observed,
            available_at: observed,
            source_ref: reference.into(),
        };
        i.iv_history = vec![
            make(0.1, "iv-low", time() - Duration::days(2)),
            make(0.3, "iv-high", time() - Duration::days(1)),
            make(4.0, "future", time() + Duration::seconds(1)),
        ];
        i.sector_comparables = vec![
            make(0.15, "sector-low", time()),
            make(0.25, "sector-high", time()),
        ];
        i.realized_volatility = Some(make(0.1, "realized", time()));
        let out = assess(&i, time()).unwrap();
        assert!((out.iv_history_rank.unwrap() - 0.5).abs() < 0.001);
        assert_eq!(out.sector_iv_percentile, Some(0.5));
        assert!((out.iv_to_realized_ratio.unwrap() - 2.0).abs() < 0.001);
        assert!(!out.comparison_source_refs.contains(&"future".into()));
    }
    #[test]
    fn nonfinite_pricing_input_and_duplicate_contracts_are_errors() {
        let mut i = fixture();
        i.assumptions.as_mut().unwrap().risk_free_rate_annualized = f64::NAN;
        assert!(assess(&i, time()).is_err());
        i = fixture();
        i.contracts.push(i.contracts[0].clone());
        assert!(assess(&i, time()).is_err());
    }
    #[test]
    fn failed_solver_does_not_become_zero_iv() {
        let mut i = fixture();
        for c in &mut i.contracts {
            c.bid = Decimal::from(101);
            c.ask = Decimal::from(102);
        }
        let out = assess(&i, time()).unwrap();
        assert_eq!(out.status, OptionsStatus::Unavailable);
        assert!(out.expiries[0].atm_iv.is_none());
    }
    #[test]
    fn strict_json_contract_rejects_unknown_fields() {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value["contracts"][0]["expected_return"] = serde_json::json!(0.2);
        assert!(serde_json::from_value::<OptionsInput>(value).is_err());
    }

    fn priced_contract(
        right: OptionsRight,
        strike: i64,
        years: f64,
        volatility: f64,
        name: &str,
    ) -> OptionsQuote {
        let mut q = fixture().contracts[0].clone();
        q.right = right;
        q.strike = Decimal::from(strike);
        q.contract_id = name.into();
        q.source_ref = name.into();
        q.expiry_at = time() + Duration::seconds((years * YEAR_SECONDS) as i64);
        let fair = price(100.0, strike as f64, 0.0, 0.0, years, volatility, right);
        let cents = (fair * 100.0).round() as i64;
        q.bid = Decimal::new(cents - 2, 2);
        q.ask = Decimal::new(cents + 2, 2);
        q
    }

    #[test]
    fn skew_requires_actual_comparable_delta_contracts() {
        let mut i = fixture();
        i.contracts
            .push(priced_contract(OptionsRight::Put, 88, 1.0, 0.25, "otm-put"));
        i.contracts.push(priced_contract(
            OptionsRight::Call,
            120,
            1.0,
            0.20,
            "otm-call",
        ));
        i.skew_delta_tolerance = 0.08;
        let out = assess(&i, time()).unwrap();
        let skew = out.expiries[0].skew.as_ref().unwrap();
        assert!((skew.put_minus_call_iv - 0.05).abs() < 0.002);
        assert!(skew.source_refs.contains(&"otm-put".into()));
        i.skew_delta_tolerance = 0.001;
        assert!(assess(&i, time()).unwrap().expiries[0].skew.is_none());
    }

    #[test]
    fn term_structure_and_event_variance_need_an_explicit_baseline() {
        let mut i = fixture();
        i.horizon_seconds = 30 * 86400;
        i.contracts = vec![
            priced_contract(OptionsRight::Call, 100, 0.1, 0.20, "near-call"),
            priced_contract(OptionsRight::Put, 100, 0.1, 0.20, "near-put"),
            priced_contract(OptionsRight::Call, 100, 0.2, 0.40, "far-call"),
            priced_contract(OptionsRight::Put, 100, 0.2, 0.40, "far-put"),
        ];
        i.event = Some(OptionsEvent {
            event_at: time() + Duration::days(50),
            published_at: time(),
            available_at: time(),
            source_ref: "event-date".into(),
            non_event_variance_rate_annualized: None,
            baseline_source_ref: None,
            baseline_available_at: None,
        });
        let out = assess(&i, time()).unwrap();
        assert_eq!(out.term_structure.len(), 1);
        assert!(out.event_variance.is_none());
        let event = i.event.as_mut().unwrap();
        event.non_event_variance_rate_annualized = Some(0.04);
        event.baseline_source_ref = Some("non-event-variance".into());
        event.baseline_available_at = Some(time());
        let out = assess(&i, time()).unwrap();
        assert!((out.term_structure[0].far_minus_near_iv - 0.2).abs() < 0.002);
        assert!((out.event_variance.unwrap() - 0.024).abs() < 0.001);
        assert!(out.event_variance_interval.unwrap()[0] > 0.0);
        i.event.as_mut().unwrap().non_event_variance_rate_annualized = Some(2.0);
        assert!(assess(&i, time()).unwrap().event_variance.is_none());
    }

    #[test]
    fn carry_is_not_silently_zero_and_expiration_dates_are_not_rewritten() {
        let mut i = fixture();
        i.assumptions = None;
        assert_eq!(
            assess(&i, time()).unwrap().status,
            OptionsStatus::Unavailable
        );
        i = fixture();
        // Synthetic Saturday expiry remains the actual supplied instant.
        let expiry = i.contracts[0].expiry_at;
        assert_eq!(assess(&i, time()).unwrap().expiries[0].expiry_at, expiry);
        i = fixture();
        i.underlying.as_mut().unwrap().observed_at -= Duration::nanoseconds(1);
        assert_eq!(
            assess(&i, time() + Duration::seconds(60)).unwrap().status,
            OptionsStatus::Unavailable
        );
    }

    #[test]
    fn pathological_statistical_history_cannot_emit_infinity() {
        let mut i = fixture();
        let v = |value, name: &str| OptionsVolatilityObservation {
            value_annualized: value,
            horizon_seconds: 365 * 86400,
            method: MODEL.into(),
            observed_at: time() - Duration::days(1),
            published_at: time() - Duration::days(1),
            available_at: time() - Duration::days(1),
            source_ref: name.into(),
        };
        i.iv_history = vec![v(0.0, "zero"), v(f64::from_bits(1), "subnormal")];
        assert!(assess(&i, time()).is_err());
    }
}

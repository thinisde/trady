//! Deterministic long-only Donchian breakout research baseline.
//!
//! This module owns the signal definition used by the first IEX30 authority.
//! It deliberately contains no order, broker or model access.  The caller must
//! provide completed regular-session bars and persist the resulting snapshot.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use trady_domain::{Error, Result, require};

use super::{FormulaFamily, FormulaParameters};
use crate::Policy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonchianAtrMethod {
    Wilder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonchianGapEntryRule {
    FirstExecutablePriceBelowSignalClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonchianGapStopRule {
    FirstExecutablePrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonchianSameBarRule {
    StopFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonchianModelFusion {
    Disabled,
}

/// The pre-registered structure of `long_donchian_atr_v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonchianAtrSpec {
    pub formula_id: String,
    pub breakout_window: u16,
    pub atr_method: DonchianAtrMethod,
    pub entry_allowance_atr: Decimal,
    pub stop_atr_multiple: Decimal,
    pub target_r_multiple: Decimal,
    pub max_holding_sessions: u8,
    pub minimum_warmup_bars: u16,
    pub gap_entry_rule: DonchianGapEntryRule,
    pub gap_stop_rule: DonchianGapStopRule,
    pub same_bar_rule: DonchianSameBarRule,
    pub model_fusion: DonchianModelFusion,
}

impl DonchianAtrSpec {
    pub fn recommended() -> Self {
        Self {
            formula_id: "long_donchian_atr_v1".into(),
            breakout_window: 20,
            atr_method: DonchianAtrMethod::Wilder,
            entry_allowance_atr: Decimal::new(10, 2),
            stop_atr_multiple: Decimal::new(150, 2),
            target_r_multiple: Decimal::new(150, 2),
            max_holding_sessions: 5,
            minimum_warmup_bars: 35,
            gap_entry_rule: DonchianGapEntryRule::FirstExecutablePriceBelowSignalClose,
            gap_stop_rule: DonchianGapStopRule::FirstExecutablePrice,
            same_bar_rule: DonchianSameBarRule::StopFirst,
            model_fusion: DonchianModelFusion::Disabled,
        }
    }

    pub fn validate(&self, parameters: &FormulaParameters, policy: &Policy) -> Result<()> {
        require(
            self.formula_id == "long_donchian_atr_v1"
                && self.formula_id.len() <= 160
                && self
                    .formula_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b)),
            "invalid Donchian formula identifier",
        )?;
        require(
            parameters.family == FormulaFamily::Trend
                && parameters.entry_allowance_fraction == Decimal::ZERO
                && parameters.stop_atr_multiple == self.stop_atr_multiple
                && parameters.max_holding_sessions == self.max_holding_sessions
                && parameters.atr_lookback == 14
                && parameters.bar_seconds == 86_400,
            "Donchian formula parameters do not match the pre-registered baseline",
        )?;
        require(
            matches!(self.atr_method, DonchianAtrMethod::Wilder)
                && (2..=252).contains(&self.breakout_window)
                && self.entry_allowance_atr > Decimal::ZERO
                && self.entry_allowance_atr <= Decimal::ONE
                && self.stop_atr_multiple > Decimal::ZERO
                && self.target_r_multiple >= policy.min_reward_to_risk
                && (1..=policy.max_holding_sessions).contains(&self.max_holding_sessions)
                && self.minimum_warmup_bars
                    >= self
                        .breakout_window
                        .saturating_add(parameters.atr_lookback)
                        .saturating_add(1)
                && matches!(self.model_fusion, DonchianModelFusion::Disabled)
                && !policy.leverage
                && !policy.add_to_position
                && !policy.rotation_enabled,
            "Donchian formula exceeds the owner policy or is not fully pre-registered",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonchianBar {
    pub observed_at: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonchianSignal {
    pub signal_at: DateTime<Utc>,
    pub signal_close: Decimal,
    pub breakout_level: Decimal,
    pub atr: Decimal,
    pub entry_ceiling: Decimal,
    pub stop_distance: Decimal,
    pub target_distance: Decimal,
}

/// The source used for a deterministic entry price.  Keeping this explicit in
/// the decision record prevents a replay from silently substituting a quote,
/// midpoint, or a later bar for the next regular-session open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonchianExecutionPriceSource {
    NextRegularSessionOpen,
    HoldingBarOpen,
    HoldingBarStop,
    HoldingBarTarget,
    HoldingBarClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonchianEntryRejection {
    MissingNextRegularSessionOpen,
    OpeningGapAboveAllowance,
}

/// The result of applying the registered opening-gap rule to a signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "decision",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum DonchianEntryDecision {
    Accepted {
        executable_price: Decimal,
        gap_threshold: Decimal,
        source: DonchianExecutionPriceSource,
    },
    Rejected {
        observed_open: Option<Decimal>,
        gap_threshold: Decimal,
        reason: DonchianEntryRejection,
    },
}

/// Apply the inclusive `signal_close + 0.10 ATR` opening-gap boundary.
///
/// A lower opening price is still executable at that opening price.  A missing
/// next open is represented as a rejection with an explicit reason so replay
/// and shadow ledgers cannot turn unavailable data into a zero return.
pub fn resolve_entry(
    signal: &DonchianSignal,
    next_open: Option<Decimal>,
) -> Result<DonchianEntryDecision> {
    require(
        signal.signal_close > Decimal::ZERO
            && signal.atr > Decimal::ZERO
            && signal.entry_ceiling >= signal.signal_close,
        "Donchian signal has invalid entry bounds",
    )?;
    let gap_threshold = signal.entry_ceiling;
    let Some(open) = next_open else {
        return Ok(DonchianEntryDecision::Rejected {
            observed_open: None,
            gap_threshold,
            reason: DonchianEntryRejection::MissingNextRegularSessionOpen,
        });
    };
    require(
        open > Decimal::ZERO,
        "next regular-session open must be positive",
    )?;
    if open > gap_threshold {
        return Ok(DonchianEntryDecision::Rejected {
            observed_open: Some(open),
            gap_threshold,
            reason: DonchianEntryRejection::OpeningGapAboveAllowance,
        });
    }
    Ok(DonchianEntryDecision::Accepted {
        executable_price: open,
        gap_threshold,
        source: DonchianExecutionPriceSource::NextRegularSessionOpen,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DonchianExitReason {
    GapThroughStop,
    StopSameBarFirst,
    Stop,
    Target,
    FifthHoldingSessionClose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonchianExitDecision {
    pub holding_session: u8,
    pub executable_price: Decimal,
    pub reason: DonchianExitReason,
    pub price_source: DonchianExecutionPriceSource,
}

/// Resolve one completed holding bar under the registered stop/target rules.
///
/// The opening gap is checked first.  When both stop and target are touched in
/// the same bar, the stop wins.  A position with no trigger is closed at the
/// close of holding session five, never after it.
pub fn resolve_exit(
    plan: &DonchianTradePlan,
    bar: &DonchianBar,
    holding_session: u8,
    spec: &DonchianAtrSpec,
) -> Result<Option<DonchianExitDecision>> {
    require(
        (1..=spec.max_holding_sessions).contains(&holding_session),
        "Donchian holding session is outside the registered horizon",
    )?;
    validate_bars(std::slice::from_ref(bar))?;
    require(
        plan.entry > Decimal::ZERO
            && plan.stop > Decimal::ZERO
            && plan.stop < plan.entry
            && plan.target > plan.entry,
        "Donchian trade plan has invalid exit ordering",
    )?;
    if bar.open <= plan.stop {
        return Ok(Some(DonchianExitDecision {
            holding_session,
            executable_price: bar.open,
            reason: DonchianExitReason::GapThroughStop,
            price_source: DonchianExecutionPriceSource::HoldingBarOpen,
        }));
    }
    if bar.low <= plan.stop && bar.high >= plan.target {
        return Ok(Some(DonchianExitDecision {
            holding_session,
            executable_price: plan.stop,
            reason: DonchianExitReason::StopSameBarFirst,
            price_source: DonchianExecutionPriceSource::HoldingBarStop,
        }));
    }
    if bar.low <= plan.stop {
        return Ok(Some(DonchianExitDecision {
            holding_session,
            executable_price: plan.stop,
            reason: DonchianExitReason::Stop,
            price_source: DonchianExecutionPriceSource::HoldingBarStop,
        }));
    }
    if bar.high >= plan.target {
        let executable_price = if bar.open >= plan.target {
            bar.open
        } else {
            plan.target
        };
        return Ok(Some(DonchianExitDecision {
            holding_session,
            executable_price,
            reason: DonchianExitReason::Target,
            price_source: if bar.open >= plan.target {
                DonchianExecutionPriceSource::HoldingBarOpen
            } else {
                DonchianExecutionPriceSource::HoldingBarTarget
            },
        }));
    }
    if holding_session == spec.max_holding_sessions {
        return Ok(Some(DonchianExitDecision {
            holding_session,
            executable_price: bar.close,
            reason: DonchianExitReason::FifthHoldingSessionClose,
            price_source: DonchianExecutionPriceSource::HoldingBarClose,
        }));
    }
    Ok(None)
}

fn checked(value: Option<Decimal>, message: &'static str) -> Result<Decimal> {
    value.ok_or_else(|| Error(message.into()))
}

fn validate_bars(bars: &[DonchianBar]) -> Result<()> {
    for (index, bar) in bars.iter().enumerate() {
        require(
            bar.open > Decimal::ZERO
                && bar.high > Decimal::ZERO
                && bar.low > Decimal::ZERO
                && bar.close > Decimal::ZERO
                && bar.high >= bar.open
                && bar.high >= bar.close
                && bar.low <= bar.open
                && bar.low <= bar.close
                && (index == 0 || bars[index - 1].observed_at < bar.observed_at),
            "Donchian bars are invalid, unordered or duplicated",
        )?;
    }
    Ok(())
}

pub fn true_range(previous_close: Decimal, bar: &DonchianBar) -> Result<Decimal> {
    require(
        previous_close > Decimal::ZERO,
        "previous close must be positive",
    )?;
    let high_low = checked(
        bar.high.checked_sub(bar.low),
        "Donchian true range overflow",
    )?;
    let high_previous = checked(
        bar.high
            .checked_sub(previous_close)
            .map(|value| value.abs()),
        "Donchian true range overflow",
    )?;
    let low_previous = checked(
        bar.low.checked_sub(previous_close).map(|value| value.abs()),
        "Donchian true range overflow",
    )?;
    Ok(high_low.max(high_previous).max(low_previous))
}

/// Wilder ATR with an arithmetic mean seed over the first `window` true ranges.
pub fn wilder_atr(bars: &[DonchianBar], window: u16) -> Result<Decimal> {
    validate_bars(bars)?;
    let window = usize::from(window);
    require(
        window >= 2 && bars.len() > window,
        "insufficient completed bars for Wilder ATR",
    )?;
    let mut ranges = Vec::with_capacity(bars.len() - 1);
    for pair in bars.windows(2) {
        ranges.push(true_range(pair[0].close, &pair[1])?);
    }
    let seed = ranges[..window]
        .iter()
        .copied()
        .try_fold(Decimal::ZERO, |sum, value| {
            sum.checked_add(value)
                .ok_or_else(|| Error("Donchian ATR sum overflow".into()))
        })?
        .checked_div(Decimal::from(window as u64))
        .ok_or_else(|| Error("Donchian ATR seed division failed".into()))?;
    let mut atr = seed;
    for range in ranges.iter().skip(window) {
        let weighted = atr
            .checked_mul(Decimal::from((window - 1) as u64))
            .and_then(|value| value.checked_add(*range))
            .ok_or_else(|| Error("Donchian ATR smoothing overflow".into()))?;
        atr = weighted
            .checked_div(Decimal::from(window as u64))
            .ok_or_else(|| Error("Donchian ATR smoothing division failed".into()))?;
    }
    require(atr > Decimal::ZERO, "Donchian ATR is nonpositive")?;
    Ok(atr)
}

/// Evaluate the completed bar at the end of `bars`. The current bar is excluded
/// from the 20-bar channel, so this function cannot look ahead.
pub fn evaluate_signal(
    bars: &[DonchianBar],
    spec: &DonchianAtrSpec,
) -> Result<Option<DonchianSignal>> {
    validate_bars(bars)?;
    require(
        bars.len() >= usize::from(spec.minimum_warmup_bars),
        "Donchian signal lacks the registered warmup window",
    )?;
    let current_index = bars.len() - 1;
    let breakout_window = usize::from(spec.breakout_window);
    require(
        current_index >= breakout_window,
        "Donchian signal lacks prior breakout bars",
    )?;
    let breakout_level = bars[current_index - breakout_window..current_index]
        .iter()
        .map(|bar| bar.high)
        .max()
        .ok_or_else(|| Error("Donchian breakout window is empty".into()))?;
    let atr = wilder_atr(bars, 14)?;
    let current = &bars[current_index];
    if current.close <= breakout_level {
        return Ok(None);
    }
    let stop_distance = spec
        .stop_atr_multiple
        .checked_mul(atr)
        .ok_or_else(|| Error("Donchian stop distance overflow".into()))?;
    let target_distance = spec
        .target_r_multiple
        .checked_mul(stop_distance)
        .ok_or_else(|| Error("Donchian target distance overflow".into()))?;
    let entry_ceiling = current
        .close
        .checked_add(
            spec.entry_allowance_atr
                .checked_mul(atr)
                .ok_or_else(|| Error("Donchian entry allowance overflow".into()))?,
        )
        .ok_or_else(|| Error("Donchian entry ceiling overflow".into()))?;
    Ok(Some(DonchianSignal {
        signal_at: current.observed_at,
        signal_close: current.close,
        breakout_level,
        atr,
        entry_ceiling,
        stop_distance,
        target_distance,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonchianTradePlan {
    pub entry: Decimal,
    pub stop: Decimal,
    pub target: Decimal,
    pub risk_unit: Decimal,
    pub reward_to_risk: Decimal,
}

pub fn trade_plan(
    entry: Decimal,
    atr: Decimal,
    spec: &DonchianAtrSpec,
) -> Result<DonchianTradePlan> {
    require(
        entry > Decimal::ZERO && atr > Decimal::ZERO,
        "Donchian plan inputs must be positive",
    )?;
    let risk_unit = spec
        .stop_atr_multiple
        .checked_mul(atr)
        .ok_or_else(|| Error("Donchian risk unit overflow".into()))?;
    let target_distance = spec
        .target_r_multiple
        .checked_mul(risk_unit)
        .ok_or_else(|| Error("Donchian target distance overflow".into()))?;
    let stop = entry
        .checked_sub(risk_unit)
        .ok_or_else(|| Error("Donchian stop overflow".into()))?;
    let target = entry
        .checked_add(target_distance)
        .ok_or_else(|| Error("Donchian target overflow".into()))?;
    require(
        stop > Decimal::ZERO && target > entry,
        "Donchian prices are not ordered",
    )?;
    Ok(DonchianTradePlan {
        entry,
        stop,
        target,
        risk_unit,
        reward_to_risk: spec.target_r_multiple,
    })
}

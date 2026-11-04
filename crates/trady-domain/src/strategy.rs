//! Deterministic, replayable strategy snapshots.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

use crate::{BarSnapshot, CompletedBar, Error, InstrumentId, Result, require};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignalDirection {
    Long,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceZone {
    pub minimum: Decimal,
    pub maximum: Decimal,
}

impl PriceZone {
    pub fn validate(&self) -> Result<()> {
        require(
            self.minimum > Decimal::ZERO && self.minimum <= self.maximum,
            "invalid signal entry zone",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalSnapshot {
    pub formula_id: String,
    pub parameter_revision: String,
    pub instrument_id: InstrumentId,
    pub bar_snapshot_id: String,
    pub direction: SignalDirection,
    pub signal_at: DateTime<Utc>,
    pub entry_zone: PriceZone,
    pub invalidation: Decimal,
    pub stop_distance: Decimal,
    pub expected_target: Decimal,
    pub max_hold_deadline: DateTime<Utc>,
    pub reward_to_risk: Decimal,
    pub confidence_inputs: BTreeMap<String, Decimal>,
}

impl SignalSnapshot {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_id(&self.formula_id)
                && valid_id(&self.parameter_revision)
                && self.instrument_id.as_str().ends_with("_EQ")
                && valid_id(&self.bar_snapshot_id)
                && self.signal_at <= now
                && self.max_hold_deadline > self.signal_at
                && self.max_hold_deadline > now
                && self.invalidation > Decimal::ZERO
                && self.stop_distance > Decimal::ZERO
                && self.expected_target > self.entry_zone.maximum
                && self.invalidation < self.entry_zone.minimum
                && self.reward_to_risk >= Decimal::new(15, 1)
                && self.confidence_inputs.len() <= 32
                && self
                    .confidence_inputs
                    .values()
                    .all(|value| *value >= Decimal::ZERO),
            "invalid signal snapshot",
        )?;
        self.entry_zone.validate()
    }
}

/// A strategy is pure: it receives a completed-bar snapshot and cannot access
/// a broker, a model, or a network.
pub trait Strategy: Send + Sync {
    fn formula_id(&self) -> &str;
    fn parameter_revision(&self) -> &str;
    fn evaluate(
        &self,
        bars: &BarSnapshot,
        max_hold_deadline: DateTime<Utc>,
    ) -> Result<Option<SignalSnapshot>>;
}

#[derive(Default)]
pub struct StrategyRegistry {
    strategies: BTreeMap<String, Arc<dyn Strategy>>,
}

impl StrategyRegistry {
    pub fn register(&mut self, strategy: Arc<dyn Strategy>) -> Result<()> {
        require(
            valid_id(strategy.formula_id()) && valid_id(strategy.parameter_revision()),
            "invalid strategy registration",
        )?;
        require(
            !self.strategies.contains_key(strategy.formula_id()),
            "strategy formula is already registered",
        )?;
        self.strategies
            .insert(strategy.formula_id().to_owned(), strategy);
        Ok(())
    }

    pub fn contains(&self, formula_id: &str) -> bool {
        self.strategies.contains_key(formula_id)
    }

    pub fn evaluate(
        &self,
        formula_id: &str,
        bars: &BarSnapshot,
        max_hold_deadline: DateTime<Utc>,
    ) -> Result<Option<SignalSnapshot>> {
        self.strategies
            .get(formula_id)
            .ok_or_else(|| Error("strategy formula is not registered".into()))?
            .evaluate(bars, max_hold_deadline)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.strategies.keys().map(String::as_str)
    }
}

/// The EUR intraday Donchian/ATR baseline. It uses only completed bars and
/// leaves executable price, quantity, costs, and risk approval to the policy
/// layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonchianStrategy {
    pub breakout_window: usize,
    pub atr_window: usize,
    pub entry_allowance_atr: Decimal,
    pub stop_atr_multiple: Decimal,
    pub target_r_multiple: Decimal,
    pub parameter_revision: String,
}

impl DonchianStrategy {
    pub fn intraday_default() -> Self {
        Self {
            breakout_window: 20,
            atr_window: 14,
            entry_allowance_atr: Decimal::new(10, 2),
            stop_atr_multiple: Decimal::ONE,
            target_r_multiple: Decimal::new(2, 0),
            parameter_revision: "donchian_atr_intraday_v1".into(),
        }
    }

    fn validate(&self) -> Result<()> {
        require(
            (2..=252).contains(&self.breakout_window)
                && (2..=252).contains(&self.atr_window)
                && self.entry_allowance_atr >= Decimal::ZERO
                && self.entry_allowance_atr <= Decimal::ONE
                && self.stop_atr_multiple > Decimal::ZERO
                && self.target_r_multiple >= Decimal::new(15, 1)
                && valid_id(&self.parameter_revision),
            "invalid intraday Donchian parameters",
        )
    }
}

impl Strategy for DonchianStrategy {
    fn formula_id(&self) -> &str {
        "donchian_atr_intraday_v1"
    }

    fn parameter_revision(&self) -> &str {
        &self.parameter_revision
    }

    fn evaluate(
        &self,
        snapshot: &BarSnapshot,
        max_hold_deadline: DateTime<Utc>,
    ) -> Result<Option<SignalSnapshot>> {
        self.validate()?;
        let now = snapshot.received_at;
        snapshot.validate(now)?;
        let required = self.breakout_window.max(self.atr_window + 1) + 1;
        require(
            snapshot.bars.len() >= required,
            "Donchian strategy lacks a completed-bar warmup window",
        )?;
        let current = snapshot
            .bars
            .last()
            .ok_or_else(|| Error("Donchian snapshot has no current bar".into()))?;
        let breakout_start = snapshot.bars.len() - 1 - self.breakout_window;
        let breakout_level = snapshot.bars[breakout_start..snapshot.bars.len() - 1]
            .iter()
            .map(|bar| bar.high)
            .max()
            .ok_or_else(|| Error("Donchian breakout window is empty".into()))?;
        if current.close <= breakout_level {
            return Ok(None);
        }
        let atr = average_true_range(&snapshot.bars, self.atr_window)?;
        let allowance = atr
            .checked_mul(self.entry_allowance_atr)
            .ok_or_else(|| Error("Donchian entry allowance overflow".into()))?;
        let stop_distance = atr
            .checked_mul(self.stop_atr_multiple)
            .ok_or_else(|| Error("Donchian stop distance overflow".into()))?;
        let target_distance = stop_distance
            .checked_mul(self.target_r_multiple)
            .ok_or_else(|| Error("Donchian target distance overflow".into()))?;
        let entry_maximum = current
            .close
            .checked_add(allowance)
            .ok_or_else(|| Error("Donchian entry ceiling overflow".into()))?;
        let invalidation = current
            .close
            .checked_sub(stop_distance)
            .ok_or_else(|| Error("Donchian invalidation overflow".into()))?;
        let expected_target = current
            .close
            .checked_add(target_distance)
            .ok_or_else(|| Error("Donchian target overflow".into()))?;
        require(
            invalidation > Decimal::ZERO,
            "Donchian invalidation is nonpositive",
        )?;
        let signal = SignalSnapshot {
            formula_id: self.formula_id().into(),
            parameter_revision: self.parameter_revision.clone(),
            instrument_id: snapshot.instrument_id.clone(),
            bar_snapshot_id: snapshot.snapshot_id.clone(),
            direction: SignalDirection::Long,
            signal_at: current.closes_at,
            entry_zone: PriceZone {
                minimum: current.close,
                maximum: entry_maximum,
            },
            invalidation,
            stop_distance,
            expected_target,
            max_hold_deadline,
            reward_to_risk: self.target_r_multiple,
            confidence_inputs: BTreeMap::from([
                ("breakout_level".into(), breakout_level),
                ("atr".into(), atr),
            ]),
        };
        signal.validate(now)?;
        Ok(Some(signal))
    }
}

/// A bounded deterministic momentum profile retained for perpetual/replay
/// comparisons. It never creates an order or a model prediction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerpetualStrategy {
    pub parameter_revision: String,
}

impl Default for PerpetualStrategy {
    fn default() -> Self {
        Self {
            parameter_revision: "perpetual_intraday_v1".into(),
        }
    }
}

impl Strategy for PerpetualStrategy {
    fn formula_id(&self) -> &str {
        "perpetual_intraday_v1"
    }

    fn parameter_revision(&self) -> &str {
        &self.parameter_revision
    }

    fn evaluate(
        &self,
        snapshot: &BarSnapshot,
        max_hold_deadline: DateTime<Utc>,
    ) -> Result<Option<SignalSnapshot>> {
        snapshot.validate(snapshot.received_at)?;
        require(
            valid_id(&self.parameter_revision),
            "invalid perpetual revision",
        )?;
        let first = snapshot
            .bars
            .first()
            .ok_or_else(|| Error("perpetual snapshot has no bars".into()))?;
        let current = snapshot
            .bars
            .last()
            .ok_or_else(|| Error("perpetual snapshot has no current bar".into()))?;
        if current.close <= first.close {
            return Ok(None);
        }
        let stop_distance = current
            .close
            .checked_mul(Decimal::new(1, 2))
            .ok_or_else(|| Error("perpetual stop overflow".into()))?;
        let invalidation = current
            .close
            .checked_sub(stop_distance)
            .ok_or_else(|| Error("perpetual invalidation overflow".into()))?;
        let expected_target = current
            .close
            .checked_mul(Decimal::new(103, 2))
            .ok_or_else(|| Error("perpetual target overflow".into()))?;
        let signal = SignalSnapshot {
            formula_id: self.formula_id().into(),
            parameter_revision: self.parameter_revision.clone(),
            instrument_id: snapshot.instrument_id.clone(),
            bar_snapshot_id: snapshot.snapshot_id.clone(),
            direction: SignalDirection::Long,
            signal_at: current.closes_at,
            entry_zone: PriceZone {
                minimum: current.close,
                maximum: current.close,
            },
            invalidation,
            stop_distance,
            expected_target,
            max_hold_deadline,
            reward_to_risk: Decimal::new(2, 0),
            confidence_inputs: BTreeMap::new(),
        };
        signal.validate(snapshot.received_at)?;
        Ok(Some(signal))
    }
}

fn average_true_range(bars: &[CompletedBar], window: usize) -> Result<Decimal> {
    require(bars.len() > window, "not enough bars for ATR")?;
    let mut total = Decimal::ZERO;
    let start = bars.len() - window - 1;
    for pair in bars[start..].windows(2) {
        let previous_close = pair[0].close;
        let bar = &pair[1];
        let range = (bar.high - bar.low)
            .max((bar.high - previous_close).abs())
            .max((bar.low - previous_close).abs());
        total = total
            .checked_add(range)
            .ok_or_else(|| Error("ATR overflow".into()))?;
    }
    total
        .checked_div(Decimal::from(window as u64))
        .ok_or_else(|| Error("ATR division failed".into()))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Currency, InstrumentId};
    use chrono::Duration;

    fn snapshot() -> BarSnapshot {
        let now = DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc();
        let mut bars = Vec::new();
        for index in 0..40 {
            let open = Decimal::from(100 + index as i64);
            let close = open + Decimal::ONE;
            bars.push(CompletedBar {
                bar_id: format!("bar_{index}"),
                instrument_id: InstrumentId::new("AIR_PA_EQ").expect("id"),
                interval_seconds: 60,
                opens_at: now - Duration::minutes(40 - index as i64),
                closes_at: now - Duration::minutes(39 - index as i64),
                open,
                high: close + Decimal::ONE,
                low: open - Decimal::ONE,
                close,
                volume: Some(Decimal::from(10_000)),
                provider: "yfinance_rs".into(),
                received_at: now,
                completed: true,
            });
        }
        if let Some(last) = bars.last_mut() {
            last.close = last.high + Decimal::ONE;
            last.high = last.close + Decimal::ONE;
        }
        BarSnapshot {
            snapshot_id: "bars_1".into(),
            instrument_id: InstrumentId::new("AIR_PA_EQ").expect("id"),
            provider: "yfinance_rs".into(),
            interval_seconds: 60,
            observed_at: now,
            received_at: now,
            bars,
        }
    }

    #[test]
    fn registry_rejects_duplicates_and_donchian_is_replayable() {
        let mut registry = StrategyRegistry::default();
        registry
            .register(Arc::new(DonchianStrategy::intraday_default()))
            .expect("register");
        assert!(
            registry
                .register(Arc::new(DonchianStrategy::intraday_default()))
                .is_err()
        );
        let snapshot = snapshot();
        let now = snapshot.received_at;
        let signal = registry
            .evaluate(
                "donchian_atr_intraday_v1",
                &snapshot,
                now + Duration::hours(1),
            )
            .expect("evaluate");
        assert!(signal.is_some());
        assert!(signal.unwrap().validate(now).is_ok());
    }

    #[test]
    fn perpetual_does_not_signal_on_nonpositive_momentum() {
        let mut snapshot = snapshot();
        let last = snapshot.bars.last_mut().expect("bar");
        last.close = Decimal::ONE;
        last.low = Decimal::ONE;
        assert!(
            PerpetualStrategy::default()
                .evaluate(&snapshot, snapshot.received_at + Duration::hours(1))
                .expect("evaluate")
                .is_none()
        );
        let _ = Currency::new("EUR").expect("currency");
    }
}

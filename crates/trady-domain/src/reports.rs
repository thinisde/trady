//! Immutable daily reconciliation and weekly learning artifacts.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{Money, Result, require};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyReconciliation {
    pub schema_version: u32,
    pub report_id: String,
    pub account_id: String,
    pub session_id: String,
    pub broker_snapshot_ids: Vec<String>,
    pub selection_run_id: Option<String>,
    pub realized_pnl: Money,
    pub unrealized_pnl: Money,
    pub fees: Money,
    pub fx_impact: Money,
    pub spread_cost: Money,
    pub slippage_cost: Money,
    pub quote_age_seconds: Vec<u64>,
    pub news_age_seconds: Vec<u64>,
    pub signal_latency_millis: Vec<u64>,
    pub rejected_orders: u32,
    pub provider_errors: Vec<String>,
    pub candidate_rankings: Vec<String>,
    pub strategy_attribution: BTreeMap<String, Decimal>,
    pub evidence_ids: Vec<String>,
    pub complete: bool,
    pub gap_reason: Option<String>,
    pub generated_at: DateTime<Utc>,
}

impl DailyReconciliation {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.schema_version == 1
                && valid_id(&self.report_id)
                && valid_id(&self.account_id)
                && valid_id(&self.session_id)
                && !self.broker_snapshot_ids.is_empty()
                && self.broker_snapshot_ids.iter().all(|id| valid_id(id))
                && self.selection_run_id.as_ref().is_none_or(|id| valid_id(id))
                && self.generated_at <= now
                && self.quote_age_seconds.len() <= 512
                && self.news_age_seconds.len() <= 512
                && self.signal_latency_millis.len() <= 512
                && self.provider_errors.len() <= 128
                && self
                    .provider_errors
                    .iter()
                    .all(|error| valid_text(error, 500))
                && self.candidate_rankings.len() <= 256
                && self.candidate_rankings.iter().all(|id| valid_id(id))
                && self.strategy_attribution.len() <= 64
                && self.strategy_attribution.keys().all(|id| valid_id(id))
                && self.strategy_attribution.values().all(|value| {
                    value.is_sign_positive() || value.is_zero() || value.is_sign_negative()
                })
                && self.evidence_ids.len() <= 256
                && self.evidence_ids.iter().all(|id| valid_id(id)),
            "invalid daily reconciliation envelope",
        )?;
        for money in [
            &self.realized_pnl,
            &self.unrealized_pnl,
            &self.fees,
            &self.fx_impact,
            &self.spread_cost,
            &self.slippage_cost,
        ] {
            require(
                money.currency.as_str() == "EUR",
                "daily report currency must be EUR",
            )?;
        }
        require(
            self.complete == self.gap_reason.is_none(),
            "daily reconciliation completeness and gap reason disagree",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeeklyPerformanceReport {
    pub schema_version: u32,
    pub report_id: String,
    pub account_id: String,
    pub week_start: DateTime<Utc>,
    pub week_end: DateTime<Utc>,
    pub daily_report_ids: Vec<String>,
    pub trade_count: u32,
    pub expectancy_fraction: Option<Decimal>,
    pub hit_rate: Option<Decimal>,
    pub max_drawdown_fraction: Option<Decimal>,
    pub average_hold_seconds: Option<u64>,
    pub profit_distribution: Vec<Decimal>,
    pub stop_adherence_fraction: Option<Decimal>,
    pub target_adherence_fraction: Option<Decimal>,
    pub exposure_fraction: Option<Decimal>,
    pub stock_type_performance: BTreeMap<String, Decimal>,
    pub calibration_error: Option<Decimal>,
    pub failure_modes: Vec<String>,
    pub raw_snapshot_ids: Vec<String>,
    pub proposal_ids: Vec<String>,
    pub activation_allowed: bool,
    pub immutable_hash: String,
    pub generated_at: DateTime<Utc>,
}

impl WeeklyPerformanceReport {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.schema_version == 1
                && valid_id(&self.report_id)
                && valid_id(&self.account_id)
                && self.week_start < self.week_end
                && self.week_end <= now
                && !self.daily_report_ids.is_empty()
                && self.daily_report_ids.iter().all(|id| valid_id(id))
                && self.profit_distribution.len() <= 10_000
                && self.stock_type_performance.keys().all(|id| valid_id(id))
                && self.stock_type_performance.len() <= 128
                && self.failure_modes.len() <= 128
                && self.failure_modes.iter().all(|mode| valid_text(mode, 500))
                && self.raw_snapshot_ids.iter().all(|id| valid_id(id))
                && self.proposal_ids.iter().all(|id| valid_id(id))
                && is_hash(&self.immutable_hash)
                && self.generated_at <= now,
            "invalid weekly performance report",
        )?;
        for value in [
            self.expectancy_fraction,
            self.hit_rate,
            self.max_drawdown_fraction,
            self.stop_adherence_fraction,
            self.target_adherence_fraction,
            self.exposure_fraction,
            self.calibration_error,
        ]
        .into_iter()
        .flatten()
        {
            require(
                value >= Decimal::ZERO && value <= Decimal::ONE,
                "weekly metric is outside its bounded fraction",
            )?;
        }
        Ok(())
    }

    /// A report is immutable evidence only. Activation always requires a
    /// separate reviewed release record, even when this flag is false.
    pub fn may_propose(&self) -> bool {
        !self.activation_allowed && !self.proposal_ids.is_empty()
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains(['\0', '\r', '\n'])
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Currency;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }
    fn money(amount: i64) -> Money {
        Money {
            amount: Decimal::from(amount),
            currency: Currency::new("EUR").expect("currency"),
        }
    }

    #[test]
    fn incomplete_daily_report_requires_an_explicit_gap() {
        let mut report = DailyReconciliation {
            schema_version: 1,
            report_id: "daily_1".into(),
            account_id: "paper_eur".into(),
            session_id: "xpar_2026_11_03".into(),
            broker_snapshot_ids: vec!["snapshot_1".into()],
            selection_run_id: None,
            realized_pnl: money(1),
            unrealized_pnl: money(0),
            fees: money(1),
            fx_impact: money(0),
            spread_cost: money(0),
            slippage_cost: money(0),
            quote_age_seconds: vec![1],
            news_age_seconds: vec![10],
            signal_latency_millis: vec![5],
            rejected_orders: 0,
            provider_errors: vec![],
            candidate_rankings: vec![],
            strategy_attribution: BTreeMap::new(),
            evidence_ids: vec![],
            complete: true,
            gap_reason: None,
            generated_at: now(),
        };
        assert!(report.validate(now()).is_ok());
        report.complete = false;
        assert!(report.validate(now()).is_err());
        report.gap_reason = Some("broker_history_gap".into());
        assert!(report.validate(now()).is_ok());
    }

    #[test]
    fn weekly_report_is_proposal_only_until_separate_release() {
        let report = WeeklyPerformanceReport {
            schema_version: 1,
            report_id: "weekly_1".into(),
            account_id: "paper_eur".into(),
            week_start: now() - Duration::days(7),
            week_end: now(),
            daily_report_ids: vec!["daily_1".into()],
            trade_count: 1,
            expectancy_fraction: Some(Decimal::new(1, 2)),
            hit_rate: Some(Decimal::ONE),
            max_drawdown_fraction: Some(Decimal::new(1, 2)),
            average_hold_seconds: Some(100),
            profit_distribution: vec![Decimal::new(1, 2)],
            stop_adherence_fraction: Some(Decimal::ONE),
            target_adherence_fraction: Some(Decimal::ONE),
            exposure_fraction: Some(Decimal::new(1, 2)),
            stock_type_performance: BTreeMap::new(),
            calibration_error: Some(Decimal::ZERO),
            failure_modes: vec![],
            raw_snapshot_ids: vec!["snapshot_1".into()],
            proposal_ids: vec!["proposal_1".into()],
            activation_allowed: false,
            immutable_hash: "a".repeat(64),
            generated_at: now(),
        };
        assert!(report.validate(now()).is_ok());
        assert!(report.may_propose());
    }
}

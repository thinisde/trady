//! Daily/weekly report scheduling and non-self-approving Codex handoff.

use chrono::{DateTime, Datelike, Duration, Timelike, Utc, Weekday};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use trady_domain::{
    Currency, DailyReconciliation, Error, Money, Result, WeeklyPerformanceReport, require,
};

use crate::harness::content_hash;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportSchedule {
    pub report_delay_after_flatten_seconds: i64,
    pub weekly_weekday: Weekday,
    pub weekly_hour_utc: u8,
}

impl Default for ReportSchedule {
    fn default() -> Self {
        Self {
            report_delay_after_flatten_seconds: 300,
            weekly_weekday: Weekday::Sat,
            weekly_hour_utc: 9,
        }
    }
}

impl ReportSchedule {
    pub fn validate(&self) -> Result<()> {
        require(
            (60..=3600).contains(&self.report_delay_after_flatten_seconds)
                && (0..=23).contains(&self.weekly_hour_utc),
            "invalid daily/weekly report schedule",
        )
    }

    pub fn daily_due(
        &self,
        flatten_deadline: DateTime<Utc>,
        last_report_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.validate()?;
        let due_at = flatten_deadline
            .checked_add_signed(Duration::seconds(self.report_delay_after_flatten_seconds))
            .ok_or_else(|| Error("daily report deadline overflow".into()))?;
        Ok(now >= due_at && last_report_at.is_none_or(|last| last < due_at))
    }

    pub fn weekly_due(
        &self,
        last_report_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        self.validate()?;
        Ok(now.weekday() == self.weekly_weekday
            && now.hour() >= u32::from(self.weekly_hour_utc)
            && last_report_at.is_none_or(|last| last.date_naive() < now.date_naive()))
    }
}

/// Build the immutable daily artifact from independently collected broker and
/// research observations. A missing provider is recorded as a gap rather than
/// converted into a clean-looking report.
#[allow(clippy::too_many_arguments)]
pub fn build_daily_reconciliation(
    account_id: String,
    session_id: String,
    broker_snapshot_ids: Vec<String>,
    selection_run_id: Option<String>,
    realized_pnl: Money,
    unrealized_pnl: Money,
    fees: Money,
    fx_impact: Money,
    spread_cost: Money,
    slippage_cost: Money,
    quote_age_seconds: Vec<u64>,
    news_age_seconds: Vec<u64>,
    signal_latency_millis: Vec<u64>,
    rejected_orders: u32,
    provider_errors: Vec<String>,
    candidate_rankings: Vec<String>,
    strategy_attribution: BTreeMap<String, Decimal>,
    evidence_ids: Vec<String>,
    gap_reason: Option<String>,
    now: DateTime<Utc>,
) -> Result<DailyReconciliation> {
    require(
        realized_pnl.currency == Currency::new("EUR")?
            && unrealized_pnl.currency == Currency::new("EUR")?
            && fees.currency == Currency::new("EUR")?
            && fx_impact.currency == Currency::new("EUR")?
            && spread_cost.currency == Currency::new("EUR")?
            && slippage_cost.currency == Currency::new("EUR")?,
        "daily reconciliation facts must be denominated in EUR",
    )?;
    let report = DailyReconciliation {
        schema_version: 1,
        report_id: format!("daily_{session_id}"),
        account_id,
        session_id,
        broker_snapshot_ids,
        selection_run_id,
        realized_pnl,
        unrealized_pnl,
        fees,
        fx_impact,
        spread_cost,
        slippage_cost,
        quote_age_seconds,
        news_age_seconds,
        signal_latency_millis,
        rejected_orders,
        provider_errors,
        candidate_rankings,
        strategy_attribution,
        evidence_ids,
        complete: gap_reason.is_none(),
        gap_reason,
        generated_at: now,
    };
    report.validate(now)?;
    Ok(report)
}

/// Produce a content-addressed weekly artifact. The function only summarizes
/// supplied daily records; it never mutates policy or activates a proposal.
#[allow(clippy::too_many_arguments)]
pub fn build_weekly_report(
    account_id: String,
    week_start: DateTime<Utc>,
    week_end: DateTime<Utc>,
    daily: &[DailyReconciliation],
    trade_count: u32,
    expectancy_fraction: Option<Decimal>,
    hit_rate: Option<Decimal>,
    max_drawdown_fraction: Option<Decimal>,
    average_hold_seconds: Option<u64>,
    profit_distribution: Vec<Decimal>,
    stop_adherence_fraction: Option<Decimal>,
    target_adherence_fraction: Option<Decimal>,
    exposure_fraction: Option<Decimal>,
    stock_type_performance: BTreeMap<String, Decimal>,
    calibration_error: Option<Decimal>,
    failure_modes: Vec<String>,
    proposal_ids: Vec<String>,
    now: DateTime<Utc>,
) -> Result<WeeklyPerformanceReport> {
    require(
        !daily.is_empty()
            && daily
                .iter()
                .all(|report| report.account_id == account_id && report.validate(now).is_ok()),
        "weekly report requires reconciled daily artifacts for the same account",
    )?;
    let raw_snapshot_ids = daily
        .iter()
        .flat_map(|report| {
            report
                .broker_snapshot_ids
                .iter()
                .chain(report.evidence_ids.iter())
                .cloned()
        })
        .collect::<Vec<_>>();
    let report_id = format!("weekly_{}_{}", week_start.timestamp(), week_end.timestamp());
    let hash_input = serde_json::to_string(&(
        &account_id,
        week_start,
        week_end,
        &daily
            .iter()
            .map(|report| &report.report_id)
            .collect::<Vec<_>>(),
        trade_count,
        &raw_snapshot_ids,
        &proposal_ids,
    ))
    .map_err(|_| Error("weekly report hash input encoding failed".into()))?;
    let report = WeeklyPerformanceReport {
        schema_version: 1,
        report_id,
        account_id,
        week_start,
        week_end,
        daily_report_ids: daily
            .iter()
            .map(|report| report.report_id.clone())
            .collect(),
        trade_count,
        expectancy_fraction,
        hit_rate,
        max_drawdown_fraction,
        average_hold_seconds,
        profit_distribution,
        stop_adherence_fraction,
        target_adherence_fraction,
        exposure_fraction,
        stock_type_performance,
        calibration_error,
        failure_modes,
        raw_snapshot_ids,
        proposal_ids,
        activation_allowed: false,
        immutable_hash: content_hash(&hash_input),
        generated_at: now,
    };
    report.validate(now)?;
    Ok(report)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexReviewHandoff {
    pub handoff_id: String,
    pub weekly_report_id: String,
    pub daily_report_ids: Vec<String>,
    pub evidence_snapshot_ids: Vec<String>,
    pub requested_at: DateTime<Utc>,
    pub reviewer: String,
    pub activation_allowed: bool,
}

impl CodexReviewHandoff {
    pub fn from_weekly(
        weekly: &WeeklyPerformanceReport,
        daily: &[DailyReconciliation],
        reviewer: String,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        weekly.validate(now)?;
        require(
            !reviewer.is_empty()
                && reviewer.len() <= 160
                && daily.iter().all(|report| report.validate(now).is_ok())
                && daily
                    .iter()
                    .all(|report| report.account_id == weekly.account_id),
            "invalid Codex weekly handoff scope",
        )?;
        Ok(Self {
            handoff_id: format!("codex_{}", weekly.report_id),
            weekly_report_id: weekly.report_id.clone(),
            daily_report_ids: daily
                .iter()
                .map(|report| report.report_id.clone())
                .collect(),
            evidence_snapshot_ids: weekly.raw_snapshot_ids.clone(),
            requested_at: now,
            reviewer,
            activation_allowed: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-07T09:00:00Z")
            .expect("time")
            .to_utc()
    }

    #[test]
    fn daily_report_waits_after_flatten_and_weekly_handoff_cannot_activate() {
        let schedule = ReportSchedule::default();
        let flatten = now() - Duration::minutes(10);
        assert!(schedule.daily_due(flatten, None, now()).expect("daily"));
        assert!(schedule.weekly_due(None, now()).expect("weekly"));
        let report = WeeklyPerformanceReport {
            schema_version: 1,
            report_id: "weekly_1".into(),
            account_id: "paper_eur".into(),
            week_start: now() - Duration::days(7),
            week_end: now(),
            daily_report_ids: vec!["daily_1".into()],
            trade_count: 0,
            expectancy_fraction: None,
            hit_rate: None,
            max_drawdown_fraction: None,
            average_hold_seconds: None,
            profit_distribution: vec![],
            stop_adherence_fraction: None,
            target_adherence_fraction: None,
            exposure_fraction: None,
            stock_type_performance: Default::default(),
            calibration_error: None,
            failure_modes: vec![],
            raw_snapshot_ids: vec!["snapshot_1".into()],
            proposal_ids: vec!["proposal_1".into()],
            activation_allowed: false,
            immutable_hash: "a".repeat(64),
            generated_at: now(),
        };
        let handoff =
            CodexReviewHandoff::from_weekly(&report, &[], "codex".into(), now()).expect("handoff");
        assert!(!handoff.activation_allowed);
    }

    #[test]
    fn builders_preserve_source_ids_and_create_immutable_weekly_hashes() {
        let at = now();
        let euro = Currency::new("EUR").expect("currency");
        let money = |amount: i64| Money {
            amount: Decimal::from(amount),
            currency: euro.clone(),
        };
        let daily = build_daily_reconciliation(
            "paper_eur".into(),
            "xpar_2026_11_07".into(),
            vec!["broker_snapshot_1".into()],
            Some("selection_1".into()),
            money(3),
            money(0),
            money(1),
            money(0),
            money(1),
            money(1),
            vec![2],
            vec![5],
            vec![100],
            0,
            vec![],
            vec!["AIR_PA_EQ".into()],
            BTreeMap::new(),
            vec!["alpha_evidence_1".into(), "yf_news_1".into()],
            None,
            at,
        )
        .expect("daily");
        let weekly = build_weekly_report(
            "paper_eur".into(),
            at - Duration::days(7),
            at,
            &[daily],
            1,
            Some(Decimal::new(2, 2)),
            Some(Decimal::ONE),
            Some(Decimal::ZERO),
            Some(600),
            vec![Decimal::new(2, 2)],
            Some(Decimal::ONE),
            Some(Decimal::ONE),
            Some(Decimal::new(1, 1)),
            BTreeMap::new(),
            Some(Decimal::ZERO),
            vec![],
            vec!["proposal_1".into()],
            at,
        )
        .expect("weekly");
        assert!(weekly.validate(at).is_ok());
        assert_eq!(weekly.immutable_hash.len(), 64);
        assert!(!weekly.activation_allowed);
    }
}

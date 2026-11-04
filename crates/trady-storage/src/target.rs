//! Append-only target-authority journal used by EUR replay and paper runs.
//!
//! PostgreSQL deployments use the same record shapes through the migration's
//! JSONB tables. The in-memory journal is intentionally strict so tests can
//! exercise idempotency and restart recovery without a database.

use chrono::{DateTime, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use trady_application::{
    broker::BrokerOrderRequest,
    eur_execution::{EurExecutionJournal, EurLifecycleSink},
    harness::content_hash,
    telegram::LifecycleEvent,
};
use trady_domain::{
    BrokerOrder, CoordinatorEvent, DailyReconciliation, Error, EurUniverseSnapshot, EvidenceItem,
    Result, SelectionRun, VenueQuote, WeeklyPerformanceReport, require,
};
use trady_policy::intraday::{DynamicExitDecision, IntradayTradePlan};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum TargetRecord {
    Universe(EurUniverseSnapshot),
    Quote(VenueQuote),
    Evidence(EvidenceItem),
    Selection(SelectionRun),
    CoordinatorEvent(CoordinatorEvent),
    TradePlan(IntradayTradePlan),
    Exit(DynamicExitDecision),
    OrderIntent(OrderIntentRecord),
    OrderAcknowledgement(OrderAcknowledgementRecord),
    Telegram(LifecycleEvent),
    DailyReport(DailyReconciliation),
    WeeklyReport(WeeklyPerformanceReport),
}

impl TargetRecord {
    pub fn id(&self) -> String {
        match self {
            Self::Universe(value) => value.snapshot_id.clone(),
            Self::Quote(value) => value.quote_id.clone(),
            Self::Evidence(value) => value.id.clone(),
            Self::Selection(value) => value.run_id.clone(),
            Self::CoordinatorEvent(value) => value.event_id.clone(),
            Self::TradePlan(value) => value.plan_id.clone(),
            Self::Exit(value) => format!(
                "exit_{}_{}",
                value.plan_id,
                value.decided_at.timestamp_millis()
            ),
            Self::OrderIntent(value) => value.intent_id.clone(),
            Self::OrderAcknowledgement(value) => format!("ack_{}", value.order.broker_order_id),
            Self::Telegram(value) => value.event_id.clone(),
            Self::DailyReport(value) => value.report_id.clone(),
            Self::WeeklyReport(value) => value.report_id.clone(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Universe(_) => "universe",
            Self::Quote(_) => "quote",
            Self::Evidence(_) => "evidence",
            Self::Selection(_) => "selection",
            Self::CoordinatorEvent(_) => "coordinator_event",
            Self::TradePlan(_) => "trade_plan",
            Self::Exit(_) => "exit",
            Self::OrderIntent(_) => "order_intent",
            Self::OrderAcknowledgement(_) => "order_ack",
            Self::Telegram(_) => "telegram",
            Self::DailyReport(_) => "daily_report",
            Self::WeeklyReport(_) => "weekly_report",
        }
    }

    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        match self {
            Self::Universe(value) => value.validate(now),
            Self::Quote(value) => value.validate(now, 300),
            Self::Evidence(value) => value.validate(now),
            Self::Selection(value) => value.validate(now),
            Self::CoordinatorEvent(value) => value.validate(now),
            Self::TradePlan(value) => {
                value.validate(now, &trady_policy::intraday::IntradayRiskPolicy::default())
            }
            Self::Exit(value) => require(
                !value.plan_id.is_empty() && value.decided_at <= now,
                "invalid target exit record",
            ),
            Self::OrderIntent(value) => value.validate(now),
            Self::OrderAcknowledgement(value) => value.validate(now),
            Self::Telegram(value) => value.validate(now),
            Self::DailyReport(value) => value.validate(now),
            Self::WeeklyReport(value) => value.validate(now),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderIntentRecord {
    pub intent_id: String,
    pub account_id: String,
    pub instrument_id: String,
    pub request_hash: String,
    pub status: String,
    pub request: BrokerOrderRequest,
    pub broker_order: Option<BrokerOrder>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderAcknowledgementRecord {
    pub intent_id: String,
    pub request_hash: String,
    pub status: String,
    pub order: BrokerOrder,
    pub acknowledged_at: DateTime<Utc>,
}

impl OrderAcknowledgementRecord {
    fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_id(&self.intent_id)
                && self.request_hash.len() == 64
                && self
                    .request_hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                && matches!(
                    self.status.as_str(),
                    "ACKNOWLEDGED" | "UNKNOWN" | "FILLED" | "REJECTED"
                )
                && !self.order.broker_order_id.is_empty()
                && self.order.quantity > rust_decimal::Decimal::ZERO
                && self.order.filled_quantity >= rust_decimal::Decimal::ZERO
                && self.order.filled_quantity <= self.order.quantity
                && self.acknowledged_at <= now,
            "invalid target broker acknowledgement",
        )
    }
}

impl OrderIntentRecord {
    fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        self.request
            .validate()
            .map_err(|_| Error("invalid target order intent".into()))?;
        require(
            self.intent_id == self.request.intent_id.to_string()
                && self.account_id == self.request.account_id.to_string()
                && self.instrument_id == self.request.instrument_id.to_string()
                && self.request_hash
                    == content_hash(
                        &serde_json::to_string(&self.request)
                            .map_err(|_| Error("order intent encoding failed".into()))?,
                    )
                && matches!(
                    self.status.as_str(),
                    "RESERVED" | "ACKNOWLEDGED" | "UNKNOWN" | "FILLED" | "REJECTED"
                )
                && self.recorded_at <= now,
            "invalid target order intent",
        )?;
        if let Some(order) = &self.broker_order {
            require(
                order.instrument_id == self.request.instrument_id
                    && order.side == self.request.side
                    && order.quantity == self.request.quantity,
                "target broker acknowledgement does not match intent",
            )?;
        }
        Ok(())
    }
}

/// In-memory implementation used by paper/replay tests and local target runs.
/// The same typed records can be forwarded to `target.append_record` by a
/// PostgreSQL deployment.
impl EurExecutionJournal for MemoryTargetStore {
    fn coordinator_event(&mut self, event: &CoordinatorEvent) -> Result<()> {
        self.append(
            TargetRecord::CoordinatorEvent(event.clone()),
            event.occurred_at,
        )
        .map(|_| ())
    }

    fn trade_plan(&mut self, plan: &IntradayTradePlan) -> Result<()> {
        self.append(TargetRecord::TradePlan(plan.clone()), plan.created_at)
            .map(|_| ())
    }

    fn order_intent(&mut self, request: &BrokerOrderRequest, at: DateTime<Utc>) -> Result<()> {
        let record = OrderIntentRecord {
            intent_id: request.intent_id.to_string(),
            account_id: request.account_id.to_string(),
            instrument_id: request.instrument_id.to_string(),
            request_hash: content_hash(
                &serde_json::to_string(request)
                    .map_err(|_| Error("order intent encoding failed".into()))?,
            ),
            status: "RESERVED".into(),
            request: request.clone(),
            broker_order: None,
            recorded_at: at,
        };
        self.append(TargetRecord::OrderIntent(record), at)
            .map(|_| ())
    }

    fn broker_order(&mut self, order: &BrokerOrder, at: DateTime<Utc>) -> Result<()> {
        let key = self
            .records("order_intent")
            .into_iter()
            .filter_map(|record| match record {
                TargetRecord::OrderIntent(value)
                    if value.request.intent_id.as_str()
                        == order
                            .client_intent_id
                            .as_ref()
                            .map_or("", |intent| intent.as_str()) =>
                {
                    Some(value.clone())
                }
                _ => None,
            })
            .next()
            .ok_or_else(|| Error("broker order lacks a reserved target intent".into()))?;
        let status = match order.status {
            trady_domain::OrderStatus::Filled => "FILLED",
            trady_domain::OrderStatus::Rejected => "REJECTED",
            trady_domain::OrderStatus::Unknown => "UNKNOWN",
            _ => "ACKNOWLEDGED",
        };
        // An append-only journal cannot rewrite the RESERVED record. Store the
        // acknowledgement under a deterministic second ID while preserving the
        // original request hash and identity.
        self.append(
            TargetRecord::OrderAcknowledgement(OrderAcknowledgementRecord {
                intent_id: key.intent_id,
                request_hash: key.request_hash,
                status: status.into(),
                order: order.clone(),
                acknowledged_at: at,
            }),
            at,
        )
        .map(|_| ())
    }

    fn dynamic_exit(&mut self, decision: &DynamicExitDecision) -> Result<()> {
        self.append(TargetRecord::Exit(decision.clone()), decision.decided_at)
            .map(|_| ())
    }
}

impl EurLifecycleSink for MemoryTargetStore {
    fn emit(&mut self, event: &LifecycleEvent) -> Result<()> {
        self.append(TargetRecord::Telegram(event.clone()), event.occurred_at)
            .map(|_| ())
    }
}

pub trait TargetStore {
    fn append(&mut self, record: TargetRecord, now: DateTime<Utc>) -> Result<bool>;
    fn get(&self, kind: &str, id: &str) -> Option<&TargetRecord>;
    fn records(&self, kind: &str) -> Vec<&TargetRecord>;
}

/// PostgreSQL target journal. The database function owns append-only
/// idempotency; this wrapper only validates and forwards typed documents.
pub struct PgTargetStore {
    client: Client,
    owner_id: String,
    account_id: String,
    pending_intents: BTreeMap<String, OrderIntentRecord>,
}

impl PgTargetStore {
    pub fn connect(dsn: &str, role: &str, owner_id: String, account_id: String) -> Result<Self> {
        require(
            valid_id(&owner_id)
                && valid_id(&account_id)
                && ["trady_executor", "trady_gateway", "trady_analyst"].contains(&role),
            "invalid target journal scope",
        )?;
        let mut client = crate::connect_local(dsn)?;
        crate::verify_role(&mut client, role)?;
        Ok(Self {
            client,
            owner_id,
            account_id,
            pending_intents: BTreeMap::new(),
        })
    }

    pub fn append_record(&mut self, record: &TargetRecord, now: DateTime<Utc>) -> Result<bool> {
        record.validate(now)?;
        let document = encode_record(record)?;
        let canonical = serde_json::to_string(&document)
            .map_err(|_| Error("target record encoding failed".into()))?;
        let hash = content_hash(&canonical);
        self.client
            .query_one(
                "SELECT target.append_record($1,$2,$3,$4,$5,$6)",
                &[
                    &record.kind(),
                    &record.id(),
                    &self.owner_id,
                    &self.account_id,
                    &document,
                    &hash,
                ],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }

    pub fn append_telegram(&mut self, event: &LifecycleEvent, now: DateTime<Utc>) -> Result<bool> {
        self.append_record(&TargetRecord::Telegram(event.clone()), now)
    }
}

impl EurExecutionJournal for PgTargetStore {
    fn coordinator_event(&mut self, event: &CoordinatorEvent) -> Result<()> {
        self.append_record(
            &TargetRecord::CoordinatorEvent(event.clone()),
            event.occurred_at,
        )
        .map(|_| ())
    }

    fn trade_plan(&mut self, plan: &IntradayTradePlan) -> Result<()> {
        self.append_record(&TargetRecord::TradePlan(plan.clone()), plan.created_at)
            .map(|_| ())
    }

    fn order_intent(&mut self, request: &BrokerOrderRequest, at: DateTime<Utc>) -> Result<()> {
        let record = OrderIntentRecord {
            intent_id: request.intent_id.to_string(),
            account_id: request.account_id.to_string(),
            instrument_id: request.instrument_id.to_string(),
            request_hash: content_hash(
                &serde_json::to_string(request)
                    .map_err(|_| Error("order intent encoding failed".into()))?,
            ),
            status: "RESERVED".into(),
            request: request.clone(),
            broker_order: None,
            recorded_at: at,
        };
        self.pending_intents
            .insert(record.intent_id.clone(), record.clone());
        self.append_record(&TargetRecord::OrderIntent(record), at)
            .map(|_| ())
    }

    fn broker_order(&mut self, order: &BrokerOrder, at: DateTime<Utc>) -> Result<()> {
        let intent_id = order
            .client_intent_id
            .as_ref()
            .map(|intent| intent.to_string())
            .ok_or_else(|| Error("broker order lacks target intent identity".into()))?;
        let intent = self
            .pending_intents
            .get(&intent_id)
            .ok_or_else(|| Error("broker order lacks a reserved target intent".into()))?;
        let status = match order.status {
            trady_domain::OrderStatus::Filled => "FILLED",
            trady_domain::OrderStatus::Rejected => "REJECTED",
            trady_domain::OrderStatus::Unknown => "UNKNOWN",
            _ => "ACKNOWLEDGED",
        };
        self.append_record(
            &TargetRecord::OrderAcknowledgement(OrderAcknowledgementRecord {
                intent_id,
                request_hash: intent.request_hash.clone(),
                status: status.into(),
                order: order.clone(),
                acknowledged_at: at,
            }),
            at,
        )
        .map(|_| ())
    }

    fn dynamic_exit(&mut self, decision: &DynamicExitDecision) -> Result<()> {
        self.append_record(&TargetRecord::Exit(decision.clone()), decision.decided_at)
            .map(|_| ())
    }
}

impl EurLifecycleSink for PgTargetStore {
    fn emit(&mut self, event: &LifecycleEvent) -> Result<()> {
        self.append_telegram(event, event.occurred_at).map(|_| ())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryTargetStore {
    rows: BTreeMap<(String, String), TargetRecord>,
}

impl MemoryTargetStore {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl TargetStore for MemoryTargetStore {
    fn append(&mut self, record: TargetRecord, now: DateTime<Utc>) -> Result<bool> {
        record.validate(now)?;
        let key = (record.kind().to_owned(), record.id());
        if let Some(existing) = self.rows.get(&key) {
            require(
                existing == &record,
                "target record ID was reused with different content",
            )?;
            return Ok(false);
        }
        self.rows.insert(key, record);
        Ok(true)
    }

    fn get(&self, kind: &str, id: &str) -> Option<&TargetRecord> {
        self.rows.get(&(kind.to_owned(), id.to_owned()))
    }

    fn records(&self, kind: &str) -> Vec<&TargetRecord> {
        self.rows
            .iter()
            .filter(|((record_kind, _), _)| record_kind == kind)
            .map(|(_, record)| record)
            .collect()
    }
}

/// Encode a record for the PostgreSQL JSONB append-only tables. Callers bind
/// the returned document with account/owner scope in the SQL function.
pub fn encode_record(record: &TargetRecord) -> Result<serde_json::Value> {
    serde_json::to_value(record).map_err(|_| Error("target record encoding failed".into()))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }

    #[test]
    fn append_is_idempotent_and_rejects_identity_reuse() {
        let mut store = MemoryTargetStore::default();
        let report = DailyReconciliation {
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
            quote_age_seconds: vec![],
            news_age_seconds: vec![],
            signal_latency_millis: vec![],
            rejected_orders: 0,
            provider_errors: vec![],
            candidate_rankings: vec![],
            strategy_attribution: BTreeMap::new(),
            evidence_ids: vec![],
            complete: true,
            gap_reason: None,
            generated_at: now(),
        };
        let record = TargetRecord::DailyReport(report.clone());
        assert!(store.append(record.clone(), now()).expect("append"));
        assert!(!store.append(record, now()).expect("idempotent"));
        let mut changed = report;
        changed.rejected_orders = 1;
        assert!(
            store
                .append(TargetRecord::DailyReport(changed), now())
                .is_err()
        );
        assert_eq!(store.len(), 1);
    }

    fn money(amount: i64) -> trady_domain::Money {
        trady_domain::Money {
            amount: amount.into(),
            currency: trady_domain::Currency::new("EUR").expect("currency"),
        }
    }

    #[test]
    fn encoded_target_records_are_tagged() {
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
            stock_type_performance: BTreeMap::new(),
            calibration_error: None,
            failure_modes: vec![],
            raw_snapshot_ids: vec![],
            proposal_ids: vec![],
            activation_allowed: false,
            immutable_hash: "a".repeat(64),
            generated_at: now(),
        };
        let value = encode_record(&TargetRecord::WeeklyReport(report)).expect("json");
        assert_eq!(value["kind"], "WEEKLY_REPORT");
    }
}

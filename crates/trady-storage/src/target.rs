//! Append-only target-authority journal used by EUR replay and paper runs.
//!
//! PostgreSQL deployments use the same record shapes through the migration's
//! JSONB tables. The in-memory journal is intentionally strict so tests can
//! exercise idempotency and restart recovery without a database.

use chrono::{DateTime, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
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
            Self::OrderAcknowledgement(value) => format!(
                "ack_{}_{}",
                value.order.broker_order_id,
                value.acknowledged_at.timestamp_micros()
            ),
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
                use trady_policy::intraday::{IntradayRiskPolicy, THIRTY_ENTRY_POLICY_VERSION};
                let policy = if value.policy_revision == THIRTY_ENTRY_POLICY_VERSION {
                    IntradayRiskPolicy {
                        revision: THIRTY_ENTRY_POLICY_VERSION.into(),
                        max_daily_entries: 30,
                        max_daily_exits: None,
                        ..IntradayRiskPolicy::default()
                    }
                } else {
                    IntradayRiskPolicy::default()
                };
                value.validate(now, &policy)
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
    fence: Option<(String, i64)>,
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
            fence: None,
        })
    }

    pub fn append_record(&mut self, record: &TargetRecord, now: DateTime<Utc>) -> Result<bool> {
        record.validate(now)?;
        let document = encode_record(record)?;
        self.append_document(record.kind(), &record.id(), &document)
    }

    /// Persist a bounded, scoped provider/coordination document through the
    /// same server-enforced hash and fence checks as typed records.
    pub fn append_document(
        &mut self,
        kind: &str,
        id: &str,
        document: &serde_json::Value,
    ) -> Result<bool> {
        let canonical = serde_json::to_string(&document)
            .map_err(|_| Error("target record encoding failed".into()))?;
        let hash = content_hash(&canonical);
        let (worker, generation) = self.fence.clone().unwrap_or_default();
        self.client
            .query_one(
                "SELECT target.append_scoped_record($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                &[
                    &kind,
                    &id,
                    &self.owner_id,
                    &self.account_id,
                    &document,
                    &canonical,
                    &hash,
                    &worker,
                    &generation,
                ],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }

    pub fn acquire_executor_lease(&mut self, worker_id: &str, ttl_seconds: i32) -> Result<i64> {
        require(
            valid_id(worker_id) && (1..=3600).contains(&ttl_seconds),
            "invalid EUR executor lease request",
        )?;
        let generation = self
            .client
            .query_one(
                "SELECT target.acquire_executor_lease($1,$2,$3)",
                &[&self.account_id, &worker_id, &ttl_seconds],
            )
            .map_err(crate::db)?
            .get(0);
        self.fence = Some((worker_id.to_owned(), generation));
        Ok(generation)
    }

    pub fn assert_executor_lease(&mut self, worker_id: &str, generation: i64) -> Result<()> {
        require(
            valid_id(worker_id) && generation > 0,
            "invalid EUR executor fence",
        )?;
        self.client
            .query_one(
                "SELECT target.assert_executor_lease($1,$2,$3)",
                &[&self.account_id, &worker_id, &generation],
            )
            .map_err(crate::db)
            .map(|_| ())
    }

    /// One physical DEMO account is shared by every logical EUR alias.
    pub fn acquire_demo_broker_lease(&mut self, worker_id: &str, ttl_seconds: i32) -> Result<i64> {
        require(
            valid_id(worker_id) && (1..=3600).contains(&ttl_seconds),
            "invalid DEMO executor fence",
        )?;
        let generation = self
            .client
            .query_one(
                "SELECT target.acquire_demo_executor_lease($1,$2,$3)",
                &[&self.account_id, &worker_id, &ttl_seconds],
            )
            .map_err(crate::db)?
            .get(0);
        self.fence = Some((worker_id.to_owned(), generation));
        Ok(generation)
    }

    pub fn renew_executor_lease(&mut self, ttl_seconds: i32) -> Result<()> {
        let (worker, generation) = self
            .fence
            .as_ref()
            .ok_or_else(|| Error("EUR executor has no fence".into()))?;
        self.client
            .query_one(
                "SELECT target.renew_executor_lease($1,$2,$3,$4)",
                &[&self.account_id, &worker, &generation, &ttl_seconds],
            )
            .map_err(crate::db)?;
        Ok(())
    }

    pub fn has_executor_fence(&self) -> bool {
        self.fence.is_some()
    }

    pub fn release_executor_lease(&mut self) -> Result<()> {
        if let Some((worker, generation)) = &self.fence {
            self.client
                .query_one(
                    "SELECT target.release_executor_lease($1,$2,$3)",
                    &[&self.account_id, &worker, &generation],
                )
                .map_err(crate::db)?;
            self.fence = None;
        }
        Ok(())
    }

    pub fn read_document(&mut self, kind: &str, id: &str) -> Result<Option<serde_json::Value>> {
        self.client
            .query_one(
                "SELECT target.read_record($1,$2,$3,$4)",
                &[&kind, &id, &self.owner_id, &self.account_id],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }

    pub fn physical_unresolved_protocols(&mut self) -> Result<serde_json::Value> {
        self.client
            .query_one("SELECT target.physical_unresolved_protocols()", &[])
            .map_err(crate::db)
            .map(|r| r.get(0))
    }

    pub fn protocol_is_externally_cleaned(&mut self, intent: &str) -> Result<bool> {
        self.client
            .query_one(
                "SELECT target.protocol_is_externally_cleaned($1,$2,$3)",
                &[&self.owner_id, &self.account_id, &intent],
            )
            .map_err(crate::db)
            .map(|r| r.get(0))
    }

    pub fn physical_unresolved_protocols_for_audit(&mut self) -> Result<serde_json::Value> {
        self.client
            .query_one(
                "SELECT target.physical_unresolved_protocols_for_audit()",
                &[],
            )
            .map_err(crate::db)
            .map(|r| r.get(0))
    }

    pub fn daily_entry_count(&mut self, day: chrono::NaiveDate) -> Result<i64> {
        self.client
            .query_one(
                "SELECT target.daily_entry_count($1,$2,$3)",
                &[&self.owner_id, &self.account_id, &day],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }
    pub fn list_documents(
        &mut self,
        kind: &str,
        since: DateTime<Utc>,
        limit: i32,
    ) -> Result<serde_json::Value> {
        self.client
            .query_one(
                "SELECT target.list_records($1,$2,$3,$4,$5)",
                &[&kind, &self.owner_id, &self.account_id, &since, &limit],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }
    pub fn list_record_window(
        &mut self,
        kind: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        after: Option<DateTime<Utc>>,
        after_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        self.client
            .query_one(
                "SELECT target.list_record_window($1,$2,$3,$4,$5,$6,$7,64)",
                &[
                    &kind,
                    &self.owner_id,
                    &self.account_id,
                    &start,
                    &end,
                    &after,
                    &after_id,
                ],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }
    pub fn read_review_artifact(&mut self, id: &str) -> Result<Option<serde_json::Value>> {
        self.client
            .query_one(
                "SELECT target.read_review_artifact($1,$2,$3)",
                &[&self.owner_id, &self.account_id, &id],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }
    pub fn append_review_artifact(&mut self, id: &str, value: &serde_json::Value) -> Result<bool> {
        let canonical = serde_json::to_string(value)
            .map_err(|_| Error("review bundle encoding failed".into()))?;
        self.client
            .query_one(
                "SELECT target.append_review_artifact($1,$2,$3,$4,$5,$6)",
                &[
                    &self.owner_id,
                    &self.account_id,
                    &id,
                    &value,
                    &canonical,
                    &content_hash(&canonical),
                ],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }

    pub fn telegram_delivery_summary(&mut self, deployment_id: &str) -> Result<serde_json::Value> {
        self.client
            .query_one(
                "SELECT target.telegram_delivery_summary($1,$2,$3)",
                &[&deployment_id, &self.owner_id, &self.account_id],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }

    pub fn append_telegram(&mut self, event: &LifecycleEvent, now: DateTime<Utc>) -> Result<bool> {
        self.append_record(&TargetRecord::Telegram(event.clone()), now)
    }

    /// Queue a delivery through the PostgreSQL outbox. The SQL function uses
    /// `event_id + notification_kind` as the idempotency key and rejects a
    /// changed payload under an existing key.
    pub fn enqueue_telegram_delivery(
        &mut self,
        deployment_id: &str,
        event: &LifecycleEvent,
        notification_kind: &str,
        chat_id: i64,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        event.validate(now)?;
        require(
            event.account_id.as_str() == self.account_id,
            "EUR lifecycle event belongs to another account",
        )?;
        require(
            !notification_kind.is_empty()
                && notification_kind.len() <= 64
                && notification_kind
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
                && chat_id > 0,
            "invalid Telegram outbox scope",
        )?;
        let payload = json!({
            "event_id": event.event_id,
            "kind": event.kind,
            "account_id": event.account_id,
            "session_id": event.session_id,
            "instrument_id": event.instrument_id,
            "evidence_ids": event.evidence_ids,
            "text": event.render(),
            "occurred_at": event.occurred_at,
        });
        let canonical = serde_json::to_string(&payload)
            .map_err(|_| Error("Telegram outbox payload encoding failed".into()))?;
        let hash = content_hash(&canonical);
        let (worker, generation) = self
            .fence
            .as_ref()
            .ok_or_else(|| Error("EUR lifecycle sink requires the executor fence".into()))?;
        self.client
            .query_one(
                "SELECT target.enqueue_telegram($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
                &[
                    &deployment_id,
                    &event.event_id,
                    &notification_kind,
                    &chat_id,
                    &self.owner_id,
                    &self.account_id,
                    &payload,
                    &canonical,
                    &hash,
                    &worker,
                    &generation,
                ],
            )
            .map_err(crate::db)
            .map(|row| row.get(0))
    }

    /// Commit the lifecycle journal record and every configured delivery in one
    /// transaction. A restart cannot leave a committed event without its outbox.
    pub fn publish_lifecycle(
        &mut self,
        deployment_id: &str,
        event: &LifecycleEvent,
        chats: &[i64],
        now: DateTime<Utc>,
    ) -> Result<()> {
        require(
            !chats.is_empty() && chats.len() <= 32,
            "EUR lifecycle delivery chats are missing",
        )?;
        let kind = serde_json::to_value(event.kind)
            .map_err(|_| Error("EUR lifecycle kind encoding failed".into()))?;
        let kind = kind
            .as_str()
            .ok_or_else(|| Error("invalid EUR lifecycle kind".into()))?;
        self.client.batch_execute("BEGIN").map_err(crate::db)?;
        let result = (|| {
            self.append_telegram(event, now)?;
            for chat in chats {
                self.enqueue_telegram_delivery(deployment_id, event, kind, *chat, now)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.client.batch_execute("COMMIT").map_err(crate::db),
            Err(error) => {
                let _ = self.client.batch_execute("ROLLBACK");
                Err(error)
            }
        }
    }
}

impl trady_application::eur_protocol::ProtocolJournal for PgTargetStore {
    fn record_protection_confirmation(
        &mut self,
        context: &trady_application::eur_protocol::ProtocolContext,
        view: &trady_application::eur_protocol::driver::ProtectionConfirmation,
    ) -> Result<()> {
        require(
            context.account_id == self.account_id,
            "protection confirmation account differs",
        )?;
        self.append_document(
            "protocol_protection_confirmation",
            &trady_application::harness::new_id("stop_confirmation"),
            &serde_json::json!({"intent_id":context.intent_id,"confirmation":view}),
        )?;
        Ok(())
    }
    fn record_observation(
        &mut self,
        context: &trady_application::eur_protocol::ProtocolContext,
        view: &trady_application::eur_protocol::driver::BrokerObservation,
    ) -> Result<()> {
        require(
            context.account_id == self.account_id,
            "broker observation account differs",
        )?;
        self.append_document(
            "protocol_broker_observation",
            &trady_application::harness::new_id("broker_view"),
            &serde_json::json!({"intent_id":context.intent_id,"observation":view}),
        )?;
        Ok(())
    }
    fn load(
        &mut self,
        intent_id: &str,
    ) -> Result<Vec<trady_application::eur_protocol::ProtocolRecord>> {
        let value: serde_json::Value = self
            .client
            .query_one(
                "SELECT target.protocol_history($1,$2,$3)",
                &[&self.owner_id, &self.account_id, &intent_id],
            )
            .map_err(crate::db)?
            .get(0);
        serde_json::from_value(value)
            .map_err(|_| Error("invalid durable EUR protocol history".into()))
    }
    fn append(
        &mut self,
        expected_revision: u64,
        record: &trady_application::eur_protocol::ProtocolRecord,
    ) -> Result<()> {
        require(
            record.state.context.account_id == self.account_id,
            "protocol account differs from journal scope",
        )?;
        let (worker, generation) = self
            .fence
            .clone()
            .ok_or_else(|| Error("protocol dispatch requires an executor fence".into()))?;
        let value = serde_json::to_value(record)
            .map_err(|_| Error("protocol record encoding failed".into()))?;
        let canonical =
            serde_json::to_string(record).map_err(|_| Error("protocol encoding failed".into()))?;
        let state = serde_json::to_string(&record.state)
            .map_err(|_| Error("protocol state encoding failed".into()))?;
        let expected = i64::try_from(expected_revision)
            .map_err(|_| Error("protocol revision overflow".into()))?;
        self.client
            .query_one(
                "SELECT target.append_protocol_event($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
                &[
                    &self.owner_id,
                    &self.account_id,
                    &record.state.context.intent_id,
                    &expected,
                    &value,
                    &canonical,
                    &content_hash(&canonical),
                    &state,
                    &worker,
                    &generation,
                ],
            )
            .map_err(crate::db)?;
        Ok(())
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
        self.append_record(&TargetRecord::OrderIntent(record), at)
            .map(|_| ())
    }

    fn broker_order(&mut self, order: &BrokerOrder, at: DateTime<Utc>) -> Result<()> {
        let intent_id = order
            .client_intent_id
            .as_ref()
            .map(|intent| intent.to_string())
            .ok_or_else(|| Error("broker order lacks target intent identity".into()))?;
        let record: TargetRecord =
            serde_json::from_value(self.read_document("order_intent", &intent_id)?.ok_or_else(
                || Error("broker order lacks a durable reserved target intent".into()),
            )?)
            .map_err(|_| Error("invalid stored EUR order intent".into()))?;
        let TargetRecord::OrderIntent(intent) = record else {
            return Err(Error("invalid stored EUR order intent kind".into()));
        };
        require(
            order.instrument_id == intent.request.instrument_id
                && order.quantity == intent.request.quantity
                && order.side == intent.request.side,
            "EUR broker observation differs from the durable intent",
        )?;
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

/// Gateway-only delivery claims survive process death. A claim whose send was
/// interrupted remains SENDING/UNKNOWN and needs operator review before retry.
pub struct PgTargetOutbox {
    client: Client,
    deployment_id: String,
    allowed_chats: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetDelivery {
    pub event_id: String,
    pub notification_kind: String,
    pub chat_id: i64,
    pub payload: serde_json::Value,
    pub payload_hash: String,
    #[serde(skip)]
    token: String,
}

impl PgTargetOutbox {
    pub fn connect(dsn: &str, deployment_id: String, allowed_chats: Vec<i64>) -> Result<Self> {
        require(
            valid_id(&deployment_id)
                && !allowed_chats.is_empty()
                && allowed_chats.len() <= 32
                && allowed_chats.iter().all(|id| *id > 0),
            "invalid EUR gateway delivery scope",
        )?;
        let mut client = crate::connect_local(dsn)?;
        crate::verify_role(&mut client, "trady_gateway")?;
        Ok(Self {
            client,
            deployment_id,
            allowed_chats,
        })
    }

    pub fn claim(&mut self) -> Result<Option<TargetDelivery>> {
        let token = trady_application::harness::new_id("delivery");
        let value: Option<serde_json::Value> = self
            .client
            .query_one(
                "SELECT target.claim_telegram($1,$2,$3)",
                &[&self.deployment_id, &self.allowed_chats, &token],
            )
            .map_err(crate::db)?
            .get(0);
        value
            .map(|value| {
                let mut delivery: TargetDelivery = serde_json::from_value(value)
                    .map_err(|_| Error("invalid persisted EUR delivery".into()))?;
                let canonical = serde_json::to_string(&delivery.payload)
                    .map_err(|_| Error("invalid persisted EUR delivery encoding".into()))?;
                require(
                    self.allowed_chats.contains(&delivery.chat_id)
                        && delivery.payload_hash == content_hash(&canonical),
                    "EUR delivery hash or chat scope differs",
                )?;
                delivery.token = token;
                Ok(delivery)
            })
            .transpose()
    }

    pub fn finish(&mut self, delivery: &TargetDelivery, sent: bool) -> Result<()> {
        self.client
            .query_one(
                "SELECT target.finish_telegram($1,$2,$3,$4,$5,$6)",
                &[
                    &self.deployment_id,
                    &delivery.event_id,
                    &delivery.notification_kind,
                    &delivery.chat_id,
                    &delivery.token,
                    &sent,
                ],
            )
            .map_err(crate::db)?;
        Ok(())
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
    fn persisted_thirty_entry_plan_keeps_reproducible_financial_checks() {
        use rust_decimal::Decimal;
        use trady_policy::intraday::{IntradayCostModel, THIRTY_ENTRY_POLICY_VERSION};
        let costs = IntradayCostModel {
            entry: trady_policy::EntryCosts {
                fixed: money(0),
                rate: Decimal::ZERO,
                uncertainty: money(0),
                fx: None,
            },
            exit: trady_policy::economics::ExitCosts {
                fixed: money(0),
                rate: Decimal::ZERO,
                uncertainty: money(0),
                fx: None,
            },
        };
        let mut plan = IntradayTradePlan {
            plan_id: "synthetic_plan_30".into(),
            policy_revision: THIRTY_ENTRY_POLICY_VERSION.into(),
            account_id: "paper_fixture".into(),
            instrument_id: trady_domain::InstrumentId::new("AIR_PA_EQ").unwrap(),
            currency: money(0).currency,
            entry_price: 100.into(),
            quantity: Decimal::ONE,
            acquisition_cost: money(100),
            stop_price: 99.into(),
            target_price: 102.into(),
            target_net_fraction: Decimal::new(2, 2),
            planned_loss_fraction: Decimal::new(1, 2),
            reward_to_risk: 2.into(),
            created_at: now(),
            flatten_deadline: now() + Duration::hours(1),
            evidence_snapshot_ids: vec!["synthetic_fixture_only".into()],
            stop_revision: 0,
            cost_model: Some(costs),
        };
        let mut store = MemoryTargetStore::default();
        store
            .append(TargetRecord::TradePlan(plan.clone()), now())
            .unwrap();
        plan.policy_revision = "invented_unreviewed_policy".into();
        assert!(
            TargetRecord::TradePlan(plan.clone())
                .validate(now())
                .is_err()
        );
        plan.policy_revision = THIRTY_ENTRY_POLICY_VERSION.into();
        plan.target_price = 103.into();
        assert!(
            TargetRecord::TradePlan(plan).validate(now()).is_err(),
            "policy change cannot waive risk arithmetic"
        );
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

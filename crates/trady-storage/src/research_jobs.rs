//! Information-only queue, verifier packets and gateway command persistence.

use chrono::{DateTime, Utc};
use postgres::{Client, GenericClient, Row};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use trady_application::{
    harness::{Scope, content_hash, new_id, valid_id},
    research_jobs::*,
    telegram::gateway::*,
};
use trady_domain::{
    Error, Result, require,
    research::{SourceItem, SourceState},
    sentiment::{SentimentAssessment, SentimentEvidence},
};

use crate::{connect_local, verify_role};

const JOB_COLUMNS: &str = "job_id,owner_id,account_id,policy_version,work,policy_hash,status,created_at,attempts,lease_generation,result,error_code";

pub struct PgResearchJobs {
    client: Client,
    role: &'static str,
}
fn db(_: postgres::Error) -> Error {
    Error("research job database operation failed (scope, budget, lease or availability)".into())
}
fn encode<T: Serialize>(v: &T) -> Result<Value> {
    serde_json::to_value(v).map_err(|_| Error("job record encoding failed".into()))
}
fn decode<T: DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v)
        .map_err(|_| Error("unsupported or corrupt research job record".into()))
}
fn version(scope: &Scope) -> Result<i32> {
    scope.validate()?;
    i32::try_from(scope.policy_version).map_err(|_| Error("policy version too large".into()))
}
fn read_policy(db_client: &mut impl GenericClient, scope: &Scope) -> Result<JobPolicy> {
    let row=db_client.query_opt("SELECT document FROM research.job_policies WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3",
        &[&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db)?.ok_or_else(||Error("research scope not provisioned by analyst".into()))?;
    let policy: JobPolicy = decode(row.get(0))?;
    policy.validate()?;
    require(policy.scope == *scope, "stored job scope mismatch")?;
    Ok(policy)
}
fn job(row: Row) -> Result<ResearchJob> {
    Ok(ResearchJob {
        id: row.get(0),
        scope: Scope {
            owner_id: row.get(1),
            account_id: row.get(2),
            policy_version: row.get::<_, i32>(3) as u32,
        },
        work: decode(row.get(4))?,
        policy_hash: row.get(5),
        status: decode(Value::String(row.get(6)))?,
        created_at: row.get(7),
        attempts: row.get::<_, i32>(8) as u32,
        generation: row.get(9),
        result: row.get(10),
        error_code: row.get(11),
    })
}
fn read_job(
    db_client: &mut impl GenericClient,
    scope: &Scope,
    id: &str,
) -> Result<Option<ResearchJob>> {
    require(valid_id(id), "invalid job ID")?;
    db_client.query_opt(&format!("SELECT {JOB_COLUMNS} FROM control.research_work WHERE job_id=$1 AND owner_id=$2 AND account_id=$3 AND policy_version=$4"),
        &[&id,&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db)?.map(job).transpose()
}
fn enqueue(
    db_client: &mut impl GenericClient,
    scope: &Scope,
    key: &str,
    work: &Work,
) -> Result<ResearchJob> {
    let policy = read_policy(db_client, scope)?;
    work.validate(&policy)?;
    require(
        !key.is_empty()
            && key.len() <= 160
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':')),
        "invalid work idempotency key",
    )?;
    let id = new_id("job");
    let document = encode(work)?;
    let row = db_client
        .query_one(
            "SELECT control.enqueue_information_work($1,$2,$3,$4,$5,$6,$7,$8)",
            &[
                &scope.owner_id,
                &scope.account_id,
                &version(scope)?,
                &key,
                &document,
                &digest(work)?,
                &digest(&policy)?,
                &id,
            ],
        )
        .map_err(db)?;
    read_job(db_client, scope, &row.get::<_, String>(0))?
        .ok_or_else(|| Error("queued job disappeared".into()))
}
fn check_lease(db_client: &mut impl GenericClient, lease: &JobLease) -> Result<()> {
    let row=db_client.query_opt("SELECT policy_hash FROM control.research_work WHERE job_id=$1 AND owner_id=$2 AND account_id=$3 AND policy_version=$4
        AND status='RUNNING' AND lease_worker=$5 AND lease_generation=$6 AND lease_until>clock_timestamp() FOR UPDATE",
        &[&lease.job.id,&lease.job.scope.owner_id,&lease.job.scope.account_id,&version(&lease.job.scope)?,&lease.worker_id,&lease.generation]).map_err(db)?
        .ok_or_else(||Error("research job lease expired or fenced".into()))?;
    let policy = read_policy(db_client, &lease.job.scope)?;
    require(
        policy.enabled && row.get::<_, String>(0) == digest(&policy)?,
        "research job policy changed or revoked",
    )
}
fn finish(
    db_client: &mut impl GenericClient,
    lease: &JobLease,
    result: Option<&Value>,
    error: Option<&str>,
) -> Result<()> {
    let status = if error.is_some() {
        "FAILED"
    } else {
        "COMPLETE"
    };
    if let Some(v) = result {
        require(
            serde_json::to_vec(v)
                .map_err(|_| Error("job result encoding failed".into()))?
                .len()
                <= 12000,
            "job result exceeds bound",
        )?;
    }
    db_client.execute("UPDATE control.research_work SET status=$2,result=$3,error_code=$4,completed_at=clock_timestamp(),lease_until=clock_timestamp() WHERE job_id=$1",
        &[&lease.job.id,&status,&result,&error]).map_err(db)?;
    let text = if let Some(error) = error {
        format!(
            "Job {} FAILED: {error}. No entry permission was created.",
            lease.job.id
        )
    } else if let Some(text) = result
        .and_then(|r| r.get("reply_text"))
        .and_then(Value::as_str)
    {
        format!(
            "Job {} COMPLETE\n{}",
            lease.job.id,
            text.chars().take(2800).collect::<String>()
        )
    } else {
        format!(
            "Job {} COMPLETE\n{}",
            lease.job.id,
            result.unwrap_or(&Value::Null)
        )
    };
    // Job completion and its final notifications commit in one transaction.
    db_client.execute("INSERT INTO control.gateway_outbox(delivery_id,bot_id,owner_user_id,chat_id,owner_id,account_id,policy_version,job_id,text)
        SELECT 'delivery_'||md5(bot_id::text||':'||chat_id::text||':'||job_id),bot_id,owner_user_id,chat_id,owner_id,account_id,policy_version,job_id,$2
        FROM control.gateway_receipts WHERE job_id=$1 GROUP BY bot_id,owner_user_id,chat_id,owner_id,account_id,policy_version,job_id ON CONFLICT DO NOTHING",
        &[&lease.job.id,&text]).map_err(db)?;
    Ok(())
}
fn load_packet(
    db_client: &mut impl GenericClient,
    scope: &Scope,
    id: &str,
    now: DateTime<Utc>,
) -> Result<(VerifiedResearchPacket, Vec<SourceItem>)> {
    require(valid_id(id), "invalid packet ID")?;
    let policy = read_policy(db_client, scope)?;
    let row=db_client.query_opt("SELECT document,packet_hash FROM research.verified_packets WHERE packet_id=$1 AND owner_id=$2 AND account_id=$3 AND policy_version=$4",
        &[&id,&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db)?.ok_or_else(||Error("verified packet unavailable in scope".into()))?;
    let packet: VerifiedResearchPacket = decode(row.get(0))?;
    require(
        digest(&packet)? == row.get::<_, String>(1),
        "packet immutable hash mismatch",
    )?;
    let mut items = vec![];
    for dep in &packet.sources {
        let row=db_client.query_opt("SELECT i.document,s.document FROM research.source_items i JOIN research.source_states s USING(source_id) WHERE i.item_id=$1 FOR SHARE OF i,s",
            &[&dep.item_id]).map_err(db)?.ok_or_else(||Error("packet dependency unavailable".into()))?;
        let mut item: SourceItem = decode(row.get(0))?;
        let state: SourceState = decode(row.get(1))?;
        require(
            state.registry_hash.as_ref() == Some(&dep.source_registry_hash)
                && state.rights.as_ref() == Some(&dep.source_rights),
            "source registry or rights changed",
        )?;
        if let Some(rights) = state.rights {
            item.rights.store_content &= rights.store_content;
            item.rights.llm_processing &= rights.llm_processing;
            item.expires_at = item.expires_at.min(
                item.first_seen_at + chrono::Duration::hours(i64::from(rights.retention_hours)),
            );
        }
        item.redact(now, true);
        items.push(item);
    }
    check_dependencies(&packet, &items, &policy, now)?;
    require(
        !db_client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM research.dependency_invalidations WHERE packet_id=$1)",
                &[&id],
            )
            .map_err(db)?
            .get::<_, bool>(0),
        "packet invalidated",
    )?;
    Ok((packet, items))
}

impl PgResearchJobs {
    fn connect(dsn: &str, role: &'static str) -> Result<Self> {
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, role)?;
        client
            .batch_execute("SET statement_timeout='5s'; SET lock_timeout='5s'")
            .map_err(db)?;
        Ok(Self { client, role })
    }
    pub fn connect_gateway(dsn: &str) -> Result<Self> {
        Self::connect(dsn, "trady_gateway")
    }
    pub fn connect_analyst(dsn: &str) -> Result<Self> {
        Self::connect(dsn, "trady_analyst")
    }
    pub fn connect_executor(dsn: &str) -> Result<Self> {
        Self::connect(dsn, "trady_executor")
    }
    pub fn install_policy(&mut self, policy: &JobPolicy) -> Result<()> {
        require(
            self.role == "trady_analyst",
            "only analyst can provision research scopes",
        )?;
        policy.validate()?;
        self.client.execute("INSERT INTO research.job_policies(owner_id,account_id,policy_version,document,policy_hash) VALUES($1,$2,$3,$4,$5)
            ON CONFLICT(owner_id,account_id,policy_version) DO UPDATE SET document=EXCLUDED.document,policy_hash=EXCLUDED.policy_hash,updated_at=clock_timestamp()",
            &[&policy.scope.owner_id,&policy.scope.account_id,&version(&policy.scope)?,&encode(policy)?,&digest(policy)?]).map_err(db)?;
        Ok(())
    }
    pub fn get_job(&mut self, scope: &Scope, id: &str) -> Result<Option<ResearchJob>> {
        read_job(&mut self.client, scope, id)
    }
    pub fn recent_jobs(&mut self, scope: &Scope, limit: usize) -> Result<Vec<ResearchJob>> {
        require((1..=20).contains(&limit), "job list limit out of range")?;
        self.client.query(&format!("SELECT {JOB_COLUMNS} FROM control.research_work WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 ORDER BY created_at DESC,job_id LIMIT $4"),
            &[&scope.owner_id,&scope.account_id,&version(scope)?,&(limit as i64)]).map_err(db)?.into_iter().map(job).collect()
    }
    pub fn budget(&mut self, scope: &Scope) -> Result<Value> {
        let policy = self.policy(scope)?;
        let row=self.client.query_one("SELECT count(*) FILTER(WHERE created_at>=date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'),count(*) FILTER(WHERE status IN ('QUEUED','RUNNING'))
            FROM control.research_work WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3",
            &[&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db)?;
        Ok(
            json!({"jobs_today":row.get::<_,i64>(0),"daily_job_limit":policy.daily_job_limit,"pending_jobs":row.get::<_,i64>(1),"pending_job_limit":policy.pending_job_limit,
            "source_spending":"configured source collectors enforce their own durable caps","model_spending":"session harness enforces token reservations; monetary cost unavailable","entry_authorized":false}),
        )
    }

    /// Source-verifier entry point. No LLM tool accepts verification outcomes.
    #[allow(
        clippy::too_many_arguments,
        reason = "trusted verifier handoff keeps scope, provenance, claims and expiry explicit without changing the integration API"
    )]
    pub fn publish_verified_packet(
        &mut self,
        scope: &Scope,
        instrument: &str,
        profile_revision: Option<String>,
        verifications: Vec<Verification>,
        claims: Vec<ClaimReview>,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<VerifiedResearchPacket> {
        require(
            self.role == "trady_analyst",
            "only trusted analyst verifier may publish packets",
        )?;
        let mut tx = self.client.transaction().map_err(db)?;
        let policy = read_policy(&mut tx, scope)?;
        let mut dependencies = vec![];
        for verification in verifications {
            let row=tx.query_opt("SELECT i.document,s.document FROM research.source_items i JOIN research.source_states s USING(source_id) WHERE i.item_id=$1 FOR SHARE OF i,s",
                &[&verification.item_id]).map_err(db)?.ok_or_else(||Error("verified packet item is not persisted".into()))?;
            let item: SourceItem = decode(row.get(0))?;
            let state: SourceState = decode(row.get(1))?;
            let rights = state
                .rights
                .ok_or_else(|| Error("source rights not reviewed".into()))?;
            dependencies.push(SourceDependency {
                item_id: item.item_id,
                source_id: item.source_id,
                content_hash: item.content_hash,
                lineage_id: item.lineage_id,
                registry_hash: policy.registry_hash.clone(),
                source_registry_hash: state
                    .registry_hash
                    .ok_or_else(|| Error("source registry revision unavailable".into()))?,
                source_rights: rights,
                item_rights: item.rights.clone(),
                rights_hash: digest(&item.rights)?,
                expires_at: item.expires_at,
                verification,
            });
        }
        let packet = VerifiedResearchPacket {
            id: new_id("packet"),
            scope: scope.clone(),
            instrument_id: instrument.into(),
            profile_revision_id: profile_revision,
            registry_hash: policy.registry_hash.clone(),
            sources: dependencies,
            claims,
            created_at: now,
            expires_at,
        };
        packet.validate(&policy, now)?;
        tx.execute("INSERT INTO research.verified_packets(packet_id,owner_id,account_id,policy_version,instrument_id,packet_hash,document,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            &[&packet.id,&scope.owner_id,&scope.account_id,&version(scope)?,&packet.instrument_id,&digest(&packet)?,&encode(&packet)?,&packet.expires_at]).map_err(db)?;
        load_packet(&mut tx, scope, &packet.id, now)?;
        tx.commit().map_err(db)?;
        Ok(packet)
    }
    pub fn latest_packets(
        &mut self,
        scope: &Scope,
        instrument: Option<&str>,
        limit: usize,
    ) -> Result<Vec<VerifiedResearchPacket>> {
        require(
            (1..=10).contains(&limit) && instrument.is_none_or(valid_id),
            "invalid packet query",
        )?;
        self.client.query("SELECT document FROM research.verified_packets WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 AND ($4::text IS NULL OR instrument_id=$4) ORDER BY created_at DESC LIMIT $5",
            &[&scope.owner_id,&scope.account_id,&version(scope)?,&instrument,&(limit as i64)]).map_err(db)?.into_iter().map(|r| decode(r.get(0))).collect()
    }
    pub fn checked_readiness_reference(
        &mut self,
        scope: &Scope,
        id: &str,
        now: DateTime<Utc>,
    ) -> Result<SentimentReadinessRef> {
        let row=self.client.query_opt("SELECT readiness_reference FROM research.sentiment_assessments WHERE assessment_id=$1 AND owner_id=$2 AND account_id=$3 AND policy_version=$4 AND expires_at>$5",
            &[&id,&scope.owner_id,&scope.account_id,&version(scope)?,&now]).map_err(db)?.ok_or_else(||Error("required sentiment assessment unavailable".into()))?;
        let value: Option<Value> = row.get(0);
        let value = value.ok_or_else(|| {
            Error("sentiment assessment has unresolved readiness blockers".into())
        })?;
        let reference: SentimentReadinessRef = decode(value.clone())?;
        require(
            reference.scope == *scope
                && reference.expires_at > now
                && self.reference_current(&reference)?,
            "sentiment reference invalidated",
        )?;
        Ok(reference)
    }
    /// For an atomic plan check, call research.sentiment_reference_current($1)
    /// from the SAME PostgreSQL transaction that accepts the plan.
    pub fn reference_current(&mut self, reference: &SentimentReadinessRef) -> Result<bool> {
        Ok(self
            .client
            .query_one(
                "SELECT research.sentiment_reference_current($1)",
                &[&encode(reference)?],
            )
            .map_err(db)?
            .get(0))
    }
    pub fn invalidate_dependencies(&mut self, scope: &Scope, now: DateTime<Utc>) -> Result<usize> {
        require(
            self.role == "trady_analyst",
            "only analyst records invalidations",
        )?;
        let packets = self.latest_packets(scope, None, 10)?;
        let mut count = 0;
        for packet in packets {
            if self.packet(scope, &packet.id, now).is_err() {
                count+=self.client.execute("INSERT INTO research.dependency_invalidations(packet_id,reason) VALUES($1,'DEPENDENCY_REVOKED_CHANGED_OR_EXPIRED') ON CONFLICT DO NOTHING",&[&packet.id]).map_err(db)? as usize;
            }
        }
        Ok(count)
    }
    pub fn schedule(
        &mut self,
        scope: &Scope,
        date: chrono::NaiveDate,
        holdings: &[HoldingWatch],
        now: DateTime<Utc>,
        interval: u32,
    ) -> Result<Vec<String>> {
        require(
            self.role == "trady_analyst" && holdings.len() <= 20,
            "invalid research scheduler caller or holdings",
        )?;
        let mut ids = vec![
            self.enqueue(
                scope,
                &daily_key(scope, date)?,
                &Work::Daily {
                    session_date: date.to_string(),
                },
            )?
            .id,
        ];
        for h in holdings.iter().filter(|h| h.active) {
            ids.push(
                self.enqueue(
                    scope,
                    &watchdog_key(scope, h, now, interval)?,
                    &Work::HoldingWatchdog {
                        holding_id: h.holding_id.clone(),
                        instrument_id: h.instrument_id.clone(),
                        holding_revision: h.revision_hash.clone(),
                    },
                )?
                .id,
            );
        }
        let active: Vec<_> = holdings
            .iter()
            .filter(|h| h.active)
            .map(|h| h.holding_id.clone())
            .collect();
        self.client.execute("UPDATE control.research_work SET status='CANCELLED',error_code='HOLDING_CLOSED_OR_REVISED',completed_at=clock_timestamp(),lease_until=clock_timestamp()
            WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 AND status IN ('QUEUED','RUNNING') AND work->>'kind'='HOLDING_WATCHDOG'
            AND NOT (work->>'holding_id'=ANY($4))",&[&scope.owner_id,&scope.account_id,&version(scope)?,&active]).map_err(db)?;
        Ok(ids)
    }
    pub fn publish_control_context(&mut self, context: &OwnerControlContext) -> Result<()> {
        require(
            self.role == "trady_executor",
            "only executor may publish reconciled controls context",
        )?;
        context.validate(Utc::now())?;
        self.client.execute("INSERT INTO control.owner_control_context(account_id,owner_id,policy_version,document,context_hash,observed_at) VALUES($1,$2,$3,$4,$5,$6)
            ON CONFLICT(account_id) DO UPDATE SET owner_id=EXCLUDED.owner_id,policy_version=EXCLUDED.policy_version,document=EXCLUDED.document,context_hash=EXCLUDED.context_hash,observed_at=EXCLUDED.observed_at
            WHERE control.owner_control_context.observed_at<=EXCLUDED.observed_at",
            &[&context.scope.account_id,&context.scope.owner_id,&version(&context.scope)?,&encode(context)?,&context.binding_hash()?,&context.observed_at]).map_err(db)?;
        Ok(())
    }
    pub fn pending_owner_controls(
        &mut self,
        account_id: &str,
        limit: usize,
    ) -> Result<Vec<OwnerControlRequest>> {
        require(
            self.role == "trady_executor" && valid_id(account_id) && (1..=100).contains(&limit),
            "invalid executor controls getter",
        )?;
        self.client.query("SELECT payload FROM control.owner_controls WHERE account_id=$1 AND status='REQUESTED' AND producer='trady_gateway' ORDER BY created_at,command_id LIMIT $2",
            &[&account_id,&(limit as i64)]).map_err(db)?.into_iter().map(|r|decode(r.get(0))).collect()
    }
    pub fn record_control_result(
        &mut self,
        command_id: &str,
        status: &str,
        result: &Value,
    ) -> Result<()> {
        require(
            self.role == "trady_executor"
                && valid_id(command_id)
                && matches!(
                    status,
                    "ACCEPTED" | "SUBMITTED" | "PARTIAL" | "COMPLETE" | "REJECTED"
                )
                && serde_json::to_vec(result)
                    .map_err(|_| Error("control result encoding failed".into()))?
                    .len()
                    <= 6000,
            "invalid executor control result",
        )?;
        let n=self.client.execute("UPDATE control.owner_controls SET status=$2,result=$3 WHERE command_id=$1 AND status IN ('REQUESTED','ACCEPTED','SUBMITTED','PARTIAL')",&[&command_id,&status,&result]).map_err(db)?;
        require(n == 1, "owner control is unknown, consumed or terminal")
    }
}

impl ResearchJobRepository for PgResearchJobs {
    fn policy(&mut self, scope: &Scope) -> Result<JobPolicy> {
        read_policy(&mut self.client, scope)
    }
    fn enqueue(&mut self, scope: &Scope, key: &str, work: &Work) -> Result<ResearchJob> {
        enqueue(&mut self.client, scope, key, work)
    }
    fn claim(&mut self, scope: &Scope, worker: &str, ttl: u32) -> Result<Option<JobLease>> {
        require(
            self.role == "trady_analyst" && valid_id(worker) && (1..=300).contains(&ttl),
            "invalid research worker lease",
        )?;
        let mut tx = self.client.transaction().map_err(db)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,7307))",
            &[&format!(
                "{}:{}:{}",
                scope.owner_id, scope.account_id, scope.policy_version
            )],
        )
        .map_err(db)?;
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM control.research_work WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 AND status='RUNNING' AND lease_until>clock_timestamp())",&[&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db)?.get::<_,bool>(0) { return Ok(None); }
        tx.execute("UPDATE control.research_work SET status='FAILED',error_code='RETRY_BUDGET_EXHAUSTED',completed_at=clock_timestamp()
            WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 AND status='RUNNING' AND lease_until<=clock_timestamp() AND attempts>=3",
            &[&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db)?;
        let row=tx.query_opt(&format!("SELECT {JOB_COLUMNS} FROM control.research_work WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3
            AND (status='QUEUED' OR status='RUNNING' AND lease_until<=clock_timestamp()) AND attempts<3 ORDER BY created_at,job_id LIMIT 1 FOR UPDATE SKIP LOCKED"),
            &[&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let mut record = job(row)?;
        let policy = read_policy(&mut tx, scope)?;
        if !policy.enabled
            || record.policy_hash != digest(&policy)?
            || record.work.validate(&policy).is_err()
        {
            tx.execute("UPDATE control.research_work SET status='FAILED',error_code='POLICY_CHANGED_OR_REVOKED',completed_at=clock_timestamp() WHERE job_id=$1",&[&record.id]).map_err(db)?;
            tx.commit().map_err(db)?;
            return Ok(None);
        }
        let row=tx.query_one("UPDATE control.research_work SET status='RUNNING',attempts=attempts+1,lease_worker=$2,lease_generation=lease_generation+1,lease_until=clock_timestamp()+make_interval(secs=>$3::integer) WHERE job_id=$1 RETURNING lease_generation",
            &[&record.id,&worker,&(ttl as i32)]).map_err(db)?;
        record.status = JobStatus::Running;
        record.attempts += 1;
        record.generation = row.get(0);
        let lease = JobLease {
            generation: record.generation,
            job: record,
            worker_id: worker.into(),
        };
        tx.commit().map_err(db)?;
        Ok(Some(lease))
    }
    fn complete(&mut self, lease: &JobLease, result: &Value) -> Result<()> {
        let mut tx = self.client.transaction().map_err(db)?;
        check_lease(&mut tx, lease)?;
        finish(&mut tx, lease, Some(result), None)?;
        tx.commit().map_err(db)
    }
    fn fail(&mut self, lease: &JobLease, code: &str) -> Result<()> {
        require(
            !code.is_empty()
                && code.len() <= 96
                && code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || matches!(b, b'_' | b'0'..=b'9')),
            "invalid job failure code",
        )?;
        let mut tx = self.client.transaction().map_err(db)?;
        // Policy revocation must not strand a failed provider call as RUNNING.
        let held=tx.query_one("SELECT EXISTS(SELECT 1 FROM control.research_work WHERE job_id=$1 AND lease_worker=$2 AND lease_generation=$3 AND status='RUNNING' AND lease_until>clock_timestamp())",
            &[&lease.job.id,&lease.worker_id,&lease.generation]).map_err(db)?.get::<_,bool>(0);
        require(held, "research job lease expired or fenced")?;
        finish(&mut tx, lease, None, Some(code))?;
        tx.commit().map_err(db)
    }
    fn packet(
        &mut self,
        scope: &Scope,
        id: &str,
        now: DateTime<Utc>,
    ) -> Result<(VerifiedResearchPacket, Vec<SourceItem>)> {
        let mut tx = self.client.transaction().map_err(db)?;
        let result = load_packet(&mut tx, scope, id, now)?;
        tx.commit().map_err(db)?;
        Ok(result)
    }
    fn store_sentiment(
        &mut self,
        lease: &JobLease,
        packet: &VerifiedResearchPacket,
        assessment: &SentimentAssessment,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        let mut tx = self.client.transaction().map_err(db)?;
        check_lease(&mut tx, lease)?;
        let (current, items) = load_packet(&mut tx, &lease.job.scope, &packet.id, now)?;
        require(
            digest(&current)? == digest(packet)?,
            "scored packet changed",
        )?;
        let policy = read_policy(&mut tx, &lease.job.scope)?;
        require(
            matches!(&lease.job.work,Work::Sentiment { packet_id,model_revision } if packet_id==&packet.id && model_revision==&assessment.model_revision)
                && policy.sentiment_revision.as_deref() == Some(&assessment.model_revision),
            "sentiment job binding mismatch",
        )?;
        let id = new_id("sentiment");
        let (reference, blocker) =
            match readiness_reference(&id, assessment, &current, &items, &policy, now) {
                Ok(reference) => (Some(encode(&reference)?), None),
                Err(_) => (None, Some("CLAIM_INDEPENDENCE_OR_READINESS_INCOMPLETE")),
            };
        tx.execute("INSERT INTO research.sentiment_assessments(assessment_id,job_id,packet_id,owner_id,account_id,policy_version,model_revision,assessment_hash,document,readiness_reference,readiness_blocker,expires_at)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",&[&id,&lease.job.id,&packet.id,&packet.scope.owner_id,&packet.scope.account_id,&version(&packet.scope)?,
                &assessment.model_revision,&digest(assessment)?,&encode(assessment)?,&reference,&blocker,&packet.expires_at]).map_err(db)?;
        let result = json!({"assessment_id":id,"packet_id":packet.id,"model_revision":assessment.model_revision,"readiness_reference":reference,"readiness_blocker":blocker,"entry_authorized":false});
        finish(&mut tx, lease, Some(&result), None)?;
        tx.commit().map_err(db)?;
        Ok(result)
    }
}

// The domain assessment is deliberately Serialize-only. Reconstruct only typed,
// persisted provider outcomes when serving a trusted information tool.
#[derive(Deserialize)]
struct StoredAssessment {
    schema_version: u32,
    model_id: String,
    model_revision: String,
    scored_at: DateTime<Utc>,
    evidence: Vec<SentimentEvidence>,
}
impl From<StoredAssessment> for SentimentAssessment {
    fn from(s: StoredAssessment) -> Self {
        Self {
            schema_version: s.schema_version,
            model_id: s.model_id,
            model_revision: s.model_revision,
            scored_at: s.scored_at,
            evidence: s.evidence,
        }
    }
}

fn context(
    db_client: &mut impl GenericClient,
    scope: &Scope,
    now: DateTime<Utc>,
) -> Result<OwnerControlContext> {
    let row=db_client.query_opt("SELECT document,context_hash FROM control.owner_control_context WHERE account_id=$1 AND owner_id=$2 AND policy_version=$3",
        &[&scope.account_id,&scope.owner_id,&version(scope)?]).map_err(db)?.ok_or_else(||Error("executor has not published a reconciled control context".into()))?;
    let context: OwnerControlContext = decode(row.get(0))?;
    context.validate(now)?;
    require(
        context.scope == *scope && context.binding_hash()? == row.get::<_, String>(1),
        "executor control scope/hash mismatch",
    )?;
    Ok(context)
}
fn create_control(
    db_client: &mut impl GenericClient,
    request: &GatewayRequest,
    kind: OwnerControlKind,
    now: DateTime<Utc>,
) -> Result<(String, String)> {
    let immediate = kind == OwnerControlKind::PauseEntries;
    let context = if immediate {
        OwnerControlContext {
            scope: request.scope.clone(),
            mode: read_policy(db_client, &request.scope)?.mode,
            plan_revision: None,
            holding_revision: None,
            holding_id: None,
            instrument_id: None,
            quantity: None,
            observed_at: now,
        }
    } else {
        context(db_client, &request.scope, now)?
    };
    require(
        kind != OwnerControlKind::ClosePosition
            || context.instrument_id.is_some()
            || context.plan_revision.is_some(),
        "no reconciled holding or pending entry is available to close",
    )?;
    let id = new_id("control");
    let token = (!immediate).then(|| new_id("confirmation"));
    let payload = OwnerControlRequest {
        id: id.clone(),
        kind: kind.clone(),
        scope: request.scope.clone(),
        bot_id: request.bot_id,
        owner_user_id: request.sender_id,
        chat_id: request.chat_id,
        context_hash: context.binding_hash()?,
        context,
        requested_at: now,
    };
    let hash = token.as_deref().map(content_hash);
    let state = if immediate { "REQUESTED" } else { "PENDING" };
    db_client.execute("INSERT INTO control.owner_controls(command_id,bot_id,update_id,owner_user_id,chat_id,owner_id,account_id,policy_version,kind,payload,context_hash,token_hash,expires_at,status,confirmed_at)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,clock_timestamp()+interval '2 minutes',$13,CASE WHEN $13='REQUESTED' THEN clock_timestamp() ELSE NULL END)",
        &[&id,&request.bot_id,&request.update_id,&request.sender_id,&request.chat_id,&request.scope.owner_id,&request.scope.account_id,&version(&request.scope)?,
            &encode(&kind)?.as_str().unwrap_or_default(),&encode(&payload)?,&payload.context_hash,&hash,&state]).map_err(db)?;
    let cached = if immediate {
        format!(
            "Entry pause REQUESTED: {id}. The executor must acknowledge it; existing protection and monitoring remain active."
        )
    } else {
        format!(
            "Control preview {id}: {:?}, account {}, plan {:?}, holding {:?}, quantity {:?}. Confirmation expires in 2 minutes. Redelivery does not reveal the token; request a new preview if needed.",
            kind,
            request.scope.account_id,
            payload.context.plan_revision,
            payload.context.holding_revision,
            payload.context.quantity
        )
    };
    let display = token.map_or_else(
        || cached.clone(),
        |token| format!("{cached}\nConfirm exactly this request with /confirm {token}"),
    );
    Ok((display, cached))
}
fn confirm_control(
    db_client: &mut impl GenericClient,
    request: &GatewayRequest,
    token: &str,
    cancel: bool,
    now: DateTime<Utc>,
) -> Result<String> {
    let row=db_client.query_opt("SELECT command_id,payload,status,expires_at>clock_timestamp() FROM control.owner_controls WHERE token_hash=$1 AND bot_id=$2 AND owner_user_id=$3 AND chat_id=$4 AND owner_id=$5 AND account_id=$6 AND policy_version=$7 FOR UPDATE",
        &[&content_hash(token),&request.bot_id,&request.sender_id,&request.chat_id,&request.scope.owner_id,&request.scope.account_id,&version(&request.scope)?]).map_err(db)?.ok_or_else(||Error("confirmation not found in owner/chat/account scope".into()))?;
    let id: String = row.get(0);
    let payload: OwnerControlRequest = decode(row.get(1))?;
    let status: String = row.get(2);
    if status != "PENDING" {
        return Ok(format!(
            "Control {id} is already {status}; no second request was created."
        ));
    }
    if !row.get::<_, bool>(3) {
        db_client
            .execute(
                "UPDATE control.owner_controls SET status='CANCELLED' WHERE command_id=$1",
                &[&id],
            )
            .map_err(db)?;
        return Ok(format!(
            "Preview {id} expired and was invalidated; request a new preview."
        ));
    }
    if cancel {
        db_client
            .execute(
                "UPDATE control.owner_controls SET status='CANCELLED' WHERE command_id=$1",
                &[&id],
            )
            .map_err(db)?;
        return Ok(format!("Preview {id} cancelled."));
    }
    let current = context(db_client, &request.scope, now)?;
    if payload.scope != request.scope
        || payload.bot_id != request.bot_id
        || payload.owner_user_id != request.sender_id
        || payload.chat_id != request.chat_id
        || current.binding_hash()? != payload.context_hash
    {
        db_client
            .execute(
                "UPDATE control.owner_controls SET status='CANCELLED' WHERE command_id=$1",
                &[&id],
            )
            .map_err(db)?;
        return Ok(format!(
            "Preview {id} invalidated: plan, policy, holding or quantity changed; request a new preview."
        ));
    }
    db_client.execute("UPDATE control.owner_controls SET status='REQUESTED',confirmed_at=clock_timestamp() WHERE command_id=$1",&[&id]).map_err(db)?;
    Ok(format!(
        "Control {id} REQUESTED. Executor acceptance and broker-confirmed outcomes are reported separately. No broker action occurred in the gateway."
    ))
}

fn report(
    db_client: &mut impl GenericClient,
    scope: &Scope,
    name: &str,
    subject: Option<&str>,
    now: DateTime<Utc>,
) -> Result<String> {
    let policy = read_policy(db_client, scope)?;
    if matches!(name, "watchlist" | "policy") {
        return Ok(format!(
            "Policy version {}\nApproved instruments: {:?}\nResearch sources: {:?}\nDeployment changes require configuration. Research never adds an instrument to the trading universe.",
            scope.policy_version, policy.instrument_ids, policy.source_ids
        ));
    }
    if name == "sources" {
        let ids: Vec<_> = policy.source_ids.iter().cloned().collect();
        let rows=db_client.query("SELECT source_id,updated_at,last_success_at,watermark,retry_at,requests_today,gaps FROM control.research_source_health WHERE source_id=ANY($1) ORDER BY source_id",&[&ids]).map_err(db)?;
        let mut text = String::from(
            "Configured source health (observation timestamps; no fresh coverage is implied):\n",
        );
        for row in rows {
            text.push_str(&format!("{} · updated {} · last success {} · watermark {} · retry {} · requests today {} · gaps {}\n",row.get::<_,String>(0),row.get::<_,DateTime<Utc>>(1),row.get::<_,Option<Value>>(2).unwrap_or(Value::Null),row.get::<_,Option<Value>>(3).unwrap_or(Value::Null),row.get::<_,Option<Value>>(4).unwrap_or(Value::Null),row.get::<_,Option<Value>>(5).unwrap_or(Value::Null),row.get::<_,Option<Value>>(6).unwrap_or(Value::Null)));
        }
        if text.lines().count() == 1 {
            text.push_str("No source observations have been persisted.");
        }
        return Ok(text.chars().take(2800).collect());
    }
    if name == "budget" {
        let row=db_client.query_one("SELECT count(*) FILTER(WHERE created_at>=date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'),count(*) FILTER(WHERE status IN ('QUEUED','RUNNING')) FROM control.research_work WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3",
            &[&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db)?;
        return Ok(format!(
            "Jobs today {}/{}; queued/running {}/{}. Source collectors enforce separate source caps. LLM token use is in /sessions; monetary model costs are unavailable.",
            row.get::<_, i64>(0),
            policy.daily_job_limit,
            row.get::<_, i64>(1),
            policy.pending_job_limit
        ));
    }
    let subject = subject.unwrap_or("");
    if let Some(row)=db_client.query_opt("SELECT document,observed_at,expires_at FROM research.gateway_reports WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 AND name=$4 AND subject=$5",
        &[&scope.owner_id,&scope.account_id,&version(scope)?,&name,&subject]).map_err(db)? {
        let at:DateTime<Utc>=row.get(1); let expiry:DateTime<Utc>=row.get(2); let doc:Value=row.get(0);
        return Ok(format!("{name} · observed {} · {}\n{}",at,if at<=now&&expiry>now{"current"}else{"STALE / not authoritative"},doc).chars().take(2800).collect());
    }
    if matches!(name,"position"|"plan"|"watchdog")
        && let Some(row)=db_client.query_opt("SELECT document FROM control.owner_control_context WHERE account_id=$1 AND owner_id=$2 AND policy_version=$3",&[&scope.account_id,&scope.owner_id,&version(scope)?]).map_err(db)? {
            let c:OwnerControlContext=decode(row.get(0))?;
            return Ok(format!("{name} · executor observation {} · {}\nHolding {:?}, instrument {:?}, actual quantity {:?}. Plan revision {:?}, holding revision {:?}.\nDetailed thresholds, protection state and selector health are unavailable unless separately published. This observation grants no entry or close permission.",c.observed_at,if c.validate(now).is_ok(){"current"}else{"STALE"},c.holding_id,c.instrument_id,c.quantity,c.plan_revision,c.holding_revision));
    }
    if matches!(name, "company" | "relations") && policy.instrument_ids.contains(subject) {
        let row=db_client.query_opt("SELECT document,expires_at FROM research.verified_packets WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 AND instrument_id=$4 ORDER BY created_at DESC LIMIT 1",
            &[&scope.owner_id,&scope.account_id,&version(scope)?,&subject]).map_err(db)?;
        if let Some(row) = row {
            let p: VerifiedResearchPacket = decode(row.get(0))?;
            return Ok(format!(
                "{} · packet {} · profile revision {:?} · expires {}\nSource references: {:?}\nClaims: {}. Detailed company facts/relations have not been published to the gateway; this packet is not an entry permission.",
                subject,
                p.id,
                p.profile_revision_id,
                p.expires_at,
                p.sources.iter().map(|d| &d.item_id).collect::<Vec<_>>(),
                p.claims.len()
            ));
        }
    }
    Ok(format!(
        "{name}: unavailable in this owner/account scope. No current verified report has been published. No performance, ranking, plan or source evidence has been inferred."
    ))
}

#[derive(Debug, Clone)]
pub struct GatewayDelivery {
    pub id: String,
    pub bot_id: i64,
    pub owner_user_id: i64,
    pub chat_id: i64,
    pub scope: Scope,
    pub text: String,
}

impl PgResearchJobs {
    /// Single transaction: replay receipt + queued job/control, before transport ack.
    /// `research_work` is resolved from configured source IDs by the CLI; URLs
    /// are never interpreted as network authority by this repository.
    pub fn accept_gateway(
        &mut self,
        request: &GatewayRequest,
        research_work: Option<Work>,
        monitor_reply: &str,
        now: DateTime<Utc>,
    ) -> Result<String> {
        require(
            self.role == "trady_gateway"
                && request.bot_id > 0
                && request.update_id >= 0
                && request.sender_id == request.chat_id,
            "invalid gateway request",
        )?;
        let mut tx = self.client.transaction().map_err(db)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,7308))",
            &[&format!("{}:{}", request.bot_id, request.update_id)],
        )
        .map_err(db)?;
        let request_hash = digest(&(
            &request.scope,
            request.bot_id,
            request.sender_id,
            request.chat_id,
            &request.command,
        ))?;
        if let Some(row)=tx.query_opt("SELECT request_hash,reply FROM control.gateway_receipts WHERE bot_id=$1 AND update_id=$2",&[&request.bot_id,&request.update_id]).map_err(db)? {
            require(row.get::<_,String>(0)==request_hash,"Telegram update ID replay payload/scope mismatch")?; return Ok(row.get(1));
        }
        let policy = read_policy(&mut tx, &request.scope)?;
        require(
            policy.enabled
                && policy.bot_id == request.bot_id
                && policy.telegram_user_id == request.sender_id,
            "Telegram identity does not match provisioned research owner",
        )?;
        let key = format!("telegram_{}_{}", request.bot_id, request.update_id);
        tx.batch_execute("SAVEPOINT accept_command").map_err(db)?;
        let mut job_id = None;
        let result = (|| -> Result<(String, String)> {
            let queued=match &request.command {
                GatewayCommand::Ask(text)=>{
                    let selected=tx.query_opt("SELECT session_id FROM control.gateway_sessions WHERE bot_id=$1 AND owner_user_id=$2 AND owner_id=$3 AND account_id=$4 AND policy_version=$5",
                        &[&request.bot_id,&request.sender_id,&request.scope.owner_id,&request.scope.account_id,&version(&request.scope)?]).map_err(db)?.map(|r|r.get::<_,String>(0));
                    Some(Work::Dialogue { text:text.clone(),session_id:selected })
                },
                GatewayCommand::Research(_)=>Some(research_work.ok_or_else(||Error("research target unavailable; use an approved instrument or registered source URL".into()))?),
                GatewayCommand::Session { action,subject }=>Some(Work::Session { action:action.clone(),session_id:subject.clone() }),
                _=>None,
            };
            let text = if let Some(work) = queued {
                let job = enqueue(&mut tx, &request.scope, &key, &work)?;
                job_id = Some(job.id.clone());
                format!(
                    "Job {} {:?}. Use /jobs {} for its result. Queued research and dialogue cannot place orders.",
                    job.id, job.status, job.id
                )
            } else {
                match &request.command {
                    GatewayCommand::PauseEntries => {
                        return create_control(
                            &mut tx,
                            request,
                            OwnerControlKind::PauseEntries,
                            now,
                        );
                    }
                    GatewayCommand::ResumeEntries => {
                        return create_control(
                            &mut tx,
                            request,
                            OwnerControlKind::ResumeEntries,
                            now,
                        );
                    }
                    GatewayCommand::ClosePosition => {
                        return create_control(
                            &mut tx,
                            request,
                            OwnerControlKind::ClosePosition,
                            now,
                        );
                    }
                    GatewayCommand::Confirm(token) => {
                        confirm_control(&mut tx, request, token, false, now)?
                    }
                    GatewayCommand::CancelConfirmation(token) => {
                        confirm_control(&mut tx, request, token, true, now)?
                    }
                    GatewayCommand::Monitor(trady_application::telegram::MonitorCommand::Help) => {
                        GATEWAY_HELP.into()
                    }
                    GatewayCommand::Monitor(
                        trady_application::telegram::MonitorCommand::Status,
                    ) => {
                        let mut text = monitor_reply.to_owned();
                        if let Some(row)=tx.query_opt("SELECT mode,recovery_required,latest_observed_at FROM execution.account_summary WHERE account_id=$1",&[&request.scope.account_id]).map_err(db)? {
                        text.push_str(&format!("\nExecutor observation: mode {}, recovery required {}, last synchronized {:?}.",row.get::<_,String>(0),row.get::<_,bool>(1),row.get::<_,Option<DateTime<Utc>>>(2)));
                    }
                        let pending:i64=tx.query_one("SELECT count(*) FROM control.owner_controls WHERE account_id=$1 AND owner_id=$2 AND policy_version=$3 AND status IN ('PENDING','REQUESTED','ACCEPTED','SUBMITTED','PARTIAL')",
                        &[&request.scope.account_id,&request.scope.owner_id,&version(&request.scope)?]).map_err(db)?.get(0);
                        text.push_str(&format!(
                            "\nPending owner controls: {pending}. /jobs shows research progress."
                        ));
                        text
                    }
                    GatewayCommand::Monitor(
                        trady_application::telegram::MonitorCommand::Balance,
                    ) => report(&mut tx, &request.scope, "balance", None, now)?,
                    GatewayCommand::Monitor(
                        trady_application::telegram::MonitorCommand::Position,
                    ) => report(&mut tx, &request.scope, "position", None, now)?,
                    GatewayCommand::Monitor(
                        trady_application::telegram::MonitorCommand::Orders,
                    ) => report(&mut tx, &request.scope, "orders", None, now)?,
                    GatewayCommand::Monitor(_) => monitor_reply.into(),
                    GatewayCommand::Jobs(id) => {
                        if let Some(id) = id.as_ref().filter(|id| id.starts_with("control_")) {
                            match tx.query_opt("SELECT kind,status,result,created_at FROM control.owner_controls WHERE command_id=$1 AND owner_id=$2 AND account_id=$3 AND policy_version=$4 AND bot_id=$5 AND owner_user_id=$6",
                            &[&id,&request.scope.owner_id,&request.scope.account_id,&version(&request.scope)?,&request.bot_id,&request.sender_id]).map_err(db)? {
                            Some(r)=>format!("Owner control {id} · {} · {} · {}\n{}",r.get::<_,String>(0),r.get::<_,String>(1),r.get::<_,DateTime<Utc>>(3),r.get::<_,Option<Value>>(2).unwrap_or(Value::Null)),
                            None=>"Owner control not found in this authenticated scope.".into(),
                        }
                        } else if let Some(id) = id {
                            match read_job(&mut tx, &request.scope, id)? {
                                Some(j) => serde_json::to_string(&j)
                                    .map_err(|_| Error("job report unavailable".into()))?,
                                None => "Job not found in this owner/account scope.".into(),
                            }
                        } else {
                            let rows=tx.query(&format!("SELECT {JOB_COLUMNS} FROM control.research_work WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 ORDER BY created_at DESC LIMIT 10"),&[&request.scope.owner_id,&request.scope.account_id,&version(&request.scope)?]).map_err(db)?;
                            let mut s = String::from("Recent jobs:\n");
                            for row in rows {
                                let j = job(row)?;
                                s.push_str(&format!(
                                    "{} {:?} · {} · {:?}\n",
                                    j.id, j.status, j.created_at, j.error_code
                                ));
                            }
                            s
                        }
                    }
                    GatewayCommand::Report { name, subject } => {
                        report(&mut tx, &request.scope, name, subject.as_deref(), now)?
                    }
                    GatewayCommand::Unsupported(reason) => reason.clone(),
                    _ => "Unsupported gateway request; no action applied.".into(),
                }
            };
            Ok((text.clone(), text))
        })();
        let (display, cached) = match result {
            Ok(v) => v,
            Err(_) => {
                tx.batch_execute("ROLLBACK TO SAVEPOINT accept_command")
                    .map_err(db)?;
                job_id = None;
                let s="Request could not be accepted: unavailable target, scope, job budget, executor context, or confirmation. No command was applied. Check /jobs, /budget and /status; expired previews require a new request.".to_owned();
                (s.clone(), s)
            }
        };
        let display: String = display.chars().take(3500).collect();
        let cached: String = cached.chars().take(3500).collect();
        tx.execute("INSERT INTO control.gateway_receipts(bot_id,update_id,owner_user_id,chat_id,owner_id,account_id,policy_version,request_hash,reply,job_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            &[&request.bot_id,&request.update_id,&request.sender_id,&request.chat_id,&request.scope.owner_id,&request.scope.account_id,&version(&request.scope)?,&request_hash,&cached,&job_id]).map_err(db)?;
        tx.commit().map_err(db)?;
        Ok(display)
    }
    pub fn pending_deliveries(
        &mut self,
        bot_id: i64,
        limit: usize,
    ) -> Result<Vec<GatewayDelivery>> {
        require(
            self.role == "trady_gateway" && bot_id > 0 && (1..=50).contains(&limit),
            "invalid gateway outbox query",
        )?;
        self.client.query("SELECT delivery_id,bot_id,owner_user_id,chat_id,owner_id,account_id,policy_version,text FROM control.gateway_outbox WHERE bot_id=$1 AND delivered_at IS NULL ORDER BY created_at,delivery_id LIMIT $2",
            &[&bot_id,&(limit as i64)]).map_err(db)?.into_iter().map(|r|Ok(GatewayDelivery { id:r.get(0),bot_id:r.get(1),owner_user_id:r.get(2),chat_id:r.get(3),
                scope:Scope { owner_id:r.get(4),account_id:r.get(5),policy_version:r.get::<_,i32>(6) as u32 },text:r.get(7) })).collect()
    }
    pub fn acknowledge_delivery(&mut self, bot_id: i64, id: &str) -> Result<()> {
        require(
            self.role == "trady_gateway",
            "invalid gateway outbox writer",
        )?;
        self.client.execute("UPDATE control.gateway_outbox SET delivered_at=clock_timestamp() WHERE delivery_id=$1 AND bot_id=$2 AND delivered_at IS NULL",&[&id,&bot_id]).map_err(db)?;
        Ok(())
    }
    pub fn selected_session(&mut self, scope: &Scope) -> Result<Option<String>> {
        let policy = self.policy(scope)?;
        self.client.query_opt("SELECT session_id FROM control.gateway_sessions WHERE bot_id=$1 AND owner_user_id=$2 AND owner_id=$3 AND account_id=$4 AND policy_version=$5",
            &[&policy.bot_id,&policy.telegram_user_id,&scope.owner_id,&scope.account_id,&version(scope)?]).map_err(db).map(|r|r.map(|r|r.get(0)))
    }
    pub fn select_session(&mut self, scope: &Scope, id: &str) -> Result<()> {
        require(
            self.role == "trady_analyst" && valid_id(id),
            "only analyst can select a validated owner session",
        )?;
        let policy = self.policy(scope)?;
        self.client.execute("INSERT INTO control.gateway_sessions(bot_id,owner_user_id,owner_id,account_id,policy_version,session_id) VALUES($1,$2,$3,$4,$5,$6)
            ON CONFLICT(bot_id,owner_user_id,owner_id,account_id,policy_version) DO UPDATE SET session_id=EXCLUDED.session_id",
            &[&policy.bot_id,&policy.telegram_user_id,&scope.owner_id,&scope.account_id,&version(scope)?,&id]).map_err(db)?;
        Ok(())
    }
    pub fn publish_report(
        &mut self,
        scope: &Scope,
        name: &str,
        subject: Option<&str>,
        document: &Value,
        observed_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        require(
            matches!(self.role, "trady_executor" | "trady_analyst")
                && valid_id(name)
                && subject.is_none_or(|s| s.len() <= 96)
                && observed_at <= Utc::now()
                && expires_at > observed_at
                && serde_json::to_vec(document)
                    .map_err(|_| Error("report encoding failed".into()))?
                    .len()
                    <= 12000,
            "invalid attributed gateway report",
        )?;
        self.client.execute("INSERT INTO research.gateway_reports(owner_id,account_id,policy_version,name,subject,document,observed_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)
            ON CONFLICT(owner_id,account_id,policy_version,name,subject) DO UPDATE SET document=EXCLUDED.document,observed_at=EXCLUDED.observed_at,expires_at=EXCLUDED.expires_at WHERE research.gateway_reports.observed_at<=EXCLUDED.observed_at",
            &[&scope.owner_id,&scope.account_id,&version(scope)?,&name,&subject.unwrap_or(""),&document,&observed_at,&expires_at]).map_err(db)?;
        Ok(())
    }
}

impl PgResearchJobs {
    /// Unknown/stale executor observations return None; they never imply flat.
    pub fn holding_watches(
        &mut self,
        scope: &Scope,
        now: DateTime<Utc>,
    ) -> Result<Option<Vec<HoldingWatch>>> {
        let row=self.client.query_opt("SELECT document FROM control.owner_control_context WHERE account_id=$1 AND owner_id=$2 AND policy_version=$3",&[&scope.account_id,&scope.owner_id,&version(scope)?]).map_err(db)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let c: OwnerControlContext = decode(row.get(0))?;
        if c.scope != *scope || c.validate(now).is_err() {
            return Ok(None);
        }
        Ok(Some(
            match (c.holding_id, c.instrument_id, c.holding_revision) {
                (Some(holding_id), Some(instrument_id), Some(revision_hash)) => {
                    vec![HoldingWatch {
                        holding_id,
                        instrument_id,
                        revision_hash,
                        active: true,
                    }]
                }
                (None, None, None) => vec![],
                _ => return Err(Error("inconsistent reconciled watchdog scope".into())),
            },
        ))
    }
    /// Persist before model I/O so recovery can resume or reuse the same run.
    pub fn bind_job_session(
        &mut self,
        lease: &JobLease,
        session_id: &str,
        baseline_runs: usize,
    ) -> Result<()> {
        require(valid_id(session_id), "invalid bound dialogue session")?;
        let mut tx = self.client.transaction().map_err(db)?;
        check_lease(&mut tx, lease)?;
        let value = json!({"resume_session_id":session_id,"baseline_runs":baseline_runs});
        tx.execute(
            "UPDATE control.research_work SET result=$2 WHERE job_id=$1 AND result IS NULL",
            &[&lease.job.id, &value],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }
    pub fn cancel_closed_watchdogs(
        &mut self,
        scope: &Scope,
        holdings: &[HoldingWatch],
    ) -> Result<()> {
        require(
            self.role == "trady_analyst" && holdings.len() <= 20,
            "invalid watchdog cancellation scope",
        )?;
        let current: Vec<_> = holdings
            .iter()
            .filter(|h| h.active)
            .map(|h| format!("{}:{}", h.holding_id, h.revision_hash))
            .collect();
        self.client.execute("UPDATE control.research_work SET status='CANCELLED',error_code='HOLDING_CLOSED_OR_REVISED',completed_at=clock_timestamp(),lease_until=clock_timestamp()
            WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 AND status IN ('QUEUED','RUNNING') AND work->>'kind'='HOLDING_WATCHDOG'
            AND NOT ((work->>'holding_id')||':'||(work->>'holding_revision')=ANY($4))",&[&scope.owner_id,&scope.account_id,&version(scope)?,&current]).map_err(db)?;
        Ok(())
    }
    pub fn queue_pending_sentiment(&mut self, scope: &Scope, now: DateTime<Utc>) -> Result<usize> {
        let policy = self.policy(scope)?;
        let Some(revision) = policy.sentiment_revision else {
            return Ok(0);
        };
        let rows=self.client.query("SELECT p.packet_id FROM research.verified_packets p WHERE p.owner_id=$1 AND p.account_id=$2 AND p.policy_version=$3 AND p.expires_at>$4
            AND NOT EXISTS(SELECT 1 FROM research.sentiment_assessments a WHERE a.packet_id=p.packet_id AND a.model_revision=$5)
            AND NOT EXISTS(SELECT 1 FROM research.dependency_invalidations i WHERE i.packet_id=p.packet_id) ORDER BY p.created_at LIMIT 10",
            &[&scope.owner_id,&scope.account_id,&version(scope)?,&now,&revision]).map_err(db)?;
        let mut count = 0;
        for row in rows {
            let id: String = row.get(0);
            let key = format!("sentiment_{}_{}", id, revision);
            if self
                .enqueue(
                    scope,
                    &key,
                    &Work::Sentiment {
                        packet_id: id,
                        model_revision: revision.clone(),
                    },
                )
                .is_ok()
            {
                count += 1;
            }
        }
        Ok(count)
    }
}

fn permitted_item(row: Row, policy: &JobPolicy, now: DateTime<Utc>) -> Result<SourceItem> {
    let mut item: SourceItem = decode(row.get(0))?;
    let state: SourceState = decode(row.get(1))?;
    require(
        policy.enabled && policy.source_ids.contains(&item.source_id),
        "source outside information scope",
    )?;
    let rights = state
        .rights
        .ok_or_else(|| Error("source rights not reviewed".into()))?;
    item.rights.store_content &= rights.store_content;
    item.rights.llm_processing &= rights.llm_processing;
    item.expires_at = item
        .expires_at
        .min(item.first_seen_at + chrono::Duration::hours(i64::from(rights.retention_hours)));
    item.redact(now, true);
    require(
        item.expires_at > now
            && item.rights.llm_processing
            && item.rights.store_content
            && !matches!(
                item.content_status,
                trady_domain::research::ContentStatus::Unavailable
                    | trady_domain::research::ContentStatus::Superseded
            ),
        "source expired or processing rights revoked",
    )?;
    Ok(item)
}
fn item_projection(item: &SourceItem) -> Value {
    let text = item
        .text
        .as_deref()
        .or(item.summary.as_deref())
        .or(item.title.as_deref())
        .unwrap_or_default();
    json!({"item_id":item.item_id,"source_id":item.source_id,"url":item.canonical_url,"lineage_id":item.lineage_id,"publisher":item.publisher,
        "published_at":item.published_at,"event_at":item.event_at,"fetched_at":item.fetched_at,"expires_at":item.expires_at,
        "content_status":item.content_status,"excerpt":text.chars().take(400).collect::<String>(),"gaps":item.gaps.iter().take(4).collect::<Vec<_>>(),
        "verification_status":"UNTRUSTED_SOURCE_OBSERVATION"})
}
impl ResearchInformation for PgResearchJobs {
    fn information(
        &mut self,
        scope: &Scope,
        name: &str,
        args: &Value,
        request_key: &str,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        require(
            self.role == "trady_analyst",
            "information tools require analyst repository",
        )?;
        let policy = self.policy(scope)?;
        require(policy.enabled, "research scope disabled")?;
        let id = |key: &str| -> Result<&str> {
            args.get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| Error("missing information tool ID".into()))
        };
        match name {
            "financial.snapshot"
            | "fundamentals.snapshot"
            | "filings.search"
            | "macro.snapshot"
            | "prices.features" => {
                use trady_application::financial::{DataRequest, FinancialSource, signals};
                use trady_domain::financial::Consumer;
                let source_id = id("source_id")?;
                require(
                    policy.source_ids.contains(source_id),
                    "financial source outside approved information scope",
                )?;
                let row = self
                    .client
                    .query_opt(
                        "SELECT document FROM research.financial_sources WHERE source_id=$1",
                        &[&source_id],
                    )
                    .map_err(db)?
                    .ok_or_else(|| Error("financial source unavailable".into()))?;
                let source: FinancialSource = decode(row.get(0))?;
                let global_context = matches!(
                    source.request,
                    DataRequest::FredObservations { .. } | DataRequest::CoinGeckoMarkets { .. }
                );
                require(
                    global_context || policy.instrument_ids.contains(&source.entity_id),
                    "financial identity outside approved instrument scope",
                )?;
                let suitable = match name {
                    "macro.snapshot" => {
                        matches!(source.request, DataRequest::FredObservations { .. })
                    }
                    "filings.search" => {
                        matches!(source.request, DataRequest::SecSubmissions { .. })
                    }
                    "fundamentals.snapshot" => matches!(
                        source.request,
                        DataRequest::SecConcept { .. } | DataRequest::FmpIncome { .. }
                    ),
                    "prices.features" => matches!(
                        source.request,
                        DataRequest::TwelveDaily { .. }
                            | DataRequest::TiingoDaily { .. }
                            | DataRequest::MassiveDaily { .. }
                            | DataRequest::YfinanceImport
                    ),
                    _ => true,
                };
                require(
                    suitable,
                    "financial endpoint is unsuitable for requested tool",
                )?;
                let limit = args["limit"]
                    .as_u64()
                    .filter(|n| (1..=3).contains(n))
                    .ok_or_else(|| Error("invalid financial tool bound".into()))?
                    as usize;
                let read_limit = if name == "prices.features" {
                    source.max_records
                } else {
                    limit
                };
                let snapshot = crate::financial::read_snapshot(
                    &mut self.client,
                    source_id,
                    now,
                    Consumer::ExternalLlm,
                    read_limit,
                    now,
                )?;
                let data = signals(snapshot)?;
                let observations:Vec<_>=data.snapshot.observations.iter().rev().take(limit).map(|o|json!({"observation_id":o.id,"source_url":o.source_url,"lineage_id":o.lineage_id,
                    "unit":o.unit,"payload":o.payload,"available_at":o.available_at,"received_at":o.received_at,"expires_at":o.expires_at,"gaps":o.gaps})).collect();
                Ok(
                    json!({"snapshot_id":data.snapshot.manifest_hash,"source_id":source_id,"entity_id":data.snapshot.entity_id,"decision_at":now,
                    "status":data.snapshot.status,"observations":observations,"gaps":data.snapshot.gaps,"feature_schema":data.feature_schema,
                    "coverage":data.snapshot.coverage,"dependency_count":data.snapshot.observations.len(),
                    "last_close":data.last_close,"return_fraction":data.return_fraction,"mean_close":data.mean_close,
                    "untrusted_source_data":true,"entry_authorized":false}),
                )
            }
            "news.search" => {
                let source = id("source_id")?;
                require(policy.source_ids.contains(source), "source not approved")?;
                let limit = args["limit"]
                    .as_i64()
                    .filter(|n| (1..=3).contains(n))
                    .ok_or_else(|| Error("invalid item limit".into()))?;
                let rows=self.client.query("SELECT i.document,s.document FROM research.source_items i JOIN research.source_states s USING(source_id) WHERE i.source_id=$1 AND i.expires_at>$2 ORDER BY fetched_at DESC,item_id LIMIT $3",&[&source,&now,&limit]).map_err(db)?;
                let mut items = vec![];
                for row in rows {
                    if let Ok(item) = permitted_item(row, &policy, now) {
                        items.push(item_projection(&item));
                    }
                }
                Ok(
                    json!({"items":items,"coverage":"PERMITTED_STORED_OBSERVATIONS_ONLY","absence_is_not_no_news":true}),
                )
            }
            "news.read_item" => {
                let item_id = id("item_id")?;
                require(hash_id(item_id), "invalid source item ID")?;
                let row=self.client.query_opt("SELECT i.document,s.document FROM research.source_items i JOIN research.source_states s USING(source_id) WHERE i.item_id=$1",&[&item_id]).map_err(db)?.ok_or_else(||Error("source item not found".into()))?;
                Ok(item_projection(&permitted_item(row, &policy, now)?))
            }
            "sources.health" => {
                Ok(json!({"health":report(&mut self.client,scope,"sources",None,now)?}))
            }
            "sources.lookup" => {
                let work = Work::SourceLookup {
                    source_id: id("source_id")?.into(),
                    external_id: id("external_id")?.into(),
                };
                let job = self.enqueue(scope, request_key, &work)?;
                Ok(json!({"job_id":job.id,"status":job.status}))
            }
            "research.request" => {
                let work = Work::Research {
                    instrument_id: args["instrument_id"].as_str().map(String::from),
                };
                let job = self.enqueue(scope, request_key, &work)?;
                Ok(json!({"job_id":job.id,"status":job.status}))
            }
            "research.job_status" => {
                let j = self
                    .get_job(scope, id("job_id")?)?
                    .ok_or_else(|| Error("job unavailable in scope".into()))?;
                Ok(
                    json!({"job_id":j.id,"status":j.status,"created_at":j.created_at,"result":j.result,"error_code":j.error_code}),
                )
            }
            "profiles.get" => {
                let entity = id("entity_id")?;
                require(
                    policy.instrument_ids.contains(entity),
                    "profile instrument outside scope",
                )?;
                let p = self
                    .latest_packets(scope, Some(entity), 1)?
                    .pop()
                    .ok_or_else(|| Error("verified company packet unavailable".into()))?;
                let (p, _) = self.packet(scope, &p.id, now)?;
                Ok(
                    json!({"instrument_id":p.instrument_id,"packet_id":p.id,"profile_revision_id":p.profile_revision_id,
                    "source_item_ids":p.sources.iter().map(|d|&d.item_id).collect::<Vec<_>>(),"expires_at":p.expires_at,"profile_facts":"unavailable unless separately published"}),
                )
            }
            "evidence.compare" | "research.get_artifact" => {
                let key = if name == "evidence.compare" {
                    "packet_id"
                } else {
                    "artifact_id"
                };
                let (packet, items) = self.packet(scope, id(key)?, now)?;
                Ok(
                    json!({"packet_id":packet.id,"packet_hash":digest(&packet)?,"instrument_id":packet.instrument_id,"claims":packet.claims,
                    "clusters":syndication_clusters(&items,&packet.sources),"verifications":packet.sources.iter().map(|d|&d.verification).collect::<Vec<_>>(),
                    "comparison_is_not_additional_evidence":true,"expires_at":packet.expires_at}),
                )
            }
            "sentiment.assess" => {
                let job = self.enqueue(
                    scope,
                    request_key,
                    &Work::Sentiment {
                        packet_id: id("packet_id")?.into(),
                        model_revision: id("model_revision")?.into(),
                    },
                )?;
                Ok(json!({"job_id":job.id,"status":job.status,"assessment_available":false}))
            }
            "sentiment.read" => {
                let id = id("assessment_id")?;
                let row=self.client.query_opt("SELECT document,packet_id,readiness_reference,readiness_blocker,assessment_hash FROM research.sentiment_assessments WHERE assessment_id=$1 AND owner_id=$2 AND account_id=$3 AND policy_version=$4 AND expires_at>$5",
                    &[&id,&scope.owner_id,&scope.account_id,&version(scope)?,&now]).map_err(db)?.ok_or_else(||Error("sentiment assessment unavailable".into()))?;
                let assessment: SentimentAssessment =
                    decode::<StoredAssessment>(row.get(0))?.into();
                let (packet, _) = self.packet(scope, &row.get::<_, String>(1), now)?;
                require(
                    digest(&assessment)? == row.get::<_, String>(4)
                        && policy.sentiment_revision.as_deref() == Some(&assessment.model_revision),
                    "sentiment outcome revision changed",
                )?;
                let reference: Option<Value> = row.get(2);
                let blocker: Option<String> = row.get(3);
                Ok(
                    json!({"assessment_id":id,"packet_id":packet.id,"assessment":encode(&assessment)?,"readiness_reference":reference,"readiness_blocker":blocker}),
                )
            }
            _ => Err(Error("research information tool not registered".into())),
        }
    }
}

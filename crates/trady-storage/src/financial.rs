//! Durable reservations precede I/O. Payload versions and manifests are immutable.

use crate::{connect_local, verify_role};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use postgres::{Client, GenericClient};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use trady_application::{financial::*, research::valid_key};
use trady_domain::{Error, Result, financial::*, require};

pub struct PgFinancialRepository {
    client: Client,
}
fn db(_: postgres::Error) -> Error {
    Error("financial database operation failed".into())
}
fn encode<T: Serialize>(v: &T) -> Result<Value> {
    serde_json::to_value(v).map_err(|_| Error("financial record encoding failed".into()))
}
fn decode<T: DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(|_| Error("invalid financial database record".into()))
}

/// Call only inside a short transaction. A sent/unknown request is never refunded.
pub(crate) fn reserve_credits(
    client: &mut impl GenericClient,
    key: &str,
    limits: &BudgetLimits,
    cost: u32,
    now: DateTime<Utc>,
) -> Result<bool> {
    limits.validate()?;
    require(cost > 0 && key.len() <= 96, "invalid credit reservation")?;
    client
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,7909))",
            &[&key],
        )
        .map_err(db)?;
    if client
        .query_opt(
            "SELECT 1 FROM research.provider_retry_deadlines WHERE budget_key=$1 AND retry_at>$2",
            &[&key, &now],
        )
        .map_err(db)?
        .is_some()
    {
        return Ok(false);
    }
    let minute = now
        .with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .ok_or_else(|| Error("invalid budget minute".into()))?;
    let day = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let month = now
        .date_naive()
        .with_day(1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let windows = [
        ("minute", minute, limits.per_minute),
        ("day", day, limits.per_day),
        ("month", month, limits.per_month),
    ];
    let mut allowed = true;
    for (kind, start, limit) in &windows {
        client.execute("INSERT INTO research.provider_budget_windows(budget_key,window_kind,window_start,credit_limit) VALUES($1,$2,$3,$4)
            ON CONFLICT(budget_key,window_kind,window_start) DO UPDATE SET credit_limit=GREATEST(research.provider_budget_windows.used_credits,LEAST(research.provider_budget_windows.credit_limit,EXCLUDED.credit_limit))",
            &[&key,kind,start,&i64::from(*limit)]).map_err(db)?;
        let row=client.query_one("SELECT used_credits,credit_limit FROM research.provider_budget_windows WHERE budget_key=$1 AND window_kind=$2 AND window_start=$3 FOR UPDATE",&[&key,kind,start]).map_err(db)?;
        allowed &= row
            .get::<_, i64>(0)
            .checked_add(i64::from(cost))
            .is_some_and(|used| used <= row.get::<_, i64>(1).min(i64::from(*limit)));
    }
    if allowed {
        for (kind, start, _) in &windows {
            client.execute("UPDATE research.provider_budget_windows SET used_credits=used_credits+$4 WHERE budget_key=$1 AND window_kind=$2 AND window_start=$3",&[&key,kind,start,&i64::from(cost)]).map_err(db)?;
        }
    }
    Ok(allowed)
}

impl PgFinancialRepository {
    pub fn connect(dsn: &str) -> Result<Self> {
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, "trady_analyst")?;
        Ok(Self { client })
    }
    pub fn health(&mut self) -> Result<Value> {
        let rows=self.client.query("SELECT s.source_id,s.enabled,c.status,c.gaps,c.finished_at,c.retry_at,c.complete
            FROM research.financial_sources s LEFT JOIN LATERAL (SELECT * FROM research.financial_collections c WHERE c.source_id=s.source_id ORDER BY started_at DESC,request_id DESC LIMIT 1) c ON true ORDER BY s.source_id",&[]).map_err(db)?;
        let sources:Vec<_>=rows.into_iter().map(|r|serde_json::json!({"source_id":r.get::<_,String>(0),"enabled":r.get::<_,bool>(1),"status":r.get::<_,Option<String>>(2),
            "gaps":r.get::<_,Option<Value>>(3),"finished_at":r.get::<_,Option<DateTime<Utc>>>(4),"retry_at":r.get::<_,Option<DateTime<Utc>>>(5),"complete":r.get::<_,Option<bool>>(6)})).collect();
        let rows=self.client.query("SELECT b.budget_key,b.window_kind,b.window_start,b.used_credits,b.credit_limit,
            ((b.window_start AT TIME ZONE 'UTC') + CASE b.window_kind WHEN 'minute' THEN interval '1 minute' WHEN 'day' THEN interval '1 day' ELSE interval '1 month' END) AT TIME ZONE 'UTC',r.retry_at
            FROM research.provider_budget_windows b LEFT JOIN research.provider_retry_deadlines r USING(budget_key)
            WHERE b.window_start=date_trunc(b.window_kind,clock_timestamp(),'UTC') ORDER BY b.budget_key,b.window_kind",&[]).map_err(db)?;
        let budgets:Vec<_>=rows.into_iter().map(|r|serde_json::json!({"budget_key":r.get::<_,String>(0),"window":r.get::<_,String>(1),"window_start":r.get::<_,DateTime<Utc>>(2),
            "used_credits":r.get::<_,i64>(3),"remaining_credits":(r.get::<_,i64>(4)-r.get::<_,i64>(3)).max(0),"credit_limit":r.get::<_,i64>(4),
            "reset_at":r.get::<_,DateTime<Utc>>(5),"retry_at":r.get::<_,Option<DateTime<Utc>>>(6)})).collect();
        Ok(serde_json::json!({"sources":sources,"budgets":budgets,"entry_authorized":false}))
    }
    pub fn purge(&mut self, now: DateTime<Utc>) -> Result<()> {
        let mut tx = self.client.transaction().map_err(db)?;
        tx.execute("DELETE FROM research.financial_manifests m WHERE expires_at<=$1 OR EXISTS (SELECT 1 FROM research.financial_sources s WHERE s.source_id=m.source_id AND (NOT s.enabled OR NOT COALESCE((s.document->'rights'->>'normalized_storage')::boolean,false)))",&[&now]).map_err(db)?;
        tx.execute("DELETE FROM research.financial_observations o USING research.financial_sources s WHERE o.source_id=s.source_id AND (o.expires_at<=$1 OR NOT s.enabled OR NOT COALESCE((s.document->'rights'->>'normalized_storage')::boolean,false)
            OR o.received_at+((s.document->'rights'->>'retention_hours')::integer)*interval '1 hour'<=$1)",&[&now]).map_err(db)?;
        tx.execute("DELETE FROM research.financial_manifests m WHERE EXISTS (SELECT 1 FROM jsonb_array_elements_text(m.observation_ids) ids(id) LEFT JOIN research.financial_observations o ON o.observation_id=ids.id WHERE o.observation_id IS NULL)",&[]).map_err(db)?;
        tx.commit().map_err(db)
    }
}

impl FinancialRepository for PgFinancialRepository {
    fn sync_registry(&mut self, config: &FinancialConfig) -> Result<()> {
        config.validate()?;
        let mut tx = self.client.transaction().map_err(db)?;
        tx.query_one("SELECT pg_advisory_xact_lock(79090001)", &[])
            .map_err(db)?;
        tx.execute("UPDATE research.financial_sources SET enabled=false", &[])
            .map_err(db)?;
        tx.execute("UPDATE research.financial_accounts SET document=jsonb_set(document,'{enabled}','false')",&[]).map_err(db)?;
        for account in &config.accounts {
            tx.execute("INSERT INTO research.financial_accounts(budget_key,document) VALUES($1,$2) ON CONFLICT(budget_key) DO UPDATE SET document=EXCLUDED.document",&[&account.provider.budget_key(),&encode(account)?]).map_err(db)?;
        }
        for source in &config.sources {
            tx.execute("INSERT INTO research.financial_sources(source_id,registry_hash,document,enabled) VALUES($1,$2,$3,$4)
            ON CONFLICT(source_id) DO UPDATE SET registry_hash=EXCLUDED.registry_hash,document=EXCLUDED.document,enabled=EXCLUDED.enabled,updated_at=clock_timestamp()",
            &[&source.id,&source.registry_hash()?,&encode(source)?,&source.enabled]).map_err(db)?;
            tx.execute("INSERT INTO research.financial_registry_revisions(registry_hash,source_id,document) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
                &[&source.registry_hash()?,&source.id,&encode(source)?]).map_err(db)?;
        }
        tx.commit().map_err(db)?;
        self.purge(Utc::now())
    }
    fn reserve(
        &mut self,
        source: &FinancialSource,
        account: &ProviderAccount,
        window: &DataWindow,
        now: DateTime<Utc>,
    ) -> Result<Option<CollectionLease>> {
        source.validate()?;
        account.limits.validate()?;
        window.validate(now)?;
        require(
            source.usable(now) && account.enabled && account.provider == source.request.provider(),
            "financial collection not approved",
        )?;
        let hash = source.registry_hash()?;
        let mut tx = self.client.transaction().map_err(db)?;
        let row=tx.query_opt("SELECT registry_hash,enabled FROM research.financial_sources WHERE source_id=$1 FOR UPDATE",&[&source.id]).map_err(db)?.ok_or_else(||Error("financial source not synchronized".into()))?;
        require(
            row.get::<_, String>(0) == hash && row.get::<_, bool>(1),
            "financial registry changed",
        )?;
        let stored: ProviderAccount = decode(
            tx.query_one(
                "SELECT document FROM research.financial_accounts WHERE budget_key=$1",
                &[&account.provider.budget_key()],
            )
            .map_err(db)?
            .get(0),
        )?;
        require(stored == *account, "financial account changed")?;
        if let Some(row)=tx.query_opt("SELECT lease_until,retry_at,started_at,finished_at,registry_hash,status FROM research.financial_collections WHERE source_id=$1 ORDER BY started_at DESC,request_id DESC LIMIT 1",&[&source.id]).map_err(db)? {
            let same=row.get::<_,String>(4)==hash;
            if row.get::<_,Option<DateTime<Utc>>>(3).is_none()&&row.get::<_,DateTime<Utc>>(0)>now
                || row.get::<_,Option<DateTime<Utc>>>(1).is_some_and(|t|t>now)
                || same&&row.get::<_,String>(5)=="DENIED"
                || same&&now-row.get::<_,DateTime<Utc>>(2)<Duration::seconds(i64::from(source.interval_seconds)) {return Ok(None);}
        }
        if !reserve_credits(
            &mut tx,
            &account.provider.budget_key(),
            &account.limits,
            source.capability.credit_cost,
            now,
        )? {
            tx.commit().map_err(db)?;
            return Ok(None);
        }
        let lease = CollectionLease {
            request_id: trady_application::harness::new_id("financial_request"),
            source_id: source.id.clone(),
            registry_hash: hash,
            window: window.clone(),
        };
        tx.execute("INSERT INTO research.financial_collections(request_id,source_id,registry_hash,window_document,status,started_at,lease_until,reserved_credits) VALUES($1,$2,$3,$4,'PARTIAL',$5,$6,$7)",
            &[&lease.request_id,&lease.source_id,&lease.registry_hash,&encode(window)?,&now,&(now+Duration::seconds(90)),&(source.capability.credit_cost as i32)]).map_err(db)?;
        tx.commit().map_err(db)?;
        Ok(Some(lease))
    }
    fn finish(
        &mut self,
        lease: &CollectionLease,
        page: &FinancialPage,
        now: DateTime<Utc>,
    ) -> Result<usize> {
        let mut tx = self.client.transaction().map_err(db)?;
        let source = check_lease(&mut tx, lease, now)?;
        require(
            page.observations.len() <= source.max_records,
            "financial page exceeds bounds",
        )?;
        let mut count = 0;
        let mut available_ids = Vec::<String>::new();
        for o in &page.observations {
            o.validate()?;
            require(
                o.source_id == source.id
                    && o.entity_id == source.entity_id
                    && o.registry_hash == lease.registry_hash
                    && o.provider == source.request.provider()
                    && o.received_at == now
                    && source.accepts(o, &lease.window),
                "financial observation scope mismatch",
            )?;
            let expected_hash = trady_application::research_jobs::digest(&(&o.payload, &o.gaps))?;
            require(
                o.content_hash == expected_hash
                    && o.id
                        == trady_application::research_jobs::digest(&(
                            &o.source_id,
                            &o.external_key,
                            &o.content_hash,
                            o.received_at,
                        ))?,
                "financial observation digest mismatch",
            )?;
            let old=tx.query_opt("SELECT observation_id,content_hash FROM research.financial_observations WHERE source_id=$1 AND external_key=$2 AND registry_hash=$3 ORDER BY received_at DESC,available_at DESC,observation_id DESC LIMIT 1",&[&o.source_id,&o.external_key,&o.registry_hash]).map_err(db)?;
            if let Some(old) = old
                .as_ref()
                .filter(|r| r.get::<_, String>(1) == o.content_hash)
            {
                available_ids.push(old.get(0));
                continue;
            }
            let inserted=tx.execute("INSERT INTO research.financial_observations(observation_id,source_id,external_key,content_hash,registry_hash,document,available_at,received_at,expires_at)
                VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                &[&o.id,&o.source_id,&o.external_key,&o.content_hash,&o.registry_hash,&encode(o)?,&o.available_at,&o.received_at,&o.expires_at]).map_err(db)? as usize;
            count += inserted;
            available_ids.push(o.id.clone());
            if let Some(old) = old {
                tx.execute("INSERT INTO research.financial_invalidations(observation_id,replacement_id,recorded_at) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&old.get::<_,String>(0),&o.id,&now]).map_err(db)?;
            }
        }
        tx.execute("UPDATE research.financial_collections SET status=$2,gaps=$3,finished_at=$4,complete=$5 WHERE request_id=$1",
            &[&lease.request_id,&if page.complete&&!page.observations.is_empty()&&page.gaps.is_empty(){"OK"}else{"PARTIAL"},&encode(&page.gaps)?,&now,&(page.complete&&!page.observations.is_empty()&&page.gaps.is_empty())]).map_err(db)?;
        tx.commit().map_err(db)?;
        // Unknown availability after a crash stays excluded until a successful
        // re-observation confirms the same content. This runs after payload commit.
        self.client.execute("UPDATE research.financial_observations SET recorded_at=clock_timestamp() WHERE observation_id=ANY($1) AND recorded_at IS NULL",&[&available_ids]).map_err(db)?;
        Ok(count)
    }
    fn fail(
        &mut self,
        lease: &CollectionLease,
        failure: &FinancialFailure,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let mut tx = self.client.transaction().map_err(db)?;
        let source = check_lease(&mut tx, lease, now)?;
        tx.execute("UPDATE research.financial_collections SET status=$2,gaps=$3,finished_at=$4,retry_at=$5 WHERE request_id=$1",
            &[&lease.request_id,&format!("{:?}",failure.status).to_uppercase(),&encode(&vec![failure.code])?,&now,&failure.retry_at]).map_err(db)?;
        if let Some(retry) = failure.retry_at {
            tx.execute("INSERT INTO research.provider_retry_deadlines(budget_key,retry_at) VALUES($1,$2) ON CONFLICT(budget_key) DO UPDATE SET retry_at=GREATEST(research.provider_retry_deadlines.retry_at,EXCLUDED.retry_at)",&[&source.request.provider().budget_key(),&retry]).map_err(db)?;
        }
        tx.commit().map_err(db)
    }
    fn read(
        &mut self,
        source_id: &str,
        cutoff: DateTime<Utc>,
        consumer: Consumer,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<FinancialSnapshot> {
        self.purge(now)?;
        read_snapshot(&mut self.client, source_id, cutoff, consumer, limit, now)
    }
}

fn check_lease(
    client: &mut impl GenericClient,
    lease: &CollectionLease,
    now: DateTime<Utc>,
) -> Result<FinancialSource> {
    let row=client.query_opt("SELECT s.document,c.window_document,c.started_at FROM research.financial_collections c JOIN research.financial_sources s USING(source_id)
        WHERE c.request_id=$1 AND c.source_id=$2 AND c.registry_hash=$3 AND s.registry_hash=$3 AND s.enabled AND c.finished_at IS NULL AND c.lease_until>$4 FOR UPDATE OF c,s",
        &[&lease.request_id,&lease.source_id,&lease.registry_hash,&now]).map_err(db)?.ok_or_else(||Error("financial lease lost, expired or registry changed".into()))?;
    let source: FinancialSource = decode(row.get(0))?;
    let account: ProviderAccount = decode(
        client
            .query_one(
                "SELECT document FROM research.financial_accounts WHERE budget_key=$1 FOR SHARE",
                &[&source.request.provider().budget_key()],
            )
            .map_err(db)?
            .get(0),
    )?;
    require(
        source.usable(now)
            && account.enabled
            && decode::<DataWindow>(row.get(1))? == lease.window
            && now >= row.get::<_, DateTime<Utc>>(2),
        "financial rights, account or collection window changed",
    )?;
    Ok(source)
}

pub(crate) fn read_snapshot(
    client: &mut Client,
    source_id: &str,
    cutoff: DateTime<Utc>,
    consumer: Consumer,
    limit: usize,
    now: DateTime<Utc>,
) -> Result<FinancialSnapshot> {
    let mut tx = client.transaction().map_err(db)?;
    let result = read_snapshot_inner(&mut tx, source_id, cutoff, consumer, limit, now)?;
    let ids: Vec<_> = result.observations.iter().map(|o| &o.id).collect();
    if let Some(expiry) = result.observations.iter().map(|o| o.expires_at).min() {
        let metadata = serde_json::json!({"schema_version":result.schema_version,"entity_id":result.entity_id,"consumer":result.consumer,"status":result.status,"coverage":result.coverage,"gaps":result.gaps});
        tx.execute("INSERT INTO research.financial_manifests(manifest_hash,source_id,decision_at,metadata,observation_ids,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
            &[&result.manifest_hash,&source_id,&cutoff,&metadata,&encode(&ids)?,&now,&expiry]).map_err(db)?;
    }
    tx.commit().map_err(db)?;
    Ok(result)
}
fn read_snapshot_inner(
    client: &mut impl GenericClient,
    source_id: &str,
    cutoff: DateTime<Utc>,
    consumer: Consumer,
    limit: usize,
    now: DateTime<Utc>,
) -> Result<FinancialSnapshot> {
    require(
        valid_key(source_id) && cutoff <= now && (1..=500).contains(&limit),
        "invalid financial snapshot bounds",
    )?;
    let row = client
        .query_opt(
            "SELECT document,enabled FROM research.financial_sources WHERE source_id=$1 FOR SHARE",
            &[&source_id],
        )
        .map_err(db)?
        .ok_or_else(|| Error("financial source unavailable".into()))?;
    let mut source: FinancialSource = decode(row.get(0))?;
    source.enabled &= row.get::<_, bool>(1);
    let account = client
        .query_opt(
            "SELECT document FROM research.financial_accounts WHERE budget_key=$1 FOR SHARE",
            &[&source.request.provider().budget_key()],
        )
        .map_err(db)?
        .map(|r| decode::<ProviderAccount>(r.get(0)))
        .transpose()?;
    source.enabled &= account.is_some_and(|a| a.enabled);
    // Latest version eligible at the cutoff is chosen before consumer filtering.
    let rows=client.query("SELECT document FROM (SELECT DISTINCT ON (external_key) document,received_at,external_key FROM research.financial_observations
        WHERE source_id=$1 AND registry_hash=$2 AND received_at<=$3 AND available_at<=$3 AND recorded_at<=$3 ORDER BY external_key,received_at DESC,available_at DESC,observation_id DESC) versions
        ORDER BY received_at DESC,external_key LIMIT $4",&[&source_id,&source.registry_hash()?,&cutoff,&((limit+1) as i64)]).map_err(db)?;
    let capped = rows.len() > limit;
    let mut observations = Vec::new();
    for row in rows.into_iter().take(limit) {
        let mut o: FinancialObservation = decode(row.get(0))?;
        o.expires_at = o
            .expires_at
            .min(o.received_at + Duration::hours(i64::from(source.rights.retention_hours)));
        observations.push(o);
    }
    let coverage=client.query_opt("SELECT complete,window_document FROM research.financial_collections WHERE source_id=$1 AND registry_hash=$2 AND finished_at<=$3 ORDER BY started_at DESC,request_id DESC LIMIT 1",&[&source_id,&source.registry_hash()?,&cutoff]).map_err(db)?;
    let coverage = coverage
        .map(|r| {
            let window: DataWindow = decode(r.get(1))?;
            Ok::<_, Error>(FinancialCoverage {
                start: window.start,
                end: window.end,
                complete: r.get(0),
            })
        })
        .transpose()?;
    if let Some(window) = &coverage {
        observations.retain(|o| match &o.payload {
            FinancialPayload::DailyBar { session, .. } => {
                *session >= window.start && *session <= window.end
            }
            FinancialPayload::Metric { period_end, .. } => {
                *period_end >= window.start && *period_end <= window.end
            }
            FinancialPayload::Filing { filed_on, .. } => {
                *filed_on >= window.start && *filed_on <= window.end
            }
            FinancialPayload::MarketContext { .. } => true,
        });
    }
    let mut result = snapshot(
        &source,
        observations,
        cutoff,
        consumer,
        coverage.as_ref().is_some_and(|c| c.complete) && !capped,
        now,
    )?;
    result.coverage = coverage;
    result.manifest_hash =
        trady_application::research_jobs::digest(&(&result.manifest_hash, &result.coverage))?;
    Ok(result)
}

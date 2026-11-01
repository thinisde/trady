//! PostgreSQL persistence through fenced, narrowly granted database functions.

pub mod autonomous;
pub mod financial;
pub mod intelligence;
pub mod research;
pub mod research_jobs;
pub mod sessions;
pub mod trading_research;
pub mod transport;
pub mod weekly;
pub mod x_monitoring;

use postgres::Client;
use serde_json::Value;
use trady_application::ExecutionRepository;
use trady_domain::*;

pub const ROLES_SQL: &str = include_str!("../../../migrations/0000_roles.sql");
pub const FOUNDATION_SQL: &str = include_str!("../../../migrations/0001_foundation.sql");
pub const SESSIONS_SQL: &str = include_str!("../../../migrations/0002_model_sessions.sql");
pub const RESEARCH_SOURCES_SQL: &str =
    include_str!("../../../migrations/0003_research_sources.sql");
pub const AUTONOMOUS_SQL: &str = include_str!("../../../migrations/0004_autonomous_execution.sql");
pub const X_API_MONITORING_SQL: &str =
    include_str!("../../../migrations/0005_x_api_monitoring.sql");
pub const VERIFIED_HOLDINGS_SQL: &str =
    include_str!("../../../migrations/0008_verified_holdings.sql");
pub const WEEKLY_LAB_SQL: &str = include_str!("../../../migrations/0006_weekly_lab.sql");
pub const RESEARCH_JOBS_SQL: &str = include_str!("../../../migrations/0007_research_jobs.sql");
pub const FINANCIAL_DATA_SQL: &str = include_str!("../../../migrations/0009_financial_data.sql");
pub const OPTIONAL_INTELLIGENCE_SQL: &str =
    include_str!("../../../migrations/0010_optional_intelligence.sql");
pub const FAIL_CLOSED_STORAGE_SQL: &str =
    include_str!("../../../migrations/0011_fail_closed_storage.sql");

fn db(error: postgres::Error) -> Error {
    // Deliberately exclude connection strings and provider error detail from service output.
    Error(match error.as_db_error() {
        Some(e) => format!("PostgreSQL {}: {}", e.code().code(), e.message()),
        None => "PostgreSQL connection or transport failed".into(),
    })
}

/// All repositories share verified remote TLS and explicit local development transport.
fn connect_local(dsn: &str) -> Result<Client> {
    transport::connect(dsn)
}

pub fn migrate(dsn: &str) -> Result<()> {
    let mut client = connect_local(dsn)?;
    let row = client
        .query_one("SELECT session_user,current_database()", &[])
        .map_err(db)?;
    require(
        row.get::<_, String>(0) == "trady_migrator" && row.get::<_, String>(1) == "trady",
        "migrations require trady_migrator on database trady",
    )?;
    let mut tx = client.transaction().map_err(db)?;
    tx.batch_execute("SET LOCAL ROLE trady_owner; SELECT pg_advisory_xact_lock(861204733); CREATE TABLE IF NOT EXISTS public.trady_migrations(version integer PRIMARY KEY, source text NOT NULL, applied_at timestamptz NOT NULL DEFAULT clock_timestamp()); REVOKE ALL ON public.trady_migrations FROM PUBLIC;").map_err(db)?;
    for (version, source) in [
        (1, FOUNDATION_SQL),
        (2, SESSIONS_SQL),
        (3, RESEARCH_SOURCES_SQL),
        (4, AUTONOMOUS_SQL),
        (5, X_API_MONITORING_SQL),
        (6, WEEKLY_LAB_SQL),
        (7, RESEARCH_JOBS_SQL),
        (8, VERIFIED_HOLDINGS_SQL),
        (9, FINANCIAL_DATA_SQL),
        (10, OPTIONAL_INTELLIGENCE_SQL),
        (11, FAIL_CLOSED_STORAGE_SQL),
    ] {
        if let Some(row) = tx
            .query_opt(
                "SELECT source FROM public.trady_migrations WHERE version=$1",
                &[&version],
            )
            .map_err(db)?
        {
            require(
                row.get::<_, String>(0) == source,
                "applied migration differs from compiled source; add a new migration",
            )?;
        } else {
            tx.batch_execute(source).map_err(db)?;
            tx.execute(
                "INSERT INTO public.trady_migrations(version,source) VALUES($1,$2)",
                &[&version, &source],
            )
            .map_err(db)?;
        }
    }
    tx.commit().map_err(db)
}

pub struct PgExecutionRepository {
    client: Client,
}
impl PgExecutionRepository {
    pub fn connect(dsn: &str) -> Result<Self> {
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, "trady_executor")?;
        Ok(Self { client })
    }
    pub fn register_paper_account(
        &mut self,
        account: &AccountId,
        currency: &Currency,
    ) -> Result<()> {
        self.client
            .query_one(
                "SELECT execution.register_paper_account($1,$2)",
                &[&account.as_str(), &currency.as_str()],
            )
            .map_err(db)?;
        Ok(())
    }
}
fn verify_role(client: &mut Client, expected: &str) -> Result<()> {
    let row=client.query_one("SELECT session_user,current_user,current_database(),r.rolsuper,r.rolcreatedb,r.rolcreaterole,pg_has_role(session_user,'trady_owner','MEMBER') FROM pg_roles r WHERE rolname=session_user",&[]).map_err(db)?;
    require(
        row.get::<_, String>(0) == expected
            && row.get::<_, String>(1) == expected
            && row.get::<_, String>(2) == "trady"
            && !row.get::<_, bool>(3)
            && !row.get::<_, bool>(4)
            && !row.get::<_, bool>(5)
            && !row.get::<_, bool>(6),
        "incorrect or elevated runtime database role",
    )
}
pub fn check_service_database(dsn: &str, role: &str) -> Result<()> {
    require(
        matches!(role, "trady_analyst" | "trady_executor" | "trady_gateway"),
        "unknown runtime role",
    )?;
    let mut client = connect_local(dsn)?;
    verify_role(&mut client, role)
}
impl ExecutionRepository for PgExecutionRepository {
    fn acquire(
        &mut self,
        account: &AccountId,
        worker: &WorkerId,
        ttl_seconds: i32,
    ) -> Result<Fence> {
        let row = self
            .client
            .query_one(
                "SELECT execution.acquire_lease($1,$2,$3)",
                &[&account.as_str(), &worker.as_str(), &ttl_seconds],
            )
            .map_err(db)?;
        Ok(Fence {
            account_id: account.clone(),
            worker_id: worker.clone(),
            generation: row.get(0),
        })
    }
    fn renew(&mut self, f: &Fence, ttl_seconds: i32) -> Result<()> {
        self.client
            .query_one(
                "SELECT execution.renew_lease($1,$2,$3,$4)",
                &[
                    &f.account_id.as_str(),
                    &f.worker_id.as_str(),
                    &f.generation,
                    &ttl_seconds,
                ],
            )
            .map_err(db)?;
        Ok(())
    }
    fn unresolved_intents(&mut self, account: &AccountId) -> Result<Vec<OrderIntent>> {
        self.client.query("SELECT document FROM execution.order_intents WHERE account_id=$1 AND state<>'RESOLVED' ORDER BY intent_id",&[&account.as_str()]).map_err(db)?.into_iter().map(|row|serde_json::from_value(row.get(0)).map_err(|_|Error("invalid stored intent".into()))).collect()
    }
    fn save_reconciliation(&mut self, f: &Fence, r: &Reconciliation) -> Result<()> {
        r.snapshot.validate()?;
        let document =
            serde_json::to_value(r).map_err(|_| Error("cannot encode reconciliation".into()))?;
        self.client
            .query_one(
                "SELECT execution.save_reconciliation($1,$2,$3,$4)",
                &[
                    &f.account_id.as_str(),
                    &f.worker_id.as_str(),
                    &f.generation,
                    &document,
                ],
            )
            .map_err(db)?;
        Ok(())
    }
    fn latest_reconciliation(&mut self, account: &AccountId) -> Result<Option<Reconciliation>> {
        let row = self
            .client
            .query_opt(
                "SELECT reconciliation FROM execution.accounts WHERE account_id=$1",
                &[&account.as_str()],
            )
            .map_err(db)?;
        match row.and_then(|r| r.get::<_, Option<Value>>(0)) {
            Some(v) => serde_json::from_value(v)
                .map(Some)
                .map_err(|_| Error("invalid stored reconciliation".into())),
            None => Ok(None),
        }
    }
    fn reserve_paper_intent(&mut self, f: &Fence, intent: &OrderIntent) -> Result<bool> {
        intent.validate()?;
        let document =
            serde_json::to_value(intent).map_err(|_| Error("cannot encode intent".into()))?;
        let row = self
            .client
            .query_one(
                "SELECT execution.reserve_paper_intent($1,$2,$3,$4)",
                &[
                    &f.account_id.as_str(),
                    &f.worker_id.as_str(),
                    &f.generation,
                    &document,
                ],
            )
            .map_err(db)?;
        Ok(row.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_hostaddr_cannot_override_local_transport_restriction() {
        let result = connect_local(
            "host=127.0.0.1 hostaddr=203.0.113.10 user=trady_executor dbname=trady password=fake_secret",
        );
        let message = result.err().unwrap().to_string();
        assert!(message.contains("hostaddr"));
        assert!(!message.contains("fake_secret"));
    }
}

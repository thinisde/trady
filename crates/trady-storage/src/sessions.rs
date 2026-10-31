//! Analyst-only durable sessions with server-time leases and optimistic revision checks.

use crate::{connect_local, verify_role};
use postgres::Client;
use trady_application::harness::*;
use trady_domain::{Error, Result, require};

pub struct PgSessionRepository {
    client: Client,
}
fn session_db(_: postgres::Error) -> Error {
    Error("session database operation failed".into())
}
fn decode(value: serde_json::Value, scope: &Scope) -> Result<Session> {
    let session: Session = serde_json::from_value(value)
        .map_err(|_| Error("unsupported or corrupt stored session".into()))?;
    require(
        session.schema_version == 1 && &session.scope == scope,
        "stored session scope/version mismatch",
    )?;
    Ok(session)
}
impl PgSessionRepository {
    pub fn connect(dsn: &str) -> Result<Self> {
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, "trady_analyst")?;
        client
            .batch_execute("SET statement_timeout='5s'; SET lock_timeout='5s';")
            .map_err(session_db)?;
        Ok(Self { client })
    }
}
impl SessionRepository for PgSessionRepository {
    fn create(&mut self, session: &Session) -> Result<()> {
        session.scope.validate()?;
        session.limits.validate()?;
        require(
            session.revision == 0 && session.schema_version == 1,
            "new session must start at revision zero",
        )?;
        let policy = i32::try_from(session.scope.policy_version)
            .map_err(|_| Error("policy version too large".into()))?;
        let value =
            serde_json::to_value(session).map_err(|_| Error("session encoding failed".into()))?;
        self.client.execute("INSERT INTO research.model_sessions(session_id,owner_id,account_id,policy_version,document) VALUES($1,$2,$3,$4,$5)",&[&session.id,&session.scope.owner_id,&session.scope.account_id,&policy,&value]).map_err(session_db)?;
        Ok(())
    }
    fn load(&mut self, scope: &Scope, id: &str) -> Result<Session> {
        let policy = i32::try_from(scope.policy_version)
            .map_err(|_| Error("policy version too large".into()))?;
        let row=self.client.query_opt("SELECT document FROM research.model_sessions WHERE session_id=$1 AND owner_id=$2 AND account_id=$3 AND policy_version=$4",&[&id,&scope.owner_id,&scope.account_id,&policy]).map_err(session_db)?.ok_or_else(||Error("session not found in owner/account/policy scope".into()))?;
        decode(row.get(0), scope)
    }
    fn list(&mut self, scope: &Scope) -> Result<Vec<Session>> {
        let policy = i32::try_from(scope.policy_version)
            .map_err(|_| Error("policy version too large".into()))?;
        self.client.query("SELECT document FROM research.model_sessions WHERE owner_id=$1 AND account_id=$2 AND policy_version=$3 ORDER BY updated_at DESC LIMIT 100",&[&scope.owner_id,&scope.account_id,&policy]).map_err(session_db)?.into_iter().map(|r|decode(r.get(0),scope)).collect()
    }
    fn acquire(
        &mut self,
        scope: &Scope,
        id: &str,
        worker: &str,
        ttl_seconds: u32,
    ) -> Result<SessionLease> {
        scope.validate()?;
        require(
            valid_id(id) && valid_id(worker) && (1..=300).contains(&ttl_seconds),
            "invalid session lease",
        )?;
        let policy = i32::try_from(scope.policy_version)
            .map_err(|_| Error("policy version too large".into()))?;
        let ttl = ttl_seconds as i32;
        let row=self.client.query_opt("UPDATE research.model_sessions SET lease_worker=$5,lease_generation=lease_generation+1,lease_until=clock_timestamp()+make_interval(secs=>$6::integer) WHERE session_id=$1 AND owner_id=$2 AND account_id=$3 AND policy_version=$4 AND lease_until<=clock_timestamp() RETURNING document,lease_generation",&[&id,&scope.owner_id,&scope.account_id,&policy,&worker,&ttl]).map_err(session_db)?.ok_or_else(||Error("session is busy or not found in owner/account/policy scope".into()))?;
        Ok(SessionLease {
            session: decode(row.get(0), scope)?,
            worker: worker.into(),
            generation: row.get(1),
            ttl_seconds,
        })
    }
    fn save(&mut self, lease: &mut SessionLease) -> Result<()> {
        let mut next = lease.session.clone();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or_else(|| Error("session revision overflow".into()))?;
        let value =
            serde_json::to_value(&next).map_err(|_| Error("session encoding failed".into()))?;
        let revision = i64::try_from(lease.session.revision)
            .map_err(|_| Error("session revision overflow".into()))?;
        require(
            (1..=300).contains(&lease.ttl_seconds),
            "invalid session lease duration",
        )?;
        let ttl = lease.ttl_seconds as i32;
        let count=self.client.execute("UPDATE research.model_sessions SET document=$1,revision=revision+1,updated_at=clock_timestamp(),lease_until=clock_timestamp()+make_interval(secs=>$8::integer) WHERE session_id=$2 AND owner_id=$3 AND account_id=$4 AND revision=$5 AND lease_worker=$6 AND lease_generation=$7 AND lease_until>clock_timestamp()",&[&value,&lease.session.id,&lease.session.scope.owner_id,&lease.session.scope.account_id,&revision,&lease.worker,&lease.generation,&ttl]).map_err(session_db)?;
        require(count == 1, "stale session writer or expired lease")?;
        lease.session.revision = next.revision;
        Ok(())
    }
    fn release(&mut self, lease: &SessionLease) -> Result<()> {
        let count=self.client.execute("UPDATE research.model_sessions SET lease_until=clock_timestamp() WHERE session_id=$1 AND lease_worker=$2 AND lease_generation=$3",&[&lease.session.id,&lease.worker,&lease.generation]).map_err(session_db)?;
        require(count == 1, "session lease no longer belongs to this worker")
    }
}

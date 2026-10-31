//! In-memory session repository for deterministic harness tests and offline embedding.

use chrono::{Duration, Utc};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use trady_application::harness::*;
use trady_domain::{Error, Result, require};

struct Entry {
    session: Session,
    worker: String,
    generation: i64,
    expires_at: chrono::DateTime<Utc>,
}
#[derive(Clone, Default)]
pub struct MemorySessionRepository {
    entries: Arc<Mutex<BTreeMap<String, Entry>>>,
}
fn scoped(entry: &Entry, scope: &Scope) -> Result<()> {
    require(
        &entry.session.scope == scope,
        "session not found in owner/account/policy scope",
    )
}
impl SessionRepository for MemorySessionRepository {
    fn create(&mut self, session: &Session) -> Result<()> {
        session.scope.validate()?;
        session.limits.validate()?;
        require(
            session.schema_version == 1 && session.revision == 0 && valid_id(&session.id),
            "invalid initial session version/revision/identity",
        )?;
        let mut map = self
            .entries
            .lock()
            .map_err(|_| Error("session store lock failed".into()))?;
        require(!map.contains_key(&session.id), "session already exists")?;
        map.insert(
            session.id.clone(),
            Entry {
                session: session.clone(),
                worker: String::new(),
                generation: 0,
                expires_at: Utc::now(),
            },
        );
        Ok(())
    }
    fn load(&mut self, scope: &Scope, id: &str) -> Result<Session> {
        let map = self
            .entries
            .lock()
            .map_err(|_| Error("session store lock failed".into()))?;
        let entry = map
            .get(id)
            .ok_or_else(|| Error("session not found".into()))?;
        scoped(entry, scope)?;
        Ok(entry.session.clone())
    }
    fn list(&mut self, scope: &Scope) -> Result<Vec<Session>> {
        let map = self
            .entries
            .lock()
            .map_err(|_| Error("session store lock failed".into()))?;
        Ok(map
            .values()
            .filter(|e| &e.session.scope == scope)
            .map(|e| e.session.clone())
            .collect())
    }
    fn acquire(
        &mut self,
        scope: &Scope,
        id: &str,
        worker: &str,
        ttl_seconds: u32,
    ) -> Result<SessionLease> {
        require(
            valid_id(worker) && (1..=300).contains(&ttl_seconds),
            "invalid session lease",
        )?;
        let mut map = self
            .entries
            .lock()
            .map_err(|_| Error("session store lock failed".into()))?;
        let entry = map
            .get_mut(id)
            .ok_or_else(|| Error("session not found".into()))?;
        scoped(entry, scope)?;
        require(entry.expires_at <= Utc::now(), "session generation is busy")?;
        entry.generation = entry
            .generation
            .checked_add(1)
            .ok_or_else(|| Error("session fence overflow".into()))?;
        entry.worker = worker.into();
        entry.expires_at = Utc::now() + Duration::seconds(i64::from(ttl_seconds));
        Ok(SessionLease {
            session: entry.session.clone(),
            worker: worker.into(),
            generation: entry.generation,
            ttl_seconds,
        })
    }
    fn save(&mut self, lease: &mut SessionLease) -> Result<()> {
        require(
            (1..=300).contains(&lease.ttl_seconds),
            "invalid session lease duration",
        )?;
        let mut map = self
            .entries
            .lock()
            .map_err(|_| Error("session store lock failed".into()))?;
        let entry = map
            .get_mut(&lease.session.id)
            .ok_or_else(|| Error("session not found".into()))?;
        scoped(entry, &lease.session.scope)?;
        require(
            entry.worker == lease.worker
                && entry.generation == lease.generation
                && entry.expires_at > Utc::now()
                && entry.session.revision == lease.session.revision,
            "stale session writer or expired lease",
        )?;
        lease.session.revision = lease
            .session
            .revision
            .checked_add(1)
            .ok_or_else(|| Error("session revision overflow".into()))?;
        entry.session = lease.session.clone();
        entry.expires_at = Utc::now() + Duration::seconds(i64::from(lease.ttl_seconds));
        Ok(())
    }
    fn release(&mut self, lease: &SessionLease) -> Result<()> {
        let mut map = self
            .entries
            .lock()
            .map_err(|_| Error("session store lock failed".into()))?;
        let entry = map
            .get_mut(&lease.session.id)
            .ok_or_else(|| Error("session not found".into()))?;
        require(
            entry.worker == lease.worker && entry.generation == lease.generation,
            "stale session lease",
        )?;
        entry.expires_at = Utc::now();
        Ok(())
    }
}

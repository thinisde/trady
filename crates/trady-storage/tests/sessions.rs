//! Disposable PostgreSQL only: analyst sessions, revision CAS and server-time fencing.
use chrono::Utc;
use postgres::{Client, NoTls};
use std::{
    env,
    sync::{Arc, Barrier},
    thread,
};
use trady_application::harness::*;
use trady_storage::{migrate, sessions::PgSessionRepository};

fn dsn(role: &str) -> String {
    if let Ok(value) = env::var(format!("{}_DSN", role.to_ascii_uppercase())) {
        return value;
    }
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        env::var("TRADY_TEST_PORT").unwrap()
    )
}
fn session() -> Session {
    Session::new(
        Scope {
            owner_id: "session_owner".into(),
            account_id: "paper_session".into(),
            policy_version: 1,
        },
        ProviderIdentity {
            provider: "test".into(),
            model: "fixture".into(),
            endpoint: "https://example.com/v1/chat/completions".into(),
        },
        RunLimits::default(),
        "Paper research".into(),
        "tools-1".into(),
        vec![],
    )
    .unwrap()
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn session_reconnect_scope_races_revisions_and_expired_worker_fencing() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    migrate(&dsn("trady_migrator")).unwrap();
    let s = session();
    let mut repo = PgSessionRepository::connect(&dsn("trady_analyst")).unwrap();
    repo.create(&s).unwrap();
    drop(repo);
    let mut repo = PgSessionRepository::connect(&dsn("trady_analyst")).unwrap();
    assert_eq!(repo.load(&s.scope, &s.id).unwrap().id, s.id);
    assert!(
        repo.list(&s.scope)
            .unwrap()
            .iter()
            .any(|row| row.id == s.id)
    );
    for scope in [
        Scope {
            owner_id: "other".into(),
            ..s.scope.clone()
        },
        Scope {
            account_id: "paper_other".into(),
            ..s.scope.clone()
        },
        Scope {
            policy_version: 2,
            ..s.scope.clone()
        },
    ] {
        assert!(repo.load(&scope, &s.id).is_err());
        assert!(repo.acquire(&scope, &s.id, "worker", 30).is_err());
    }
    let mut lease = repo.acquire(&s.scope, &s.id, "worker_a", 30).unwrap();
    let mut stale = SessionLease {
        session: lease.session.clone(),
        worker: lease.worker.clone(),
        generation: lease.generation,
        ttl_seconds: lease.ttl_seconds,
    };
    lease.session.memory.push(MemoryItem {
        id: "note_1".into(),
        source: "owner".into(),
        content: "Attributed note, not financial authority".into(),
        as_of: Utc::now(),
        expires_at: None,
    });
    repo.save(&mut lease).unwrap();
    assert!(repo.save(&mut stale).is_err());
    let mut peer = PgSessionRepository::connect(&dsn("trady_analyst")).unwrap();
    assert!(peer.acquire(&s.scope, &s.id, "worker_b", 30).is_err());
    let mut admin = Client::connect(&dsn("postgres"), NoTls).unwrap();
    admin.execute("UPDATE research.model_sessions SET lease_until=clock_timestamp()-interval '1 second' WHERE session_id=$1",&[&s.id]).unwrap();
    assert!(repo.save(&mut lease).is_err());
    let mut takeover = peer.acquire(&s.scope, &s.id, "worker_b", 30).unwrap();
    assert!(takeover.generation > lease.generation);
    assert!(repo.release(&lease).is_err());
    assert!(repo.save(&mut lease).is_err());
    takeover.session.status = SessionStatus::WaitingOwner;
    peer.save(&mut takeover).unwrap();
    peer.release(&takeover).unwrap();
    drop(peer);
    let mut recovered = PgSessionRepository::connect(&dsn("trady_analyst")).unwrap();
    let recovered = recovered.load(&s.scope, &s.id).unwrap();
    assert_eq!(recovered.status, SessionStatus::WaitingOwner);
    assert_eq!(recovered.memory.len(), 1);
    for role in ["trady_executor", "trady_gateway"] {
        assert!(PgSessionRepository::connect(&dsn(role)).is_err());
        let mut client = Client::connect(&dsn(role), NoTls).unwrap();
        assert!(
            client
                .query("SELECT document FROM research.model_sessions", &[])
                .is_err()
        );
        assert!(
            client
                .execute(
                    "UPDATE research.model_sessions SET revision=revision+1",
                    &[]
                )
                .is_err()
        );
    }
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    assert!(
        analyst
            .execute("DELETE FROM research.model_sessions", &[])
            .is_err()
    );
    let race = session();
    repo.create(&race).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|i| {
            let barrier = barrier.clone();
            let scope = race.scope.clone();
            let id = race.id.clone();
            thread::spawn(move || {
                let mut repo = PgSessionRepository::connect(&dsn("trady_analyst")).unwrap();
                barrier.wait();
                repo.acquire(&scope, &id, &format!("racer_{i}"), 30).is_ok()
            })
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .map(|w| usize::from(w.join().unwrap()))
            .sum::<usize>(),
        1
    );
}

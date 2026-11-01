//! Uses only the disposable database created by scripts/test-postgres.py.
use chrono::Utc;
use postgres::{Client, NoTls};
use std::{collections::BTreeMap, env};
use trady_application::autonomous::{ExecutionJournal, TradingConfig, TradingState};
use trady_domain::*;
use trady_policy::Policy;
use trady_storage::{autonomous::PgTradingJournal, migrate, trading_research::PgTradingResearch};

fn dsn(role: &str) -> String {
    if let Ok(value) = env::var(format!("{}_DSN", role.to_ascii_uppercase())) {
        return value;
    }
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        env::var("TRADY_TEST_PORT").unwrap()
    )
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn autonomous_journal_is_durable_fenced_scoped_and_compare_and_swap() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let account = AccountId::new("paper_autonomous_sql").unwrap();
    let config = TradingConfig {
        config_version: 1,
        account_id: account.clone(),
        mode: Mode::Paper,
        base_currency: Currency::new("EUR").unwrap(),
        owner_id: "owner_auto".into(),
        policy: Policy::default(),
        capabilities: Capabilities::default(),
        approved_skills: BTreeMap::new(),
        live_release: None,
        limits_authority: None,
    };
    let mut repo = PgTradingJournal::connect(&dsn("trady_executor")).unwrap();
    let fence = repo
        .acquire(&config, &WorkerId::new("worker_a").unwrap(), 60)
        .unwrap();
    assert!(
        repo.acquire(&config, &WorkerId::new("worker_b").unwrap(), 60)
            .is_err()
    );
    let mut state = repo.load(&fence).unwrap();
    assert_eq!(state.revision, 0);
    state.revision = 1;
    state.consumed_sessions.insert("session_a".into());
    state.loss_paused_sessions.insert("session_a".into());
    repo.save(&fence, 0, &state).unwrap();
    assert!(repo.save(&fence, 0, &state).is_err());
    let mut tampered = state.clone();
    tampered.revision = 2;
    tampered.consumed_sessions.clear();
    assert!(repo.save(&fence, 1, &tampered).is_err());
    tampered = state.clone();
    tampered.revision = 2;
    tampered.loss_paused_sessions.clear();
    assert!(repo.save(&fence, 1, &tampered).is_err());
    drop(repo);
    let mut resumed = PgTradingJournal::connect(&dsn("trady_executor")).unwrap();
    assert!(
        resumed
            .load(&fence)
            .unwrap()
            .consumed_sessions
            .contains("session_a")
    );
    resumed.release(&fence).unwrap();
    let next = resumed
        .acquire(&config, &WorkerId::new("worker_b").unwrap(), 60)
        .unwrap();
    assert!(next.generation > fence.generation);
    assert!(resumed.renew(&fence, 60).is_err());
    assert!(resumed.load(&fence).is_err());
    assert!(resumed.save(&fence, 1, &TradingState::default()).is_err());
    resumed.release(&next).unwrap();
    for role in ["trady_analyst", "trady_gateway"] {
        assert!(PgTradingJournal::connect(&dsn(role)).is_err());
        let mut other = Client::connect(&dsn(role), NoTls).unwrap();
        assert!(
            other
                .batch_execute("SELECT execution.load_trader('paper_autonomous_sql','worker_a',1)")
                .is_err()
        );
        assert!(
            other
                .batch_execute("UPDATE execution.autonomous_accounts SET revision=999")
                .is_err()
        );
    }
    let mut executor = Client::connect(&dsn("trady_executor"), NoTls).unwrap();
    assert!(executor.batch_execute("SELECT research.put_trading_record('owner_auto','paper_autonomous_sql','evidence','forged','{}',clock_timestamp()+interval '1 day')").is_err());
    assert!(
        PgTradingResearch::connect(&dsn("trady_executor"), "owner_auto".into(), account).is_err()
    );
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    let retain = Utc::now() + chrono::Duration::hours(1);
    let body = serde_json::json!({"record":"original"});
    analyst
        .query_one(
            "SELECT research.put_trading_record($1,$2,'evidence','immutable',$3,$4)",
            &[&"owner_auto", &"paper_autonomous_sql", &body, &retain],
        )
        .unwrap();
    analyst
        .query_one(
            "SELECT research.put_trading_record($1,$2,'evidence','immutable',$3,$4)",
            &[&"owner_auto", &"paper_autonomous_sql", &body, &retain],
        )
        .unwrap();
    assert!(
        analyst
            .query_one(
                "SELECT research.put_trading_record($1,$2,'evidence','immutable',$3,$4)",
                &[
                    &"owner_auto",
                    &"paper_autonomous_sql",
                    &serde_json::json!({"record":"changed"}),
                    &retain
                ]
            )
            .is_err()
    );
    assert!(
        analyst
            .batch_execute("DELETE FROM research.trading_records")
            .is_err()
    );
}

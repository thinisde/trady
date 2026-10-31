//! Uses only scripts/test-postgres.py's disposable database.
use chrono::{DateTime, Duration, Utc};
use postgres::{Client, NoTls};
use rust_decimal::Decimal;
use std::{
    sync::{Arc, Barrier},
    thread,
};
use trady_application::{financial::*, research_jobs::digest};
use trady_domain::financial::*;
use trady_storage::{financial::PgFinancialRepository, migrate};

fn dsn(role: &str) -> String {
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        std::env::var("TRADY_TEST_PORT").unwrap()
    )
}
fn observation(source: &FinancialSource, now: DateTime<Utc>, value: i64) -> FinancialObservation {
    let payload = FinancialPayload::Metric {
        metric: "DGS10".into(),
        value: Some(Decimal::from(value)),
        period_start: None,
        period_end: (now - Duration::days(1)).date_naive(),
        vintage: Some((now - Duration::days(1)).date_naive()),
        source_revision: value.to_string(),
    };
    let hash = digest(&(&payload, Vec::<String>::new())).unwrap();
    FinancialObservation {
        id: digest(&(&source.id, "period_1", &hash, now)).unwrap(),
        source_id: source.id.clone(),
        provider: source.request.provider(),
        entity_id: source.entity_id.clone(),
        external_key: "period_1".into(),
        lineage_id: source.lineage_id.clone(),
        registry_hash: source.registry_hash().unwrap(),
        content_hash: hash,
        parser_version: "fixture".into(),
        source_url: "https://api.stlouisfed.org/fred/series/observations".into(),
        unit: source.unit.clone(),
        available_at: now - Duration::days(1),
        received_at: now,
        expires_at: now + Duration::hours(168),
        payload,
        gaps: vec![],
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn durable_shared_credits_revisions_replay_retention_and_role_boundaries() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let now = DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap();
    let mut config: FinancialConfig =
        serde_json::from_str(include_str!("../../../config/financial-data.json")).unwrap();
    let mut source = config
        .sources
        .iter()
        .find(|s| s.id == "fred_treasury_10y")
        .unwrap()
        .clone();
    source.id = "fixture_financial_fred".into();
    source.enabled = true;
    source.mapping_reference = "fixture_series_metadata".into();
    source.capability.status = AccessStatus::Verified;
    source.capability.revision = "fixture_v1".into();
    source.capability.evidence_reference = "fixture_review".into();
    source.capability.reviewed_at = Some(now - Duration::days(2));
    source.capability.valid_until = Some(now + Duration::days(20));
    source.rights.normalized_storage = true;
    source.rights.display = true;
    source.rights.external_llm = true;
    source.rights.terms_reference = "fixture_rights".into();
    source.interval_seconds = 60;
    let mut second = source.clone();
    second.id = "fixture_financial_fred_second".into();
    let mut price = source.clone();
    price.id = "fixture_massive_one".into();
    price.request = DataRequest::MassiveDaily {
        symbol: "AAPL".into(),
    };
    price.unit = "price".into();
    price.currency = Some("USD".into());
    let mut other_price = price.clone();
    other_price.id = "fixture_massive_two".into();
    config.sources = vec![
        source.clone(),
        second.clone(),
        price.clone(),
        other_price.clone(),
    ];
    let account = ProviderAccount {
        provider: Provider::Fred,
        enabled: true,
        limits: BudgetLimits {
            per_minute: 10,
            per_day: 10,
            per_month: 10,
        },
    };
    let small = ProviderAccount {
        provider: Provider::Massive,
        enabled: true,
        limits: BudgetLimits {
            per_minute: 1,
            per_day: 1,
            per_month: 1,
        },
    };
    config.accounts = vec![account.clone(), small.clone()];
    config.validate().unwrap();
    let window = DataWindow {
        start: (now - Duration::days(30)).date_naive(),
        end: (now - Duration::days(1)).date_naive(),
    };
    let mut repo = PgFinancialRepository::connect(&dsn("trady_analyst")).unwrap();
    repo.sync_registry(&config).unwrap();
    let lease = repo
        .reserve(&source, &account, &window, now)
        .unwrap()
        .unwrap();
    let first = observation(&source, now, 10);
    let page = FinancialPage {
        observations: vec![first.clone()],
        complete: true,
        gaps: vec![],
    };
    assert_eq!(repo.finish(&lease, &page, now).unwrap(), 1);
    assert!(repo.finish(&lease, &page, now).is_err());
    let cutoff = now + Duration::seconds(1);
    let saved = repo
        .read(&source.id, cutoff, Consumer::Display, 100, cutoff)
        .unwrap();
    assert_eq!(saved.observations.len(), 1);
    assert!(!saved.entry_authorized);
    let t2 = now + Duration::seconds(61);
    let lease = repo
        .reserve(&source, &account, &window, t2)
        .unwrap()
        .unwrap();
    assert_eq!(
        repo.finish(
            &lease,
            &FinancialPage {
                observations: vec![observation(&source, t2, 10)],
                complete: true,
                gaps: vec![]
            },
            t2
        )
        .unwrap(),
        0
    );
    let t3 = now + Duration::seconds(122);
    let lease = repo
        .reserve(&source, &account, &window, t3)
        .unwrap()
        .unwrap();
    let revised = observation(&source, t3, 11);
    assert_eq!(
        repo.finish(
            &lease,
            &FinancialPage {
                observations: vec![revised.clone()],
                complete: true,
                gaps: vec![]
            },
            t3
        )
        .unwrap(),
        1
    );
    let replay = repo
        .read(&source.id, cutoff, Consumer::Display, 100, t3)
        .unwrap();
    assert_eq!(replay.observations[0].id, first.id);
    let latest = repo
        .read(&source.id, t3, Consumer::Display, 100, t3)
        .unwrap();
    assert_eq!(latest.observations[0].id, revised.id);
    let mut analyst = Client::connect(&dsn("trady_analyst"), NoTls).unwrap();
    let invalidations: i64 = analyst
        .query_one(
            "SELECT count(*) FROM research.financial_invalidations WHERE observation_id=$1",
            &[&first.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(invalidations, 1);
    // A crash after reservation is charged; another worker cannot steal a live lease.
    let crashed = repo
        .reserve(&second, &account, &window, t3)
        .unwrap()
        .unwrap();
    assert!(
        repo.reserve(&second, &account, &window, t3 + Duration::seconds(1))
            .unwrap()
            .is_none()
    );
    assert!(
        repo.reserve(&second, &account, &window, t3 + Duration::seconds(91))
            .unwrap()
            .is_some()
    );
    assert!(
        repo.finish(
            &crashed,
            &FinancialPage {
                observations: vec![],
                complete: true,
                gaps: vec![]
            },
            t3 + Duration::seconds(91)
        )
        .is_err()
    );
    let used:i64=analyst.query_one("SELECT used_credits FROM research.provider_budget_windows WHERE budget_key=$1 AND window_kind='month'",&[&Provider::Fred.budget_key()]).unwrap().get(0);
    assert_eq!(used, 5);
    assert!(
        !repo.health().unwrap()["budgets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = vec![];
    for source in [price, other_price] {
        let barrier = barrier.clone();
        let account = small.clone();
        let window = window.clone();
        threads.push(thread::spawn(move || {
            let mut repo = PgFinancialRepository::connect(&dsn("trady_analyst")).unwrap();
            barrier.wait();
            repo.reserve(&source, &account, &window, now)
                .unwrap()
                .is_some()
        }));
    }
    assert_eq!(
        threads
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>(),
        1
    );
    for role in ["trady_executor", "trady_gateway", "trady_lab"] {
        assert!(PgFinancialRepository::connect(&dsn(role)).is_err());
        let mut client = Client::connect(&dsn(role), NoTls).unwrap();
        assert!(
            client
                .query("SELECT document FROM research.financial_observations", &[])
                .is_err()
        );
    }
    source.rights.external_llm = false;
    config.sources[0] = source.clone();
    repo.sync_registry(&config).unwrap();
    assert_eq!(
        repo.read(&source.id, t3, Consumer::ExternalLlm, 100, t3)
            .unwrap()
            .status,
        DataStatus::Denied
    );
    source.rights.normalized_storage = false;
    config.sources[0] = source.clone();
    repo.sync_registry(&config).unwrap();
    let count: i64 = analyst
        .query_one(
            "SELECT count(*) FROM research.financial_observations WHERE source_id=$1",
            &[&source.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
}

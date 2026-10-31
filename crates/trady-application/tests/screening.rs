use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use trady_application::screening::*;
use trady_domain::{Error, Result, research::SourceRights, screening::*};

fn now() -> DateTime<Utc> {
    "2026-10-30T14:00:00Z".parse().unwrap()
}
fn request() -> ScreeningRequest {
    ScreeningRequest {
        owner_id: "owner_1".into(),
        run_id: "run_1".into(),
        owner_allowlist: BTreeSet::new(),
        policy: ScreeningPolicy {
            max_membership_age_seconds: 86400,
            max_broker_age_seconds: 600,
            max_quote_age_seconds: 60,
            max_priority_age_seconds: 3600,
            max_timing_age_seconds: 300,
            minimum_seconds_to_close: 1800,
            max_spread_bps: Decimal::from(20),
            max_round_trip_cost_bps: Decimal::from(50),
            minimum_daily_notional: Decimal::from(100_000),
            max_order_participation_bps: Decimal::from(10),
            candidate_limit: 5,
            peer_limit: 2,
        },
    }
}
fn universe() -> UniverseSnapshot {
    UniverseSnapshot {
        snapshot_id: "universe_1".into(),
        classification_snapshot_id: "gics_1".into(),
        index_id: "SP500".into(),
        complete: true,
        taxonomy_edition: "GICS_2026".into(),
        as_of: now() - Duration::seconds(30),
        available_at: now() - Duration::seconds(20),
        expires_at: now() + Duration::hours(1),
        provenance: UniverseProvenance {
            source_id: "fixture".into(),
            source_url: "https://example.com/universe.json".into(),
            provider: "fixture".into(),
            content_sha256: "a".repeat(64),
            parser_version: "test_v1".into(),
            fetched_at: now(),
            rights: SourceRights {
                store_content: true,
                llm_processing: false,
                retention_hours: 24,
                terms_reference: "fixture_only".into(),
            },
        },
        members: vec![],
    }
}
struct Source(Result<UniverseSnapshot>);
impl UniverseSource for Source {
    fn fetch_universe(&mut self, _: DateTime<Utc>) -> Result<UniverseSnapshot> {
        self.0.clone()
    }
}
#[derive(Default)]
struct Data {
    calls: usize,
    fail: bool,
}
impl ScreeningDataPort for Data {
    fn observations(
        &mut self,
        _: &UniverseSnapshot,
        _: DateTime<Utc>,
    ) -> Result<ScreeningObservations> {
        self.calls += 1;
        if self.fail {
            Err(Error("untrusted provider text".into()))
        } else {
            Ok(ScreeningObservations::default())
        }
    }
}
#[derive(Default)]
struct Store {
    values: BTreeMap<(String, String), ScreeningSnapshot>,
    fail: bool,
}
impl ScreeningRepository for Store {
    fn store_screening_snapshot(&mut self, snapshot: &ScreeningSnapshot) -> Result<()> {
        if self.fail {
            return Err(Error("storage offline".into()));
        }
        let key = (
            snapshot.request.owner_id.clone(),
            snapshot.request.run_id.clone(),
        );
        if let Some(previous) = self.values.get(&key) {
            if previous != snapshot {
                return Err(Error("changed immutable run".into()));
            }
        } else {
            self.values.insert(key, snapshot.clone());
        }
        Ok(())
    }
}

#[test]
fn screening_persists_full_inputs_and_empty_universe_blocker_before_returning() {
    let mut store = Store::default();
    let result = run_screening(
        &request(),
        &mut Source(Ok(universe())),
        &mut Data::default(),
        &mut store,
        now(),
    )
    .unwrap();
    assert_eq!(store.values.values().next(), Some(&result));
    assert!(result.universe.is_some() && result.observations.is_some());
    assert_eq!(
        result.result.unwrap().blockers,
        vec![RejectionReason::EmptyUniverse]
    );
    assert!(result.failure.is_none());
}

#[test]
fn screening_persists_sanitized_source_and_observation_failure_attempts() {
    let mut store = Store::default();
    let mut data = Data::default();
    let snapshot = run_screening(
        &request(),
        &mut Source(Err(Error("secret raw HTTP body".into()))),
        &mut data,
        &mut store,
        now(),
    )
    .unwrap();
    assert_eq!(
        snapshot.failure,
        Some(ScreeningFailure::UniverseUnavailable)
    );
    assert!(snapshot.universe.is_none() && snapshot.result.is_none());
    assert_eq!(data.calls, 0);
    assert!(!serde_json::to_string(&snapshot).unwrap().contains("secret"));
    let snapshot = run_screening(
        &request(),
        &mut Source(Ok(universe())),
        &mut Data {
            fail: true,
            calls: 0,
        },
        &mut Store::default(),
        now(),
    )
    .unwrap();
    assert_eq!(
        snapshot.failure,
        Some(ScreeningFailure::ObservationsUnavailable)
    );
    assert!(snapshot.universe.is_some() && snapshot.result.is_none());
}

#[test]
fn screening_forbidden_storage_does_not_persist_dataset_or_derivatives() {
    let mut value = universe();
    value.provenance.rights.store_content = false;
    value.provenance.provider = "restricted_provider_content".into();
    let mut data = Data::default();
    let snapshot = run_screening(
        &request(),
        &mut Source(Ok(value)),
        &mut data,
        &mut Store::default(),
        now(),
    )
    .unwrap();
    assert_eq!(
        snapshot.failure,
        Some(ScreeningFailure::SourceStorageNotPermitted)
    );
    assert!(
        snapshot.universe.is_none() && snapshot.observations.is_none() && snapshot.result.is_none()
    );
    assert_eq!(data.calls, 0);
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("restricted_provider_content")
    );
}

#[test]
fn screening_storage_failure_never_returns_an_unpersisted_snapshot() {
    let mut store = Store {
        fail: true,
        ..Store::default()
    };
    assert!(
        run_screening(
            &request(),
            &mut Source(Ok(universe())),
            &mut Data::default(),
            &mut store,
            now()
        )
        .is_err()
    );
    assert!(store.values.is_empty());
}

#[test]
fn screening_snapshot_identity_is_deterministic_and_immutable_replays_are_scoped() {
    let mut store = Store::default();
    let first = run_screening(
        &request(),
        &mut Source(Ok(universe())),
        &mut Data::default(),
        &mut store,
        now(),
    )
    .unwrap();
    let same = run_screening(
        &request(),
        &mut Source(Ok(universe())),
        &mut Data::default(),
        &mut store,
        now(),
    )
    .unwrap();
    assert_eq!(first, same);
    assert_eq!(first.snapshot_id.len(), 71);
    assert_eq!(store.values.len(), 1);
    let mut changed = universe();
    changed.snapshot_id = "universe_2".into();
    assert!(
        run_screening(
            &request(),
            &mut Source(Ok(changed)),
            &mut Data::default(),
            &mut store,
            now()
        )
        .is_err()
    );
    let mut another_owner = request();
    another_owner.owner_id = "owner_2".into();
    let other = run_screening(
        &another_owner,
        &mut Source(Ok(universe())),
        &mut Data::default(),
        &mut store,
        now(),
    )
    .unwrap();
    assert_ne!(first.snapshot_id, other.snapshot_id);
}

#[test]
fn screening_invalid_request_stops_before_external_work() {
    let mut bad = request();
    bad.policy.candidate_limit = 6;
    let mut data = Data::default();
    let mut store = Store::default();
    assert!(
        run_screening(
            &bad,
            &mut Source(Ok(universe())),
            &mut data,
            &mut store,
            now()
        )
        .is_err()
    );
    assert_eq!(data.calls, 0);
    assert!(store.values.is_empty());
}

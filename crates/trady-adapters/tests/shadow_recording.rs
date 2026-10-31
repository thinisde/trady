use chrono::Utc;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::fs;
use trady_adapters::shadow::*;
use trady_application::{
    autonomous::{ExecutionBroker, ExecutionObservation},
    broker::*,
};
use trady_domain::*;

#[test]
fn shadow_only_records_intents_across_restart_and_never_acknowledges_orders() {
    let directory = std::env::temp_dir().join(format!(
        "trady-shadow-worker-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let account = AccountId::new("shadow_recording_test").unwrap();
    let observation = ExecutionObservation {
        snapshot: AccountSnapshot {
            snapshot_id: "observed".into(),
            account_id: account.clone(),
            mode: Mode::Shadow,
            spendable_cash: Money {
                amount: Decimal::TEN,
                currency: Currency::new("EUR").unwrap(),
            },
            positions: vec![],
            orders: vec![],
            observed_at: Utc::now(),
        },
        position_costs: BTreeMap::new(),
        available_quantities: BTreeMap::new(),
        total_equity: Money {
            amount: Decimal::TEN,
            currency: Currency::new("EUR").unwrap(),
        },
    };
    let source = ShadowSource::Fixture {
        description: "explicit test fixture".into(),
    };
    let mut broker = ShadowBroker::new(&directory, observation.clone(), source.clone()).unwrap();
    let request = BrokerOrderRequest {
        intent_id: IntentId::new("intent_a").unwrap(),
        account_id: account.clone(),
        instrument_id: InstrumentId::new("stock").unwrap(),
        instrument_currency: Currency::new("EUR").unwrap(),
        side: Side::Buy,
        quantity: Decimal::ONE,
        kind: OrderKind::Limit {
            price: Decimal::ONE,
            time_in_force: TimeInForce::Day,
        },
    };
    assert!(matches!(
        broker.submit_order(&request),
        Err(BrokerFailure {
            kind: BrokerFailureKind::Unsupported,
            ..
        })
    ));
    assert!(broker.cancel_order(&account, "order_a").is_err());
    assert!(
        broker
            .observe(&account, &[])
            .unwrap()
            .snapshot
            .orders
            .is_empty()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&directory).unwrap().mode() & 0o777, 0o700);
        for item in fs::read_dir(&directory).unwrap() {
            assert_eq!(item.unwrap().metadata().unwrap().mode() & 0o777, 0o600);
        }
    }
    let first = broker.record_proposal(&request).unwrap();
    drop(broker);
    let broker = ShadowBroker::new(&directory, observation.clone(), source.clone()).unwrap();
    assert_eq!(broker.record_proposal(&request).unwrap(), first);
    let mut changed = request.clone();
    changed.quantity = Decimal::TEN;
    assert!(broker.record_proposal(&changed).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(ShadowBroker::new(&directory, observation.clone(), source.clone()).is_err());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut live = observation;
    live.snapshot.mode = Mode::Live;
    live.snapshot.account_id = AccountId::new("live_forbidden").unwrap();
    assert!(ShadowBroker::new(&directory, live, source).is_err());
    fs::remove_dir_all(directory).unwrap();
}

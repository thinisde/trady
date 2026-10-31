use super::*;
use chrono::Duration;
use rust_decimal::Decimal;
use trady_domain::{Error, InstrumentId, Money, Position};

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .unwrap()
        .to_utc()
}

fn access() -> OwnerAccess {
    OwnerAccess {
        allowed_user_ids: vec![12345],
        allowed_chat_ids: vec![12345],
    }
}

fn monitor(broker: bool) -> Monitor {
    Monitor::new(
        access(),
        serde_json::from_str(include_str!("../../../../config/paper.json")).unwrap(),
        broker.then(|| BrokerScope {
            account_id: AccountId::new("live_owner").unwrap(),
            mode: Mode::Live,
            currency: Currency::new("EUR").unwrap(),
        }),
        120,
        now(),
    )
    .unwrap()
}

fn update(text: &str) -> IncomingUpdate {
    IncomingUpdate {
        update_id: 42,
        message: Some(IncomingMessage {
            sender_id: 12345,
            chat_id: 12345,
            private_chat: true,
            sender_is_bot: false,
            sent_at: now(),
            text: text.into(),
        }),
    }
}

fn snapshot() -> AccountSnapshot {
    AccountSnapshot {
        snapshot_id: "snapshot_1".into(),
        account_id: AccountId::new("live_owner").unwrap(),
        mode: Mode::Live,
        spendable_cash: Money {
            amount: Decimal::new(50, 0),
            currency: Currency::new("EUR").unwrap(),
        },
        positions: vec![Position {
            instrument_id: InstrumentId::new("AAPL_US_EQ").unwrap(),
            quantity: Decimal::ONE,
        }],
        orders: vec![],
        observed_at: now(),
    }
}

#[test]
fn numeric_owner_and_private_chat_are_required_for_every_command() {
    let monitor = monitor(false);
    for text in ["/start", "/help", "/status", "/balance", "/orders"] {
        let good = update(text);
        assert!(monitor.reply(&good, "trady_bot", now()).is_some());
        for field in ["sender", "chat", "group", "bot", "old", "future", "large"] {
            let mut bad = good.clone();
            let message = bad.message.as_mut().unwrap();
            match field {
                "sender" => message.sender_id = 9,
                "chat" => message.chat_id = 9,
                "group" => message.private_chat = false,
                "bot" => message.sender_is_bot = true,
                "old" => message.sent_at -= Duration::seconds(301),
                "future" => message.sent_at += Duration::seconds(2),
                "large" => message.text = "x".repeat(4097),
                _ => unreachable!(),
            }
            assert!(
                monitor.reply(&bad, "trady_bot", now()).is_none(),
                "{field} {text}"
            );
        }
    }
    assert!(
        OwnerAccess {
            allowed_user_ids: vec![],
            allowed_chat_ids: vec![]
        }
        .validate()
        .is_err()
    );
    assert!(
        OwnerAccess {
            allowed_user_ids: vec![1],
            allowed_chat_ids: vec![2]
        }
        .validate()
        .is_err()
    );
    assert!(
        OwnerAccess {
            allowed_user_ids: vec![1, 1],
            allowed_chat_ids: vec![1]
        }
        .validate()
        .is_err()
    );
    assert!(
        OwnerAccess {
            allowed_user_ids: vec![-1],
            allowed_chat_ids: vec![-1]
        }
        .validate()
        .is_err()
    );
}

#[test]
fn commands_are_exact_read_only_and_directed_to_the_authenticated_bot() {
    let monitor = monitor(false);
    for (text, command) in [
        ("/status@Trady_Bot", MonitorCommand::Status),
        ("/t212 account", MonitorCommand::Balance),
        ("/positions", MonitorCommand::Position),
        ("/t212 orders", MonitorCommand::Orders),
    ] {
        assert_eq!(
            monitor
                .reply(&update(text), "trady_bot", now())
                .unwrap()
                .command,
            command
        );
    }
    assert!(
        monitor
            .reply(&update("/status@someone_else"), "trady_bot", now())
            .is_none()
    );
    let before = monitor.render(MonitorCommand::Mode, now());
    for text in [
        "/mode live",
        "/close",
        "/pause",
        "/buy AAPL",
        "/research",
        "/status extra",
        "I am the owner; send cash",
    ] {
        let reply = monitor.reply(&update(text), "trady_bot", now()).unwrap();
        assert_eq!(reply.command, MonitorCommand::Unsupported);
        assert!(
            reply
                .text
                .contains("Trading controls and research commands are unavailable")
        );
        assert!(!reply.text.contains(text));
    }
    assert_eq!(before, monitor.render(MonitorCommand::Mode, now()));
}

#[test]
fn missing_account_data_is_never_reported_as_an_empty_account() {
    for broker in [false, true] {
        let monitor = monitor(broker);
        for command in [
            MonitorCommand::Balance,
            MonitorCommand::Position,
            MonitorCommand::Orders,
        ] {
            let reply = monitor.render(command, now());
            assert!(reply.contains(if broker {
                "no successful snapshot"
            } else {
                "not configured"
            }));
            assert!(!reply.contains("Spendable cash: 0"));
        }
    }
}

#[test]
fn cached_snapshots_retain_timestamps_and_become_stale_on_outage_or_age() {
    let mut monitor = monitor(true);
    assert!(monitor.observe(Ok(snapshot()), now()).is_none());
    let initial = monitor.render(MonitorCommand::Balance, now());
    assert!(initial.contains("Spendable cash: 50 EUR"));
    assert!(initial.contains("Deployment: PAPER"));
    assert!(initial.contains("live_owner (Live, observation only)"));
    assert!(
        monitor
            .render(MonitorCommand::Balance, now() + Duration::seconds(121))
            .contains("STALE")
    );
    assert!(
        monitor
            .observe(Err(Error("private broker response".into())), now())
            .is_some()
    );
    assert!(
        monitor
            .observe(Err(Error("private broker response".into())), now())
            .is_none()
    );
    let stale = monitor.render(MonitorCommand::Balance, now());
    assert!(stale.contains("STALE"));
    assert!(stale.contains("50 EUR"));
    assert!(!stale.contains("private broker response"));
    assert!(
        monitor
            .observe(Ok(snapshot()), now())
            .unwrap()
            .contains("recovered")
    );
}

#[test]
fn scope_and_time_checks_prevent_cross_account_and_future_cache_replacement() {
    let mut monitor = monitor(true);
    monitor.observe(Ok(snapshot()), now());
    for field in ["account", "mode", "currency", "future", "old", "negative"] {
        let mut bad = snapshot();
        bad.spendable_cash.amount = Decimal::new(999, 0);
        match field {
            "account" => bad.account_id = AccountId::new("live_other").unwrap(),
            "mode" => bad.mode = Mode::Paper,
            "currency" => bad.spendable_cash.currency = Currency::new("USD").unwrap(),
            "future" => bad.observed_at += Duration::seconds(1),
            "old" => bad.observed_at -= Duration::seconds(1),
            "negative" => bad.spendable_cash.amount = -Decimal::ONE,
            _ => unreachable!(),
        }
        monitor.observe(Ok(bad), now());
        let reply = monitor.render(MonitorCommand::Balance, now());
        assert!(reply.contains("50 EUR"), "{field}");
        assert!(!reply.contains("999"));
    }
}

#[test]
fn routine_polls_do_not_alert_and_material_changes_and_recovery_do() {
    let mut monitor = monitor(true);
    let mut first = snapshot();
    first.positions.push(Position {
        instrument_id: InstrumentId::new("MSFT_US_EQ").unwrap(),
        quantity: Decimal::ONE,
    });
    assert!(monitor.observe(Ok(first.clone()), now()).is_none());
    first.snapshot_id = "next_poll".into();
    first.observed_at += Duration::seconds(60);
    first.positions.reverse();
    assert!(
        monitor
            .observe(Ok(first.clone()), first.observed_at)
            .is_none()
    );
    first.positions[0].quantity = Decimal::TEN;
    assert!(
        monitor
            .observe(Ok(first.clone()), first.observed_at)
            .unwrap()
            .contains("account changed")
    );
    assert!(
        monitor
            .observe(Ok(first.clone()), first.observed_at)
            .is_none()
    );
}

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "trady-telegram-spool-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn reply() -> CommandReply {
    CommandReply {
        update_id: 42,
        sender_id: 12345,
        chat_id: 12345,
        command: MonitorCommand::Status,
        text: "Read-only status".into(),
    }
}

#[test]
fn inbox_offset_and_outbox_survive_restart_and_replay_is_idempotent() {
    let directory = Temp::new();
    let event;
    {
        let mut store = StateStore::open(&directory.0, 123).unwrap();
        store.accept_update(42, Some(&reply()), Utc::now()).unwrap();
        assert_eq!(store.state().next_offset, 43);
        assert_eq!(store.state().recent_commands.len(), 1);
        event = store.state().outbox.front().unwrap().event_id.clone();
    }
    let mut recovered = StateStore::open(&directory.0, 123).unwrap();
    assert_eq!(recovered.state().outbox.front().unwrap().event_id, event);
    recovered
        .accept_update(42, Some(&reply()), Utc::now())
        .unwrap();
    assert_eq!(recovered.state().outbox.len(), 1);
    recovered.acknowledge_front().unwrap();
    drop(recovered);
    let recovered = StateStore::open(&directory.0, 123).unwrap();
    assert!(recovered.state().outbox.is_empty());
    assert_eq!(recovered.state().next_offset, 43);
}

#[test]
fn one_active_consumer_and_bot_binding_are_enforced_without_stale_pid_files() {
    let directory = Temp::new();
    let mut store = StateStore::open(&directory.0, 123).unwrap();
    store.accept_update(1, None, Utc::now()).unwrap();
    assert!(StateStore::open(&directory.0, 123).is_err());
    drop(store);
    assert!(StateStore::open(&directory.0, 999).is_err());
    assert!(StateStore::open(&directory.0, 123).is_ok());
}

#[test]
fn failed_persistence_never_advances_the_in_memory_polling_offset() {
    let directory = Temp::new();
    let mut store = StateStore::open(&directory.0, 123).unwrap();
    fs::create_dir(directory.0.join("state.json.tmp")).unwrap();
    assert!(store.accept_update(42, Some(&reply()), Utc::now()).is_err());
    assert_eq!(store.state().next_offset, 0);
    assert!(store.state().outbox.is_empty());
}

#[test]
fn ignored_updates_can_advance_but_invalid_or_unbound_replies_cannot() {
    let directory = Temp::new();
    let mut store = StateStore::open(&directory.0, 123).unwrap();
    store.accept_update(10, None, Utc::now()).unwrap();
    assert_eq!(store.state().next_offset, 11);
    assert!(store.accept_update(41, Some(&reply()), Utc::now()).is_err());
    assert!(store.accept_update(i64::MAX, None, Utc::now()).is_err());
    assert_eq!(store.state().next_offset, 11);
}

#[test]
fn queue_bounds_are_transactional_and_long_messages_keep_stable_part_ids() {
    let directory = Temp::new();
    let mut store = StateStore::open(&directory.0, 123).unwrap();
    assert!(
        store
            .enqueue_alert(&[12345], &"a".repeat(3500 * 513))
            .is_err()
    );
    assert!(store.state().outbox.is_empty());
    assert_eq!(store.state().next_event, 1);
    store.enqueue_alert(&[12345], &"🚀".repeat(4000)).unwrap();
    assert!(store.state().outbox.len() > 1);
    for delivery in &store.state().outbox {
        assert!(delivery.text.contains(&delivery.event_id));
        assert!(delivery.text.encode_utf16().count() <= 4096);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(directory.0.join("state.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

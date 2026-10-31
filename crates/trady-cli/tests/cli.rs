use serde_json::{Value, json};
use std::{
    fs,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

fn command(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .env_remove("TRADY_ANALYST_DSN")
        .env_remove("TRADY_EXECUTOR_DSN")
        .env_remove("TRADY_GATEWAY_DSN")
        .output()
        .unwrap()
}
#[test]
fn service_entry_points_are_explicitly_blocked_probes() {
    for service in ["analyst", "executor", "gateway"] {
        let result = command(&[service, "--once"]);
        assert!(result.status.success());
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["entry_authorized"], false);
        assert_eq!(value["database"], "not_configured");
        assert!(value["entry_blockers"].as_array().unwrap().len() >= 4);
        assert!(!command(&[service]).status.success());
    }
}
#[test]
fn live_modes_unknown_fields_and_unsafe_policy_are_rejected_at_startup() {
    let original: Value = serde_json::from_str(include_str!("../../../config/paper.json")).unwrap();
    for (path, value) in [
        ("/mode", json!("live")),
        ("/mode", json!("shadow")),
        ("/config_version", json!(2)),
        ("/policy/entry_cash_fraction", json!("0.99")),
        ("/policy/confidence_entry_min", json!(89)),
    ] {
        let mut config = original.clone();
        *config.pointer_mut(path).unwrap() = value;
        assert_config_rejected(&config);
    }
    let mut config = original;
    config["model_can_trade"] = json!(true);
    assert_config_rejected(&config);
}
fn assert_config_rejected(config: &Value) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let file = std::env::temp_dir().join(format!(
        "trady-config-test-{}-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&file, config.to_string()).unwrap();
    let output = command(&["executor", "--once", "--config", file.to_str().unwrap()]);
    fs::remove_file(file).unwrap();
    assert!(!output.status.success());
}
#[test]
fn offline_demo_preserves_unknown_outcomes_and_recovers_one_fill() {
    let output = command(&["demo"]);
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["duplicate_intent_created"], false);
    assert_eq!(value["missing_response_state"], "RECOVERY_REQUIRED");
    assert_eq!(value["reconciled_fill_state"], "OPEN_UNPROTECTED");
    assert_eq!(value["remaining_unresolved_intents"], 0);
    assert_eq!(value["cash"]["amount"], "1");
}

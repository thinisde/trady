//! Credential-free EUR diagnostics must not imply operational acceptance.
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("trady-eur-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .env(
            "TRADY_CREDENTIALS_FILE",
            "/missing-eur-fixture-credentials.json",
        )
        .output()
        .unwrap()
}
fn result(args: &[&str]) -> Value {
    let out = run(args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn both_diagnostic_spellings_use_the_same_blockers() {
    let common = ["--config", "config/eur-demo.example.json"];
    let direct = result(&["gate", "explain", common[0], common[1]]);
    let nested = result(&["eur", "gate-explain", common[0], common[1]]);
    assert_eq!(direct["readiness"], nested["readiness"]);
    assert_eq!(direct["entry_authorized"], false);
    assert!(
        direct["readiness"]["blockers"]
            .as_array()
            .unwrap()
            .contains(&json!("SNAPSHOT_UNAVAILABLE"))
    );
    assert!(
        !run(&["gate", "explain", "--config", common[1], "--unknown", "x"])
            .status
            .success()
    );
}
#[test]
fn flipping_enabled_and_demo_never_authorizes_an_order_or_reports_a_cycle() {
    let scratch = Scratch::new();
    let path = scratch.0.join("runtime.json");
    let mut config: Value =
        serde_json::from_str(include_str!("../../../config/eur-demo.example.json")).unwrap();
    config["enabled"] = json!(true);
    config["execution_stage"] = json!("DEMO");
    fs::write(&path, config.to_string()).unwrap();
    assert_eq!(
        result(&["eur", "config", "--config", path.to_str().unwrap()])["entry_authorized"],
        false
    );
    let out = run(&[
        "autonomous",
        "run",
        "--config",
        path.to_str().unwrap(),
        "--once",
    ]);
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("RECORDED"));
    config["execution_stage"] = json!("LIVE");
    config["live_enabled"] = json!(true);
    fs::write(&path, config.to_string()).unwrap();
    assert!(
        !run(&["eur", "config", "--config", path.to_str().unwrap()])
            .status
            .success()
    );
}

#[test]
fn eur_source_policy_rejects_yahoo_quote_fallback_before_credentials() {
    let scratch = Scratch::new();
    let path = scratch.0.join("shadow.json");
    let mut config: Value =
        serde_json::from_str(include_str!("../../../config/eur-shadow.example.json")).unwrap();
    config["enabled"] = json!(true);
    config["shadow"]["ibkr_config"] = Value::Null;
    fs::write(&path, config.to_string()).unwrap();
    let output = run(&["eur", "daily", "--config", path.to_str().unwrap(), "--once"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("IBKR SMART Live configuration is required")
    );
}

#[test]
fn protocol_driver_rejects_missing_release_before_any_provider_operation() {
    let output = run(&[
        "eur",
        "protocol-drive",
        "--config",
        "config/eur-demo.example.json",
        "--bundle",
        "/missing-release-bundle",
        "--owner-approval",
        "/missing-approval",
        "--intent",
        "rejected_intent",
        "--once",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).is_empty());
}
#[test]
fn input_integrity_check_is_offline_detects_tampering_and_does_not_certify_a_paper_day() {
    let scratch = Scratch::new();
    let path = scratch.0.join("manifest.json");
    fs::write(scratch.0.join("capture.json"), "{}").unwrap();
    let sha = trady_application::harness::content_hash("{}");
    let mut inputs = serde_json::Map::new();
    for name in [
        "yfinance",
        "alpha_vantage",
        "rss",
        "x",
        "broker_catalog",
        "session_calendar",
        "quote_observations",
        "cost_fx",
    ] {
        inputs.insert(name.into(), json!({"path":"capture.json","sha256":sha}));
    }
    fs::write(
        &path,
        json!({"schema_version":1,"run_id":"fixture","inputs":inputs}).to_string(),
    )
    .unwrap();
    let args = [
        "replay",
        "manifest-check",
        "--config",
        "config/eur-demo.example.json",
        "--manifest",
        path.to_str().unwrap(),
    ];
    let first = result(&args);
    let second = result(&args);
    assert_eq!(first, second);
    assert_eq!(first["network_calls"], 0);
    assert_eq!(first["paper_day_passed"], false);
    fs::write(scratch.0.join("capture.json"), "{\"changed\":true}").unwrap();
    assert!(!run(&args).status.success());
    assert!(
        !run(&[
            "replay",
            "paper-day",
            "--config",
            "config/eur-demo.example.json",
            "--manifest",
            path.to_str().unwrap()
        ])
        .status
        .success()
    );
}

//! The real analyst child uses only its scoped PostgreSQL role and pipe handoff.
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn incomplete_shadow_deployment_reports_actions_without_credentials_or_network() {
    let directory = env::temp_dir().join(format!(
        "trady-shadow-preflight-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir(&directory).unwrap();
    let journal = directory.join("journal");
    let path = directory.join("runtime.json");
    let mut config: Value = serde_json::from_str(include_str!(
        "../../../config/autonomous.demo-no-submit.json"
    ))
    .unwrap();
    config["shadow_directory"] = json!(journal);
    // Keep this missing-prerequisite fixture independent of the owner's demo selections.
    config["inputs"]["screening"]["owner_allowlist"] = json!([]);
    config["trading"]["approved_skills"] = json!({});
    fs::write(&path, config.to_string()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_trady"))
        .args([
            "autonomous",
            "run",
            "--config",
            path.to_str().unwrap(),
            "--once",
        ])
        .env_clear()
        .env(
            "TRADY_CREDENTIALS_FILE",
            directory.join("must-not-be-read.json"),
        )
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "shadow_blocked");
    assert_eq!(result["cycles"], 0);
    assert_eq!(result["order_mutations"], 0);
    assert_eq!(result["preflight"]["configuration_ready"], false);
    assert_eq!(result["preflight"]["entry_controls_enabled"], true);
    assert_eq!(result["preflight"]["entry_authorized"], false);
    assert_eq!(
        result["preflight"]["order_submission_policy"],
        "record_only"
    );
    let blockers = result["preflight"]["blockers"].as_array().unwrap();
    for code in [
        "approved_model_registry_empty",
        "approved_input_service_unconfigured",
        "universe_source_disabled",
        "owner_allowlist_empty",
        "approved_skill_hashes_missing",
        "entry_capabilities_unverified",
        "approved_formula_unavailable",
    ] {
        assert!(blockers.contains(&json!(code)));
        assert!(
            result["preflight"]["required_actions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|action| action["code"] == code
                    && !action["required"].as_str().unwrap().is_empty())
        );
    }
    let records: Vec<_> = fs::read_dir(&journal)
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    assert_eq!(records.len(), 1);
    let audit: Value = serde_json::from_slice(&fs::read(records[0].path()).unwrap()).unwrap();
    assert_eq!(audit["phase"], "configuration");
    assert_eq!(audit["order_mutations"], 0);
    assert_eq!(audit["preflight"], result["preflight"]);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn configuration_check_reports_unknown_unsupported_and_expired_entry_capabilities() {
    let directory = env::temp_dir().join(format!(
        "trady-capability-preflight-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("runtime.json");
    let mut config: Value = serde_json::from_str(include_str!(
        "../../../config/autonomous.demo-no-submit.json"
    ))
    .unwrap();
    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    let verified =
        json!({"support":"VERIFIED","evidence_id":"fixture-evidence","valid_until":future});
    for name in ["cash_account", "instrument", "protective_orders", "quotes"] {
        config["trading"]["capabilities"][name] = verified.clone();
    }
    // Fractional support is conditional on quantity sizing, not an unconditional
    // prerequisite for an otherwise reviewed whole-share deployment.
    for invalid in [
        json!({"support":"UNKNOWN","evidence_id":null,"valid_until":null}),
        json!({"support":"UNSUPPORTED","evidence_id":"unsupported","valid_until":future}),
        json!({"support":"VERIFIED","evidence_id":"expired","valid_until":"2020-01-01T00:00:00Z"}),
        json!({"support":"VERIFIED","evidence_id":"","valid_until":future}),
        json!({"support":"VERIFIED","evidence_id":"missing-expiry","valid_until":null}),
        verified,
    ] {
        let usable = invalid["evidence_id"] == "fixture-evidence";
        config["trading"]["capabilities"]["protective_orders"] = invalid;
        fs::write(&path, config.to_string()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_trady"))
            .args(["autonomous", "check", "--config", path.to_str().unwrap()])
            .env_clear()
            .env("TRADY_CREDENTIALS_FILE", directory.join("absent.json"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            result["unverified_entry_capabilities"],
            if usable {
                json!([])
            } else {
                json!(["protective_orders"])
            }
        );
        assert_eq!(
            result["blockers"]
                .as_array()
                .unwrap()
                .contains(&json!("entry_capabilities_unverified")),
            !usable
        );
        assert_eq!(result["entry_authorized"], false);
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn analyst_child_returns_no_entry_without_formula_or_broker_credentials() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    let mut config: Value =
        serde_json::from_str(include_str!("../../../config/autonomous.example.json"))
            .expect("disabled deployment fixture");
    config["enabled"] = json!(true);
    let boot = json!({
        "config":config,
        "config_path":concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/autonomous.example.json"),
        "analyst_dsn":env::var("TRADY_ANALYST_DSN").expect("isolated analyst DSN"),
        "token":null
    });
    let snapshot = json!({
        "snapshot_id":"isolated-worker-snapshot", "account_id":"paper_autonomous_demo", "mode":"paper",
        "spendable_cash":{"amount":"50","currency":"EUR"}, "positions":[],"orders":[],
        "observed_at":chrono::Utc::now()
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(["autonomous", "analyst-worker"])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("analyst child");
    let mut input = child.stdin.take().expect("child stdin");
    writeln!(input, "{boot}").expect("boot handoff");
    writeln!(input, "{snapshot}").expect("snapshot handoff");
    drop(input);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().expect("child status").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("analyst child did not finish within the test deadline");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().expect("child output");
    assert!(output.status.success(), "analyst child failed");
    let first = output
        .stdout
        .split(|b| *b == b'\n')
        .next()
        .expect("worker reply");
    let reply: Value = serde_json::from_slice(first).expect("worker reply JSON");
    assert_eq!(reply["snapshot_id"], "isolated-worker-snapshot");
    assert_eq!(reply["failed"], true);
    assert!(reply["candidate"].is_null());
    assert!(output.stderr.is_empty());
}

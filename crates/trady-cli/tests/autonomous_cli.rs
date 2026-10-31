//! The real analyst child uses only its scoped PostgreSQL role and pipe handoff.
use serde_json::{Value, json};
use std::{
    env,
    io::Write,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

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

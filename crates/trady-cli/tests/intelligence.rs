#[path = "../../trady-adapters/tests/intelligence_support/mod.rs"]
mod intelligence_support;

use chrono::Utc;
use intelligence_support::*;
use serde_json::{Value, json};
use std::{
    fs,
    process::{Command, Output},
};
use trady_domain::intelligence::FeatureKind;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .env_remove("TRADY_ANALYST_DSN")
        .output()
        .unwrap()
}
#[test]
fn intelligence_cli_enables_all_ten_by_default_without_database_or_source_grants() {
    let result = run(&["intelligence", "status"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["features"].as_array().unwrap().len(), 10);
    assert!(
        result["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["enabled"] == true && f["authority"] == "RESEARCH_ONLY_UNCALIBRATED")
    );
    assert!(result["sources"].as_array().unwrap().is_empty());
    assert_eq!(result["entry_authorized"], false);
}

#[test]
fn intelligence_cli_default_source_gates_and_explicit_opt_out_remain_effective() {
    let directory = std::env::temp_dir().join(format!(
        "trady-intelligence-cli-defaults-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let input_path = directory.join("observation.json");
    let at = Utc::now();
    for feature in FeatureKind::ALL {
        let (_, mut observation, _) = envelope_at(input(feature), at);
        observation.scope.owner_id = "paper_owner".into();
        observation.scope.account_id = "paper_demo".into();
        fs::write(&input_path, serde_json::to_vec(&observation).unwrap()).unwrap();
        let output = run(&[
            "intelligence",
            "assess",
            "--input",
            input_path.to_str().unwrap(),
        ]);
        assert!(output.status.success(), "{feature:?}: {:?}", output.stderr);
        let record: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(record["status"], "UNAVAILABLE", "{feature:?}");
        assert!(record["result"].is_null());
        assert_eq!(record["entry_authorized"], false);
        assert_eq!(
            record["reasons"],
            json!(["source_rights_mapping_or_input_unavailable"])
        );
    }
    let config_path = directory.join("disabled.json");
    fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "schema_version": 1,
            "scope": {"owner_id": "paper_owner", "account_id": "paper_demo"},
            "enabled": []
        }))
        .unwrap(),
    )
    .unwrap();
    let output = run(&[
        "intelligence",
        "status",
        "--config",
        config_path.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        status["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|feature| feature["enabled"] == false)
    );
    fs::remove_dir_all(directory).unwrap();
}
#[test]
fn intelligence_cli_assessment_and_replay_use_real_typed_service_and_preserve_unavailable() {
    let directory =
        std::env::temp_dir().join(format!("trady-intelligence-cli-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let at = Utc::now();
    let (config, observation, _) = envelope_at(input(FeatureKind::Accounting), at);
    let c = directory.join("config.json");
    let o = directory.join("observation.json");
    let replay = directory.join("replay.json");
    fs::write(&c, serde_json::to_vec(&config).unwrap()).unwrap();
    fs::write(&o, serde_json::to_vec(&observation).unwrap()).unwrap();
    let args = [
        "intelligence",
        "assess",
        "--config",
        c.to_str().unwrap(),
        "--input",
        o.to_str().unwrap(),
    ];
    let result = run(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["result"]["feature"], "ACCOUNTING");
    assert_eq!(value["entry_authorized"], false);
    assert!(
        value["result"]["assessment"]["signals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| !s["value"].is_null())
    );
    fs::write(&replay,serde_json::to_vec(&json!({"schema_version":1,"events":[{"at":at,"observation":observation}],
        "queries":[{"at":at-chrono::Duration::seconds(1),"feature":"ACCOUNTING","entity_id":observation.entity_id},
        {"at":at,"feature":"ACCOUNTING","entity_id":observation.entity_id}]})).unwrap()).unwrap();
    let result = run(&[
        "intelligence",
        "replay",
        "--config",
        c.to_str().unwrap(),
        "--input",
        replay.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["accepted"], 1);
    assert!(value["assessments"][0]["result"].is_null());
    assert_eq!(value["assessments"][1]["result"]["feature"], "ACCOUNTING");
    let mut collision = observation.clone();
    collision.evidence[0].complete = false;
    let mut missing = observation.clone();
    missing.observation_id = "missing_predecessor_correction".into();
    missing.supersedes = Some("missing_predecessor".into());
    fs::write(&replay,serde_json::to_vec(&json!({"schema_version":1,"events":[
        {"at":at,"observation":observation},{"at":at,"observation":collision},{"at":at,"observation":missing}],
        "queries":[{"at":at-chrono::Duration::seconds(1),"feature":"ACCOUNTING","entity_id":observation.entity_id}]})).unwrap()).unwrap();
    let result = run(&[
        "intelligence",
        "replay",
        "--config",
        c.to_str().unwrap(),
        "--input",
        replay.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["processed"], 3);
    assert_eq!(value["accepted"], 1);
    assert_eq!(value["rejected"], 2);
    assert_eq!(value["rejected_events"][0]["event_index"], 1);
    assert_eq!(value["rejected_events"][1]["event_index"], 2);
    assert!(
        !run(&[
            "intelligence",
            "assess",
            "--input",
            o.to_str().unwrap(),
            "--input",
            o.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert!(
        !run(&["intelligence", "status", "--entity", "unexpected"])
            .status
            .success()
    );
    fs::remove_dir_all(directory).unwrap();
}

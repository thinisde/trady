use serde_json::Value;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .env_remove("TRADY_ANALYST_DSN")
        .env_remove("TRADY_X_BEARER_TOKEN")
        .env_remove("TRADY_ALPHA_VANTAGE_API_KEY")
        .output()
        .unwrap()
}

#[test]
fn news_status_exposes_disabled_five_account_seeds_without_provider_access() {
    let output = run(&["news", "status"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["entry_authorized"], false);
    assert_eq!(value["provider_access_verified"], false);
    let sources = value["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 12);
    let accounts: Vec<_> = sources
        .iter()
        .filter(|s| s["kind"] == "X_TIMELINE")
        .collect();
    assert_eq!(accounts.len(), 5);
    assert!(
        accounts
            .iter()
            .all(|s| s["enabled"] == false && s["account"]["user_id"].is_null())
    );
    let output = run(&["news", "status", "--source", "x_gill"]);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["sources"].as_array().unwrap().len(), 1);
    assert_eq!(value["sources"][0]["account"]["handle"], "TheRoaringKitty");
}

#[test]
fn offline_feed_reading_uses_the_same_provenance_and_rights_rules() {
    let path = format!("{}/tests/fixtures/news.rss", env!("CARGO_MANIFEST_DIR"));
    let output = run(&["news", "parse-feed", &path, "--source", "google_market"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["stored"], false);
    assert_eq!(value["untrusted_source_data"], true);
    let item = &value["items"][0];
    assert_eq!(item["publisher"], "Fixture publisher");
    assert_eq!(item["canonical_url"], "https://example.com/release");
    assert_eq!(item["content_status"], "METADATA_ONLY");
    assert!(item["title"].is_null());
    assert!(item["summary"].is_null());
    assert!(item["event_at"].is_null());
}

#[test]
fn news_input_errors_and_missing_authorization_fail_before_network_calls() {
    for args in [
        vec!["news", "collect"],
        vec!["news", "read", "--limit", "101"],
        vec!["news", "status", "--source", "unknown"],
        vec!["news", "status", "--force"],
        vec!["news", "status", "--source", "x_musk", "--source", "x_gill"],
        vec![
            "news",
            "x",
            "https://x.com/user/status/123",
            "--source",
            "x_lookup",
        ],
        vec!["news", "read"],
    ] {
        let output = run(&args);
        assert!(!output.status.success(), "unexpected success for {args:?}");
    }
    let output = run(&["--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("news collect --once"));
}

#[test]
fn pilot_status_is_offline_disabled_and_exposes_cost_and_coverage_policy() {
    let path = format!(
        "{}/../../config/x-pilot.example.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = run(&["news", "status", "--config", &path]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["x_budget"]["ceiling_microusd"], 10_000_000);
    assert_eq!(value["x_budget"]["enabled"], false);
    let sources = value["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 5);
    assert!(
        sources
            .iter()
            .all(|s| s["enabled"] == false && s["delivery"] == "INCREMENTAL_RECENT_SEARCH")
    );
    let disabled = run(&[
        "news",
        "counts-x",
        "--source",
        "x_pilot_ackman",
        "--config",
        &path,
    ]);
    assert!(disabled.status.success());
    let value: Value = serde_json::from_slice(&disabled.stdout).unwrap();
    assert_eq!(value["status"], "DISABLED");
    for args in [
        vec!["news", "export-x", "--as-of", "invalid"],
        vec!["news", "status", "--as-of", "2026-10-31T12:00:00Z"],
        vec!["news", "export-x", "--for-model"],
        vec!["news", "budget-x"],
    ] {
        assert!(!run(&args).status.success());
    }
}

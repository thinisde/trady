use super::*;

fn config() -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../../config/telegram.example.json")).unwrap();
    value["access"]["allowed_user_ids"] = json!([12345]);
    value["access"]["allowed_chat_ids"] = json!([12345]);
    value
}

#[test]
fn configuration_requires_explicit_private_ids_and_rejects_unknown_authority_or_endpoints() {
    assert!(
        serde_json::from_str::<Config>(include_str!("../../../../config/telegram.example.json"))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<Config>(config())
            .unwrap()
            .validate()
            .is_ok()
    );
    for (key, value) in [
        ("bot_token", json!("private")),
        ("base_url", json!("https://evil.invalid")),
        ("can_trade", json!(true)),
    ] {
        let mut invalid = config();
        invalid[key] = value;
        assert!(serde_json::from_value::<Config>(invalid).is_err());
    }
    for (key, value) in [
        ("config_version", json!(2)),
        ("poll_timeout_seconds", json!(0)),
        ("refresh_interval_seconds", json!(1)),
        ("stale_after_seconds", json!(30)),
    ] {
        let mut invalid = config();
        invalid[key] = value;
        assert!(
            serde_json::from_value::<Config>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn invalid_arguments_fail_before_requesting_credentials_or_network_access() {
    for args in [
        vec![],
        vec!["run"],
        vec!["discover", "--once"],
        vec!["run", "--config"],
        vec!["check", "--config", "somewhere", "--once"],
        vec!["run", "--config", "somewhere", "--force"],
    ] {
        assert!(run(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
    }
}

fn expanded_config() -> Value {
    let mut value = config();
    value["expanded_gateway"] = json!({"enabled":true,"bot_id":98765,"owners":[{"telegram_user_id":12345,"owner_id":"owner_fixture"}],
        "research_config":"research.json","finbert_config":null,"llm_config":null,"skill_approval":null,"skills_root":null,
        "approved_instrument_ids":["stock_fixture"],"daily_job_limit":10,"pending_job_limit":5,
        "daily_research_enabled":false,"daily_research_hour_utc":7,"holding_watchdog_interval_seconds":300});
    value
}

#[test]
fn expanded_gateway_validates_private_owner_mapping_and_rejects_credential_authority_fields() {
    let cfg: Config = serde_json::from_value(expanded_config()).unwrap();
    assert!(cfg.validate().is_ok());
    for key in ["executor_dsn", "analyst_dsn", "trading_secret", "base_url"] {
        let mut value = expanded_config();
        value["expanded_gateway"][key] = json!("not-a-real-secret");
        assert!(serde_json::from_value::<Config>(value).is_err());
    }
    let mut value = expanded_config();
    value["expanded_gateway"]["owners"][0]["telegram_user_id"] = json!(999);
    assert!(
        serde_json::from_value::<Config>(value)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn expanded_gateway_check_supports_all_information_modes_and_does_not_open_broker_configuration() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let scratch = std::env::temp_dir().join(format!(
        "trady-research-jobs-cli-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&scratch).unwrap();
    let mut cfg = expanded_config();
    cfg["foundation_config"] = json!("foundation.json");
    cfg["trading212_config"] = json!("this-broker-file-must-not-be-read.json");
    std::fs::write(
        scratch.join("gateway.json"),
        serde_json::to_vec(&cfg).unwrap(),
    )
    .unwrap();
    std::fs::write(
        scratch.join("research.json"),
        include_str!("../../../../config/research.json"),
    )
    .unwrap();
    for mode in ["paper", "shadow", "live"] {
        let mut foundation: Value =
            serde_json::from_str(include_str!("../../../../config/paper.json")).unwrap();
        foundation["mode"] = json!(mode);
        foundation["account_id"] = json!(format!("{mode}_fixture_information"));
        std::fs::write(
            scratch.join("foundation.json"),
            serde_json::to_vec(&foundation).unwrap(),
        )
        .unwrap();
        let result = run(&[
            "check".into(),
            "--config".into(),
            scratch.join("gateway.json").to_str().unwrap().into(),
        ])
        .unwrap();
        assert_eq!(result["deployment_mode"], mode);
        assert_eq!(result["broker_monitoring"], false);
        assert_eq!(result["network_access_verified"], false);
    }
    std::fs::remove_dir_all(scratch).unwrap();
}

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

use std::{fs, process::Command};

#[test]
fn financial_status_is_offline_disabled_and_resolves_only_central_secret_metadata() {
    let directory = std::env::temp_dir().join(format!(
        "trady-financial-credentials-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("credentials.json");
    let mut template: serde_json::Value =
        serde_json::from_str(include_str!("../../../config/credentials.example.json")).unwrap();
    template["credentials"]["TRADY_FRED_API_KEY"] =
        serde_json::json!("TEST_ONLY_PRIVATE_FRED_VALUE");
    fs::write(&path, serde_json::to_vec(&template).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_trady"))
            .args(args)
            .env("TRADY_CREDENTIALS_FILE", &path)
            .env_remove("TRADY_FRED_API_KEY")
            .env_remove("TRADY_ANALYST_DSN")
            .output()
            .unwrap()
    };
    let result = run(&["financial", "status"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(!text.contains("TEST_ONLY_PRIVATE_FRED_VALUE"));
    let status: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(status["entry_authorized"], false);
    let sources = status["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 9);
    assert!(
        sources
            .iter()
            .all(|s| s["enabled"] == false && s["collection_ready"] == false)
    );
    assert_eq!(
        sources.iter().find(|s| s["provider"] == "FRED").unwrap()["credential_configured"],
        true
    );
    let result = run(&["credentials", "status"]);
    let status: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    for name in [
        "TRADY_SEC_USER_AGENT",
        "TRADY_FRED_API_KEY",
        "TRADY_TWELVE_DATA_API_KEY",
        "TRADY_FINNHUB_API_KEY",
        "TRADY_FMP_API_KEY",
        "TRADY_COINGECKO_API_KEY",
        "TRADY_TIINGO_API_KEY",
        "TRADY_MASSIVE_API_KEY",
    ] {
        assert!(
            status["credentials"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["name"] == name)
        );
    }
    assert!(
        !run(&["financial", "status", "--source", "unknown"])
            .status
            .success()
    );
    assert!(!run(&["financial", "collect"]).status.success());
    assert!(
        !run(&["financial", "status", "--as-of", "2026-10-01T00:00:00Z"])
            .status
            .success()
    );
    fs::remove_dir_all(directory).unwrap();
}

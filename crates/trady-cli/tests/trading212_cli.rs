use std::{
    fs,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

fn run(args: &[&str]) -> Output {
    let directory = std::env::temp_dir().join(format!("trady-t212-cli-{}", uuid::Uuid::new_v4()));
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory).unwrap();
    let credentials = directory.join("credentials.json");
    fs::write(&credentials, r#"{"config_version":1,"credentials":{}}"#).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&credentials, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .current_dir(&directory)
        .env_clear()
        .env("TRADY_CREDENTIALS_FILE", credentials)
        .output()
        .unwrap();
    fs::remove_dir_all(directory).unwrap();
    output
}

#[test]
fn snapshot_requires_private_environment_credentials_and_documents_command() {
    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("t212 snapshot --config FILE")
    );
    for (path, key_name) in [
        (
            "config/trading212.demo.example.json",
            "TRADY_T212_DEMO_API_KEY",
        ),
        (
            "config/trading212.live.example.json",
            "TRADY_T212_LIVE_API_KEY",
        ),
    ] {
        let absolute = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path);
        let result = run(&["t212", "snapshot", "--config", absolute.to_str().unwrap()]);
        assert!(!result.status.success());
        assert!(String::from_utf8(result.stderr).unwrap().contains(key_name));
    }
}

#[test]
fn snapshot_rejects_endpoint_injection_and_no_cli_mutation_is_exposed() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "trady-t212-config-{}-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut config: serde_json::Value =
        serde_json::from_str(include_str!("../../../config/trading212.live.example.json")).unwrap();
    config["base_url"] = serde_json::json!("http://127.0.0.1:1/credential-capture");
    fs::write(&path, config.to_string()).unwrap();
    let result = run(&["t212", "snapshot", "--config", path.to_str().unwrap()]);
    fs::remove_file(path).unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("unsupported Trading 212 configuration")
    );
    for command in ["buy", "sell", "cancel", "submit"] {
        assert!(!run(&["t212", command]).status.success());
    }
}

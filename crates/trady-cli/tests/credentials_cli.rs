use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "trady-credentials-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join("config")).unwrap();
        Self(path)
    }

    fn path(&self) -> PathBuf {
        self.0.join("config/credentials.local.json")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_trady"));
        command.args(args).current_dir(&self.0).env_clear();
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn write(&self, credentials: Value) {
        let result = self.run(&["credentials", "init"]);
        assert!(result.status.success());
        fs::write(
            self.path(),
            json!({"config_version":1,"credentials":credentials}).to_string(),
        )
        .unwrap();
    }

    fn telegram_config(&self, ids: &[i64]) -> PathBuf {
        let mut config: Value =
            serde_json::from_str(include_str!("../../../config/telegram.example.json")).unwrap();
        config["access"] = json!({"allowed_user_ids": ids, "allowed_chat_ids": ids});
        fs::write(
            self.0.join("paper.json"),
            include_bytes!("../../../config/paper.json"),
        )
        .unwrap();
        let path = self.0.join("telegram.json");
        fs::write(&path, config.to_string()).unwrap();
        path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn entry<'a>(status: &'a Value, name: &str) -> &'a Value {
    status["credentials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == name)
        .unwrap()
}

#[test]
fn initialization_is_private_blank_and_never_overwrites() {
    let workspace = Workspace::new();
    success(workspace.run(&["credentials", "init"]));
    let content = fs::read_to_string(workspace.path()).unwrap();
    let config: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(config["config_version"], 1);
    assert!(
        config["credentials"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v == "")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(workspace.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let status = success(workspace.run(&["credentials", "status"]));
    assert_eq!(
        config["credentials"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|name| name.starts_with("TRADY_T212_"))
            .count(),
        4
    );
    assert_eq!(
        status["credentials"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["name"].as_str().unwrap().starts_with("TRADY_T212_"))
            .count(),
        4
    );
    for name in [
        "TRADY_T212_DEMO_API_KEY",
        "TRADY_T212_DEMO_API_SECRET",
        "TRADY_T212_LIVE_API_KEY",
        "TRADY_T212_LIVE_API_SECRET",
    ] {
        assert_eq!(entry(&status, name)["configured"], false);
    }
    assert_eq!(
        entry(&status, "TRADY_HASHING_SECRET_KEY")["configured"],
        false
    );
    assert_eq!(
        entry(&status, "TRADY_SIGNING_SECRET_KEY")["configured"],
        false
    );
    assert!(!workspace.run(&["credentials", "init"]).status.success());
    assert_eq!(fs::read_to_string(workspace.path()).unwrap(), content);
    assert!(
        !workspace
            .run(&["credentials", "get", "OPENCODE_API_KEY"])
            .status
            .success()
    );
}

#[test]
fn central_telegram_whitelist_selects_one_private_owner_before_access_validation() {
    let workspace = Workspace::new();
    workspace.write(json!({"TRADY_TELEGRAM_WHITELIST_USER_ID": "54321"}));
    for ids in [vec![], vec![12345, 54321]] {
        let config = workspace.telegram_config(&ids);
        let output = workspace.run(&["telegram", "check", "--config", config.to_str().unwrap()]);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("54321"));
        let status = success(output);
        assert_eq!(status["status"], "configuration_valid");
        assert_eq!(status["owner_count"], 1);
        assert_eq!(status["chat_count"], 1);
        assert_eq!(status["network_access_verified"], false);
        assert!(!workspace.0.join("var").exists());
    }
    let status = success(workspace.run(&["credentials", "status"]));
    assert_eq!(
        entry(&status, "TRADY_TELEGRAM_WHITELIST_USER_ID")["source"],
        "file"
    );
}

#[test]
fn telegram_whitelist_environment_wins_and_blank_disables_the_file_entry() {
    let workspace = Workspace::new();
    workspace.write(json!({"TRADY_TELEGRAM_WHITELIST_USER_ID": "invalid-file-whitelist"}));
    let config = workspace.telegram_config(&[]);
    let args = ["telegram", "check", "--config", config.to_str().unwrap()];
    let status = success(
        workspace
            .command(&args)
            .env("TRADY_TELEGRAM_WHITELIST_USER_ID", "98765")
            .output()
            .unwrap(),
    );
    assert_eq!(status["owner_count"], 1);
    assert_eq!(status["chat_count"], 1);
    let output = workspace
        .command(&args)
        .env("TRADY_TELEGRAM_WHITELIST_USER_ID", "")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("allowlists"));
    let config = workspace.telegram_config(&[12345, 54321]);
    let status = success(
        workspace
            .command(&["telegram", "check", "--config", config.to_str().unwrap()])
            .env("TRADY_TELEGRAM_WHITELIST_USER_ID", "")
            .output()
            .unwrap(),
    );
    assert_eq!(status["owner_count"], 2);
    assert_eq!(status["chat_count"], 2);
}

#[test]
fn malformed_telegram_whitelist_fails_without_echoing_values_or_using_config_access() {
    let workspace = Workspace::new();
    workspace.write(json!({"TRADY_TELEGRAM_WHITELIST_USER_ID": "invalid-file-whitelist"}));
    let config = workspace.telegram_config(&[12345]);
    let args = ["telegram", "check", "--config", config.to_str().unwrap()];
    let output = workspace.run(&args);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("TRADY_TELEGRAM_WHITELIST_USER_ID"));
    assert!(!error.contains("invalid-file-whitelist"));
    for invalid in [
        "0",
        "-7",
        "4503599627370496",
        "9223372036854775808",
        "12,34",
        "12x345",
        " 12345 ",
        "+12345",
    ] {
        let output = workspace
            .command(&args)
            .env("TRADY_TELEGRAM_WHITELIST_USER_ID", invalid)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("TRADY_TELEGRAM_WHITELIST_USER_ID"));
        assert!(!error.contains(invalid));
    }
}

#[test]
fn environment_overrides_file_values_and_status_contains_no_values() {
    let workspace = Workspace::new();
    workspace.write(json!({
        "TRADY_HASHING_SECRET_KEY":"file-hashing-value",
        "TRADY_TEST_CUSTOM_KEY":"custom-private-value"
    }));
    let status = success(workspace.run(&["credentials", "status"]));
    assert_eq!(entry(&status, "TRADY_HASHING_SECRET_KEY")["source"], "file");
    assert_eq!(entry(&status, "TRADY_TEST_CUSTOM_KEY")["source"], "file");
    let result = workspace
        .command(&["credentials", "status"])
        .env("TRADY_HASHING_SECRET_KEY", "environment-hashing-value")
        .output()
        .unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    for secret in [
        "file-hashing-value",
        "custom-private-value",
        "environment-hashing-value",
    ] {
        assert!(!output.contains(secret));
    }
    let status: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        entry(&status, "TRADY_HASHING_SECRET_KEY")["source"],
        "environment"
    );
    let status = success(
        workspace
            .command(&["credentials", "status"])
            .env("TRADY_HASHING_SECRET_KEY", "")
            .output()
            .unwrap(),
    );
    assert_eq!(
        entry(&status, "TRADY_HASHING_SECRET_KEY")["configured"],
        false
    );
}

#[test]
fn explicit_paths_are_required_and_environment_only_deployments_still_work() {
    let workspace = Workspace::new();
    let status = success(
        workspace
            .command(&["credentials", "status"])
            .env("OPENCODE_API_KEY", "environment-only-value")
            .output()
            .unwrap(),
    );
    assert_eq!(entry(&status, "OPENCODE_API_KEY")["source"], "environment");
    assert!(
        !workspace
            .command(&["credentials", "status"])
            .env("TRADY_CREDENTIALS_FILE", workspace.0.join("missing.json"))
            .output()
            .unwrap()
            .status
            .success()
    );
    let custom_path = workspace.0.join("private.json");
    success(
        workspace
            .command(&["credentials", "init"])
            .env("TRADY_CREDENTIALS_FILE", &custom_path)
            .output()
            .unwrap(),
    );
    assert!(!workspace.path().exists());
    success(
        workspace
            .command(&["credentials", "status"])
            .env("TRADY_CREDENTIALS_FILE", custom_path)
            .output()
            .unwrap(),
    );
}

#[test]
fn broker_cli_reads_file_credentials_and_respects_empty_environment_override() {
    let workspace = Workspace::new();
    workspace.write(json!({
        "TRADY_T212_DEMO_API_KEY":"demo-file-api-key-value",
        "TRADY_T212_LIVE_API_KEY":"live-file-api-key-value"
    }));
    for mode in ["DEMO", "LIVE"] {
        let key_name = format!("TRADY_T212_{mode}_API_KEY");
        let secret_name = format!("TRADY_T212_{mode}_API_SECRET");
        let config = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../config/trading212.{}.example.json",
            mode.to_lowercase()
        ));
        let args = ["t212", "snapshot", "--config", config.to_str().unwrap()];
        // A missing paired secret stops before a request, after resolving the selected key from disk.
        let result = workspace.run(&args);
        assert!(!result.status.success());
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains(&secret_name));
        assert!(!error.contains("file-api-key-value"));
        let result = workspace
            .command(&args)
            .env(&key_name, "")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8(result.stderr)
                .unwrap()
                .contains(&key_name)
        );
    }
}

#[test]
fn broker_modes_require_their_own_pair_without_other_mode_fallback() {
    for (selected, other) in [("DEMO", "LIVE"), ("LIVE", "DEMO")] {
        let workspace = Workspace::new();
        let key_name = format!("TRADY_T212_{selected}_API_KEY");
        let secret_name = format!("TRADY_T212_{selected}_API_SECRET");
        let other_key = format!("TRADY_T212_{other}_API_KEY");
        let other_secret = format!("TRADY_T212_{other}_API_SECRET");
        workspace.write(json!({
            (other_key.as_str()):"other-mode-key-fixture",
            (other_secret.as_str()):"other-mode-secret-fixture"
        }));
        let config = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../config/trading212.{}.example.json",
            selected.to_lowercase()
        ));
        let args = ["t212", "snapshot", "--config", config.to_str().unwrap()];
        let result = workspace.run(&args);
        assert!(!result.status.success());
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains(&key_name));
        assert!(!error.contains("fixture"));
        // Supplying only the selected key must still require the secret from that same mode.
        let result = workspace
            .command(&args)
            .env(&key_name, "selected-key-fixture")
            .env(&other_secret, "opposite-environment-secret-fixture")
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains(&secret_name));
        assert!(!error.contains("fixture"));
    }
}

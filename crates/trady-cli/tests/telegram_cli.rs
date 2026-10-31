use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "trady-telegram-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("paper.json"),
            include_bytes!("../../../config/paper.json"),
        )
        .unwrap();
        let mut config: Value =
            serde_json::from_str(include_str!("../../../config/telegram.example.json")).unwrap();
        config["access"]["allowed_user_ids"] = json!([12345]);
        config["access"]["allowed_chat_ids"] = json!([12345]);
        fs::write(directory.join("telegram.json"), config.to_string()).unwrap();
        Self(directory)
    }

    fn config(&self) -> PathBuf {
        self.0.join("telegram.json")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .current_dir(cwd)
        .env_remove("TRADY_TELEGRAM_BOT_TOKEN")
        .env("TRADY_TELEGRAM_WHITELIST_USER_ID", "")
        .env("TRADY_T212_DEMO_API_KEY", "")
        .env("TRADY_T212_DEMO_API_SECRET", "")
        .env("TRADY_T212_LIVE_API_KEY", "")
        .env("TRADY_T212_LIVE_API_SECRET", "")
        .output()
        .unwrap()
}

#[test]
fn telegram_offline_check_resolves_files_relative_to_the_selected_config() {
    let fixture = Fixture::new();
    let config = fixture.config();
    let result = command(
        &["telegram", "check", "--config", config.to_str().unwrap()],
        &std::env::temp_dir(),
    );
    assert!(result.status.success(), "{:?}", result.stderr);
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["status"], "configuration_valid");
    assert_eq!(value["broker_monitoring"], false);
    assert_eq!(value["network_access_verified"], false);
    assert_eq!(value["entry_authorized"], false);
    assert!(!fixture.0.join("state.json").exists());
}

#[test]
fn telegram_runtime_and_discovery_require_environment_credentials_and_are_in_help() {
    let fixture = Fixture::new();
    let config = fixture.config();
    for args in [
        vec!["telegram", "discover"],
        vec![
            "telegram",
            "run",
            "--config",
            config.to_str().unwrap(),
            "--once",
        ],
    ] {
        let result = command(&args, &fixture.0);
        assert!(!result.status.success());
        assert!(
            String::from_utf8(result.stderr)
                .unwrap()
                .contains("TRADY_TELEGRAM_BOT_TOKEN")
        );
    }
    let help = command(&["--help"], &fixture.0);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("telegram run --config FILE [--once]"));
    assert!(text.contains("telegram discover"));
    assert!(text.contains("news status"));
    assert!(text.contains("chat"));
}

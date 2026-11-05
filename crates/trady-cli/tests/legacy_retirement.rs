//! Retired entrypoints must fail before config, credentials or provider access.
use std::{path::Path, process::Command};

fn rejected(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .env_clear()
        .env(
            "TRADY_CREDENTIALS_FILE",
            "/missing-retired-credentials.json",
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn retired_commands_cannot_reach_configuration_or_provider_construction() {
    for command in ["account", "alpaca", "watchdog", "demo-entry"] {
        assert!(
            rejected(&[command, "check", "--config", "/missing-legacy-runtime.json"])
                .starts_with("trady: unsupported arguments"),
            "{command}"
        );
    }
    for action in [
        "analyst-service",
        "holding",
        "review-holding",
        "recover-order",
    ] {
        assert!(
            rejected(&[
                "autonomous",
                action,
                "--config",
                "/missing-legacy-runtime.json"
            ])
            .contains("legacy execution commands were retired"),
            "{action}"
        );
    }
}

#[test]
fn legacy_configuration_cannot_select_an_execution_runtime() {
    for file in [
        "config/autonomous.example.json",
        "config/autonomous.demo-no-submit.json",
        "config/autonomous.iex30.demo.json",
    ] {
        for action in ["check", "run"] {
            assert!(
                rejected(&["autonomous", action, "--config", file])
                    .contains("only EUR runtime configurations are supported"),
                "{file} {action}"
            );
        }
    }
}

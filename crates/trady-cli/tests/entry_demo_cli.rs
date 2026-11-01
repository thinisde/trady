use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "trady-simulated-entry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .current_dir(root)
        .env_clear()
        .env(
            "TRADY_CREDENTIALS_FILE",
            root.join("must-not-be-loaded.json"),
        )
        .output()
        .unwrap()
}

fn decimal(value: &Value) -> Decimal {
    Decimal::from_str_exact(value.as_str().unwrap()).unwrap()
}

#[test]
fn simulated_entry_calculates_records_and_suppresses_submission_without_credentials() {
    let root = Directory::new();
    let target = root.0.join("run");
    let args = ["demo-entry", "--directory", target.to_str().unwrap()];
    let output = command(&root.0, &args);
    assert!(output.status.success(), "{:?}", output.stderr);
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["classification"], "SIMULATION_ONLY");
    assert_eq!(result["status"], "simulated_entry_recorded");
    assert_eq!(result["source"]["kind"], "FIXTURE");
    assert_eq!(result["entry_authorized"], false);
    assert_eq!(result["production_approved"], false);
    assert_eq!(result["simulated_entry_generated"], true);
    assert_eq!(result["submission_result"], "SUPPRESSED_BY_SHADOW_BROKER");
    for key in [
        "broker_order_api_calls",
        "order_mutations",
        "network_clients_constructed",
    ] {
        assert_eq!(result[key], 0);
    }
    assert_eq!(result["input"]["account"], result["after_snapshot"]);
    assert_eq!(result["request"]["side"], json!(trady_domain::Side::Buy));
    assert_eq!(result["request"]["account_id"], "shadow_simulated_entry");
    assert_eq!(result["request"]["instrument_id"], "SIMULATED_STOCK");
    let calculation = &result["calculation"];
    let quantity = decimal(&result["request"]["quantity"]);
    let price = decimal(&result["request"]["kind"]["Limit"]["price"]);
    assert!(quantity > Decimal::ZERO);
    assert_eq!(quantity, decimal(&calculation["quantity"]));
    assert_eq!(price, decimal(&calculation["buy_limit"]));
    assert!(quantity * price <= Decimal::from(4900));
    assert!(decimal(&calculation["stop_trigger"]) < price);
    assert!(price < decimal(&calculation["target_sell_limit"]));
    assert!(decimal(&calculation["account_risk"]["amount"]) <= Decimal::from(50));
    let journal = PathBuf::from(result["recording_directory"].as_str().unwrap());
    assert_eq!(journal, target.join("simulation-only"));
    let path = PathBuf::from(result["proposal_record"].as_str().unwrap());
    let record: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(record["source"]["kind"], "FIXTURE");
    assert!(
        record["source"]["description"]
            .as_str()
            .unwrap()
            .starts_with("SIMULATION_ONLY")
    );
    assert_eq!(record["request"], result["request"]);
    assert_eq!(record["order_mutations"], 0);
    assert_eq!(
        trady_application::optimization::digest(&record).unwrap(),
        result["intent_hash"]
    );
    assert_eq!(fs::read_dir(&journal).unwrap().count(), 2);
    let rerun = command(&root.0, &args);
    assert!(rerun.status.success());
    let rerun: Value = serde_json::from_slice(&rerun.stdout).unwrap();
    assert_eq!(rerun["intent_hash"], result["intent_hash"]);
    assert_eq!(rerun["request"], result["request"]);
    assert_eq!(fs::read_dir(&journal).unwrap().count(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&journal).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for item in fs::read_dir(&journal).unwrap() {
            assert_eq!(
                item.unwrap().metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn simulation_rejects_runtime_or_broker_options_and_relative_output_paths() {
    let root = Directory::new();
    for args in [
        vec!["demo-entry"],
        vec!["demo-entry", "--directory", "relative"],
        vec!["demo-entry", "--config", "runtime.json"],
        vec![
            "demo-entry",
            "--directory",
            root.0.to_str().unwrap(),
            "--live",
        ],
    ] {
        assert!(!command(&root.0, &args).status.success());
    }
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    let help = command(&root.0, &["--help"]);
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("demo-entry --directory ABSOLUTE_DIRECTORY")
    );
}

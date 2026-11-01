use chrono::{Duration, Utc};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, time::Instant};
use trady_adapters::weekly_lab::runner::*;
use trady_adapters::weekly_lab::{HelperLab, LabConfig};
use trady_application::optimization::*;
use trady_domain::{AccountId, Mode};

fn fixture(script: &str, timeout: u32, max_output: usize) -> (PathBuf, HelperCommand) {
    let directory = std::env::temp_dir().join(format!(
        "trady-weekly-helper-test-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir(&directory).unwrap();
    let script_path = directory.join("helper.py");
    fs::write(&script_path, script).unwrap();
    let python = fs::canonicalize("/usr/bin/python3").unwrap();
    let pin = |path: PathBuf| PinnedFile {
        sha256: bytes_hash(&fs::read(&path).unwrap()),
        path,
    };
    let command = HelperCommand {
        tool_id: "isolated_fixture".into(),
        executable: pin(python),
        arguments: vec![script_path.to_string_lossy().into_owned()],
        dependencies: vec![pin(script_path)],
        working_directory: directory.clone(),
        artifact_directory: directory.join("artifacts"),
        timeout_seconds: timeout,
        cpu_seconds: 2,
        memory_bytes: 256 * 1024 * 1024,
        max_output_bytes: max_output,
    };
    (directory, command)
}
#[test]
fn weekly_helper_deadline_kills_descendants_and_reaps_pipes() {
    let (dir, command) = fixture(
        "import subprocess,time\nsubprocess.Popen(['/bin/sleep','30'])\ntime.sleep(30)\n",
        1,
        4096,
    );
    let start = Instant::now();
    let result = command.run(b"{}", &"a".repeat(64));
    assert!(result.err().unwrap().0.contains("DEADLINE"));
    assert!(start.elapsed().as_secs() < 4);
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn weekly_helper_output_and_input_are_bounded_without_secret_environment() {
    let (dir, mut command) = fixture(
        "import os,json\nprint(json.dumps(sorted(os.environ)))\n",
        2,
        4096,
    );
    let output = command.run(b"{}", &"a".repeat(64)).unwrap();
    let keys: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        keys.iter()
            .all(|k| matches!(k.as_str(), "PATH" | "LANG" | "LC_CTYPE"))
    );
    assert!(
        command
            .run(&vec![0; MAX_INPUT_BYTES + 1], &"a".repeat(64))
            .is_err()
    );
    let script = dir.join("helper.py");
    fs::write(
        &script,
        "import sys\nwhile True: sys.stdout.write('x'*10000);sys.stdout.flush()\n",
    )
    .unwrap();
    command.dependencies[0].sha256 = bytes_hash(&fs::read(&script).unwrap());
    command.max_output_bytes = 128;
    assert!(
        command
            .run(b"{}", &"a".repeat(64))
            .err()
            .unwrap()
            .0
            .contains("OUTPUT_LIMIT")
    );
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn weekly_helper_tampering_shell_eval_and_misbound_reports_fail_closed() {
    let (dir, mut command) = fixture("print('{}')", 1, 1024);
    fs::write(dir.join("helper.py"), "print('tampered')").unwrap();
    assert!(command.validate().is_err());
    command.arguments = vec!["-c".into(), "echo stolen".into()];
    assert!(command.validate().is_err());
    let result = HelperOutput {
        stdout: br#"{"schema_version":1,"input_hash":"wrong","payload":{}}"#.to_vec(),
        stderr_hash: "a".repeat(64),
        report: ToolReport {
            report_id: "fixture".into(),
            tool_id: "fixture".into(),
            input_hash: "a".repeat(64),
            output_hash: "b".repeat(64),
            exit_status: 0,
            duration_ms: 1,
            recorded_at: Utc::now(),
        },
    };
    assert!(result.payload::<Value>(&"a".repeat(64)).is_err());
    fs::remove_dir_all(dir).unwrap();
}

fn weekly_inputs() -> (RunManifest, ExperimentPlan, TrialRecord) {
    let now = Utc::now();
    let plan: ExperimentPlan = serde_json::from_value(json!({
        "campaign_id":"fixture_campaign", "hypothesis":"Synthetic helper boundary checks",
        "envelope":{"max_entry_allowance_fraction":"0.005", "min_stop_atr_multiple":"1",
            "max_stop_atr_multiple":"4", "min_target_net_fraction":"0.01", "max_target_net_fraction":"0.03"},
        "candidates":[{"version":1,"family":"TREND","entry_allowance_fraction":"0.001",
            "stop_atr_multiple":"2","target_net_fraction":"0.03","max_holding_sessions":3,
            "atr_lookback":14,"bar_seconds":60}],
        "max_cumulative_trials":2, "wall_clock_seconds":60,
        "objective":{"drawdown_weight":"0.1","instability_weight":"0.2","complexity_weight":"0"},
        "minimum_improvement":"0.001", "min_outer_intervals":6, "min_closed_trades":100,
        "min_shadow_weeks":4, "min_shadow_closed_trades":20,
        "development_folds":[
            {"session_id":"older", "opens_at":now-Duration::days(60), "closes_at":now-Duration::days(40)},
            {"session_id":"newer", "opens_at":now-Duration::days(30), "closes_at":now-Duration::days(10)}],
        "development_fold_sessions":14
    })).unwrap();
    let run = RunManifest {
        schema_version: 1,
        account_id: AccountId::new("paper_helper_fixture").unwrap(),
        mode: Mode::Paper,
        source_commit: "b".repeat(40),
        source_clean: true,
        champion_id: "champion".into(),
        dataset_hash: "c".repeat(64),
        cutoff_at: now - Duration::hours(1),
        completed_sessions: vec![SessionInterval {
            session_id: "completed".into(),
            opens_at: now - Duration::days(1),
            closes_at: now - Duration::hours(2),
        }],
        next_session: SessionInterval {
            session_id: "next".into(),
            opens_at: now + Duration::days(2),
            closes_at: now + Duration::days(2) + Duration::hours(6),
        },
        activation_deadline: now + Duration::days(1),
        component_hashes: [
            "calendar",
            "features",
            "models",
            "calibrators",
            "fusion",
            "labels",
            "policy",
            "costs",
            "rust_build",
            "engine",
            "driver",
            "lean_toolchain",
            "mathlib",
            "skill",
            "prompt",
            "objective",
            "splits",
            "gates",
        ]
        .into_iter()
        .map(|key| (key.into(), "a".repeat(64)))
        .collect::<BTreeMap<_, _>>(),
        random_seed: 42,
    };
    let trial = TrialRecord {
        candidate_id: "candidate_fixture".into(),
        parameters: plan.candidates[0].clone(),
        registered_at: now,
        development: None,
        failure: None,
    };
    run.validate(now).unwrap();
    (run, plan, trial)
}

#[test]
fn direct_weekly_methods_reject_invalid_manifests_before_starting_any_helper() {
    let (dir, command) = fixture(
        "open('invoked', 'a').write('unexpected helper execution')",
        1,
        1024,
    );
    let mut lab = HelperLab::new(LabConfig {
        schema_version: 1,
        reconcile: command.clone(),
        develop: command.clone(),
        proof: command.clone(),
        parity: command.clone(),
        evaluate: command,
        approved_axioms: vec![],
        required_theorems: vec!["fixture_theorem".into()],
        protected_safety_hash: "d".repeat(64),
    })
    .unwrap();
    let (run, plan, trial) = weekly_inputs();
    let mut invalid = Vec::new();
    // Every required pin is checked, including the two formerly indexed keys.
    for key in run.component_hashes.keys() {
        let mut missing = run.clone();
        missing.component_hashes.remove(key);
        invalid.push(missing);
        let mut malformed = run.clone();
        malformed
            .component_hashes
            .insert(key.clone(), "not-a-hash".into());
        invalid.push(malformed);
    }
    let mut bad_identity = run.clone();
    bad_identity.source_commit = "invalid".into();
    invalid.push(bad_identity);
    let mut bad_mode = run.clone();
    bad_mode.mode = Mode::Live;
    invalid.push(bad_mode);
    let mut bad_sessions = run.clone();
    bad_sessions
        .completed_sessions
        .push(bad_sessions.completed_sessions[0].clone());
    invalid.push(bad_sessions);
    let mut future_cutoff = run.clone();
    future_cutoff.cutoff_at = Utc::now() + Duration::hours(1);
    invalid.push(future_cutoff);
    let mut bad_deadline = run;
    bad_deadline.activation_deadline = bad_deadline.next_session.closes_at;
    invalid.push(bad_deadline);
    for manifest in invalid {
        let expected = manifest.validate(Utc::now()).unwrap_err();
        assert_eq!(lab.reconcile(&manifest).unwrap_err(), expected);
        assert_eq!(lab.develop(&manifest, &plan, &trial).unwrap_err(), expected);
        assert_eq!(
            lab.verify_candidate(&manifest, &plan, &trial).unwrap_err(),
            expected
        );
        assert_eq!(lab.verify(&manifest, &plan, &trial).unwrap_err(), expected);
        assert_eq!(
            lab.evaluate(&manifest, &plan, &trial).unwrap_err(),
            expected
        );
        assert!(!dir.join("invoked").exists());
        assert!(!dir.join("artifacts").exists());
    }
    fs::remove_dir_all(dir).unwrap();
}

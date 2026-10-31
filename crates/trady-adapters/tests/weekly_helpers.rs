use chrono::Utc;
use serde_json::Value;
use std::{fs, path::PathBuf, time::Instant};
use trady_adapters::weekly_lab::runner::*;
use trady_application::optimization::ToolReport;

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

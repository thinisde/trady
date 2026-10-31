//! Complete CLI round-trip with real disposable PostgreSQL and a local model endpoint.
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output},
    sync::mpsc,
    thread,
    time::Duration,
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .env("OPENCODE_API_KEY", "fixture-secret")
        .output()
        .unwrap()
}
fn successful(args: &[&str]) -> Value {
    let out = run(args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn final_reply(status: &str, text: &str) -> Value {
    json!({"protocol_version":1,"message_type":"FINAL","reply":{"status":status,"text":text,"evidence_ids":[],"snapshot_ids":[],"questions":if status=="NEEDS_INPUT" {vec!["Which issuer?"]}else{vec![]}},"research":null,"proposal":null})
}
#[test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test-postgres.py"]
fn cli_creates_reopens_inspects_and_closes_a_persistent_opencode_conversation() {
    assert_eq!(env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        for final_value in [
            final_reply("NEEDS_INPUT", "Which issuer?"),
            final_reply("ANSWER", "The follow-up retained the first question."),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut bytes = vec![];
            let end = loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let size: usize = headers
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() - end < size {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            tx.send(serde_json::from_slice::<Value>(&bytes[end..end + size]).unwrap())
                .unwrap();
            let body=json!({"choices":[{"message":{"role":"assistant","content":final_value.to_string()},"finish_reason":"stop"}],"usage":{"prompt_tokens":40,"completion_tokens":20}}).to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let directory = env::temp_dir().join(format!(
        "trady-opencode-cli-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_micros()
    ));
    fs::create_dir(&directory).unwrap();
    let approval = directory.join("approval.json");
    let config = directory.join("llm.json");
    let candidate = successful(&["skill-manifest", "skills"]);
    assert_eq!(candidate["files"].as_object().unwrap().len(), 52);
    fs::write(&approval, candidate.to_string()).unwrap();
    fs::write(
        &config,
        json!({"base_url":format!("http://{address}/v1"),"model":"glm-5.2","stream":false})
            .to_string(),
    )
    .unwrap();
    let approval = approval.to_str().unwrap();
    let config = config.to_str().unwrap();
    let first = successful(&[
        "chat",
        "--owner",
        "cli_owner",
        "--approval",
        approval,
        "--llm-config",
        config,
        "--message",
        "Research the issuer",
    ]);
    let id = first["session_id"].as_str().unwrap();
    assert_eq!(first["run_status"], "WAITING_OWNER");
    assert_eq!(first["entry_authorized"], false);
    let second = successful(&[
        "chat",
        "--owner",
        "cli_owner",
        "--approval",
        approval,
        "--llm-config",
        config,
        "--session",
        id,
        "--message",
        "Issuer A",
    ]);
    assert_eq!(second["session_id"], id);
    assert_eq!(second["run_status"], "COMPLETED");
    let _first_request = rx.recv().unwrap();
    let second_request = rx.recv().unwrap();
    assert!(
        second_request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["content"] == "Research the issuer")
    );
    worker.join().unwrap();
    let shown = successful(&["sessions", "show", id, "--owner", "cli_owner"]);
    assert_eq!(shown["runs"].as_array().unwrap().len(), 2);
    assert!(!shown.to_string().contains("fixture-secret"));
    assert!(
        !run(&["sessions", "show", id, "--owner", "someone_else"])
            .status
            .success()
    );
    let listed = successful(&["sessions", "list", "--owner", "cli_owner"]);
    assert!(
        listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["session_id"] == id)
    );
    assert_eq!(
        successful(&["sessions", "close", id, "--owner", "cli_owner"])["status"],
        "CLOSED"
    );
    assert!(
        !run(&[
            "chat",
            "--owner",
            "cli_owner",
            "--approval",
            approval,
            "--llm-config",
            config,
            "--session",
            id,
            "--message",
            "Cannot reopen"
        ])
        .status
        .success()
    );
    fs::remove_dir_all(directory).unwrap();
}

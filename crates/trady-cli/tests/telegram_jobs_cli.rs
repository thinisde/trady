//! Isolated PostgreSQL and two-request local model fixture; no Telegram sends.
use chrono::Utc;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    thread,
};
use trady_adapters::opencode::{llm::OpenCodeGoConfig, runtime_tools::SkillBundle};
use trady_application::{
    harness::{Scope, new_id},
    research_jobs::ResearchJobRepository,
    telegram::gateway::{GatewayCommand, GatewayRequest},
};
use trady_storage::{migrate, research_jobs::PgResearchJobs};

fn dsn(role: &str) -> String {
    format!(
        "host=127.0.0.1 port={} user={role} dbname=trady",
        std::env::var("TRADY_TEST_PORT").unwrap()
    )
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn worker(path: &std::path::Path) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_trady"))
        .args([
            "telegram",
            "worker",
            "--config",
            path.to_str().unwrap(),
            "--once",
        ])
        .env("TRADY_ANALYST_DSN", dsn("trady_analyst"))
        .env("TRADY_GATEWAY_FIXTURE_MODEL_KEY", "offline-fixture-only")
        .env_remove("TRADY_GATEWAY_DSN")
        .env_remove("TRADY_EXECUTOR_DSN")
        .env_remove("TRADY_TELEGRAM_BOT_TOKEN")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
#[ignore = "requires isolated PostgreSQL; root disposable runner sets TRADY_TEST_ISOLATED"]
fn expanded_worker_runs_persistent_followup_dialogue_without_gateway_or_executor_credentials() {
    assert_eq!(std::env::var("TRADY_TEST_ISOLATED").as_deref(), Ok("1"));
    migrate(&dsn("trady_migrator")).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = Scratch(std::env::temp_dir().join(new_id("trady-research-jobs-e2e")));
    std::fs::create_dir(&scratch.0).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let mut llm = OpenCodeGoConfig {
        base_url: format!("http://{address}/v1"),
        stream: false,
        api_key_env: "TRADY_GATEWAY_FIXTURE_MODEL_KEY".into(),
        ..OpenCodeGoConfig::default()
    };
    llm.limits.max_tool_result_bytes = 12000;
    llm.limits.max_tool_input_bytes = 12000;
    let approval = SkillBundle::candidate(&root.join("skills")).unwrap();
    std::fs::write(
        scratch.0.join("approval.json"),
        serde_json::to_vec(&approval).unwrap(),
    )
    .unwrap();
    std::fs::write(
        scratch.0.join("llm.json"),
        serde_json::to_vec(&llm).unwrap(),
    )
    .unwrap();
    let mut foundation: Value =
        serde_json::from_str(include_str!("../../../config/paper.json")).unwrap();
    let account = new_id("shadow_gateway_fixture");
    foundation["mode"] = json!("shadow");
    foundation["account_id"] = json!(account);
    std::fs::write(
        scratch.0.join("foundation.json"),
        serde_json::to_vec(&foundation).unwrap(),
    )
    .unwrap();
    let research: Value =
        serde_json::from_str(include_str!("../../../config/research.json")).unwrap();
    std::fs::write(
        scratch.0.join("research.json"),
        serde_json::to_vec(&research).unwrap(),
    )
    .unwrap();
    let owner = new_id("owner_gateway_fixture");
    let bot = 22334455;
    let cfg = json!({"config_version":1,"access":{"allowed_user_ids":[12345],"allowed_chat_ids":[12345]},"foundation_config":"foundation.json","trading212_config":null,
        "state_directory":"unused-fixture-spool","poll_timeout_seconds":1,"refresh_interval_seconds":60,"stale_after_seconds":60,"notifications":false,
        "expanded_gateway":{"enabled":true,"bot_id":bot,"owners":[{"telegram_user_id":12345,"owner_id":owner}],"research_config":"research.json","finbert_config":null,"llm_config":"llm.json","skill_approval":"approval.json","skills_root":root.join("skills"),"approved_instrument_ids":[],"daily_job_limit":20,"pending_job_limit":10,"daily_research_enabled":false,"daily_research_hour_utc":7,"holding_watchdog_interval_seconds":300}});
    let config_path = scratch.0.join("telegram.json");
    std::fs::write(&config_path, serde_json::to_vec(&cfg).unwrap()).unwrap();
    // Provisioning makes no model request. The sole listener accepts the two
    // actual queued dialogue runs below, and can never contact the Internet.
    assert!(
        worker(&config_path)["outcomes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let scope = Scope {
        owner_id: owner,
        account_id: account,
        policy_version: 1,
    };
    let server = thread::spawn(move || {
        let mut requests = vec![];
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let (start, len) = loop {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
                    let len = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.parse::<usize>().unwrap())
                        })
                        .unwrap();
                    break (end + 4, len);
                }
            };
            while bytes.len() < start + len {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let request: Value = serde_json::from_slice(&bytes[start..start + len]).unwrap();
            requests.push(request);
            let canonical = json!({"protocol_version":1,"message_type":"FINAL","reply":{"status":"ANSWER","text":"Offline gateway dialogue fixture response.","evidence_ids":[],"snapshot_ids":[],"questions":[]},"research":null,"proposal":null});
            let body=json!({"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":canonical.to_string()}}],"usage":{"prompt_tokens":50,"completion_tokens":50}}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
        requests
    });
    let mut gateway = PgResearchJobs::connect_gateway(&dsn("trady_gateway")).unwrap();
    let mut sessions = vec![];
    for (update_id, text) in [
        (1, "First offline fixture question"),
        (2, "Followup offline fixture question"),
    ] {
        let request = GatewayRequest {
            bot_id: bot,
            update_id,
            sender_id: 12345,
            chat_id: 12345,
            scope: scope.clone(),
            command: GatewayCommand::Ask(text.into()),
        };
        assert!(
            gateway
                .accept_gateway(&request, None, "", Utc::now())
                .unwrap()
                .contains("Queued")
        );
        let result = worker(&config_path);
        let outcome = &result["outcomes"][0];
        assert_eq!(outcome["status"], "COMPLETE", "{result}");
        assert_eq!(outcome["result"]["entry_authorized"], false);
        sessions.push(outcome["result"]["session_id"].clone());
    }
    assert_eq!(sessions[0], sessions[1]);
    let requests = server.join().unwrap();
    let second = requests[1].to_string();
    assert!(
        second.contains("First offline fixture question")
            && second.contains("Followup offline fixture question")
    );
    let jobs = gateway.recent_jobs(&scope, 10).unwrap();
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|j| {
        j.result
            .as_ref()
            .is_some_and(|v| v["entry_authorized"] == false)
    }));
    assert_eq!(
        gateway.policy(&scope).unwrap().mode,
        trady_domain::Mode::Shadow
    );
}

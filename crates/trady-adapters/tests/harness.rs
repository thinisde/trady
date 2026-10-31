use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use trady_adapters::{
    opencode::{
        llm::{OpenCodeGoConfig, OpenCodeGoProvider},
        runtime_tools::{SkillApproval, SkillBundle},
    },
    sessions::MemorySessionRepository,
};
use trady_application::harness::*;
use trady_domain::{Error, Result};

fn final_value(status: &str, text: &str) -> Value {
    json!({"protocol_version":1,"message_type":"FINAL","reply":{"status":status,"text":text,"evidence_ids":[],"snapshot_ids":[],"questions":if status=="NEEDS_INPUT" {vec!["Which issuer?"]}else{vec![]}},"research":null,"proposal":null})
}
fn final_reply(status: &str, text: &str) -> Completion {
    Completion {
        message: ChatMessage::text("assistant", final_value(status, text).to_string()),
        usage: Some(Usage {
            input_tokens: 10,
            output_tokens: 10,
        }),
    }
}
fn tool_reply(id: &str, arguments: &str) -> Completion {
    let mut message = ChatMessage::text("assistant", String::new());
    message.reasoning_content = Some("provider continuation state".into());
    message.tool_calls = Some(vec![NativeCall {
        id: id.into(),
        kind: "function".into(),
        function: NativeFunction {
            name: "memory_search_v1".into(),
            arguments: arguments.into(),
        },
    }]);
    Completion {
        message,
        usage: None,
    }
}
fn identity() -> ProviderIdentity {
    ProviderIdentity {
        provider: "test".into(),
        model: "fixture".into(),
        endpoint: "http://127.0.0.1/chat/completions".into(),
    }
}
struct Scripted {
    replies: VecDeque<std::result::Result<Completion, ProviderError>>,
    requests: Vec<Vec<ChatMessage>>,
    finalizing: Vec<bool>,
}
impl Scripted {
    fn new(replies: Vec<Completion>) -> Self {
        Self {
            replies: replies.into_iter().map(Ok).collect(),
            requests: vec![],
            finalizing: vec![],
        }
    }
}
impl LlmProvider for Scripted {
    fn identity(&self) -> ProviderIdentity {
        identity()
    }
    fn complete(
        &mut self,
        r: &ModelRequest,
        _: &CancellationToken,
        _: &mut dyn FnMut(&str),
    ) -> std::result::Result<Completion, ProviderError> {
        self.requests.push(r.messages.clone());
        self.finalizing.push(r.force_final);
        self.replies.pop_front().expect("unexpected provider call")
    }
}
#[derive(Default)]
struct Tools {
    calls: usize,
    oversized: bool,
}
impl ToolExecutor for Tools {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "memory.search".into(),
            version: "1.0.0".into(),
            description: "test scoped search".into(),
            arguments_schema: json!({"type":"object","additionalProperties":false,"required":["query"],"properties":{"query":{"type":"string","maxLength":100}}}),
            result_schema: json!({"type":"object","additionalProperties":false,"required":["items"],"properties":{"items":{"type":"array"}}}),
        }]
    }
    fn revision(&self) -> String {
        "tools-1".into()
    }
    fn execute(
        &mut self,
        _: &str,
        _: &Value,
        _: &Session,
        _: Instant,
        _: &CancellationToken,
    ) -> Result<ToolOutput> {
        self.calls += 1;
        Ok(ToolOutput::ok(
            json!({"items":if self.oversized {vec!["x".repeat(3000)]}else{vec!["attributed memory".into()]} }),
        ))
    }
}
fn session(limits: RunLimits) -> Session {
    Session::new(
        Scope {
            owner_id: "owner1".into(),
            account_id: "paper_test".into(),
            policy_version: 1,
        },
        identity(),
        limits,
        "Paper research only.".into(),
        "tools-1".into(),
        vec![],
    )
    .unwrap()
}
fn fixture() -> (MemorySessionRepository, Session, Tools) {
    let s = session(RunLimits::default());
    let mut repo = MemorySessionRepository::default();
    repo.create(&s).unwrap();
    (repo, s, Tools::default())
}
fn respond(
    repo: &mut impl SessionRepository,
    s: &Session,
    p: &mut impl LlmProvider,
    t: &mut Tools,
    input: Option<&str>,
) -> Value {
    AgentHarness::new()
        .unwrap()
        .respond(
            repo,
            p,
            t,
            RunInput {
                scope: &s.scope,
                session_id: &s.id,
                input,
                cancel: &CancellationToken::default(),
            },
            &mut |_| {},
        )
        .unwrap()
}

#[test]
fn native_tool_replay_followup_and_waiting_owner_are_durable() {
    let (mut repo, s, mut tools) = fixture();
    let mut p = Scripted::new(vec![
        tool_reply("call_1", r#"{"query":"issuer"}"#),
        final_reply("NEEDS_INPUT", "Which issuer?"),
        final_reply("ANSWER", "Follow-up answered from retained context."),
    ]);
    assert_eq!(
        respond(
            &mut repo,
            &s,
            &mut p,
            &mut tools,
            Some("Research the issuer")
        )["reply"]["status"],
        "NEEDS_INPUT"
    );
    let waiting = repo.load(&s.scope, &s.id).unwrap();
    assert_eq!(waiting.status, SessionStatus::WaitingOwner);
    assert_eq!(tools.calls, 1);
    assert_eq!(p.requests.len(), 2);
    let after_tools = p.requests.last().unwrap();
    let request = after_tools.iter().find(|m| m.tool_calls.is_some()).unwrap();
    assert!(request.reasoning_content.is_some());
    let result = after_tools.iter().find(|m| m.role == "tool").unwrap();
    assert_eq!(result.tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(
        respond(&mut repo, &s, &mut p, &mut tools, Some("Issuer A"))["reply"]["status"],
        "ANSWER"
    );
    let stored = repo.load(&s.scope, &s.id).unwrap();
    assert_eq!(stored.runs.len(), 2);
    assert_eq!(stored.messages.len(), 6);
    assert_eq!(
        stored.runs[0].calls[0].result.as_ref().unwrap()["status"],
        "OK"
    );
    assert!(
        stored
            .runs
            .iter()
            .all(|r| r.final_message.is_some() && !r.manifests.is_empty())
    );
    for (i, message) in stored.messages.iter().enumerate() {
        assert_eq!(message.sequence, i as u64 + 1);
        assert_eq!(
            message.hash,
            content_hash(&serde_json::to_string(&message.wire).unwrap())
        );
    }
}

#[test]
fn unregistered_tools_extra_arguments_and_duplicate_keys_never_execute() {
    for first in [
        Completion {message:ChatMessage::text("assistant",json!({"protocol_version":1,"message_type":"TOOL_REQUEST","tool_calls":[{"call_id":"call_1","tool":"broker.submit","arguments":{}}]}).to_string()),usage:None},
        tool_reply("call_1",r#"{"query":"issuer","account":"other"}"#),
        tool_reply("call_1",r#"{"query":"a","query":"b"}"#),
    ] {
        let (mut repo,s,mut tools)=fixture();let mut p=Scripted::new(vec![first,final_reply("ANSWER","Unavailable tools cannot supply evidence.")]);
        respond(&mut repo,&s,&mut p,&mut tools,Some("Check"));assert_eq!(tools.calls,0);
        let stored=repo.load(&s.scope,&s.id).unwrap();
        if let Some(call)=stored.runs[0].calls.first() {assert_eq!(call.result.as_ref().unwrap()["status"],"DENIED");}
    }
}

#[test]
fn malformed_output_repairs_once_and_retains_attempts_and_reservations() {
    let (mut repo, s, mut tools) = fixture();
    let bad = || Completion {
        message: ChatMessage::text("assistant", "```json\n{}\n```".into()),
        usage: None,
    };
    let mut p = Scripted::new(vec![bad(), bad()]);
    let value = respond(&mut repo, &s, &mut p, &mut tools, Some("Question"));
    assert_eq!(value["reply"]["status"], "INCOMPLETE");
    assert!(value["proposal"].is_null());
    assert_eq!(tools.calls, 0);
    let saved = repo.load(&s.scope, &s.id).unwrap();
    assert_eq!(saved.runs[0].attempts.len(), 2);
    assert!(saved.charged_tokens() > 8000);
    assert_eq!(saved.runs[0].status, RunStatus::Failed);
}

#[test]
fn transient_errors_retry_once_but_authentication_does_not_retry() {
    for retryable in [false, true] {
        let (mut repo, s, mut tools) = fixture();
        let mut p = Scripted::new(vec![]);
        p.replies.push_back(Err(ProviderError {
            code: "provider_failure",
            retryable,
        }));
        if retryable {
            p.replies.push_back(Ok(final_reply("ANSWER", "Recovered")));
        }
        let value = respond(&mut repo, &s, &mut p, &mut tools, Some("Question"));
        assert_eq!(
            value["reply"]["status"],
            if retryable { "ANSWER" } else { "INCOMPLETE" }
        );
        assert_eq!(p.requests.len(), if retryable { 2 } else { 1 });
    }
}

#[test]
fn repeated_call_ids_reuse_results_and_changed_arguments_are_rejected() {
    for changed in [false, true] {
        let (mut repo, s, mut tools) = fixture();
        let next = if changed {
            r#"{"query":"changed"}"#
        } else {
            r#"{"query":"issuer"}"#
        };
        let mut replies = vec![
            tool_reply("call_1", r#"{"query":"issuer"}"#),
            tool_reply("call_1", next),
        ];
        if !changed {
            replies.push(final_reply("ANSWER", "Done"));
        }
        let mut p = Scripted::new(replies);
        let value = respond(&mut repo, &s, &mut p, &mut tools, Some("Research"));
        assert_eq!(tools.calls, 1);
        assert_eq!(
            value["reply"]["status"],
            if changed { "INCOMPLETE" } else { "ANSWER" }
        );
    }
}

struct CrashRepo {
    inner: MemorySessionRepository,
    saves: usize,
    fail_at: usize,
}
impl SessionRepository for CrashRepo {
    fn create(&mut self, s: &Session) -> Result<()> {
        self.inner.create(s)
    }
    fn load(&mut self, s: &Scope, id: &str) -> Result<Session> {
        self.inner.load(s, id)
    }
    fn list(&mut self, s: &Scope) -> Result<Vec<Session>> {
        self.inner.list(s)
    }
    fn acquire(&mut self, s: &Scope, id: &str, w: &str, t: u32) -> Result<SessionLease> {
        self.inner.acquire(s, id, w, t)
    }
    fn save(&mut self, l: &mut SessionLease) -> Result<()> {
        self.saves += 1;
        if self.saves == self.fail_at {
            Err(Error("simulated process/storage interruption".into()))
        } else {
            self.inner.save(l)
        }
    }
    fn release(&mut self, l: &SessionLease) -> Result<()> {
        self.inner.release(l)
    }
}
#[test]
fn restart_resumes_persisted_results_without_reexecuting_tool() {
    let (inner, s, mut tools) = fixture();
    let mut repo = CrashRepo {
        inner,
        saves: 0,
        fail_at: 6,
    };
    let mut p = Scripted::new(vec![tool_reply("call_1", r#"{"query":"issuer"}"#)]);
    let result = AgentHarness::new().unwrap().respond(
        &mut repo,
        &mut p,
        &mut tools,
        RunInput {
            scope: &s.scope,
            session_id: &s.id,
            input: Some("Research"),
            cancel: &CancellationToken::default(),
        },
        &mut |_| {},
    );
    assert!(result.is_err());
    assert_eq!(tools.calls, 1);
    let interrupted = repo.load(&s.scope, &s.id).unwrap();
    assert_eq!(interrupted.runs[0].pending_calls.len(), 1);
    assert!(interrupted.runs[0].calls[0].result.is_some());
    let mut continuation = Scripted::new(vec![final_reply("ANSWER", "Resumed")]);
    assert_eq!(
        respond(&mut repo, &s, &mut continuation, &mut tools, None)["reply"]["text"],
        "Resumed"
    );
    assert_eq!(tools.calls, 1);
    assert_eq!(
        continuation.requests[0]
            .iter()
            .filter(|m| m.role == "tool")
            .count(),
        1
    );
}

#[test]
fn expired_pending_runs_close_native_calls_before_a_followup() {
    let (inner, s, mut tools) = fixture();
    let mut repo = CrashRepo {
        inner,
        saves: 0,
        fail_at: 4,
    };
    let mut provider = Scripted::new(vec![tool_reply("pending_1", r#"{"query":"issuer"}"#)]);
    assert!(
        AgentHarness::new()
            .unwrap()
            .respond(
                &mut repo,
                &mut provider,
                &mut tools,
                RunInput {
                    scope: &s.scope,
                    session_id: &s.id,
                    input: Some("Question"),
                    cancel: &CancellationToken::default()
                },
                &mut |_| {}
            )
            .is_err()
    );
    assert_eq!(tools.calls, 0);
    let mut lease = repo.acquire(&s.scope, &s.id, "recovery", 30).unwrap();
    lease.session.runs[0].deadline_at = chrono::Utc::now() - chrono::Duration::seconds(1);
    repo.save(&mut lease).unwrap();
    repo.release(&lease).unwrap();
    let mut none = Scripted::new(vec![]);
    assert_eq!(
        respond(&mut repo, &s, &mut none, &mut tools, None)["reply"]["status"],
        "INCOMPLETE"
    );
    let saved = repo.load(&s.scope, &s.id).unwrap();
    assert_eq!(
        saved.runs[0].calls[0].result.as_ref().unwrap()["status"],
        "CANCELLED"
    );
    assert_eq!(
        saved
            .messages
            .iter()
            .filter(|m| m.wire.tool_call_id.as_deref() == Some("pending_1"))
            .count(),
        1
    );
    let mut followup = Scripted::new(vec![final_reply("ANSWER", "Continued safely")]);
    respond(&mut repo, &s, &mut followup, &mut tools, Some("Continue"));
    assert_eq!(tools.calls, 0);
}

#[test]
fn finalization_reserves_a_turn_and_disables_new_tool_work() {
    let limits = RunLimits {
        max_model_turns: 2,
        max_tool_rounds: 1,
        ..RunLimits::default()
    };
    let s = session(limits);
    let mut repo = MemorySessionRepository::default();
    repo.create(&s).unwrap();
    let mut tools = Tools::default();
    let mut p = Scripted::new(vec![
        tool_reply("call_1", r#"{"query":"issuer"}"#),
        tool_reply("call_2", r#"{"query":"more"}"#),
    ]);
    let answer = respond(&mut repo, &s, &mut p, &mut tools, Some("Research"));
    assert_eq!(answer["reply"]["status"], "INCOMPLETE");
    assert_eq!(tools.calls, 1);
    assert_eq!(p.finalizing, vec![false, true]);
}

#[test]
fn owner_account_policy_scope_and_session_leases_reject_competing_writers() {
    let (mut repo, s, _) = fixture();
    let mut other = repo.clone();
    for scope in [
        Scope {
            owner_id: "owner2".into(),
            ..s.scope.clone()
        },
        Scope {
            account_id: "paper_other".into(),
            ..s.scope.clone()
        },
        Scope {
            policy_version: 2,
            ..s.scope.clone()
        },
    ] {
        assert!(repo.load(&scope, &s.id).is_err());
        assert!(repo.acquire(&scope, &s.id, "worker", 30).is_err());
    }
    let first = repo.acquire(&s.scope, &s.id, "worker_a", 30).unwrap();
    assert!(other.acquire(&s.scope, &s.id, "worker_b", 30).is_err());
    repo.release(&first).unwrap();
    let mut second = other.acquire(&s.scope, &s.id, "worker_b", 30).unwrap();
    let mut stale = first;
    assert!(repo.save(&mut stale).is_err());
    other.save(&mut second).unwrap();
    assert!(repo.release(&stale).is_err());
}

#[test]
fn limits_cancellation_and_large_results_are_explicit_incomplete_or_partial() {
    let (mut repo, s, mut tools) = fixture();
    tools.oversized = true;
    let mut p = Scripted::new(vec![
        tool_reply("call_1", r#"{"query":"issuer"}"#),
        final_reply("ANSWER", "Tool data was partial."),
    ]);
    respond(&mut repo, &s, &mut p, &mut tools, Some("Research"));
    assert_eq!(
        repo.load(&s.scope, &s.id).unwrap().runs[0].calls[0]
            .result
            .as_ref()
            .unwrap()["status"],
        "PARTIAL"
    );
    let s2 = session(RunLimits {
        max_run_tokens: 4096,
        ..RunLimits::default()
    });
    repo.create(&s2).unwrap();
    let mut none = Scripted::new(vec![]);
    assert_eq!(
        respond(&mut repo, &s2, &mut none, &mut tools, Some("Budget"))["reply"]["status"],
        "INCOMPLETE"
    );
    assert!(none.requests.is_empty());
    let s3 = session(RunLimits::default());
    repo.create(&s3).unwrap();
    let cancel = CancellationToken::default();
    cancel.cancel();
    let answer = AgentHarness::new()
        .unwrap()
        .respond(
            &mut repo,
            &mut none,
            &mut tools,
            RunInput {
                scope: &s3.scope,
                session_id: &s3.id,
                input: Some("Cancelled"),
                cancel: &cancel,
            },
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(answer["reply"]["status"], "INCOMPLETE");
    assert_eq!(
        repo.load(&s3.scope, &s3.id).unwrap().runs[0].status,
        RunStatus::Cancelled
    );
}

#[test]
fn context_compaction_keeps_original_records_and_complete_run_groups() {
    let mut s = session(RunLimits {
        context_window_tokens: 64_000,
        ..RunLimits::default()
    });
    s.governing_context = "policy".into();
    let mut repo = MemorySessionRepository::default();
    repo.create(&s).unwrap();
    let mut tools = Tools::default();
    let mut p = Scripted::new(
        (0..18)
            .map(|i| final_reply("ANSWER", &format!("Answer {i}: {}", "a".repeat(1600))))
            .collect(),
    );
    for _ in 0..18 {
        respond(&mut repo, &s, &mut p, &mut tools, Some(&"q".repeat(1200)));
    }
    let saved = repo.load(&s.scope, &s.id).unwrap();
    assert_eq!(saved.messages.len(), 36);
    assert!(!saved.summaries.is_empty());
    let manifest = saved.runs.last().unwrap().manifests.last().unwrap();
    assert!(!manifest.excluded_sequences.is_empty());
    assert!(manifest.estimated_input_tokens <= 38_400);
    assert!(p.requests.last().unwrap().iter().any(|m| {
        m.content
            .as_ref()
            .is_some_and(|s| s.contains("untrusted_historical_summary"))
    }));
}

fn server(
    responses: Vec<(u16, &'static str, String)>,
) -> (String, mpsc::Receiver<Value>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        for (status, kind, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let header_end = loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let header = String::from_utf8_lossy(&bytes[..header_end]);
            assert!(header.contains("authorization: Bearer fixture-secret"));
            let size: usize = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() - header_end < size {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            tx.send(serde_json::from_slice(&bytes[header_end..header_end + size]).unwrap())
                .unwrap();
            write!(socket,"HTTP/1.1 {status} OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    (format!("http://{addr}/v1"), rx, handle)
}
fn event(value: Value) -> String {
    format!("data: {value}\n\n")
}
#[test]
fn real_http_transport_accumulates_native_tool_deltas_and_preserves_correlation() {
    let stream = event(
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"memory_search_v1","arguments":"{\"query\":"}}]},"finish_reason":null}]}),
    ) + &event(
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"issuer\"}"}}]},"finish_reason":"tool_calls"}]}),
    ) + &event(
        json!({"choices":[],"usage":{"prompt_tokens":33,"completion_tokens":22}}),
    ) + "data: [DONE]\n\n";
    let final_stream = event(
        json!({"choices":[{"index":0,"delta":{"content":final_value("ANSWER","HTTP tool loop passed").to_string()},"finish_reason":"stop"}]}),
    ) + "data: [DONE]\n\n";
    let (base_url, requests, thread) = server(vec![
        (200, "text/event-stream", stream),
        (200, "text/event-stream", final_stream),
    ]);
    let mut provider = OpenCodeGoProvider::new(
        OpenCodeGoConfig {
            base_url,
            ..OpenCodeGoConfig::default()
        },
        "fixture-secret".into(),
    )
    .unwrap();
    let mut s = session(RunLimits::default());
    s.provider = provider.identity();
    let mut repo = MemorySessionRepository::default();
    repo.create(&s).unwrap();
    let mut tools = Tools::default();
    let result = respond(&mut repo, &s, &mut provider, &mut tools, Some("Research"));
    assert_eq!(result["reply"]["text"], "HTTP tool loop passed");
    assert_eq!(tools.calls, 1);
    let first = requests.recv().unwrap();
    let second = requests.recv().unwrap();
    assert_eq!(first["model"], "glm-5.2");
    assert_eq!(first["tools"][0]["function"]["name"], "memory_search_v1");
    assert!(
        second["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "tool" && m["tool_call_id"] == "call_1")
    );
    thread.join().unwrap();
    assert_eq!(
        repo.load(&s.scope, &s.id).unwrap().runs[0].attempts[0]
            .usage
            .as_ref()
            .unwrap()
            .total(),
        55
    );
}

#[test]
fn json_envelope_mode_and_non_streaming_http_keep_results_in_the_conversation() {
    let request = json!({"protocol_version":1,"message_type":"TOOL_REQUEST","tool_calls":[{"call_id":"json_1","tool":"memory.search","arguments":{"query":"issuer"}}]});
    let response = |v: Value| {
        json!({"choices":[{"message":{"role":"assistant","content":v.to_string()},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":11}}).to_string()
    };
    let (base_url, requests, thread) = server(vec![
        (200, "application/json", response(request)),
        (
            200,
            "application/json",
            response(final_value("ANSWER", "JSON fallback passed")),
        ),
    ]);
    let mut provider = OpenCodeGoProvider::new(
        OpenCodeGoConfig {
            base_url,
            stream: false,
            ..OpenCodeGoConfig::default()
        },
        "fixture-secret".into(),
    )
    .unwrap();
    let mut s = session(RunLimits {
        native_tools: false,
        ..RunLimits::default()
    });
    s.provider = provider.identity();
    let mut repo = MemorySessionRepository::default();
    repo.create(&s).unwrap();
    let mut tools = Tools::default();
    assert_eq!(
        respond(&mut repo, &s, &mut provider, &mut tools, Some("Research"))["reply"]["text"],
        "JSON fallback passed"
    );
    assert_eq!(tools.calls, 1);
    assert!(requests.recv().unwrap().get("tools").is_none());
    let next = requests.recv().unwrap();
    assert!(next["messages"].as_array().unwrap().iter().any(|m| {
        m["role"] == "user"
            && m["content"]
                .as_str()
                .is_some_and(|s| s.contains("TOOL_RESULTS"))
    }));
    thread.join().unwrap();
}

#[test]
fn truncated_streams_and_authentication_errors_never_expose_response_bodies() {
    for (status, kind, body, code) in [
        (
            200,
            "text/event-stream",
            event(
                json!({"choices":[{"index":0,"delta":{"content":"unfinished"},"finish_reason":null}]}),
            ),
            "provider_stream_incomplete",
        ),
        (
            401,
            "application/json",
            "fixture-secret should never be logged".into(),
            "provider_authentication",
        ),
    ] {
        let (base_url, _requests, thread) = server(vec![(status, kind, body)]);
        let mut provider = OpenCodeGoProvider::new(
            OpenCodeGoConfig {
                base_url,
                ..OpenCodeGoConfig::default()
            },
            "fixture-secret".into(),
        )
        .unwrap();
        let result = provider.complete(
            &ModelRequest {
                messages: vec![ChatMessage::text("user", "test".into())],
                tools: vec![],
                max_output_tokens: 1000,
                timeout: Duration::from_secs(2),
                native_tools: true,
                force_final: false,
            },
            &CancellationToken::default(),
            &mut |_| {},
        );
        let error = result.err().unwrap();
        assert_eq!(error.code, code);
        assert!(!format!("{error:?}").contains("fixture-secret"));
        thread.join().unwrap();
    }
}

#[test]
fn approved_bundle_rejects_hash_changes_and_unapproved_skills() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
    let approval = SkillBundle::candidate(&root).unwrap();
    let bundle = SkillBundle::load(&root, &approval).unwrap();
    assert_eq!(approval.files.len(), 52);
    assert_eq!(bundle.initial_skills().unwrap().len(), 3);
    assert!(bundle.read("../../secret").is_err());
    let mut altered: SkillApproval = approval;
    altered.catalog_hash = "invalid".into();
    assert!(SkillBundle::load(&root, &altered).is_err());
}

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use trady_adapters::{
    opencode::runtime_tools::{RuntimeTools, SkillBundle},
    research_jobs::research_tool_specs,
};
use trady_application::{
    harness::*,
    research_jobs::{ResearchInformation, valid_tool_input},
};
use trady_domain::Result;

#[derive(Default)]
struct ScopedFixture {
    calls: Vec<(Scope, String, Value)>,
}
impl ResearchInformation for ScopedFixture {
    fn information(
        &mut self,
        scope: &Scope,
        name: &str,
        args: &Value,
        _: &str,
        _: DateTime<Utc>,
    ) -> Result<Value> {
        self.calls.push((scope.clone(), name.into(), args.clone()));
        Ok(json!({"job_id":"job_fixture_only","status":"QUEUED"}))
    }
}
fn tools() -> trady_adapters::research_jobs::ResearchTools<ScopedFixture> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
    let approval = SkillBundle::candidate(&root).unwrap();
    RuntimeTools {
        bundle: SkillBundle::load(&root, &approval).unwrap(),
    }
    .with_research(ScopedFixture::default(), true)
}
fn session(tools: &impl ToolExecutor) -> Session {
    Session::new(
        Scope {
            owner_id: "owner_fixture".into(),
            account_id: "live_fixture_information".into(),
            policy_version: 1,
        },
        ProviderIdentity {
            provider: "offline_fixture".into(),
            model: "fixture".into(),
            endpoint: "http://127.0.0.1/fixture".into(),
        },
        RunLimits::default(),
        "No execution authority.".into(),
        tools.revision(),
        vec![],
    )
    .unwrap()
}

#[test]
fn exact_tool_schemas_deny_arbitrary_network_shell_scope_or_probability_inputs() {
    let specs = research_tool_specs(true);
    for spec in &specs {
        assert_eq!(spec.arguments_schema["additionalProperties"], false);
        assert!(!spec.name.contains("shell"));
    }
    let lookup = specs.iter().find(|s| s.name == "sources.lookup").unwrap();
    assert!(valid_tool_input(
        &lookup.arguments_schema,
        &json!({"source_id":"approved_fixture","external_id":"12345"})
    ));
    for input in [
        json!({"source_id":"approved_fixture","external_id":"https://evil.invalid/a"}),
        json!({"source_id":"approved_fixture","external_id":"12345","owner_id":"other"}),
        json!({"url":"http://127.0.0.1/private"}),
    ] {
        assert!(!valid_tool_input(&lookup.arguments_schema, &input));
    }
    let sentiment = specs.iter().find(|s| s.name == "sentiment.assess").unwrap();
    assert!(!valid_tool_input(
        &sentiment.arguments_schema,
        &json!({"packet_id":"packet_fixture","model_revision":"a".repeat(40),"probabilities":{"positive":1}})
    ));
    assert!(
        !research_tool_specs(false)
            .iter()
            .any(|s| s.name.starts_with("sentiment."))
    );
}

#[test]
fn tools_pass_the_persisted_session_scope_and_reject_invalid_or_expired_calls_before_backend_access()
 {
    let mut tools = tools();
    let session = session(&tools);
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(1);
    let out = tools
        .execute(
            "research.request",
            &json!({"instrument_id":"stock_fixture"}),
            &session,
            deadline,
            &cancel,
        )
        .unwrap();
    assert_eq!(out.status, "OK");
    assert_eq!(tools.repository.calls.len(), 1);
    assert_eq!(tools.repository.calls[0].0, session.scope);
    for (name, args) in [
        (
            "sources.lookup",
            json!({"source_id":"fixture","external_id":"123","account_id":"paper_other"}),
        ),
        ("shell.exec", json!({"command":"curl evil"})),
    ] {
        assert_eq!(
            tools
                .execute(name, &args, &session, deadline, &cancel)
                .unwrap()
                .status,
            "DENIED"
        );
    }
    let before = tools.repository.calls.len();
    cancel.cancel();
    assert_eq!(
        tools
            .execute(
                "research.request",
                &json!({"instrument_id":null}),
                &session,
                deadline,
                &cancel
            )
            .unwrap()
            .status,
        "CANCELLED"
    );
    assert_eq!(tools.repository.calls.len(), before);
}

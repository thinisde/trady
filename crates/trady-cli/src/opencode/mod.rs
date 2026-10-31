//! Explicit local owner interface for durable research conversations.

use chrono::Utc;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::Path,
};
use trady_adapters::opencode::{
    llm::{OpenCodeGoConfig, OpenCodeGoProvider},
    runtime_tools::{RuntimeTools, SkillApproval, SkillBundle},
};
use trady_application::{FoundationConfig, contracts::parse_json, harness::*};
use trady_domain::{Error, Result, require};
use trady_storage::sessions::PgSessionRepository;

pub const HELP: &str = "\nLLM conversations (PostgreSQL analyst role):\n  trady skill-manifest SKILL_ROOT\n  trady chat --owner OWNER --approval MANIFEST [--session ID] --message TEXT\n  trady chat --owner OWNER --approval MANIFEST --session ID --resume\n  trady sessions list|show ID|close ID --owner OWNER\n\nChat options: --llm-config FILE --config PAPER_CONFIG --skills-root DIR\n              --message-file FILE --context FILE --stream\nGenerate and review the skill-manifest output, then deploy it with --approval.\nOpenCode Go credentials use OPENCODE_API_KEY; sessions use TRADY_ANALYST_DSN.\nThe local CLI is a trusted owner interface; model text cannot invoke these commands.";

fn options(args: &[String]) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        require(
            matches!(
                key,
                "--owner"
                    | "--approval"
                    | "--session"
                    | "--message"
                    | "--message-file"
                    | "--llm-config"
                    | "--config"
                    | "--skills-root"
                    | "--context"
                    | "--resume"
                    | "--stream"
            ),
            "unknown conversation option",
        )?;
        let value = if matches!(key, "--resume" | "--stream") {
            "true".into()
        } else {
            i += 1;
            args.get(i)
                .ok_or_else(|| Error("missing conversation option value".into()))?
                .clone()
        };
        require(
            out.insert(key.into(), value).is_none(),
            "duplicate conversation option",
        )?;
        i += 1;
    }
    Ok(out)
}
fn read(path: &str) -> Result<String> {
    require(
        fs::metadata(path)
            .map_err(|_| Error("input file unavailable".into()))?
            .len()
            <= 262_144,
        "input file exceeds limit",
    )?;
    fs::read_to_string(path).map_err(|_| Error("cannot read conversation input".into()))
}
fn repository() -> Result<PgSessionRepository> {
    PgSessionRepository::connect(&super::secret("TRADY_ANALYST_DSN")?)
}
fn binding(opts: &BTreeMap<String, String>) -> Result<(Scope, FoundationConfig)> {
    let text = opts
        .get("--config")
        .map(|p| read(p))
        .transpose()?
        .unwrap_or_else(|| include_str!("../../../../config/paper.json").into());
    let config: FoundationConfig = serde_json::from_value(parse_json(&text)?)
        .map_err(|_| Error("invalid paper configuration".into()))?;
    config.validate()?;
    let scope = Scope {
        owner_id: opts
            .get("--owner")
            .ok_or_else(|| Error("--owner is required".into()))?
            .clone(),
        account_id: config.account_id.to_string(),
        policy_version: config.policy.version,
    };
    scope.validate()?;
    Ok((scope, config))
}
pub fn run(command: &str, args: &[String]) -> Result<Value> {
    match command {
        "skill-manifest" => {
            require(args.len() == 1, "usage: trady skill-manifest SKILL_ROOT")?;
            serde_json::to_value(SkillBundle::candidate(Path::new(&args[0]))?)
                .map_err(|_| Error("cannot encode skill manifest".into()))
        }
        "sessions" => sessions(args),
        "chat" => chat(args),
        _ => Err(Error("unsupported conversation command".into())),
    }
}
fn sessions(args: &[String]) -> Result<Value> {
    let action = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error("session action required".into()))?;
    let (id, flags) = if action == "list" {
        (None, &args[1..])
    } else {
        (
            Some(
                args.get(1)
                    .ok_or_else(|| Error("session ID required".into()))?
                    .as_str(),
            ),
            &args[2..],
        )
    };
    let opts = options(flags)?;
    require(
        opts.keys()
            .all(|k| matches!(k.as_str(), "--owner" | "--config")),
        "unsupported session option",
    )?;
    let (scope, _) = binding(&opts)?;
    let mut repo = repository()?;
    match action {
        "list" => {
            let rows:Vec<_>=repo.list(&scope)?.iter().map(|s|json!({"session_id":s.id,"kind":s.kind,"status":s.status,"model":s.provider.model,"last_used_at":s.last_used_at,"expires_at":s.expires_at,"charged_tokens":s.charged_tokens(),"run_count":s.runs.len()})).collect();
            Ok(json!({"sessions":rows,"limit":100}))
        }
        "show" => serde_json::to_value(repo.load(&scope, id.unwrap_or_default())?)
            .map_err(|_| Error("session output failed".into())),
        "close" => {
            let mut lease = repo.acquire(&scope, id.unwrap_or_default(), &new_id("owner"), 30)?;
            let result = {
                if let Some(run) = lease
                    .session
                    .runs
                    .last_mut()
                    .filter(|r| !r.status.terminal())
                {
                    run.status = RunStatus::Cancelled;
                    run.events.push(RunEvent {
                        status: RunStatus::Cancelled,
                        at: Utc::now(),
                    });
                }
                lease.session.status = SessionStatus::Closed;
                repo.save(&mut lease)
            };
            let release = repo.release(&lease);
            result?;
            release?;
            Ok(json!({"session_id":lease.session.id,"status":"CLOSED","entry_authorized":false}))
        }
        _ => Err(Error("unsupported session action".into())),
    }
}
fn chat(args: &[String]) -> Result<Value> {
    let opts = options(args)?;
    let (scope, foundation) = binding(&opts)?;
    let sources = ["--message", "--message-file", "--resume"]
        .into_iter()
        .filter(|k| opts.contains_key(*k))
        .count();
    require(
        sources == 1,
        "supply exactly one of --message, --message-file or --resume",
    )?;
    require(
        !opts.contains_key("--resume") || opts.contains_key("--session"),
        "--resume requires --session",
    )?;
    require(
        !opts.contains_key("--context") || !opts.contains_key("--session"),
        "initial context is pinned when creating a new session",
    )?;
    let config: OpenCodeGoConfig = match opts.get("--llm-config") {
        Some(path) => serde_json::from_value(parse_json(&read(path)?)?)
            .map_err(|_| Error("invalid LLM configuration".into()))?,
        None => OpenCodeGoConfig::default(),
    };
    config.validate()?;
    let approval_path=opts.get("--approval").ok_or_else(||Error("--approval must name a reviewed skill manifest (generate with trady skill-manifest skills)".into()))?;
    let approval: SkillApproval = serde_json::from_value(parse_json(&read(approval_path)?)?)
        .map_err(|_| Error("invalid skill approval manifest".into()))?;
    let bundle = SkillBundle::load(
        Path::new(opts.get("--skills-root").map_or("skills", String::as_str)),
        &approval,
    )?;
    let mut tools = RuntimeTools { bundle };
    let mut provider = OpenCodeGoProvider::from_env(config.clone())?;
    let mut repo = repository()?;
    let session_id = if let Some(id) = opts.get("--session") {
        id.clone()
    } else {
        let governing = serde_json::to_string(&foundation)
            .map_err(|_| Error("governing policy encoding failed".into()))?;
        let mut session = Session::new(
            scope.clone(),
            provider.identity(),
            config.limits,
            governing,
            tools.revision(),
            tools.bundle.initial_skills()?,
        )?;
        if let Some(path) = opts.get("--context") {
            let item: MemoryItem =
                serde_json::from_value(parse_json(&read(path)?)?).map_err(|_| {
                    Error(
                        "context requires id, source, content, as_of and nullable expires_at"
                            .into(),
                    )
                })?;
            require(
                valid_id(&item.id)
                    && !item.source.is_empty()
                    && item.source.len() <= 256
                    && !item.content.is_empty()
                    && item.content.len() <= 16_000
                    && item.as_of <= Utc::now()
                    && item.expires_at.is_none_or(|t| t > Utc::now()),
                "invalid or expired context item",
            )?;
            session.memory.push(item);
        }
        let id = session.id.clone();
        repo.create(&session)?;
        id
    };
    let question = match opts.get("--message-file") {
        Some(path) => Some(read(path)?),
        None => opts.get("--message").cloned(),
    };
    let cancel = CancellationToken::default();
    let signal = cancel.clone();
    ctrlc::set_handler(move || signal.cancel())
        .map_err(|_| Error("cannot initialize cancellation handler".into()))?;
    eprintln!("Session: {session_id}");
    let stream = opts.contains_key("--stream");
    let final_message = AgentHarness::new()?.respond(
        &mut repo,
        &mut provider,
        &mut tools,
        RunInput {
            scope: &scope,
            session_id: &session_id,
            input: question.as_deref(),
            cancel: &cancel,
        },
        &mut |text| {
            if stream {
                eprint!("{text}");
                let _ = io::stderr().flush();
            }
        },
    )?;
    if stream {
        eprintln!();
    }
    let session = repo.load(&scope, &session_id)?;
    Ok(
        json!({"session_id":session_id,"run_id":session.runs.last().map(|r|&r.id),"run_status":session.runs.last().map(|r|r.status),"final":final_message,"charged_tokens":session.charged_tokens(),"entry_authorized":false}),
    )
}

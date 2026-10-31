//! Expanded gateway and its separately credentialed analyst worker.

use chrono::{Timelike, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::atomic::AtomicBool, time::Duration};
use trady_adapters::{
    finbert::{FinBertClient, FinBertConfig},
    news::{MeteredHttp, SafeHttp, SourceClient},
    opencode::{
        llm::{OpenCodeGoConfig, OpenCodeGoProvider},
        runtime_tools::{RuntimeTools, SkillApproval, SkillBundle},
    },
    research_jobs::{collect_registered, lookup_registered},
};
use trady_application::{
    FoundationConfig,
    harness::*,
    research::*,
    research_jobs::*,
    telegram::{CommandReply, Monitor, MonitorCommand, gateway::*},
};
use trady_domain::{Error, Result, require};
use trady_storage::{
    research::PgResearchRepository, research_jobs::PgResearchJobs, sessions::PgSessionRepository,
    x_monitoring::PgXCostRepository,
};

use super::{Config, Loaded, StepError, read_json};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayOwner {
    pub telegram_user_id: i64,
    pub owner_id: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayConfig {
    pub enabled: bool,
    pub bot_id: i64,
    pub owners: Vec<GatewayOwner>,
    pub research_config: String,
    pub finbert_config: Option<String>,
    pub llm_config: Option<String>,
    pub skill_approval: Option<String>,
    pub skills_root: Option<String>,
    pub approved_instrument_ids: BTreeSet<String>,
    pub daily_job_limit: u32,
    pub pending_job_limit: u32,
    pub daily_research_enabled: bool,
    pub daily_research_hour_utc: u32,
    pub holding_watchdog_interval_seconds: u32,
}
impl GatewayConfig {
    pub fn validate(&self, config: &Config, foundation: Option<&FoundationConfig>) -> Result<()> {
        require(
            self.enabled
                && self.bot_id > 0
                && !self.owners.is_empty()
                && self.owners.len() <= 32
                && self.owners.iter().all(|o| {
                    valid_id(&o.owner_id)
                        && config.access.allowed_user_ids.contains(&o.telegram_user_id)
                        && config.access.allowed_chat_ids.contains(&o.telegram_user_id)
                })
                && self
                    .owners
                    .iter()
                    .map(|o| o.telegram_user_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.owners.len()
                && self
                    .owners
                    .iter()
                    .map(|o| &o.owner_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.owners.len()
                && !self.research_config.trim().is_empty()
                && [
                    &self.finbert_config,
                    &self.llm_config,
                    &self.skill_approval,
                    &self.skills_root,
                ]
                .iter()
                .all(|s| s.as_ref().is_none_or(|s| !s.trim().is_empty()))
                && self.approved_instrument_ids.len() <= 1000
                && self.approved_instrument_ids.iter().all(|id| valid_id(id))
                && (1..=1000).contains(&self.daily_job_limit)
                && (1..=100).contains(&self.pending_job_limit)
                && self.daily_research_hour_utc < 24
                && (60..=3600).contains(&self.holding_watchdog_interval_seconds),
            "invalid expanded Telegram gateway owner, scope, path or schedule",
        )?;
        if let Some(f) = foundation {
            for owner in &self.owners {
                self.scope(owner.telegram_user_id, f)?.validate()?;
            }
        }
        Ok(())
    }
    fn scope(&self, user: i64, f: &FoundationConfig) -> Result<Scope> {
        let owner = self
            .owners
            .iter()
            .find(|o| o.telegram_user_id == user)
            .ok_or_else(|| Error("Telegram owner mapping unavailable".into()))?;
        let scope = Scope {
            owner_id: owner.owner_id.clone(),
            account_id: f.account_id.as_str().into(),
            policy_version: f.policy.version,
        };
        scope.validate()?;
        Ok(scope)
    }
    fn policy(
        &self,
        user: i64,
        f: &FoundationConfig,
        research: &ResearchConfig,
        finbert: Option<&FinBertConfig>,
    ) -> Result<JobPolicy> {
        let policy = JobPolicy {
            scope: self.scope(user, f)?,
            mode: f.mode,
            telegram_user_id: user,
            bot_id: self.bot_id,
            source_ids: research.sources.iter().map(|s| s.id.clone()).collect(),
            instrument_ids: self.approved_instrument_ids.clone(),
            registry_hash: digest(research)?,
            sentiment_revision: finbert
                .filter(|f| f.enabled)
                .map(|f| f.model_revision.clone()),
            daily_job_limit: self.daily_job_limit,
            pending_job_limit: self.pending_job_limit,
            enabled: true,
        };
        policy.validate()?;
        Ok(policy)
    }
}

pub struct Gateway {
    repository: PgResearchJobs,
    config: GatewayConfig,
    research: ResearchConfig,
    foundation: FoundationConfig,
}
impl Gateway {
    pub fn open(loaded: &Loaded, bot_id: i64) -> Result<Option<Self>> {
        let Some(config) = loaded
            .config
            .expanded_gateway
            .as_ref()
            .filter(|g| g.enabled)
        else {
            return Ok(None);
        };
        require(
            config.bot_id == bot_id,
            "Telegram bot identity differs from expanded gateway configuration",
        )?;
        Ok(Some(Self {
            repository: PgResearchJobs::connect_gateway(&super::super::secret(
                "TRADY_GATEWAY_DSN",
            )?)?,
            config: config.clone(),
            research: loaded
                .research
                .clone()
                .ok_or_else(|| Error("expanded research registry unavailable".into()))?,
            foundation: loaded.foundation.clone(),
        }))
    }
    fn research_work(&self, target: Option<&str>) -> Result<Work> {
        match target {
            None => Ok(Work::Research {
                instrument_id: None,
            }),
            Some(id) if self.config.approved_instrument_ids.contains(id) => Ok(Work::Research {
                instrument_id: Some(id.into()),
            }),
            Some(url)
                if url.starts_with("https://x.com/") || url.starts_with("https://twitter.com/") =>
            {
                let external_id = trady_adapters::news::x_post_id(url)?;
                let source = self
                    .research
                    .sources
                    .iter()
                    .find(|s| s.enabled && s.kind == trady_domain::research::SourceKind::XLookup)
                    .ok_or_else(|| Error("registered X lookup source unavailable".into()))?;
                Ok(Work::SourceLookup {
                    source_id: source.id.clone(),
                    external_id,
                })
            }
            _ => Err(Error(
                "unregistered research target; no URL was fetched".into(),
            )),
        }
    }
    pub fn reply(
        &mut self,
        update: &trady_application::telegram::IncomingUpdate,
        bot_id: i64,
        username: &str,
        access: &trady_application::telegram::OwnerAccess,
        monitor: &Monitor,
        now: chrono::DateTime<Utc>,
    ) -> Result<Option<CommandReply>> {
        let Some(message) = update.message.as_ref() else {
            return Ok(None);
        };
        if !access.authorizes(message, now) {
            return Ok(None);
        }
        let Ok(scope) = self.config.scope(message.sender_id, &self.foundation) else {
            return Ok(None);
        };
        let Some(request) = authenticate(access, update, bot_id, username, &scope, now)? else {
            return Ok(None);
        };
        let work = match &request.command {
            GatewayCommand::Research(target) => self.research_work(target.as_deref()).ok(),
            _ => None,
        };
        let monitor_command = match request.command {
            GatewayCommand::Monitor(command) => command,
            _ => MonitorCommand::Unsupported,
        };
        let text = self.repository.accept_gateway(
            &request,
            work,
            &monitor.render(monitor_command, now),
            now,
        )?;
        Ok(Some(CommandReply {
            update_id: request.update_id,
            sender_id: request.sender_id,
            chat_id: request.chat_id,
            command: monitor_command,
            text,
        }))
    }
    pub fn drain(
        &mut self,
        api: &trady_adapters::telegram::TelegramClient,
        access: &trady_application::telegram::OwnerAccess,
        stopped: &AtomicBool,
    ) -> std::result::Result<u64, StepError> {
        let mut sent = 0;
        for delivery in self.repository.pending_deliveries(self.config.bot_id, 20)? {
            if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let eligible = access.allowed_user_ids.contains(&delivery.owner_user_id)
                && access.allowed_chat_ids.contains(&delivery.chat_id)
                && self
                    .config
                    .scope(delivery.owner_user_id, &self.foundation)
                    .is_ok_and(|scope| scope == delivery.scope)
                && self.repository.policy(&delivery.scope).is_ok_and(|p| {
                    p.enabled
                        && p.bot_id == delivery.bot_id
                        && p.telegram_user_id == delivery.owner_user_id
                });
            if eligible {
                api.send_message(delivery.chat_id, &delivery.text)?;
                sent += 1;
            }
            self.repository
                .acknowledge_delivery(self.config.bot_id, &delivery.id)?;
        }
        Ok(sent)
    }
}

fn llm_config(loaded: &Loaded, config: &GatewayConfig) -> Result<OpenCodeGoConfig> {
    let config = match &config.llm_config {
        Some(path) => read_json(&loaded.base_directory.join(path))?,
        None => OpenCodeGoConfig::default(),
    };
    config.validate()?;
    Ok(config)
}
fn tools(
    loaded: &Loaded,
    config: &GatewayConfig,
    dsn: &str,
    sentiment_enabled: bool,
) -> Result<trady_adapters::research_jobs::ResearchTools<PgResearchJobs>> {
    let approval_path = config
        .skill_approval
        .as_ref()
        .ok_or_else(|| Error("reviewed skill approval required for owner dialogue".into()))?;
    let approval: SkillApproval = read_json(&loaded.base_directory.join(approval_path))?;
    let root = config.skills_root.as_deref().unwrap_or("skills");
    let bundle = SkillBundle::load(&loaded.base_directory.join(root), &approval)?;
    Ok(RuntimeTools { bundle }
        .with_research(PgResearchJobs::connect_analyst(dsn)?, sentiment_enabled))
}
fn new_session(
    loaded: &Loaded,
    config: &GatewayConfig,
    scope: &Scope,
    dsn: &str,
    repo: &mut PgSessionRepository,
    sentiment_enabled: bool,
) -> Result<Session> {
    let llm = llm_config(loaded, config)?;
    let tools = tools(loaded, config, dsn, sentiment_enabled)?;
    let session = Session::new(
        scope.clone(),
        ProviderIdentity {
            provider: "opencode-go".into(),
            model: llm.model.clone(),
            endpoint: llm.endpoint(),
        },
        llm.limits,
        serde_json::to_string(&loaded.foundation)
            .map_err(|_| Error("cannot encode dialogue policy context".into()))?,
        tools.revision(),
        tools.base.bundle.initial_skills()?,
    )?;
    repo.create(&session)?;
    Ok(session)
}
fn eligible(session: &Session) -> bool {
    session.kind == SessionKind::OwnerResearch
        && matches!(
            session.status,
            SessionStatus::Active | SessionStatus::WaitingOwner
        )
        && session.expires_at > Utc::now()
}
fn queue_collectors(
    research: &ResearchConfig,
    policy: &JobPolicy,
    jobs: &mut PgResearchJobs,
    lease: &JobLease,
    holding: Option<HoldingWatch>,
) -> Result<Value> {
    require(
        digest(research)? == policy.registry_hash,
        "collector registry changed",
    )?;
    let mut children = vec![];
    let mut gaps = vec![];
    for source in research.sources.iter().filter(|s| {
        policy.source_ids.contains(&s.id) && s.kind != trady_domain::research::SourceKind::XLookup
    }) {
        if !source.enabled {
            gaps.push(format!("{}:source_disabled", source.id));
            continue;
        }
        let key = format!(
            "collector_{}",
            digest(&(&lease.job.id, &source.id, &holding))?
        );
        match jobs.enqueue(
            &policy.scope,
            &key,
            &Work::CollectSource {
                source_id: source.id.clone(),
                holding: holding.clone(),
            },
        ) {
            Ok(job) => {
                children.push(json!({"source_id":source.id,"job_id":job.id,"status":job.status}))
            }
            Err(_) => gaps.push(format!("{}:queue_budget_or_scope_unavailable", source.id)),
        }
    }
    Ok(
        json!({"collection_jobs":children,"gaps":gaps,"verification_status":"COLLECTION_QUEUED_NOT_VERIFIED","entry_authorized":false}),
    )
}
fn dialogue(
    loaded: &Loaded,
    config: &GatewayConfig,
    dsn: &str,
    jobs: &mut PgResearchJobs,
    lease: &JobLease,
) -> Result<Value> {
    let scope = &lease.job.scope;
    let mut repo = PgSessionRepository::connect(dsn)?;
    let policy = jobs.policy(scope)?;
    let sentiment_enabled = policy.sentiment_revision.is_some();
    if let Work::Session { action, session_id } = &lease.job.work {
        return match action.as_str() {
            "list" => {
                let rows:Vec<_>=repo.list(scope)?.into_iter().filter(|s|s.kind==SessionKind::OwnerResearch).take(10)
                    .map(|s|json!({"session_id":s.id,"subject":s.subject,"status":s.status,"model":s.provider.model,"charged_tokens":s.charged_tokens(),"last_used_at":s.last_used_at})).collect();
                Ok(json!({"sessions":rows,"entry_authorized":false}))
            }
            "new" => {
                let session =
                    new_session(loaded, config, scope, dsn, &mut repo, sentiment_enabled)?;
                jobs.select_session(scope, &session.id)?;
                Ok(json!({"session_id":session.id,"status":"SELECTED","entry_authorized":false}))
            }
            "show" | "use" | "close" => {
                let id = session_id
                    .as_deref()
                    .ok_or_else(|| Error("session ID required".into()))?;
                let session = repo.load(scope, id)?;
                require(
                    session.kind == SessionKind::OwnerResearch,
                    "scheduled sessions cannot be selected or closed by the owner gateway",
                )?;
                if action == "use" {
                    require(eligible(&session), "session closed or expired")?;
                    jobs.select_session(scope, id)?;
                }
                if action == "close" {
                    let mut locked = repo.acquire(scope, id, &new_id("gateway_session"), 30)?;
                    require(
                        locked.session.active_run().is_none(),
                        "session has an active model run",
                    )?;
                    locked.session.status = SessionStatus::Closed;
                    repo.save(&mut locked)?;
                    repo.release(&locked)?;
                }
                Ok(
                    json!({"session_id":session.id,"status":if action=="close"{SessionStatus::Closed}else{session.status},"summary":session.summaries.last(),"model":session.provider.model,"charged_tokens":session.charged_tokens(),"expires_at":session.expires_at,"entry_authorized":false}),
                )
            }
            _ => Err(Error("unsupported owner session action".into())),
        };
    }
    let Work::Dialogue { text, session_id } = &lease.job.work else {
        return Err(Error("not a dialogue job".into()));
    };
    let recovered_id = lease
        .job
        .result
        .as_ref()
        .and_then(|r| r["resume_session_id"].as_str());
    let requested = recovered_id
        .map(String::from)
        .or_else(|| session_id.clone())
        .or(jobs.selected_session(scope)?);
    let mut session = if let Some(id) = requested {
        match repo.load(scope, &id) {
            Ok(s) if eligible(&s) => s,
            _ if recovered_id.is_none() && session_id.is_none() => {
                new_session(loaded, config, scope, dsn, &mut repo, sentiment_enabled)?
            }
            _ => {
                return Err(Error(
                    "dialogue session unavailable in current owner/account scope".into(),
                ));
            }
        }
    } else {
        new_session(loaded, config, scope, dsn, &mut repo, sentiment_enabled)?
    };
    jobs.select_session(scope, &session.id)?;
    let baseline = lease
        .job
        .result
        .as_ref()
        .and_then(|r| r["baseline_runs"].as_u64())
        .map_or(session.runs.len(), |n| n as usize);
    jobs.bind_job_session(lease, &session.id, baseline)?;
    let final_message = if session.runs.len() > baseline
        && session.runs.last().is_some_and(|r| r.status.terminal())
    {
        session
            .runs
            .last()
            .and_then(|r| r.final_message.clone())
            .ok_or_else(|| Error("previous dialogue run ended without final reply".into()))?
    } else {
        let mut provider = OpenCodeGoProvider::from_env(llm_config(loaded, config)?)?;
        let mut tools = tools(loaded, config, dsn, sentiment_enabled)?;
        let cancel = CancellationToken::default();
        AgentHarness::new()?.respond(
            &mut repo,
            &mut provider,
            &mut tools,
            RunInput {
                scope,
                session_id: &session.id,
                input: if session.runs.len() > baseline {
                    None
                } else {
                    Some(text)
                },
                cancel: &cancel,
            },
            &mut |_| {},
        )?
    };
    session = repo.load(scope, &session.id)?;
    let mut reply = final_message["reply"]["text"]
        .as_str()
        .unwrap_or("Model did not return a displayable reply.")
        .chars()
        .take(2200)
        .collect::<String>();
    if let Some(questions) = final_message["reply"]["questions"].as_array() {
        for q in questions.iter().take(3) {
            reply.push_str(&format!(
                "\n{}",
                q.as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect::<String>()
            ));
        }
    }
    if let Some(ids) = final_message["reply"]["evidence_ids"]
        .as_array()
        .filter(|a| !a.is_empty())
    {
        reply.push_str(&format!(
            "\nEvidence references: {}",
            Value::Array(ids.iter().take(5).cloned().collect())
        ));
    }
    Ok(
        json!({"session_id":session.id,"run_id":session.runs.last().map(|r|&r.id),"run_status":session.runs.last().map(|r|r.status),"reply_text":reply,"charged_tokens":session.charged_tokens(),"entry_authorized":false}),
    )
}

pub fn worker(loaded: Loaded, once: bool) -> Result<Value> {
    let config = loaded
        .config
        .expanded_gateway
        .as_ref()
        .filter(|g| g.enabled)
        .ok_or_else(|| Error("telegram worker requires expanded_gateway.enabled=true".into()))?;
    let research = loaded
        .research
        .as_ref()
        .ok_or_else(|| Error("research registry unavailable".into()))?;
    let dsn = super::super::secret("TRADY_ANALYST_DSN")?;
    let mut jobs = PgResearchJobs::connect_analyst(&dsn)?;
    let mut observations = PgResearchRepository::connect(&dsn)?;
    let finbert: Option<FinBertConfig> = config
        .finbert_config
        .as_ref()
        .map(|p| read_json::<FinBertConfig>(&loaded.base_directory.join(p)))
        .transpose()?;
    if let Some(f) = &finbert {
        f.validate()?;
    }
    for source in &research.sources {
        let old = observations.state(&source.id)?;
        let next = prepare_state(source, old.clone())?;
        if old.registry_hash != next.registry_hash {
            observations.commit_page(&source.id, old.revision, &next, &[], Utc::now())?;
        }
    }
    for owner in &config.owners {
        jobs.install_policy(&config.policy(
            owner.telegram_user_id,
            &loaded.foundation,
            research,
            finbert.as_ref(),
        )?)?;
    }
    let ledger = PgXCostRepository::connect(&dsn)?;
    let mut client = SourceClient::with_transport(
        MeteredHttp::new(SafeHttp, ledger, research.x_budget.clone()),
        |name| trady_adapters::credentials::optional(name).map(|s| s.map(|s| s.into_inner())),
    );
    let stopped = std::sync::Arc::new(AtomicBool::new(false));
    if !once {
        let flag = stopped.clone();
        ctrlc::set_handler(move || flag.store(true, std::sync::atomic::Ordering::Relaxed))
            .map_err(|_| Error("cannot install worker shutdown handler".into()))?;
    }
    let worker_id = new_id("research_worker");
    let mut outcomes = vec![];
    loop {
        for owner in &config.owners {
            if stopped.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let scope = config.scope(owner.telegram_user_id, &loaded.foundation)?;
            let now = Utc::now();
            jobs.invalidate_dependencies(&scope, now)?;
            jobs.queue_pending_sentiment(&scope, now)?;
            let holdings = jobs.holding_watches(&scope, now)?;
            if config.daily_research_enabled && now.hour() >= config.daily_research_hour_utc {
                let key = daily_key(&scope, now.date_naive())?;
                // Budget exhaustion prevents new work but never stops draining
                // already accepted work or handling pause controls in gateway.
                let _ = jobs.enqueue(
                    &scope,
                    &key,
                    &Work::Daily {
                        session_date: now.date_naive().to_string(),
                    },
                );
            }
            if let Some(holdings) = &holdings {
                for holding in holdings {
                    let key = watchdog_key(
                        &scope,
                        holding,
                        now,
                        config.holding_watchdog_interval_seconds,
                    )?;
                    let _ = jobs.enqueue(
                        &scope,
                        &key,
                        &Work::HoldingWatchdog {
                            holding_id: holding.holding_id.clone(),
                            instrument_id: holding.instrument_id.clone(),
                            holding_revision: holding.revision_hash.clone(),
                        },
                    );
                }
                jobs.cancel_closed_watchdogs(&scope, holdings)?;
            }
            let Some(lease) = jobs.claim(&scope, &worker_id, 300)? else {
                continue;
            };
            let policy = jobs.policy(&scope)?;
            let result = match &lease.job.work {
                Work::Sentiment { .. } => match finbert.as_ref().filter(|f| f.enabled) {
                    Some(f) => FinBertClient::new(f.clone())
                        .and_then(|p| score_sentiment_job(&mut jobs, &p, &lease, Utc::now())),
                    None => Err(Error("FinBERT is disabled; no assessment produced".into())),
                },
                Work::Dialogue { .. } | Work::Session { .. } => {
                    dialogue(&loaded, config, &dsn, &mut jobs, &lease)
                }
                Work::SourceLookup {
                    source_id,
                    external_id,
                } => lookup_registered(
                    research,
                    &policy,
                    source_id,
                    external_id,
                    &mut client,
                    &mut observations,
                    Utc::now(),
                ),
                Work::CollectSource { source_id, holding } => {
                    let valid_holding = if let Some(h) = holding {
                        jobs.holding_watches(&scope, Utc::now())?
                            .is_some_and(|current| current.contains(h))
                    } else {
                        true
                    };
                    if !valid_holding {
                        Err(Error("collection watchdog dependency changed".into()))
                    } else {
                        let mut selected = policy.clone();
                        selected.source_ids = BTreeSet::from([source_id.clone()]);
                        collect_registered(
                            research,
                            &selected,
                            &mut client,
                            &mut observations,
                            Utc::now(),
                        )
                    }
                }
                Work::HoldingWatchdog {
                    holding_id,
                    holding_revision,
                    instrument_id,
                } => {
                    if !jobs.holding_watches(&scope, Utc::now())?.is_some_and(|hs| {
                        hs.iter().any(|h| {
                            &h.holding_id == holding_id
                                && &h.revision_hash == holding_revision
                                && &h.instrument_id == instrument_id
                        })
                    }) {
                        Err(Error(
                            "holding watchdog no longer matches current reconciled holding".into(),
                        ))
                    } else {
                        queue_collectors(
                            research,
                            &policy,
                            &mut jobs,
                            &lease,
                            Some(HoldingWatch {
                                holding_id: holding_id.clone(),
                                instrument_id: instrument_id.clone(),
                                revision_hash: holding_revision.clone(),
                                active: true,
                            }),
                        )
                    }
                }
                Work::Research { .. } | Work::Daily { .. } => {
                    queue_collectors(research, &policy, &mut jobs, &lease, None)
                }
            };
            match result {
                Ok(value) => {
                    if !matches!(lease.job.work, Work::Sentiment { .. }) {
                        jobs.complete(&lease, &value)?;
                    }
                    outcomes
                        .push(json!({"job_id":lease.job.id,"status":"COMPLETE","result":value}));
                }
                Err(_) => {
                    let code = match lease.job.work {
                        Work::Sentiment { .. } => "SENTIMENT_PROVIDER_OR_DEPENDENCY_UNAVAILABLE",
                        Work::Dialogue { .. } | Work::Session { .. } => {
                            "DIALOGUE_PROVIDER_SKILL_OR_SESSION_UNAVAILABLE"
                        }
                        _ => "RESEARCH_PROVIDER_OR_SCOPE_UNAVAILABLE",
                    };
                    jobs.fail(&lease, code)?;
                    outcomes
                        .push(json!({"job_id":lease.job.id,"status":"FAILED","error_code":code}));
                }
            }
        }
        if once || stopped.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        outcomes.clear();
        super::sleep_until_stopped(Duration::from_secs(5), &stopped);
    }
    Ok(
        json!({"service":"telegram_research_worker","status":if once{"poll_complete"}else{"stopped"},"outcomes":outcomes,"entry_authorized":false}),
    )
}

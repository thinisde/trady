//! Typed expanded gateway. Authentication precedes parsing; commands cannot change mode.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use trady_domain::{Mode, Result, require};

use super::{IncomingUpdate, MonitorCommand, OwnerAccess};
use crate::{
    harness::{Scope, valid_id},
    research_jobs::{digest, hash_id},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "command",
    content = "argument",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub enum GatewayCommand {
    Monitor(MonitorCommand),
    Ask(String),
    Research(Option<String>),
    Jobs(Option<String>),
    Report {
        name: String,
        subject: Option<String>,
    },
    Session {
        action: String,
        subject: Option<String>,
    },
    PauseEntries,
    ResumeEntries,
    ClosePosition,
    Confirm(String),
    CancelConfirmation(String),
    Unsupported(String),
}
impl GatewayCommand {
    pub fn parse(text: &str, bot_username: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if !text.starts_with('/') {
            return Some(
                if text.starts_with("https://x.com/") || text.starts_with("https://twitter.com/") {
                    Self::Research(Some(text.into()))
                } else {
                    Self::Ask(text.into())
                },
            );
        }
        let (head, tail) = text
            .split_once(char::is_whitespace)
            .map_or((text, ""), |(a, b)| (a, b.trim()));
        let head = match head.split_once('@') {
            Some((_, who)) if !who.eq_ignore_ascii_case(bot_username) => return None,
            Some((head, _)) => head,
            None => head,
        };
        let parts: Vec<_> = tail.split_whitespace().collect();
        Some(match (head,parts.as_slice()) {
            ("/ask", _) if !tail.is_empty() => Self::Ask(tail.into()),
            ("/research", []) => Self::Research(None),
            ("/research", [target]) => Self::Research(Some((*target).into())),
            ("/jobs", []) => Self::Jobs(None),
            ("/jobs", [id]) if valid_id(id) => Self::Jobs(Some((*id).into())),
            ("/pause_entries"|"/pause", []) => Self::PauseEntries,
            ("/resume_entries"|"/resume", []) => Self::ResumeEntries,
            ("/close_position"|"/close", []) => Self::ClosePosition,
            ("/confirm", [token]) if valid_id(token) => Self::Confirm((*token).into()),
            ("/cancel-confirm", [token]) if valid_id(token) => Self::CancelConfirmation((*token).into()),
            ("/sessions", []) => Self::Session { action:"list".into(), subject:None },
            ("/session", ["new"]) => Self::Session { action:"new".into(), subject:None },
            ("/session", [action,id]) if matches!(*action,"show"|"use"|"close") && valid_id(id) =>
                Self::Session { action:(*action).into(), subject:Some((*id).into()) },
            ("/plan"|"/sources"|"/budget"|"/watchlist"|"/policy"|"/sectors"|"/influencers"|"/watchdog"|"/perpetual", []) =>
                Self::Report { name:head[1..].into(), subject:None },
            ("/company"|"/relations"|"/sectors"|"/perpetual", [subject]) if valid_id(subject) =>
                Self::Report { name:head[1..].into(), subject:Some((*subject).into()) },
            ("/report", ["today"|"week"]) => Self::Report { name:"performance".into(),subject:Some(tail.into()) },
            ("/performance", []) => Self::Report { name:"performance".into(),subject:Some("today".into()) },
            ("/memory", _) => Self::Unsupported("Memory administration is unavailable in this gateway. Dialogue retains its scoped persistent session; notes cannot change policy.".into()),
            ("/mode", []) => Self::Monitor(MonitorCommand::Mode),
            ("/mode", _) => Self::Unsupported("Mode changes require deployment configuration.".into()),
            _ => match MonitorCommand::parse(head, bot_username) {
                Some(command) if command != MonitorCommand::Unsupported && tail.is_empty() => Self::Monitor(command),
                _ if head=="/t212" && parts.len()==1 => match parts[0] {
                    "account" => Self::Monitor(MonitorCommand::Balance),
                    "positions" => Self::Monitor(MonitorCommand::Position),
                    "orders" => Self::Monitor(MonitorCommand::Orders),
                    _ => Self::Unsupported("Broker history is unavailable through this gateway. No broker operation was requested.".into()),
                },
                _ => Self::Unsupported("Unsupported command. Use /help for the implemented commands. No change was applied.".into()),
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct GatewayRequest {
    pub bot_id: i64,
    pub update_id: i64,
    pub sender_id: i64,
    pub chat_id: i64,
    pub scope: Scope,
    pub command: GatewayCommand,
}
pub fn authenticate(
    access: &OwnerAccess,
    update: &IncomingUpdate,
    bot_id: i64,
    username: &str,
    scope: &Scope,
    now: DateTime<Utc>,
) -> Result<Option<GatewayRequest>> {
    scope.validate()?;
    let Some(message) = update.message.as_ref() else {
        return Ok(None);
    };
    if bot_id <= 0 || update.update_id < 0 || !access.authorizes(message, now) {
        return Ok(None);
    }
    Ok(
        GatewayCommand::parse(&message.text, username).map(|command| GatewayRequest {
            bot_id,
            update_id: update.update_id,
            sender_id: message.sender_id,
            chat_id: message.chat_id,
            scope: scope.clone(),
            command,
        }),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OwnerControlKind {
    PauseEntries,
    ResumeEntries,
    ClosePosition,
}

/// Executor publishes this after reconciliation. A new plan, quantity, holding
/// or policy produces a new hash, invalidating every old sensitive preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerControlContext {
    pub scope: Scope,
    pub mode: Mode,
    pub plan_revision: Option<String>,
    pub holding_revision: Option<String>,
    pub holding_id: Option<String>,
    pub instrument_id: Option<String>,
    pub quantity: Option<String>,
    pub observed_at: DateTime<Utc>,
}
impl OwnerControlContext {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        self.scope.validate()?;
        require(
            self.scope.account_id.starts_with(match self.mode {
                Mode::Paper => "paper_",
                Mode::Shadow => "shadow_",
                Mode::Live => "live_",
            }) && self.observed_at <= now
                && now - self.observed_at <= chrono::Duration::seconds(60)
                && self.plan_revision.as_ref().is_none_or(|s| hash_id(s))
                && self.holding_revision.as_ref().is_none_or(|s| hash_id(s))
                && self.holding_id.as_ref().is_none_or(|s| valid_id(s))
                && self.instrument_id.as_ref().is_none_or(|s| valid_id(s))
                && self.quantity.as_ref().is_none_or(|s| {
                    s.len() <= 64
                        && s.parse::<rust_decimal::Decimal>()
                            .is_ok_and(|q| q > rust_decimal::Decimal::ZERO)
                })
                && self.instrument_id.is_some() == self.quantity.is_some()
                && self.instrument_id.is_some() == self.holding_revision.is_some()
                && self.instrument_id.is_some() == self.holding_id.is_some(),
            "owner control context is stale or inconsistent",
        )
    }
    /// Observation time is excluded: an unchanged reconciliation refresh should
    /// not invalidate a preview. All financial/version bindings are included.
    pub fn binding_hash(&self) -> Result<String> {
        digest(&(
            &self.scope,
            self.mode,
            &self.plan_revision,
            &self.holding_revision,
            &self.holding_id,
            &self.instrument_id,
            &self.quantity,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerControlRequest {
    pub id: String,
    pub kind: OwnerControlKind,
    pub scope: Scope,
    pub bot_id: i64,
    pub owner_user_id: i64,
    pub chat_id: i64,
    pub context: OwnerControlContext,
    pub context_hash: String,
    pub requested_at: DateTime<Utc>,
}

pub const GATEWAY_HELP: &str = "Commands: /status /mode /balance /position /orders /plan /research [approved-instrument-or-registered-X-URL] /ask TEXT /jobs [id] /sources /budget /company INSTRUMENT /relations INSTRUMENT /sectors [code] /watchlist /policy /report today|week /watchdog /influencers /perpetual [id] /sessions /session new|show ID|use ID|close ID /pause_entries /resume_entries /close_position /confirm TOKEN /cancel-confirm TOKEN.\nResearch and dialogue are queued. Missing providers/evidence are reported as unavailable. Owner controls request executor work; acknowledgement never proves execution. Mode changes and policy editing require deployment configuration.";

//! Owner authentication and read-only monitoring. No execution or model ports.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use trady_domain::{AccountId, AccountSnapshot, Currency, Mode, Result, require};

use crate::FoundationConfig;

const MAX_ID: i64 = (1_i64 << 52) - 1;
const MAX_COMMAND_BYTES: usize = 4096;
const MAX_COMMAND_AGE_SECONDS: i64 = 300;
const MAX_DISPLAYED_ITEMS: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerAccess {
    pub allowed_user_ids: Vec<i64>,
    pub allowed_chat_ids: Vec<i64>,
}

impl OwnerAccess {
    pub fn validate(&self) -> Result<()> {
        for ids in [&self.allowed_user_ids, &self.allowed_chat_ids] {
            require(
                !ids.is_empty()
                    && ids.len() <= 32
                    && ids.iter().all(|id| (1..=MAX_ID).contains(id))
                    && ids.iter().collect::<std::collections::BTreeSet<_>>().len() == ids.len(),
                "Telegram requires nonempty, unique numeric private owner/chat allowlists",
            )?;
        }
        require(
            self.allowed_chat_ids
                .iter()
                .all(|id| self.allowed_user_ids.contains(id)),
            "each Telegram private chat must belong to an allowed user",
        )
    }

    pub fn authorizes(&self, message: &IncomingMessage, now: DateTime<Utc>) -> bool {
        let age = now.signed_duration_since(message.sent_at).num_seconds();
        message.private_chat
            && !message.sender_is_bot
            && message.sender_id == message.chat_id
            && self.allowed_user_ids.contains(&message.sender_id)
            && self.allowed_chat_ids.contains(&message.chat_id)
            && (0..=MAX_COMMAND_AGE_SECONDS).contains(&age)
            && message.text.len() <= MAX_COMMAND_BYTES
    }
}

#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub sender_id: i64,
    pub chat_id: i64,
    pub private_chat: bool,
    pub sender_is_bot: bool,
    pub sent_at: DateTime<Utc>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct IncomingUpdate {
    pub update_id: i64,
    pub message: Option<IncomingMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorCommand {
    Help,
    Status,
    Mode,
    Balance,
    Position,
    Orders,
    Unsupported,
}

impl MonitorCommand {
    fn parse(text: &str, bot_username: &str) -> Option<Self> {
        let parts: Vec<_> = text.split_whitespace().collect();
        let name = parts.first()?;
        let name = if let Some((name, recipient)) = name.split_once('@') {
            if !recipient.eq_ignore_ascii_case(bot_username) {
                return None;
            }
            name
        } else {
            name
        };
        Some(match (name, &parts[1..]) {
            ("/start" | "/help", []) => Self::Help,
            ("/status" | "/health", []) => Self::Status,
            ("/mode", []) => Self::Mode,
            ("/balance", []) | ("/t212", ["account"]) => Self::Balance,
            ("/position" | "/positions", []) | ("/t212", ["positions"]) => Self::Position,
            ("/orders", []) | ("/t212", ["orders"]) => Self::Orders,
            _ => Self::Unsupported,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CommandReply {
    pub update_id: i64,
    pub sender_id: i64,
    pub chat_id: i64,
    pub command: MonitorCommand,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct BrokerScope {
    pub account_id: AccountId,
    pub mode: Mode,
    pub currency: Currency,
}

pub struct Monitor {
    access: OwnerAccess,
    foundation: FoundationConfig,
    scope: Option<BrokerScope>,
    snapshot: Option<AccountSnapshot>,
    degraded: bool,
    stale_after_seconds: i64,
    started_at: DateTime<Utc>,
}

impl Monitor {
    pub fn new(
        access: OwnerAccess,
        foundation: FoundationConfig,
        scope: Option<BrokerScope>,
        stale_after_seconds: i64,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        access.validate()?;
        foundation.validate()?;
        require(
            (60..=3600).contains(&stale_after_seconds),
            "Telegram snapshot freshness must be 60 through 3600 seconds",
        )?;
        Ok(Self {
            access,
            foundation,
            scope,
            snapshot: None,
            degraded: false,
            stale_after_seconds,
            started_at: now,
        })
    }

    /// Invalid, stale, regressing or incorrectly bound data never replaces the cache.
    /// Returns an alert only on changed data, first failure or recovery.
    pub fn observe(
        &mut self,
        result: Result<AccountSnapshot>,
        now: DateTime<Utc>,
    ) -> Option<String> {
        let valid = result.ok().filter(|snapshot| {
            self.scope.as_ref().is_some_and(|scope| {
                snapshot.account_id == scope.account_id
                    && snapshot.mode == scope.mode
                    && snapshot.spendable_cash.currency == scope.currency
            }) && snapshot.validate().is_ok()
                && snapshot.observed_at <= now
                && now
                    .signed_duration_since(snapshot.observed_at)
                    .num_seconds()
                    <= self.stale_after_seconds
                && self
                    .snapshot
                    .as_ref()
                    .is_none_or(|old| snapshot.observed_at >= old.observed_at)
        });
        let Some(mut snapshot) = valid else {
            let first_failure = !self.degraded;
            self.degraded = true;
            return first_failure.then(|| {
                format!(
                    "Broker monitoring degraded. Account data could not be refreshed.\n{}",
                    self.render(MonitorCommand::Status, now)
                )
            });
        };
        snapshot
            .positions
            .sort_by(|a, b| a.instrument_id.cmp(&b.instrument_id));
        snapshot
            .orders
            .sort_by(|a, b| a.broker_order_id.cmp(&b.broker_order_id));
        let changed = self.snapshot.as_ref().is_some_and(|old| {
            old.spendable_cash != snapshot.spendable_cash
                || old.positions != snapshot.positions
                || old.orders != snapshot.orders
        });
        let recovered = self.degraded;
        self.degraded = false;
        self.snapshot = Some(snapshot);
        (changed || recovered).then(|| {
            format!(
                "{}\n{}\n{}\n{}",
                if recovered {
                    "Broker monitoring recovered."
                } else {
                    "Broker account changed."
                },
                self.render(MonitorCommand::Balance, now),
                self.render(MonitorCommand::Position, now),
                self.render(MonitorCommand::Orders, now)
            )
        })
    }

    pub fn reply(
        &self,
        update: &IncomingUpdate,
        bot_username: &str,
        now: DateTime<Utc>,
    ) -> Option<CommandReply> {
        if update.update_id < 0 {
            return None;
        }
        let message = update.message.as_ref()?;
        if !self.access.authorizes(message, now) {
            return None;
        }
        let command = MonitorCommand::parse(&message.text, bot_username)?;
        Some(CommandReply {
            update_id: update.update_id,
            sender_id: message.sender_id,
            chat_id: message.chat_id,
            command,
            text: self.render(command, now),
        })
    }

    pub fn render(&self, command: MonitorCommand, now: DateTime<Utc>) -> String {
        let mode = match self.foundation.mode {
            Mode::Paper => "PAPER",
            Mode::Shadow => "SHADOW",
            Mode::Live => "LIVE",
        };
        let mut text = format!("Trady · read-only monitor\nDeployment: {mode}\n");
        if let Some(scope) = &self.scope {
            text.push_str(&format!(
                "Broker account: {} ({:?}, observation only)\n",
                scope.account_id, scope.mode
            ));
        }
        if matches!(command, MonitorCommand::Help | MonitorCommand::Unsupported) {
            text.push_str("Commands: /status /mode /balance /position /orders /help\nAliases: /health, /positions, /t212 account|positions|orders.\nTrading controls and research commands are unavailable.");
            return text;
        }
        if command == MonitorCommand::Mode {
            text.push_str(
                "Execution is not enabled. Mode changes require deployment configuration.",
            );
            return text;
        }
        if command == MonitorCommand::Status {
            text.push_str(&format!(
                "Foundation account: {}\nPolicy version: {}\nMonitor uptime: {} seconds\nExecution: not enabled\nOther services: not supervised by this monitor\n",
                self.foundation.account_id,
                self.foundation.policy.version,
                now.signed_duration_since(self.started_at).num_seconds().max(0)
            ));
            for blocker in self.foundation.entry_blockers(now) {
                text.push_str(&format!("Entry blocked: {blocker}\n"));
            }
        }
        let Some(snapshot) = &self.snapshot else {
            text.push_str(if self.scope.is_none() {
                "Broker data: not configured. No balance, position or order observation is available."
            } else {
                "Broker data: unavailable; no successful snapshot."
            });
            return text;
        };
        let age = now
            .signed_duration_since(snapshot.observed_at)
            .num_seconds();
        let stale = self.degraded || age < 0 || age > self.stale_after_seconds;
        text.push_str(&format!(
            "Broker data: {}\nLast successful sync: {} (age: {age}s)\n",
            if stale {
                "STALE / degraded"
            } else {
                "available (cached, non-atomic snapshot)"
            },
            snapshot.observed_at.to_rfc3339()
        ));
        match command {
            MonitorCommand::Status => text.push_str(&format!(
                "Holdings: {} · pending orders: {}\nActive plan and protection state: unavailable.",
                snapshot.positions.len(), snapshot.orders.len()
            )),
            MonitorCommand::Balance => text.push_str(&format!(
                "Spendable cash: {} {}\nEquity, reserved/unsettled funds and performance: unavailable in this snapshot.",
                snapshot.spendable_cash.amount, snapshot.spendable_cash.currency.as_str()
            )),
            MonitorCommand::Position => {
                text.push_str(&format!("Holdings: {}\n", snapshot.positions.len()));
                for position in snapshot.positions.iter().take(MAX_DISPLAYED_ITEMS) {
                    text.push_str(&format!("{} · quantity {}\n", position.instrument_id, position.quantity));
                }
                if snapshot.positions.len() > MAX_DISPLAYED_ITEMS {
                    text.push_str("Only the first 20 holdings are shown.\n");
                }
                text.push_str("Cost, return, active plan, target/stop and protection state: unavailable.");
            }
            MonitorCommand::Orders => {
                text.push_str(&format!("Pending orders: {}\n", snapshot.orders.len()));
                for order in snapshot.orders.iter().take(MAX_DISPLAYED_ITEMS) {
                    text.push_str(&format!(
                        "{} · {} {:?} · {:?} · filled {}/{}\n",
                        order.broker_order_id, order.instrument_id, order.side,
                        order.status, order.filled_quantity, order.quantity
                    ));
                }
                if snapshot.orders.len() > MAX_DISPLAYED_ITEMS {
                    text.push_str("Only the first 20 pending orders are shown.\n");
                }
                text.push_str("Absence from pending orders does not prove a fill or cancellation.");
            }
            _ => {}
        }
        text
    }
}

#[cfg(test)]
mod tests;

use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use trady_adapters::{
    credentials::CredentialStore,
    telegram::{BotToken, TelegramClient, TelegramFailure, spool::StateStore},
    trading212::{Credentials, Trading212Config, Trading212Reader},
};
use trady_application::{
    FoundationConfig,
    telegram::{BrokerScope, Monitor, MonitorCommand, OwnerAccess},
};
use trady_domain::{Error, Result, require};

mod gateway;

pub const HELP: &str = "\n  trady telegram discover\n  trady telegram check --config FILE\n  trady telegram run --config FILE [--once]\n  trady telegram worker --config FILE [--once]\n\nTelegram uses TRADY_TELEGRAM_BOT_TOKEN. Legacy optional monitoring uses read-only broker credentials.\nExpanded gateway uses only TRADY_GATEWAY_DSN plus the bot token; its separate worker uses TRADY_ANALYST_DSN and explicitly enabled source/model credentials.\nConfiguration and durable state contain no bot token. Commands authorize numeric private owner/chat IDs.\n";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    config_version: u32,
    access: OwnerAccess,
    foundation_config: String,
    trading212_config: Option<String>,
    state_directory: String,
    poll_timeout_seconds: u64,
    refresh_interval_seconds: u64,
    stale_after_seconds: i64,
    notifications: bool,
    #[serde(default)]
    expanded_gateway: Option<gateway::GatewayConfig>,
}

impl Config {
    fn apply_whitelist(&mut self, credentials: &CredentialStore) -> Result<()> {
        let Some(value) = credentials.optional("TRADY_TELEGRAM_WHITELIST_USER_ID")? else {
            return Ok(());
        };
        let invalid = || {
            Error(
                "TRADY_TELEGRAM_WHITELIST_USER_ID must be a valid numeric private Telegram user ID"
                    .into(),
            )
        };
        if !value.expose().bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid());
        }
        let user_id = value.expose().parse::<i64>().map_err(|_| invalid())?;
        let access = OwnerAccess {
            allowed_user_ids: vec![user_id],
            allowed_chat_ids: vec![user_id],
        };
        access.validate().map_err(|_| invalid())?;
        self.access = access;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        self.access.validate()?;
        require(
            self.config_version == 1,
            "unsupported Telegram configuration version",
        )?;
        require(
            !self.foundation_config.trim().is_empty()
                && !self.state_directory.trim().is_empty()
                && self
                    .trading212_config
                    .as_ref()
                    .is_none_or(|s| !s.trim().is_empty()),
            "Telegram configuration paths must be nonempty",
        )?;
        require(
            (1..=25).contains(&self.poll_timeout_seconds)
                && (60..=3600).contains(&self.refresh_interval_seconds)
                && (60..=3600).contains(&self.stale_after_seconds)
                && self.stale_after_seconds as u64 >= self.refresh_interval_seconds,
            "invalid Telegram polling, refresh or freshness interval",
        )?;
        if let Some(gateway) = self.expanded_gateway.as_ref().filter(|g| g.enabled) {
            gateway.validate(self, None)?;
        }
        Ok(())
    }
}

struct Loaded {
    config: Config,
    foundation: FoundationConfig,
    broker: Option<Trading212Config>,
    state_directory: PathBuf,
    base_directory: PathBuf,
    research: Option<trady_application::research::ResearchConfig>,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = vec![];
    File::open(path)
        .and_then(|file| file.take(65537).read_to_end(&mut bytes))
        .map_err(|_| Error("cannot read Telegram monitoring configuration".into()))?;
    require(
        bytes.len() <= 65536,
        "Telegram configuration exceeds size bound",
    )?;
    serde_json::from_slice(&bytes).map_err(|_| {
        Error("invalid or unsupported Telegram monitoring configuration fields".into())
    })
}

fn load(path: &Path) -> Result<Loaded> {
    let mut config: Config = read_json(path)?;
    config.apply_whitelist(&CredentialStore::load()?)?;
    config.validate()?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let foundation: FoundationConfig = read_json(&parent.join(&config.foundation_config))?;
    let expanded = config.expanded_gateway.as_ref().filter(|g| g.enabled);
    if let Some(gateway) = expanded {
        gateway.validate(&config, Some(&foundation))?;
        Monitor::new_for_gateway(
            config.access.clone(),
            foundation.clone(),
            config.stale_after_seconds,
            Utc::now(),
        )?;
    } else {
        foundation.validate()?;
    }
    let research = expanded
        .map(|g| {
            read_json::<trady_application::research::ResearchConfig>(
                &parent.join(&g.research_config),
            )
        })
        .transpose()?;
    if let Some(research) = &research {
        research.validate()?;
    }
    let broker = config
        .trading212_config
        .as_ref()
        .filter(|_| expanded.is_none())
        .map(|path| {
            let broker: Trading212Config = read_json(&parent.join(path))?;
            broker.validate().map_err(|e| Error(e.to_string()))?;
            Ok(broker)
        })
        .transpose()?;
    let state_directory = parent.join(&config.state_directory);
    Ok(Loaded {
        config,
        foundation,
        broker,
        state_directory,
        base_directory: parent.to_path_buf(),
        research,
    })
}

pub fn run(args: &[String]) -> Result<Value> {
    if args == ["discover"] {
        return discover();
    }
    require(
        matches!(
            args.first().map(String::as_str),
            Some("check" | "run" | "worker")
        ) && matches!(args.len(), 3 | 4)
            && args[1] == "--config"
            && !args[2].starts_with('-')
            && (args.len() == 3
                || matches!(args[0].as_str(), "run" | "worker") && args[3] == "--once"),
        &format!("unsupported Telegram arguments; use trady --help\n{HELP}"),
    )?;
    let loaded = load(Path::new(&args[2]))?;
    if args[0] == "check" {
        return Ok(json!({
            "status": "configuration_valid", "entry_authorized": false,
            "owner_count": loaded.config.access.allowed_user_ids.len(),
            "chat_count": loaded.config.access.allowed_chat_ids.len(),
            "deployment_mode": loaded.foundation.mode,
            "broker_monitoring": loaded.broker.is_some(),
            "expanded_gateway":loaded.config.expanded_gateway.as_ref().is_some_and(|g|g.enabled),
            "gateway_credentials":"bot token and TRADY_GATEWAY_DSN only when expanded",
            "network_access_verified": false
        }));
    }
    if args[0] == "worker" {
        return gateway::worker(loaded, args.len() == 4);
    }
    monitor(loaded, args.len() == 4)
}

fn token() -> Result<BotToken> {
    BotToken::new(super::secret("TRADY_TELEGRAM_BOT_TOKEN")?)
}

fn discover() -> Result<Value> {
    let api = TelegramClient::new(token()?, 1).map_err(safe_error)?;
    let identity = api.identity().map_err(safe_error)?;
    let updates = api.updates(None, false).map_err(safe_error)?;
    let owners: Vec<_> = updates.into_iter().filter_map(|update| {
        let message = update.message?;
        (message.private_chat && !message.sender_is_bot && message.chat_id == message.sender_id)
            .then(|| json!({"user_id": message.sender_id, "chat_id": message.chat_id, "sent_at": message.sent_at}))
    }).collect();
    Ok(json!({
        "bot_id": identity.id, "bot_username": identity.username,
        "pending_private_senders": owners,
        "authorized": false, "updates_acknowledged": false,
        "note": "Verify your own numeric ID and message time before editing the allowlists. Discovery grants no access."
    }))
}

fn safe_error(error: TelegramFailure) -> Error {
    Error(error.to_string())
}

enum StepError {
    Telegram(TelegramFailure),
    Local(Error),
}

impl From<TelegramFailure> for StepError {
    fn from(error: TelegramFailure) -> Self {
        Self::Telegram(error)
    }
}
impl From<Error> for StepError {
    fn from(error: Error) -> Self {
        Self::Local(error)
    }
}

fn drain(
    api: &TelegramClient,
    store: &mut StateStore,
    access: &OwnerAccess,
    stopped: &AtomicBool,
) -> std::result::Result<u64, StepError> {
    let mut sent = 0;
    while !stopped.load(Ordering::Relaxed) {
        let Some(delivery) = store.state().outbox.front() else {
            break;
        };
        // A revoked recipient never receives a reply left over from the old configuration.
        if access.allowed_chat_ids.contains(&delivery.chat_id)
            && access.allowed_user_ids.contains(&delivery.chat_id)
        {
            api.send_message(delivery.chat_id, &delivery.text)?;
            sent += 1;
        } else {
            eprintln!("trady: discarded Telegram delivery to a revoked recipient");
        }
        store.acknowledge_front()?;
    }
    Ok(sent)
}

fn refresh(monitor: &mut Monitor, broker: &mut Option<Trading212Reader>) -> Option<String> {
    let broker = broker.as_mut()?;
    let account = broker.config().account_id.clone();
    let snapshot = broker
        .read_snapshot(&account)
        .map_err(|_| Error("broker snapshot unavailable".into()));
    monitor.observe(snapshot, Utc::now())
}

fn monitor(loaded: Loaded, once: bool) -> Result<Value> {
    let token = token()?;
    let mut gateway = gateway::Gateway::open(&loaded, token.bot_id())?;
    let mut store = StateStore::open(&loaded.state_directory, token.bot_id())?;
    let scope = loaded.broker.as_ref().map(|config| BrokerScope {
        account_id: config.account_id.clone(),
        mode: config.environment.mode(),
        currency: config.base_currency.clone(),
    });
    let mut broker = loaded
        .broker
        .map(|config| {
            let credentials =
                Credentials::from_store(config.environment, &CredentialStore::load()?)?;
            Trading212Reader::new(config, credentials).map_err(|e| Error(e.to_string()))
        })
        .transpose()?;
    let config = loaded.config;
    let api = TelegramClient::new(token, config.poll_timeout_seconds).map_err(safe_error)?;
    let identity = api.identity().map_err(safe_error)?;
    let mut monitor = if gateway.is_some() {
        Monitor::new_for_gateway(
            config.access.clone(),
            loaded.foundation,
            config.stale_after_seconds,
            Utc::now(),
        )?
    } else {
        Monitor::new(
            config.access.clone(),
            loaded.foundation,
            scope,
            config.stale_after_seconds,
            Utc::now(),
        )?
    };
    let stopped = Arc::new(AtomicBool::new(false));
    if !once {
        let flag = stopped.clone();
        ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))
            .map_err(|_| Error("cannot install Telegram shutdown handler".into()))?;
    }
    refresh(&mut monitor, &mut broker);
    let mut startup_pending = config.notifications;
    let mut last_refresh = Instant::now();
    let mut backoff = 1;
    let mut handled = 0_u64;
    let mut delivered = 0_u64;
    while !stopped.load(Ordering::Relaxed) {
        let result = (|| -> std::result::Result<(), StepError> {
            delivered += drain(&api, &mut store, &config.access, &stopped)?;
            if let Some(gateway) = &mut gateway {
                delivered += gateway.drain(&api, &config.access, &stopped)?;
            }
            if stopped.load(Ordering::Relaxed) {
                return Ok(());
            }
            if startup_pending {
                store.enqueue_alert(
                    &config.access.allowed_chat_ids,
                    &format!(
                        "Telegram monitor started.\n{}",
                        monitor.render(MonitorCommand::Status, Utc::now())
                    ),
                )?;
                startup_pending = false;
                delivered += drain(&api, &mut store, &config.access, &stopped)?;
            }
            // Refresh on schedule, regardless of whether an owner requests a command.
            if last_refresh.elapsed() >= Duration::from_secs(config.refresh_interval_seconds) {
                if let Some(alert) = refresh(&mut monitor, &mut broker)
                    && config.notifications
                {
                    store.enqueue_alert(&config.access.allowed_chat_ids, &alert)?;
                }
                last_refresh = Instant::now();
                delivered += drain(&api, &mut store, &config.access, &stopped)?;
            }
            if stopped.load(Ordering::Relaxed) {
                return Ok(());
            }
            let updates = api.updates(Some(store.state().next_offset), !once)?;
            for update in updates {
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                if update.update_id < store.state().next_offset {
                    continue;
                }
                let now = Utc::now();
                let reply = if let Some(gateway) = &mut gateway {
                    gateway.reply(
                        &update,
                        identity.id,
                        &identity.username,
                        &config.access,
                        &monitor,
                        now,
                    )?
                } else {
                    monitor.reply(&update, &identity.username, now)
                };
                store.accept_update(update.update_id, reply.as_ref(), now)?;
                handled += u64::from(reply.is_some());
            }
            delivered += drain(&api, &mut store, &config.access, &stopped)?;
            Ok(())
        })();
        match result {
            Ok(()) => backoff = 1,
            Err(StepError::Local(error)) => return Err(error),
            Err(StepError::Telegram(error)) if once || error.permanent => {
                return Err(safe_error(error));
            }
            Err(StepError::Telegram(error)) => {
                let delay = error.retry_after_seconds.unwrap_or(backoff).max(backoff);
                eprintln!("trady: {error}; retrying in {delay}s; durable replies retained");
                sleep_until_stopped(Duration::from_secs(delay), &stopped);
                backoff = (backoff * 2).min(60);
            }
        }
        if once {
            break;
        }
    }
    Ok(
        json!({"service":"telegram_monitor", "status":if once {"poll_complete"} else {"stopped"},
        "commands_recorded":handled,"messages_delivered":delivered,
        "pending_deliveries":store.state().outbox.len(),"entry_authorized":false}),
    )
}

fn sleep_until_stopped(duration: Duration, stopped: &AtomicBool) {
    let start = Instant::now();
    while !stopped.load(Ordering::Relaxed) && start.elapsed() < duration {
        std::thread::sleep(
            (duration - start.elapsed().min(duration)).min(Duration::from_millis(100)),
        );
    }
}

#[cfg(test)]
mod tests;

//! Atomic local inbox/offset/outbox commits, protected by a process-lifetime OS lock.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use trady_application::telegram::{CommandReply, MonitorCommand};
use trady_domain::{Error, Result, require};

use super::split_message;

const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_OUTBOX: usize = 512;
const MAX_RECENT_COMMANDS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub event_id: String,
    pub chat_id: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedCommand {
    pub update_id: i64,
    pub sender_id: i64,
    pub chat_id: i64,
    pub command: MonitorCommand,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorState {
    pub version: u32,
    pub bot_id: i64,
    pub next_offset: i64,
    pub next_event: u64,
    pub recent_commands: VecDeque<RecordedCommand>,
    pub outbox: VecDeque<Delivery>,
}

pub struct StateStore {
    directory: PathBuf,
    state: MonitorState,
    _lock: File,
}

impl StateStore {
    pub fn open(directory: &Path, bot_id: i64) -> Result<Self> {
        require(bot_id > 0, "invalid Telegram bot identity")?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(directory)
            .map_err(|_| Error("cannot create Telegram state directory".into()))?;
        let lock = private_file(&directory.join("monitor.lock"), false)?;
        lock.try_lock()
            .map_err(|_| Error("Telegram state is locked by another monitor".into()))?;
        let path = directory.join("state.json");
        let state = match File::open(path) {
            Ok(file) => {
                let mut bytes = vec![];
                file.take(MAX_STATE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| Error("cannot read Telegram state".into()))?;
                require(
                    bytes.len() as u64 <= MAX_STATE_BYTES,
                    "Telegram state exceeds size bound",
                )?;
                serde_json::from_slice::<MonitorState>(&bytes)
                    .map_err(|_| Error("invalid Telegram state; preserve it for recovery".into()))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => MonitorState {
                version: 1,
                bot_id,
                next_offset: 0,
                next_event: 1,
                recent_commands: VecDeque::new(),
                outbox: VecDeque::new(),
            },
            Err(_) => return Err(Error("cannot open Telegram state".into())),
        };
        require(
            state.version == 1
                && state.bot_id == bot_id
                && state.next_offset >= 0
                && state.next_event > 0
                && state.outbox.len() <= MAX_OUTBOX
                && state.recent_commands.len() <= MAX_RECENT_COMMANDS
                && state.outbox.iter().all(|d| {
                    d.chat_id > 0
                        && !d.event_id.is_empty()
                        && !d.text.is_empty()
                        && d.text.encode_utf16().count() <= 4096
                }),
            "Telegram state version, bot binding or outbox is invalid",
        )?;
        Ok(Self {
            directory: directory.into(),
            state,
            _lock: lock,
        })
    }

    pub fn state(&self) -> &MonitorState {
        &self.state
    }

    /// Replies, the accepted typed command and its offset become durable together.
    pub fn accept_update(
        &mut self,
        id: i64,
        reply: Option<&CommandReply>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        require((0..i64::MAX).contains(&id), "invalid Telegram update ID")?;
        if id < self.state.next_offset {
            return Ok(());
        }
        let mut next = self.state.clone();
        if let Some(reply) = reply {
            require(
                reply.update_id == id,
                "Telegram reply/update binding mismatch",
            )?;
            append(
                &mut next,
                reply.chat_id,
                &format!("update:{id}"),
                &reply.text,
            )?;
            next.recent_commands.push_back(RecordedCommand {
                update_id: id,
                sender_id: reply.sender_id,
                chat_id: reply.chat_id,
                command: reply.command,
                received_at: now,
            });
            while next.recent_commands.len() > MAX_RECENT_COMMANDS {
                next.recent_commands.pop_front();
            }
        }
        next.next_offset = id + 1;
        self.commit(next)
    }

    pub fn enqueue_alert(&mut self, chats: &[i64], text: &str) -> Result<()> {
        let mut next = self.state.clone();
        let event = format!("monitor:{}", next.next_event);
        next.next_event = next
            .next_event
            .checked_add(1)
            .ok_or_else(|| Error("Telegram event sequence exhausted".into()))?;
        for chat in chats {
            append(&mut next, *chat, &event, text)?;
        }
        self.commit(next)
    }

    pub fn acknowledge_front(&mut self) -> Result<()> {
        require(!self.state.outbox.is_empty(), "Telegram outbox is empty")?;
        let mut next = self.state.clone();
        next.outbox.pop_front();
        self.commit(next)
    }

    fn commit(&mut self, next: MonitorState) -> Result<()> {
        let bytes = serde_json::to_vec(&next)
            .map_err(|_| Error("cannot serialize Telegram state".into()))?;
        require(
            bytes.len() as u64 <= MAX_STATE_BYTES,
            "Telegram state exceeds size bound",
        )?;
        let temporary = self.directory.join("state.json.tmp");
        let mut file = private_file(&temporary, true)?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| Error("cannot persist Telegram state".into()))?;
        fs::rename(temporary, self.directory.join("state.json"))
            .map_err(|_| Error("cannot replace Telegram state".into()))?;
        #[cfg(unix)]
        File::open(&self.directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| Error("cannot sync Telegram state directory".into()))?;
        self.state = next;
        Ok(())
    }
}

fn append(state: &mut MonitorState, chat: i64, event: &str, text: &str) -> Result<()> {
    require(chat > 0 && !text.is_empty(), "invalid Telegram delivery")?;
    for (part, text) in split_message(text).into_iter().enumerate() {
        require(
            state.outbox.len() < MAX_OUTBOX,
            "Telegram outbox is full; delivery must recover before polling",
        )?;
        let event_id = format!("tg:{}:{event}:{}", state.bot_id, part + 1);
        let text = format!("{text}\n[event {event_id}]");
        require(
            text.encode_utf16().count() <= 4096,
            "Telegram delivery exceeds message limit",
        )?;
        state.outbox.push_back(Delivery {
            event_id,
            chat_id: chat,
            text,
        });
    }
    Ok(())
}

fn private_file(path: &Path, truncate: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(true)
        .truncate(truncate);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|_| Error("cannot open private Telegram state file".into()))
}

#[cfg(test)]
mod tests;

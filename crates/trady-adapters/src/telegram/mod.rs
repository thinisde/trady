//! Fixed-origin Telegram Bot API transport. Provider text and token-bearing URLs
//! never cross the sanitized error boundary.

pub mod spool;

use chrono::DateTime;
use reqwest::blocking::Client;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{fmt, io::Read, time::Duration};
use trady_application::telegram::{IncomingMessage, IncomingUpdate};
use trady_domain::{Error, Result, require};

const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
pub const MESSAGE_UNITS: usize = 3500;

pub struct BotToken {
    value: String,
    bot_id: i64,
}

impl BotToken {
    pub fn new(value: String) -> Result<Self> {
        let (prefix, suffix) = value
            .split_once(':')
            .ok_or_else(|| Error("invalid Telegram bot token".into()))?;
        let bot_id = prefix.parse::<i64>().unwrap_or(0);
        require(
            bot_id > 0
                && prefix.len() <= 16
                && prefix.bytes().all(|b| b.is_ascii_digit())
                && (20..=128).contains(&suffix.len())
                && suffix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "invalid Telegram bot token",
        )?;
        Ok(Self { value, bot_id })
    }

    pub fn bot_id(&self) -> i64 {
        self.bot_id
    }
}

impl fmt::Debug for BotToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BotToken([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramFailure {
    pub http_status: Option<u16>,
    pub retry_after_seconds: Option<u64>,
    pub permanent: bool,
}

impl TelegramFailure {
    fn transport() -> Self {
        Self {
            http_status: None,
            retry_after_seconds: None,
            permanent: false,
        }
    }

    fn invalid() -> Self {
        Self {
            permanent: true,
            ..Self::transport()
        }
    }
}

impl fmt::Display for TelegramFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Telegram request failed")?;
        if let Some(code) = self.http_status {
            write!(f, " (HTTP/API {code})")?;
        }
        if let Some(seconds) = self.retry_after_seconds {
            write!(f, "; retry after {seconds} seconds")?;
        }
        Ok(())
    }
}

impl std::error::Error for TelegramFailure {}
pub type TelegramResult<T> = std::result::Result<T, TelegramFailure>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotIdentity {
    pub id: i64,
    pub username: String,
}

pub struct TelegramClient {
    client: Client,
    origin: String,
    token: BotToken,
    poll_timeout_seconds: u64,
}

impl TelegramClient {
    pub fn new(token: BotToken, poll_timeout_seconds: u64) -> TelegramResult<Self> {
        Self::build(
            token,
            poll_timeout_seconds,
            "https://api.telegram.org".into(),
            true,
        )
    }

    fn build(token: BotToken, seconds: u64, origin: String, https: bool) -> TelegramResult<Self> {
        if !(1..=25).contains(&seconds) {
            return Err(TelegramFailure::invalid());
        }
        let client = Client::builder()
            .https_only(https)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(seconds + 10))
            .user_agent("trady/0.1 Telegram-monitor")
            .build()
            .map_err(|_| TelegramFailure::transport())?;
        Ok(Self {
            client,
            origin,
            token,
            poll_timeout_seconds: seconds,
        })
    }

    pub fn identity(&self) -> TelegramResult<BotIdentity> {
        let user: UserDto = self.call("getMe", &json!({}))?;
        let username = user.username.filter(|name| {
            !name.is_empty()
                && name.len() <= 64
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        });
        if !user.is_bot || user.id != self.token.bot_id {
            return Err(TelegramFailure::invalid());
        }
        Ok(BotIdentity {
            id: user.id,
            username: username.ok_or_else(TelegramFailure::invalid)?,
        })
    }

    /// None is discovery: no offset is sent, so pending updates are not confirmed.
    pub fn updates(&self, offset: Option<i64>, wait: bool) -> TelegramResult<Vec<IncomingUpdate>> {
        if offset.is_some_and(|n| n < 0) {
            return Err(TelegramFailure::invalid());
        }
        let mut body = json!({
            "limit": 100,
            "timeout": if wait { self.poll_timeout_seconds } else { 0 },
            "allowed_updates": ["message"]
        });
        if let Some(offset) = offset {
            body["offset"] = json!(offset);
        }
        let mut raw: Vec<UpdateDto> = self.call("getUpdates", &body)?;
        if raw.len() > 100
            || raw
                .iter()
                .any(|u| u.update_id < 0 || u.update_id == i64::MAX)
        {
            return Err(TelegramFailure::invalid());
        }
        raw.sort_by_key(|u| u.update_id);
        if raw
            .windows(2)
            .any(|pair| pair[0].update_id == pair[1].update_id)
        {
            return Err(TelegramFailure::invalid());
        }
        Ok(raw
            .into_iter()
            .map(|update| IncomingUpdate {
                update_id: update.update_id,
                message: update.message.and_then(|message| {
                    let sender = message.from?;
                    Some(IncomingMessage {
                        sender_id: sender.id,
                        chat_id: message.chat.id,
                        private_chat: message.chat.kind == "private",
                        sender_is_bot: sender.is_bot,
                        sent_at: DateTime::from_timestamp(message.date, 0)?,
                        text: message.text?,
                    })
                }),
            })
            .collect())
    }

    pub fn send_message(&self, chat_id: i64, text: &str) -> TelegramResult<()> {
        if chat_id <= 0 || text.is_empty() || text.encode_utf16().count() > 4096 {
            return Err(TelegramFailure::invalid());
        }
        let result: Value = self.call(
            "sendMessage",
            &json!({
                "chat_id": chat_id,
                "text": text,
                "link_preview_options": {"is_disabled": true},
                "protect_content": true
            }),
        )?;
        // A malformed success must not remove a pending outbox item.
        if result
            .get("message_id")
            .and_then(Value::as_i64)
            .is_none_or(|id| id <= 0)
            || result.pointer("/chat/id").and_then(Value::as_i64) != Some(chat_id)
            || result.pointer("/chat/type").and_then(Value::as_str) != Some("private")
        {
            return Err(TelegramFailure::invalid());
        }
        Ok(())
    }

    fn call<T: DeserializeOwned>(&self, method: &str, body: &Value) -> TelegramResult<T> {
        let response = self
            .client
            .post(format!("{}/bot{}/{method}", self.origin, self.token.value))
            .json(body)
            .send()
            .map_err(|_| TelegramFailure::transport())?;
        let status = response.status().as_u16();
        let header_retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        let mut bytes = vec![];
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| TelegramFailure::transport())?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(TelegramFailure::invalid());
        }
        let envelope = serde_json::from_slice::<Envelope<T>>(&bytes);
        if let Ok(envelope) = envelope {
            if (200..300).contains(&status) && envelope.ok {
                return envelope.result.ok_or_else(TelegramFailure::invalid);
            }
            let code = envelope.error_code.unwrap_or(status);
            let retry = envelope
                .parameters
                .and_then(|p| p.retry_after)
                .or(header_retry);
            return Err(TelegramFailure {
                http_status: Some(code),
                retry_after_seconds: retry,
                permanent: matches!(code, 400 | 401 | 403 | 404 | 409),
            });
        }
        Err(TelegramFailure {
            http_status: Some(status),
            retry_after_seconds: header_retry,
            permanent: (200..500).contains(&status) && status != 429,
        })
    }
}

/// Conservative UTF-16 bound also protects clients that count supplementary characters twice.
pub fn split_message(text: &str) -> Vec<String> {
    let mut result = vec![];
    let mut current = String::new();
    let mut units = 0;
    for ch in text.chars() {
        if units + ch.len_utf16() > MESSAGE_UNITS {
            result.push(std::mem::take(&mut current));
            units = 0;
        }
        units += ch.len_utf16();
        current.push(ch);
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    result: Option<T>,
    error_code: Option<u16>,
    parameters: Option<Parameters>,
}

#[derive(Deserialize)]
struct Parameters {
    retry_after: Option<u64>,
}

#[derive(Deserialize)]
struct UserDto {
    id: i64,
    is_bot: bool,
    username: Option<String>,
}

#[derive(Deserialize)]
struct UpdateDto {
    update_id: i64,
    message: Option<MessageDto>,
}

#[derive(Deserialize)]
struct MessageDto {
    from: Option<UserDto>,
    chat: ChatDto,
    date: i64,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ChatDto {
    id: i64,
    #[serde(rename = "type")]
    kind: String,
}

#[cfg(test)]
mod tests;

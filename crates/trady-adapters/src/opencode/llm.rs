//! OpenCode Go's OpenAI-compatible Chat Completions transport. Secrets never enter sessions.

use reqwest::{
    Url,
    blocking::{Client, Response},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read},
    time::Duration,
};
use trady_application::{contracts::parse_json, harness::*};
use trady_domain::{Error, Result, require};

pub const OPENCODE_GO_BASE_URL: &str = "https://opencode.ai/inference/go/openai/v1";
pub const OPENCODE_GO_MODELS_URL: &str = "https://opencode.ai/inference/go/v1/models";
const MAX_RESPONSE_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpenCodeGoConfig {
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
    pub stream: bool,
    pub limits: RunLimits,
}
impl Default for OpenCodeGoConfig {
    fn default() -> Self {
        Self {
            base_url: OPENCODE_GO_BASE_URL.into(),
            model: "deepseek-v4.1-flash".into(),
            api_key_env: "OPENCODE_API_KEY".into(),
            stream: true,
            limits: RunLimits::default(),
        }
    }
}
impl OpenCodeGoConfig {
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        let url = Url::parse(&self.base_url).map_err(|_| Error("invalid LLM base URL".into()))?;
        let local = url
            .host_str()
            .is_some_and(|h| matches!(h, "127.0.0.1" | "[::1]" | "::1"));
        require(
            (url.scheme() == "https" || (url.scheme() == "http" && local))
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "LLM URL requires HTTPS (or literal loopback HTTP) and cannot contain credentials, query or fragment",
        )?;
        require(
            !self.model.is_empty()
                && self.model.len() <= 128
                && self
                    .model
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.')),
            "supply the OpenCode Go chat model ID without a provider prefix",
        )?;
        require(
            !self.api_key_env.is_empty()
                && self.api_key_env.len() <= 96
                && self
                    .api_key_env
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_'),
            "invalid API key environment variable",
        )
    }
    pub fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
    pub fn models_endpoint(&self) -> String {
        if self.base_url == OPENCODE_GO_BASE_URL {
            return OPENCODE_GO_MODELS_URL.into();
        }
        let Ok(mut url) = Url::parse(&self.base_url) else {
            return format!("{}/models", self.base_url.trim_end_matches('/'));
        };
        let path = url.path().trim_end_matches('/');
        let models_path = if let Some(prefix) = path.strip_suffix("/openai/v1") {
            format!("{prefix}/v1/models")
        } else if let Some(prefix) = path.strip_suffix("/v1") {
            format!("{prefix}/v1/models")
        } else {
            format!("{path}/models")
        };
        url.set_path(&models_path);
        url.to_string()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenCodeModelVerification {
    pub endpoint: String,
    pub model: String,
    pub model_count: usize,
    pub catalog_sha256: String,
}

pub struct OpenCodeGoProvider {
    config: OpenCodeGoConfig,
    client: Client,
    api_key: String,
    model_verification: Option<OpenCodeModelVerification>,
}
impl OpenCodeGoProvider {
    pub fn from_env(config: OpenCodeGoConfig) -> Result<Self> {
        config.validate()?;
        let api_key = crate::credentials::required(&config.api_key_env)?;
        Self::new(config, api_key.into_inner())
    }
    pub fn new(config: OpenCodeGoConfig, api_key: String) -> Result<Self> {
        config.validate()?;
        require(
            !api_key.trim().is_empty() && !api_key.contains(['\r', '\n']),
            "invalid LLM API key",
        )?;
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| Error("cannot initialize LLM HTTPS client".into()))?;
        Ok(Self {
            config,
            client,
            api_key,
            model_verification: None,
        })
    }

    fn verify_model_catalog(
        &self,
    ) -> std::result::Result<OpenCodeModelVerification, ProviderError> {
        let response = self
            .client
            .get(self.config.models_endpoint())
            .bearer_auth(&self.api_key)
            .header("Accept", "application/json")
            .header("User-Agent", "trady/0.1")
            .timeout(Duration::from_secs(30))
            .send()
            .map_err(transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => failed("provider_authentication", false),
                408 | 429 | 500 | 502 | 503 | 504 => {
                    failed("provider_model_catalog_unavailable", true)
                }
                _ => failed("provider_model_catalog_rejected", false),
            });
        }
        let mut bytes = vec![];
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| failed("provider_model_catalog_read", true))?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(failed("provider_model_catalog_limit", false));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| failed("provider_model_catalog_invalid_json", false))?;
        let models = value
            .get("data")
            .and_then(Value::as_array)
            .or_else(|| value.as_array())
            .ok_or_else(|| failed("provider_model_catalog_invalid_shape", false))?;
        let available = models
            .iter()
            .filter_map(|model| model.get("id").and_then(Value::as_str))
            .any(|model| model == self.config.model);
        if !available {
            return Err(failed("provider_model_unavailable", false));
        }
        Ok(OpenCodeModelVerification {
            endpoint: self.config.models_endpoint(),
            model: self.config.model.clone(),
            model_count: models.len(),
            catalog_sha256: Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        })
    }

    fn ensure_model_verified(&mut self) -> std::result::Result<(), ProviderError> {
        if self.model_verification.is_none() {
            self.model_verification = Some(self.verify_model_catalog()?);
        }
        Ok(())
    }

    pub fn verify_model(&mut self) -> Result<OpenCodeModelVerification> {
        self.ensure_model_verified().map_err(|error| {
            Error(format!(
                "OpenCode Go model verification failed: {}",
                error.code
            ))
        })?;
        self.model_verification
            .clone()
            .ok_or_else(|| Error("OpenCode Go model verification missing".into()))
    }
}
fn failed(code: &'static str, retryable: bool) -> ProviderError {
    ProviderError { code, retryable }
}
fn transport(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        failed("provider_timeout", true)
    } else {
        failed("provider_transport", true)
    }
}
fn usage(value: &Value) -> Option<Usage> {
    Some(Usage {
        input_tokens: value.get("prompt_tokens")?.as_u64()?,
        output_tokens: value.get("completion_tokens")?.as_u64()?,
    })
}
fn checked_response(
    message: ChatMessage,
    finish: &str,
    usage: Option<Usage>,
) -> std::result::Result<Completion, ProviderError> {
    if !matches!(finish, "stop" | "tool_calls") {
        return Err(failed("provider_unfinished_output", false));
    }
    if message.content.as_ref().is_some_and(|s| s.len() > 262_144)
        || message
            .reasoning_content
            .as_ref()
            .is_some_and(|s| s.len() > 262_144)
    {
        return Err(failed("provider_output_limit", false));
    }
    if let Some(calls) = &message.tool_calls {
        if calls.is_empty()
            || calls.len() > 4
            || calls.iter().any(|c| {
                c.id.is_empty()
                    || c.function.name.is_empty()
                    || c.function.arguments.len() > 262_144
            })
        {
            return Err(failed("provider_invalid_tool_calls", false));
        }
    } else if message.content.as_deref().is_none_or(str::is_empty) {
        return Err(failed("provider_empty_response", false));
    }
    Ok(Completion { message, usage })
}
fn read_json(
    mut response: Response,
    on_text: &mut dyn FnMut(&str),
) -> std::result::Result<Completion, ProviderError> {
    let mut bytes = vec![];
    response
        .by_ref()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failed("provider_response_read", true))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(failed("provider_output_limit", false));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| failed("provider_invalid_json", false))?;
    let choice = &value["choices"][0];
    if !choice["message"]["refusal"].is_null() {
        return Err(failed("provider_refused", false));
    }
    let message: ChatMessage = serde_json::from_value(choice["message"].clone())
        .map_err(|_| failed("provider_invalid_message", false))?;
    if message.role != "assistant" {
        return Err(failed("provider_invalid_role", false));
    }
    let completed = checked_response(
        message,
        choice["finish_reason"].as_str().unwrap_or_default(),
        usage(&value["usage"]),
    )?;
    if let Some(text) = &completed.message.content {
        on_text(text);
    }
    Ok(completed)
}

fn read_stream(
    response: Response,
    cancel: &CancellationToken,
    on_text: &mut dyn FnMut(&str),
) -> std::result::Result<Completion, ProviderError> {
    // A bounded reader prevents a missing newline or malicious stream from allocating without limit.
    let mut reader = BufReader::new(response.take(MAX_RESPONSE_BYTES + 1));
    let mut message = ChatMessage::text("assistant", String::new());
    let mut calls: BTreeMap<usize, NativeCall> = BTreeMap::new();
    let mut finish = String::new();
    let mut tokens = None;
    let mut done = false;
    let mut consumed = 0usize;
    let mut event = String::new();
    loop {
        if cancel.is_cancelled() {
            return Err(failed("provider_cancelled", false));
        }
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .map_err(|_| failed("provider_stream_interrupted", true))?;
        if n == 0 {
            break;
        }
        consumed += n;
        if consumed as u64 > MAX_RESPONSE_BYTES || line.len() > 262_144 {
            return Err(failed("provider_output_limit", false));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if let Some(data) = line.strip_prefix("data:") {
            if !event.is_empty() {
                event.push('\n');
            }
            event.push_str(data.strip_prefix(' ').unwrap_or(data));
            continue;
        }
        if !line.is_empty() || event.is_empty() {
            continue;
        }
        if event.trim() == "[DONE]" {
            done = true;
            break;
        }
        let value =
            parse_json(&event).map_err(|_| failed("provider_invalid_stream_json", false))?;
        event.clear();
        if value.get("error").is_some() {
            return Err(failed("provider_stream_error", false));
        }
        if let Some(u) = usage(&value["usage"]) {
            tokens = Some(u);
        }
        let Some(choices) = value["choices"].as_array() else {
            return Err(failed("provider_invalid_stream", false));
        };
        for choice in choices {
            if choice["index"].as_u64() != Some(0) {
                continue;
            }
            let delta = &choice["delta"];
            if !delta["refusal"].is_null() {
                return Err(failed("provider_refused", false));
            }
            if let Some(role) = delta["role"].as_str()
                && role != "assistant"
            {
                return Err(failed("provider_invalid_role", false));
            }
            if let Some(text) = delta["content"].as_str() {
                message.content.get_or_insert_default().push_str(text);
                on_text(text);
            }
            if let Some(reasoning) = delta["reasoning_content"].as_str() {
                message
                    .reasoning_content
                    .get_or_insert_default()
                    .push_str(reasoning);
            }
            if let Some(parts) = delta["tool_calls"].as_array() {
                for part in parts {
                    let index = part["index"]
                        .as_u64()
                        .ok_or_else(|| failed("provider_invalid_tool_index", false))?
                        as usize;
                    if index >= 4 {
                        return Err(failed("provider_invalid_tool_batch", false));
                    }
                    let call = calls.entry(index).or_insert_with(|| NativeCall {
                        id: String::new(),
                        kind: "function".into(),
                        function: NativeFunction {
                            name: String::new(),
                            arguments: String::new(),
                        },
                    });
                    if let Some(id) = part["id"].as_str() {
                        call.id.push_str(id);
                    }
                    if let Some(kind) = part["type"].as_str()
                        && kind != "function"
                    {
                        return Err(failed("provider_invalid_tool_type", false));
                    }
                    if let Some(name) = part["function"]["name"].as_str() {
                        call.function.name.push_str(name);
                    }
                    if let Some(args) = part["function"]["arguments"].as_str() {
                        call.function.arguments.push_str(args);
                    }
                }
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                finish = reason.into();
            }
        }
    }
    if !done {
        return Err(failed("provider_stream_incomplete", true));
    }
    if !calls.is_empty() {
        message.tool_calls = Some(calls.into_values().collect());
    }
    checked_response(message, &finish, tokens)
}

impl LlmProvider for OpenCodeGoProvider {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider: "opencode-go".into(),
            model: self.config.model.clone(),
            endpoint: self.config.endpoint(),
        }
    }
    fn complete(
        &mut self,
        request: &ModelRequest,
        cancel: &CancellationToken,
        on_text: &mut dyn FnMut(&str),
    ) -> std::result::Result<Completion, ProviderError> {
        if cancel.is_cancelled() {
            return Err(failed("provider_cancelled", false));
        }
        self.ensure_model_verified()?;
        let mut body = json!({"model":self.config.model,"messages":request.messages,"max_tokens":request.max_output_tokens,"stream":self.config.stream});
        if self.config.stream {
            body["stream_options"] = json!({"include_usage":true});
        }
        if request.native_tools && !request.tools.is_empty() {
            body["tools"]=Value::Array(request.tools.iter().map(|t|json!({"type":"function","function":{"name":t.alias(),"description":t.description,"parameters":t.arguments_schema}})).collect());
            body["tool_choice"] = json!(if request.force_final { "none" } else { "auto" });
        }
        let response = self
            .client
            .post(self.config.endpoint())
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .header("User-Agent", "trady/0.1")
            .header("x-opencode-session", &request.session_id)
            .timeout(request.timeout.min(Duration::from_secs(30)))
            .json(&body)
            .send()
            .map_err(transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => failed("provider_authentication", false),
                429 => failed("provider_rate_limited", true),
                408 | 500 | 502 | 503 | 504 => failed("provider_unavailable", true),
                _ => failed("provider_request_rejected", false),
            });
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if self.config.stream {
            if !content_type.starts_with("text/event-stream") {
                return Err(failed("provider_expected_event_stream", false));
            }
            read_stream(response, cancel, on_text)
        } else {
            read_json(response, on_text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OPENCODE_GO_BASE_URL, OPENCODE_GO_MODELS_URL, OpenCodeGoConfig};

    #[test]
    fn models_endpoint_uses_the_go_catalog_path() {
        let config = OpenCodeGoConfig::default();
        assert_eq!(config.models_endpoint(), OPENCODE_GO_MODELS_URL);

        let custom = OpenCodeGoConfig {
            base_url: "http://127.0.0.1:8080/inference/go/openai/v1".into(),
            ..config
        };
        assert_eq!(
            custom.models_endpoint(),
            "http://127.0.0.1:8080/inference/go/v1/models"
        );
    }

    #[test]
    fn default_endpoint_remains_openai_compatible() {
        assert_eq!(
            OpenCodeGoConfig::default().endpoint(),
            format!("{OPENCODE_GO_BASE_URL}/chat/completions")
        );
    }
}

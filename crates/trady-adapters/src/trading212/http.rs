use super::{Credentials, Trading212Config};
use chrono::Utc;
use reqwest::{Method, blocking::Client, header};
use serde::de::DeserializeOwned;
use std::{
    collections::BTreeMap,
    io::Read,
    time::{Duration, Instant},
};
use trady_application::broker::{BrokerFailure, BrokerFailureKind, BrokerResult};

const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Bucket {
    Summary,
    Positions,
    Orders,
    Order,
    Instruments,
    History,
    Market,
    Limit,
    Stop,
    StopLimit,
    Cancel,
}
impl Bucket {
    fn interval(self) -> Duration {
        Duration::from_millis(match self {
            Self::Summary | Self::Orders => 5_000,
            Self::Positions | Self::Order => 1_000,
            Self::Instruments => 50_000,
            Self::History => 3_000,
            Self::Cancel => 1_200,
            _ => 2_000,
        })
    }
}

pub(super) struct Http {
    client: Client,
    base: String,
    credentials: Credentials,
    not_before: BTreeMap<Bucket, Instant>,
}

impl Http {
    pub(super) fn new(config: &Trading212Config, credentials: Credentials) -> BrokerResult<Self> {
        Self::build(
            config.environment.base_url().into(),
            credentials,
            config.timeout_seconds,
            true,
        )
    }
    fn build(
        base: String,
        credentials: Credentials,
        seconds: u64,
        https_only: bool,
    ) -> BrokerResult<Self> {
        let client = Client::builder()
            .https_only(https_only)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .timeout(Duration::from_secs(seconds))
            .connect_timeout(Duration::from_secs(seconds.min(5)))
            .user_agent("trady/0.1 Trading212-adapter")
            .build()
            .map_err(|_| BrokerFailure::new(BrokerFailureKind::Transport))?;
        Ok(Self {
            client,
            base,
            credentials,
            not_before: BTreeMap::new(),
        })
    }

    #[cfg(test)]
    pub(super) fn for_test(base: String, seconds: u64) -> Self {
        Self::build(
            base,
            Credentials::new("fixture-key".into(), "fixture-secret".into()).unwrap(),
            seconds,
            false,
        )
        .unwrap()
    }

    pub(super) fn json<T: DeserializeOwned>(
        &mut self,
        bucket: Bucket,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> BrokerResult<T> {
        let mutation = method != Method::GET;
        let bytes = self.request(bucket, method, path, body)?;
        serde_json::from_slice(&bytes).map_err(|_| {
            let mut e = BrokerFailure::new(BrokerFailureKind::InvalidResponse);
            e.outcome_unknown = mutation;
            e
        })
    }

    pub(super) fn request(
        &mut self,
        bucket: Bucket,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> BrokerResult<Vec<u8>> {
        let now = Instant::now();
        if let Some(until) = self.not_before.get(&bucket).filter(|until| **until > now) {
            let mut e = BrokerFailure::new(BrokerFailureKind::RateLimited);
            e.retry_after_seconds = Some(until.duration_since(now).as_secs().saturating_add(1));
            return Err(e);
        }
        let mutation = method != Method::GET;
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .basic_auth(&self.credentials.key, Some(&self.credentials.secret))
            .header(header::ACCEPT, "application/json");
        if let Some(body) = body {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let request = request
            .build()
            .map_err(|_| BrokerFailure::new(BrokerFailureKind::InvalidRequest))?;
        self.not_before.insert(bucket, now + bucket.interval());
        // Deliberately one send: Trading 212 order POSTs are not idempotent.
        let response = self.client.execute(request).map_err(|_| {
            let mut e = BrokerFailure::new(BrokerFailureKind::Transport);
            e.outcome_unknown = mutation;
            e
        })?;
        let status = response.status().as_u16();
        let retry = retry_delay(response.headers());
        if let Some(seconds) = retry
            && let Some(until) = Instant::now().checked_add(Duration::from_secs(seconds))
        {
            self.not_before
                .entry(bucket)
                .and_modify(|v| *v = (*v).max(until));
        }
        if !(200..300).contains(&status) {
            let rejected = matches!(status, 400 | 401 | 403 | 404 | 405 | 415 | 422 | 429);
            let mut e = BrokerFailure::new(if status == 429 {
                BrokerFailureKind::RateLimited
            } else if rejected {
                BrokerFailureKind::Rejected
            } else {
                BrokerFailureKind::Http
            });
            e.http_status = Some(status);
            e.retry_after_seconds = retry;
            e.outcome_unknown = mutation && !rejected;
            return Err(e);
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                let mut e = BrokerFailure::new(BrokerFailureKind::Transport);
                e.outcome_unknown = mutation;
                e
            })?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            let mut e = BrokerFailure::new(BrokerFailureKind::InvalidResponse);
            e.outcome_unknown = mutation;
            return Err(e);
        }
        Ok(bytes)
    }
}

fn retry_delay(headers: &header::HeaderMap) -> Option<u64> {
    let now = Utc::now();
    let retry_after = headers
        .get(header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.parse::<u64>().ok().or_else(|| {
                chrono::DateTime::parse_from_rfc2822(s)
                    .ok()
                    .map(|date| (date.to_utc() - now).num_seconds().max(0) as u64 + 1)
            })
        });
    let reset = if headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        == Some("0")
    {
        headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i64>().ok())
            .map(|reset| reset.saturating_sub(now.timestamp()).max(0) as u64 + 1)
    } else {
        None
    };
    retry_after.into_iter().chain(reset).max()
}

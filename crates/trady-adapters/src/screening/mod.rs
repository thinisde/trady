//! Approved runtime JSON universe ingestion over the shared protected transport.

use crate::news::{
    HttpRequest, HttpTransport, MAX_BODY_BYTES, SafeHttp, http::validate_destination,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use trady_application::screening::{UniverseSource, hex_digest};
use trady_domain::{
    Error, Result, require,
    research::SourceRights,
    screening::{
        MAX_UNIVERSE_ROWS, UniverseMember, UniverseProvenance, UniverseSnapshot, valid_reference,
    },
};

pub const PARSER_VERSION: &str = "trady-sp500-json-v1";

/// Owner/deployment configuration, never accepted from the downloaded document.
/// There is no default approved vendor or source-rights grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedUniverseConfig {
    pub config_version: u32,
    pub enabled: bool,
    pub source_id: String,
    pub url: String,
    pub allowed_hosts: Vec<String>,
    pub expected_provider: String,
    pub expected_taxonomy_edition: String,
    pub rights: SourceRights,
    pub minimum_interval_seconds: u32,
    pub daily_request_limit: u32,
}
impl ApprovedUniverseConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1
                && valid_reference(&self.source_id)
                && valid_reference(&self.expected_taxonomy_edition)
                && !self.expected_provider.trim().is_empty()
                && self.expected_provider.len() <= 256
                && !self.expected_provider.chars().any(char::is_control)
                && !self.url.is_empty()
                && self.url.len() <= 2048
                && !self.allowed_hosts.is_empty()
                && self.allowed_hosts.len() <= 16
                && self.allowed_hosts.iter().all(|h| {
                    !h.is_empty()
                        && h.len() <= 253
                        && h.bytes().all(|b| {
                            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-')
                        })
                })
                && (60..=86400).contains(&self.minimum_interval_seconds)
                && (1..=10000).contains(&self.daily_request_limit)
                && (1..=8760).contains(&self.rights.retention_hours)
                && self.rights.terms_reference.len() <= 512,
            "invalid approved universe configuration",
        )?;
        let url = self.parsed_url()?;
        // Source fetching never reads credentials or accepts token-bearing URLs.
        require(
            url.query().is_none() && url.fragment().is_none(),
            "universe URL must not contain query or fragment",
        )?;
        validate_destination(&url, &self.allowed_hosts)
            .map_err(|_| Error("unapproved universe destination".into()))?;
        if self.enabled {
            require(
                self.rights.store_content && !self.rights.terms_reference.trim().is_empty(),
                "reviewed universe storage rights are required",
            )?;
        }
        Ok(())
    }

    fn parsed_url(&self) -> Result<Url> {
        Url::parse(&self.url).map_err(|_| Error("invalid universe URL".into()))
    }
}

/// Wire contract for an owner-approved licensed provider/export endpoint. The
/// runtime HTTP response supplies every constituent; no constituent list is built in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniverseDocument {
    pub schema_version: u32,
    pub index_id: String,
    pub complete: bool,
    pub snapshot_id: String,
    pub classification_snapshot_id: String,
    pub provider: String,
    pub taxonomy_edition: String,
    pub as_of: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub members: Vec<UniverseMember>,
}

pub struct HttpUniverseSource<T = SafeHttp> {
    config: ApprovedUniverseConfig,
    transport: T,
    last_attempt_at: Option<DateTime<Utc>>,
    request_day: Option<NaiveDate>,
    requests_today: u32,
    retry_at: Option<DateTime<Utc>>,
    bearer: Option<String>,
}
impl HttpUniverseSource<SafeHttp> {
    pub fn new(config: ApprovedUniverseConfig) -> Result<Self> {
        Self::with_transport(config, SafeHttp)
    }
}
impl<T: HttpTransport> HttpUniverseSource<T> {
    pub fn with_transport(config: ApprovedUniverseConfig, transport: T) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            transport,
            last_attempt_at: None,
            request_day: None,
            requests_today: 0,
            retry_at: None,
            bearer: None,
        })
    }

    /// Credential is provided out of band and is never included in a URL.
    pub fn with_bearer(mut self, bearer: Option<String>) -> Result<Self> {
        require(
            bearer.as_ref().is_none_or(|value| {
                !value.is_empty()
                    && value.len() <= 8192
                    && !value.bytes().any(|b| b.is_ascii_control())
            }),
            "invalid universe credential",
        )?;
        self.bearer = bearer;
        Ok(self)
    }

    pub fn retry_at(&self) -> Option<DateTime<Utc>> {
        self.retry_at
    }
}

impl<T: HttpTransport> UniverseSource for HttpUniverseSource<T> {
    fn fetch_universe(&mut self, now: DateTime<Utc>) -> Result<UniverseSnapshot> {
        self.config.validate()?;
        require(self.config.enabled, "universe source disabled")?;
        require(
            self.config.rights.store_content,
            "universe storage rights are required",
        )?;
        require(
            self.last_attempt_at.is_none_or(|at| {
                now.signed_duration_since(at)
                    >= Duration::seconds(i64::from(self.config.minimum_interval_seconds))
            }) && self.retry_at.is_none_or(|at| now >= at),
            "universe source interval or backoff active",
        )?;
        if self.request_day != Some(now.date_naive()) {
            self.request_day = Some(now.date_naive());
            self.requests_today = 0;
        }
        require(
            self.requests_today < self.config.daily_request_limit,
            "universe source daily request limit reached",
        )?;
        self.last_attempt_at = Some(now);
        self.requests_today += 1;
        let response = self
            .transport
            .get(HttpRequest {
                url: self.config.parsed_url()?,
                allowed_hosts: self.config.allowed_hosts.clone(),
                bearer: self.bearer.clone(),
                etag: None,
                last_modified: None,
                redirects: 0,
            })
            .map_err(|_| Error("universe transport unavailable".into()))?;
        if matches!(response.status, 429 | 503) {
            let delay = response
                .retry_after
                .as_deref()
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|v| (1..=86400).contains(v))
                .unwrap_or(60);
            self.retry_at = now.checked_add_signed(Duration::seconds(delay));
            return Err(Error("universe source rate limited".into()));
        }
        require(
            response.status == 200,
            "universe source HTTP response unavailable",
        )?;
        require(
            !response.body.is_empty() && response.body.len() <= MAX_BODY_BYTES,
            "universe response size invalid",
        )?;
        let final_url = response
            .final_url
            .ok_or_else(|| Error("universe response destination missing".into()))?;
        validate_destination(&final_url, &self.config.allowed_hosts)
            .map_err(|_| Error("unapproved universe response destination".into()))?;
        require(
            final_url == self.config.parsed_url()?,
            "universe source redirects are forbidden",
        )?;
        let document: UniverseDocument = serde_json::from_slice(&response.body)
            .map_err(|_| Error("universe JSON document invalid".into()))?;
        require(
            document.schema_version == 1
                && document.index_id == "SP500"
                && document.complete
                && document.provider == self.config.expected_provider
                && document.taxonomy_edition == self.config.expected_taxonomy_edition
                && valid_reference(&document.snapshot_id)
                && valid_reference(&document.classification_snapshot_id)
                && document.members.len() <= MAX_UNIVERSE_ROWS
                && document.as_of <= document.available_at
                && document.available_at <= now
                && document.as_of < document.expires_at
                && now < document.expires_at,
            "universe document scope, completeness or time invalid",
        )?;
        self.retry_at = None;
        Ok(UniverseSnapshot {
            snapshot_id: document.snapshot_id,
            classification_snapshot_id: document.classification_snapshot_id,
            index_id: document.index_id,
            complete: document.complete,
            taxonomy_edition: document.taxonomy_edition,
            as_of: document.as_of,
            available_at: document.available_at,
            expires_at: document.expires_at,
            provenance: UniverseProvenance {
                source_id: self.config.source_id.clone(),
                source_url: final_url.to_string(),
                provider: document.provider,
                content_sha256: hex_digest(&response.body),
                parser_version: PARSER_VERSION.into(),
                fetched_at: now,
                rights: self.config.rights.clone(),
            },
            members: document.members,
        })
    }
}

#[cfg(test)]
mod tests;

//! Explicitly configured, read-only source adapters. Remote text is never executable.

mod alpha;
mod financial_news;
pub mod http;
mod rss;
mod x;
mod x_meter;

use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use sha2::{Digest, Sha256};
use trady_application::research::*;
use trady_domain::{Error, Result, require, research::*};

pub use http::{HttpRequest, HttpResponse, HttpTransport, SafeHttp};
pub use x::XUser;
pub use x_meter::MeteredHttp;

pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
pub const PARSER_VERSION: &str = "trady-news-2";

pub struct SourceClient<T = SafeHttp> {
    transport: T,
    credential: fn(&str) -> Result<Option<String>>,
}
impl Default for SourceClient<SafeHttp> {
    fn default() -> Self {
        Self {
            transport: SafeHttp,
            credential: |name| {
                crate::credentials::optional(name).map(|value| value.map(|s| s.into_inner()))
            },
        }
    }
}
impl<T: HttpTransport> SourceClient<T> {
    pub fn with_transport(transport: T, credential: fn(&str) -> Result<Option<String>>) -> Self {
        Self {
            transport,
            credential,
        }
    }

    fn token(&self, source: &SourceConfig) -> std::result::Result<String, SourceFailure> {
        let name = source
            .credential_env
            .as_deref()
            .ok_or_else(|| SourceFailure::unavailable("source_credential_missing"))?;
        (self.credential)(name)
            .map_err(|_| SourceFailure::unavailable("source_credential_unavailable"))?
            .filter(|s| !s.trim().is_empty() && !s.chars().any(char::is_control))
            .ok_or_else(|| SourceFailure::unavailable("source_credential_missing"))
    }

    fn get(
        &mut self,
        request: HttpRequest,
        now: DateTime<Utc>,
    ) -> std::result::Result<HttpResponse, SourceFailure> {
        if request.url.host_str() == Some("api.x.com") && !self.transport.x_budget_enforced() {
            return Err(SourceFailure::unavailable("x_durable_cost_gate_required"));
        }
        let response = self.transport.get(request)?;
        if response.status == 429 || response.status == 503 {
            let retry_at = response
                .retry_after
                .as_deref()
                .and_then(|s| {
                    s.parse::<i64>()
                        .ok()
                        .map(|n| now + Duration::seconds(n.clamp(1, 86400)))
                })
                .or_else(|| {
                    response
                        .retry_after
                        .as_deref()
                        .and_then(|s| DateTime::parse_from_rfc2822(s).ok())
                        .map(|at| at.to_utc())
                })
                .or_else(|| {
                    response
                        .rate_reset
                        .and_then(|s| DateTime::from_timestamp(s, 0))
                })
                .unwrap_or(now + Duration::seconds(60));
            return Err(SourceFailure {
                status: CollectionStatus::RateLimited,
                reason: "provider_rate_limited".into(),
                retry_at: Some(retry_at.max(now + Duration::seconds(1))),
            });
        }
        if !matches!(response.status, 200 | 304) {
            return Err(SourceFailure::unavailable(match response.status {
                401 | 403 => "provider_access_denied",
                404 | 410 => "content_unavailable",
                402 => "provider_credits_exhausted",
                _ => "provider_http_failure",
            }));
        }
        Ok(response)
    }

    pub fn resolve_x(
        &mut self,
        source: &SourceConfig,
        handle: &str,
        now: DateTime<Utc>,
    ) -> std::result::Result<XUser, SourceFailure> {
        check_source(source)?;
        self.transport.set_x_scope(source)?;
        x::resolve(self, source, handle, now)
    }

    pub fn lookup_x(
        &mut self,
        source: &SourceConfig,
        url: &str,
        now: DateTime<Utc>,
    ) -> std::result::Result<SourcePage, SourceFailure> {
        check_source(source)?;
        self.transport.set_x_scope(source)?;
        if source.kind != SourceKind::XLookup {
            return Err(SourceFailure::unavailable(
                "lookup_requires_x_lookup_source",
            ));
        }
        x::lookup(self, source, url, now)
    }

    pub fn recent_x_counts(
        &mut self,
        source: &SourceConfig,
        window: &CollectionRequest,
        now: DateTime<Utc>,
    ) -> std::result::Result<XCountsMeasurement, SourceFailure> {
        check_source(source)?;
        if let Some(gap) = source.activation_gap(now) {
            return Err(SourceFailure::unavailable(gap));
        }
        self.transport.set_x_scope(source)?;
        x::counts(self, source, window, now)
    }
}

impl<T: HttpTransport> NewsSource for SourceClient<T> {
    fn fetch(
        &mut self,
        source: &SourceConfig,
        request: &CollectionRequest,
        now: DateTime<Utc>,
    ) -> std::result::Result<SourcePage, SourceFailure> {
        check_source(source)?;
        if let Some(gap) = source.activation_gap(now) {
            return Err(SourceFailure::unavailable(gap));
        }
        self.transport.set_x_scope(source)?;
        match source.kind {
            SourceKind::XTimeline => x::timeline(self, source, request, now),
            SourceKind::XRecentSearch => x::recent_search(self, source, request, now),
            SourceKind::GoogleNewsRss | SourceKind::PublisherRss => {
                rss::fetch(self, source, request, now)
            }
            SourceKind::AlphaVantage => alpha::fetch(self, source, request, now),
            SourceKind::Finnhub | SourceKind::Gdelt => {
                financial_news::fetch(self, source, request, now)
            }
            SourceKind::XLookup => Err(SourceFailure::unavailable(
                "lookup_source_is_not_a_timeline",
            )),
        }
    }
}

impl<T: HttpTransport> trady_application::x_monitoring::XCountsSource for SourceClient<T> {
    fn counts(
        &mut self,
        source: &SourceConfig,
        window: &CollectionRequest,
        now: DateTime<Utc>,
    ) -> std::result::Result<XCountsMeasurement, SourceFailure> {
        self.recent_x_counts(source, window, now)
    }
}

fn check_source(source: &SourceConfig) -> std::result::Result<(), SourceFailure> {
    source
        .validate()
        .map_err(|_| SourceFailure::unavailable("invalid_source_configuration"))?;
    if !source.enabled {
        return Err(SourceFailure::unavailable("source_disabled"));
    }
    Ok(())
}

pub fn digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Offline parsing uses the same bounds and source-rights projection as network collection.
pub fn parse_feed(
    source: &SourceConfig,
    body: &[u8],
    limit: usize,
    now: DateTime<Utc>,
) -> std::result::Result<SourcePage, SourceFailure> {
    source
        .validate()
        .map_err(|_| SourceFailure::unavailable("invalid_source_configuration"))?;
    if !matches!(
        source.kind,
        SourceKind::GoogleNewsRss | SourceKind::PublisherRss
    ) || !(1..=100).contains(&limit)
    {
        return Err(SourceFailure::unavailable("invalid_feed_parser_scope"));
    }
    rss::parse(source, body, limit, now, None, None, None)
}

/// Canonicalizes references without fetching them or granting access to their hosts.
pub fn canonical_url(input: &str) -> Result<String> {
    require(input.len() <= 4096, "source URL is too long")?;
    let mut url = Url::parse(input).map_err(|_| Error("invalid source URL".into()))?;
    require(
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
            && url.host_str().is_some(),
        "unsupported source URL",
    )?;
    let host = url.host_str().unwrap_or_default();
    require(
        !matches!(host, "localhost" | "localhost.localdomain")
            && !host.ends_with(".local")
            && !host.ends_with(".localhost"),
        "local source URL is forbidden",
    )?;
    if let Ok(address) = host.trim_matches(['[', ']']).parse() {
        require(
            http::public_address(address),
            "nonpublic source URL is forbidden",
        )?;
    }
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| {
            let key = k.to_ascii_lowercase();
            !key.starts_with("utm_")
                && !matches!(key.as_str(), "fbclid" | "gclid" | "ref_src" | "ref_url")
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    url.set_fragment(None);
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    Ok(url.into())
}

pub fn x_post_id(input: &str) -> Result<String> {
    let url = Url::parse(input).map_err(|_| Error("invalid X URL".into()))?;
    require(
        url.scheme() == "https"
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && matches!(
                url.host_str(),
                Some(
                    "x.com"
                        | "www.x.com"
                        | "twitter.com"
                        | "www.twitter.com"
                        | "mobile.twitter.com"
                )
            ),
        "unapproved X URL host or scheme",
    )?;
    let parts: Vec<_> = url
        .path_segments()
        .ok_or_else(|| Error("invalid X URL path".into()))?
        .collect();
    let id = match parts.as_slice() {
        [handle, "status", id]
            if !handle.is_empty()
                && handle.len() <= 15
                && handle
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_') =>
        {
            *id
        }
        ["i", "web", "status", id] => *id,
        _ => return Err(Error("expected an X post URL".into())),
    };
    require(valid_x_id(id), "invalid X post ID")?;
    Ok(id.into())
}

struct ItemDraft {
    external_id: String,
    url: String,
    publisher: String,
    feed_title: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    text: Option<String>,
    published_at: Option<DateTime<Utc>>,
    language: Option<String>,
    status: ContentStatus,
    social: Option<SocialObservation>,
    related_urls: Vec<String>,
    gaps: Vec<String>,
}

fn item(source: &SourceConfig, draft: ItemDraft, now: DateTime<Utc>) -> Result<SourceItem> {
    let url = canonical_url(&draft.url)?;
    require(
        !draft.external_id.is_empty() && draft.external_id.len() <= 4096,
        "invalid external item ID",
    )?;
    require(
        draft.feed_title.as_ref().is_none_or(|s| s.len() <= 4096)
            && draft.title.as_ref().is_none_or(|s| s.len() <= 4096)
            && draft.summary.as_ref().is_none_or(|s| s.len() <= 32768)
            && draft.text.as_ref().is_none_or(|s| s.len() <= 65536),
        "source content field exceeds bound",
    )?;
    let mut related_urls: Vec<_> = draft
        .related_urls
        .iter()
        .filter_map(|s| canonical_url(s).ok())
        .take(16)
        .collect();
    related_urls.sort();
    related_urls.dedup();
    let lineage = related_urls.first().unwrap_or(&url);
    let lineage_id = if let Some(social) = &draft.social {
        if let Some(original) = social.reposted_id.as_ref().or(social.quoted_id.as_ref()) {
            digest(&format!("https://x.com/i/status/{original}"))
        } else if related_urls.is_empty() {
            digest(&format!(
                "https://x.com/i/status/{}",
                social
                    .edit_history_ids
                    .first()
                    .unwrap_or(&draft.external_id)
            ))
        } else {
            digest(lineage)
        }
    } else {
        digest(lineage)
    };
    let content_hash = digest(
        &serde_json::to_string(&(
            &draft.external_id,
            &url,
            &draft.publisher,
            &draft.feed_title,
            &draft.title,
            &draft.summary,
            &draft.text,
            &draft.published_at,
            &draft.language,
            &draft.social,
            &related_urls,
            &source.rights,
            draft.status,
        ))
        .map_err(|_| Error("content hashing failed".into()))?,
    );
    let mut item = SourceItem {
        item_id: digest(&format!("{}:{}", source.id, draft.external_id)),
        source_id: source.id.clone(),
        external_id: draft.external_id,
        canonical_url: url,
        lineage_id,
        content_hash,
        parser_version: PARSER_VERSION.into(),
        publisher: draft.publisher,
        feed_title: draft.feed_title,
        title: draft.title,
        summary: draft.summary,
        text: draft.text,
        published_at: draft.published_at,
        event_at: None,
        first_seen_at: now,
        fetched_at: now,
        expires_at: now + Duration::hours(i64::from(source.rights.retention_hours)),
        language: draft.language,
        rights: source.rights.clone(),
        content_status: draft.status,
        social: draft.social,
        related_urls,
        gaps: draft.gaps,
    };
    item.redact(now, false);
    Ok(item)
}

#[cfg(test)]
mod tests;

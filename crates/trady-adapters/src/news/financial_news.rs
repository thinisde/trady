//! Company discovery shares SourceItem lineage, rights, corrections and FinBERT paths.
use super::*;
use chrono::NaiveDateTime;
use serde_json::Value;

pub(super) fn fetch<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    window: &CollectionRequest,
    now: DateTime<Utc>,
) -> std::result::Result<SourcePage, SourceFailure> {
    if window.next_token.is_some() {
        return Err(SourceFailure::unavailable(
            "financial_news_cursor_unsupported",
        ));
    }
    let mut url = Url::parse(&source.url)
        .map_err(|_| SourceFailure::unavailable("financial_news_url_invalid"))?;
    let query = source
        .query
        .as_deref()
        .ok_or_else(|| SourceFailure::unavailable("financial_news_query_required"))?;
    match source.kind {
        SourceKind::Finnhub => {
            if url.as_str() != "https://api.finnhub.io/api/v1/company-news"
                || source.credential_env.as_deref() != Some("TRADY_FINNHUB_API_KEY")
            {
                return Err(SourceFailure::unavailable(
                    "unapproved_finnhub_endpoint_or_credential",
                ));
            }
            url.query_pairs_mut()
                .append_pair("symbol", query)
                .append_pair("from", &window.start_at.date_naive().to_string())
                .append_pair("to", &window.end_at.date_naive().to_string())
                .append_pair("token", &client.token(source)?);
        }
        SourceKind::Gdelt => {
            if url.as_str() != "https://api.gdeltproject.org/api/v2/doc/doc"
                || source.credential_env.is_some()
            {
                return Err(SourceFailure::unavailable("unapproved_gdelt_endpoint"));
            }
            url.query_pairs_mut()
                .append_pair("query", query)
                .append_pair("mode", "artlist")
                .append_pair("format", "json")
                .append_pair(
                    "startdatetime",
                    &window.start_at.format("%Y%m%d%H%M%S").to_string(),
                )
                .append_pair(
                    "enddatetime",
                    &window.end_at.format("%Y%m%d%H%M%S").to_string(),
                )
                .append_pair("maxrecords", &window.limit.to_string())
                .append_pair("sort", "datedesc");
        }
        _ => return Err(SourceFailure::unavailable("financial_news_kind_invalid")),
    }
    let response = client.get(
        HttpRequest {
            url,
            allowed_hosts: source.allowed_hosts.clone(),
            bearer: None,
            etag: None,
            last_modified: None,
            redirects: 0,
        },
        now,
    )?;
    if source.kind == SourceKind::Gdelt
        && std::str::from_utf8(&response.body).is_ok_and(|text| {
            text.trim_start()
                .starts_with("Please limit requests to one every 5 seconds")
        })
    {
        return Err(SourceFailure {
            status: CollectionStatus::RateLimited,
            reason: "gdelt_rate_limited".into(),
            retry_at: Some(now + Duration::seconds(60)),
        });
    }
    let value: Value = serde_json::from_slice(&response.body)
        .map_err(|_| SourceFailure::unavailable("financial_news_json_invalid"))?;
    if value.get("error").is_some() || value.get("Information").is_some() {
        return Err(SourceFailure::unavailable("financial_news_provider_error"));
    }
    let rows = if source.kind == SourceKind::Finnhub {
        value.as_array()
    } else {
        value["articles"].as_array()
    }
    .ok_or_else(|| {
        if source.kind == SourceKind::Gdelt {
            SourceFailure {
                status: CollectionStatus::Unavailable,
                reason: "gdelt_articles_missing".into(),
                retry_at: Some(now + Duration::seconds(60)),
            }
        } else {
            SourceFailure::unavailable("financial_news_rows_missing")
        }
    })?;
    let saturated = rows.len() >= window.limit;
    let mut items = vec![];
    for row in rows.iter().take(window.limit) {
        let field = |key: &str| {
            row[key]
                .as_str()
                .ok_or_else(|| SourceFailure::unavailable("financial_news_required_field_missing"))
        };
        let raw_url = field("url")?;
        // Publisher URLs are provenance only, never fetched by this collector.
        let canonical = canonical_url(raw_url)
            .map_err(|_| SourceFailure::unavailable("financial_news_article_url_invalid"))?;
        let (published_at, external_id, publisher, title, summary, language) =
            if source.kind == SourceKind::Finnhub {
                let at = row["datetime"]
                    .as_i64()
                    .and_then(|s| DateTime::from_timestamp(s, 0))
                    .ok_or_else(|| SourceFailure::unavailable("finnhub_publication_invalid"))?;
                let id = row["id"]
                    .as_u64()
                    .ok_or_else(|| SourceFailure::unavailable("finnhub_item_id_missing"))?;
                (
                    Some(at),
                    id.to_string(),
                    field("source")?.to_owned(),
                    field("headline")?.to_owned(),
                    row["summary"].as_str().map(String::from),
                    None,
                )
            } else {
                let seen = NaiveDateTime::parse_from_str(field("seendate")?, "%Y%m%dT%H%M%SZ")
                    .map_err(|_| SourceFailure::unavailable("gdelt_seen_time_invalid"))?
                    .and_utc();
                if seen > now || seen < window.start_at || seen > window.end_at {
                    return Err(SourceFailure::unavailable("gdelt_seen_time_outside_window"));
                }
                (
                    None,
                    canonical.clone(),
                    field("domain")?.to_owned(),
                    field("title")?.to_owned(),
                    None,
                    row["language"].as_str().map(String::from),
                )
            };
        if published_at.is_some_and(|at| at > now) {
            return Err(SourceFailure::unavailable(
                "financial_news_future_publication",
            ));
        }
        if published_at.is_some_and(|at| at < window.start_at || at > window.end_at) {
            continue;
        }
        let mut gaps = vec!["primary_source_not_verified".into()];
        if source.kind == SourceKind::Gdelt {
            gaps.push("discovery_time_is_not_publication_time".into());
        }
        items.push(
            item(
                source,
                ItemDraft {
                    external_id,
                    url: canonical,
                    publisher,
                    feed_title: None,
                    title: Some(title),
                    summary,
                    text: None,
                    published_at,
                    language,
                    status: ContentStatus::Snippet,
                    social: None,
                    related_urls: vec![],
                    gaps,
                },
                now,
            )
            .map_err(|_| SourceFailure::unavailable("financial_news_item_invalid"))?,
        );
    }
    // Neither route offers a trustworthy bounded continuation. Saturation remains PARTIAL.
    Ok(SourcePage {
        items,
        next_token: None,
        etag: None,
        last_modified: None,
        complete: !saturated,
        gaps: if saturated {
            vec!["financial_news_window_saturated_history_incomplete".into()]
        } else {
            vec![]
        },
    })
}

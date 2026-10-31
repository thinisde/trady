use super::*;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Cursor {
    windows: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

#[derive(Deserialize)]
struct Article {
    title: String,
    url: String,
    time_published: String,
    summary: String,
    source: String,
}

pub(super) fn fetch<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    window: &CollectionRequest,
    now: DateTime<Utc>,
) -> std::result::Result<SourcePage, SourceFailure> {
    let mut cursor = if let Some(token) = &window.next_token {
        serde_json::from_str::<Cursor>(token)
            .map_err(|_| SourceFailure::unavailable("news_api_cursor_invalid"))?
    } else {
        Cursor {
            windows: vec![(window.start_at, window.end_at)],
        }
    };
    if cursor.windows.is_empty() || cursor.windows.len() > 24 {
        return Err(SourceFailure::unavailable("news_api_cursor_invalid"));
    }
    let (start, end) = cursor.windows.remove(0);
    if start >= end || start < window.start_at || end > window.end_at {
        return Err(SourceFailure::unavailable("news_api_cursor_outside_window"));
    }
    let mut url =
        Url::parse(&source.url).map_err(|_| SourceFailure::unavailable("invalid_news_api_url"))?;
    if url.as_str() != "https://www.alphavantage.co/query" {
        return Err(SourceFailure::unavailable("unapproved_news_api_endpoint"));
    }
    let token = client.token(source)?;
    url.query_pairs_mut()
        .append_pair("function", "NEWS_SENTIMENT")
        .append_pair("apikey", &token)
        .append_pair("sort", "EARLIEST")
        .append_pair("limit", &window.limit.to_string())
        .append_pair("time_from", &start.format("%Y%m%dT%H%M").to_string())
        .append_pair("time_to", &end.format("%Y%m%dT%H%M").to_string());
    if let Some(tickers) = &source.query {
        url.query_pairs_mut().append_pair("tickers", tickers);
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
    let value: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|_| SourceFailure::unavailable("news_api_response_invalid"))?;
    if value.get("Note").is_some() || value.get("Information").is_some() {
        return Err(SourceFailure {
            status: CollectionStatus::RateLimited,
            reason: "news_api_quota_or_access_notice".into(),
            retry_at: Some(now + Duration::minutes(15)),
        });
    }
    let feed = value
        .get("feed")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| SourceFailure::unavailable("news_api_missing_feed"))?;
    if feed.len() > window.limit {
        return Err(SourceFailure::unavailable("news_api_item_budget_exceeded"));
    }
    let mut items = vec![];
    for value in feed {
        let article: Article = serde_json::from_value(value.clone())
            .map_err(|_| SourceFailure::unavailable("news_article_invalid"))?;
        let at = NaiveDateTime::parse_from_str(&article.time_published, "%Y%m%dT%H%M%S")
            .map_err(|_| SourceFailure::unavailable("news_article_date_invalid"))?
            .and_utc();
        items.push(
            item(
                source,
                ItemDraft {
                    external_id: canonical_url(&article.url)
                        .map_err(|_| SourceFailure::unavailable("news_article_url_invalid"))?,
                    url: article.url,
                    publisher: article.source,
                    feed_title: None,
                    title: Some(article.title),
                    summary: Some(article.summary),
                    text: None,
                    published_at: Some(at),
                    language: Some("en".into()),
                    status: ContentStatus::Snippet,
                    social: None,
                    related_urls: vec![],
                    gaps: vec!["primary_source_not_verified".into()],
                },
                now,
            )
            .map_err(|_| SourceFailure::unavailable("news_article_validation_failed"))?,
        );
    }
    // Recover result caps through minute-aligned subdivision, retaining pending work.
    let saturated = feed.len() == window.limit;
    if saturated {
        let middle =
            (start.timestamp() / 60 + (end.timestamp() / 60 - start.timestamp() / 60) / 2) * 60;
        let middle = DateTime::from_timestamp(middle, 0)
            .ok_or_else(|| SourceFailure::unavailable("news_api_window_invalid"))?;
        if middle <= start || middle >= end {
            if !cursor.windows.is_empty() {
                // Keep a saturated minute pending after its recoverable siblings.
                cursor.windows.push((start, end));
                return Ok(SourcePage {
                    items,
                    next_token: Some(
                        serde_json::to_string(&cursor)
                            .map_err(|_| SourceFailure::unavailable("news_api_cursor_invalid"))?,
                    ),
                    etag: None,
                    last_modified: None,
                    complete: true,
                    gaps: vec!["news_api_minute_window_saturated_history_incomplete".into()],
                });
            }
            return Ok(SourcePage {
                items,
                next_token: None,
                etag: None,
                last_modified: None,
                complete: false,
                gaps: vec!["news_api_minute_window_saturated_history_incomplete".into()],
            });
        }
        cursor.windows.insert(0, (middle, end));
        cursor.windows.insert(0, (start, middle));
    }
    if cursor.windows.len() > 24 {
        return Err(SourceFailure::unavailable(
            "news_api_pending_window_budget_exhausted",
        ));
    }
    let next_token = if cursor.windows.is_empty() {
        None
    } else {
        Some(
            serde_json::to_string(&cursor)
                .map_err(|_| SourceFailure::unavailable("news_api_cursor_invalid"))?,
        )
    };
    Ok(SourcePage {
        items,
        next_token,
        etag: None,
        last_modified: None,
        complete: true,
        gaps: if saturated {
            vec!["news_api_result_cap_window_subdivided".into()]
        } else {
            vec![]
        },
    })
}

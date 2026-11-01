use super::*;
use serde::{Deserialize, Serialize};

const FIELDS: &str = "id,text,author_id,created_at,conversation_id,entities,lang,referenced_tweets,edit_history_tweet_ids";
const GAPS: &[&str] = &[
    "POLLING_not_realtime",
    "deletions_and_unreposts_not_delivered",
    "timeline_history_is_provider_limited",
    "media_content_not_interpreted",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XUser {
    pub id: String,
    pub username: String,
    pub name: String,
}

#[derive(Deserialize)]
struct UserResponse {
    data: Option<XUser>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Clone, Deserialize)]
struct Reference {
    #[serde(rename = "type")]
    kind: String,
    id: String,
}
#[derive(Clone, Default, Deserialize)]
struct Entities {
    #[serde(default)]
    urls: Vec<Link>,
}
#[derive(Clone, Deserialize)]
struct Link {
    expanded_url: Option<String>,
    unwound_url: Option<String>,
}

#[derive(Clone, Deserialize)]
struct Post {
    id: String,
    text: String,
    author_id: String,
    created_at: DateTime<Utc>,
    conversation_id: Option<String>,
    lang: Option<String>,
    // With referenced_tweets requested, X omits this optional field on originals.
    // An explicit null still means unavailable metadata and stays fail-closed.
    #[serde(default = "original_references", alias = "referenced_tweets")]
    referenced_posts: Option<Vec<Reference>>,
    #[serde(default, alias = "edit_history_tweet_ids")]
    edit_history_post_ids: Vec<String>,
    #[serde(default)]
    entities: Entities,
}
#[derive(Default, Deserialize)]
struct Includes {
    #[serde(default, alias = "tweets")]
    posts: Vec<Post>,
}
#[derive(Default, Deserialize)]
struct Meta {
    next_token: Option<String>,
    result_count: Option<usize>,
}
#[derive(Deserialize)]
struct Timeline {
    data: Option<Vec<Post>>,
    #[serde(default)]
    includes: Includes,
    #[serde(default)]
    meta: Meta,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
struct Lookup {
    data: Option<Post>,
    #[serde(default)]
    includes: Includes,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

fn original_references() -> Option<Vec<Reference>> {
    Some(vec![])
}

fn base(source: &SourceConfig) -> std::result::Result<Url, SourceFailure> {
    let url =
        Url::parse(&source.url).map_err(|_| SourceFailure::unavailable("invalid_x_api_url"))?;
    if !matches!(
        source.kind,
        SourceKind::XTimeline | SourceKind::XLookup | SourceKind::XRecentSearch
    ) || url.as_str() != "https://api.x.com/2/"
        || !source.allowed_hosts.iter().any(|h| h == "api.x.com")
    {
        return Err(SourceFailure::unavailable("unapproved_x_api_endpoint"));
    }
    Ok(url)
}

fn request(source: &SourceConfig, url: Url, token: String) -> HttpRequest {
    HttpRequest {
        url,
        allowed_hosts: source.allowed_hosts.clone(),
        bearer: Some(token),
        etag: None,
        last_modified: None,
        redirects: 0,
    }
}

pub(super) fn resolve<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    handle: &str,
    now: DateTime<Utc>,
) -> std::result::Result<XUser, SourceFailure> {
    if handle.is_empty()
        || handle.len() > 15
        || !handle
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(SourceFailure::unavailable("invalid_x_handle"));
    }
    let url = base(source)?
        .join(&format!("users/by/username/{handle}"))
        .map_err(|_| SourceFailure::unavailable("invalid_x_api_url"))?;
    let token = client.token(source)?;
    let response = client.get(request(source, url, token), now)?;
    let parsed: UserResponse = serde_json::from_slice(&response.body)
        .map_err(|_| SourceFailure::unavailable("x_identity_response_invalid"))?;
    let user = parsed
        .data
        .filter(|u| valid_x_id(&u.id) && u.username.eq_ignore_ascii_case(handle))
        .ok_or_else(|| SourceFailure::unavailable("x_identity_unavailable_or_changed"))?;
    if !parsed.errors.is_empty() {
        return Err(SourceFailure::unavailable("x_identity_response_partial"));
    }
    Ok(user)
}

pub(super) fn timeline<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    window: &CollectionRequest,
    now: DateTime<Utc>,
) -> std::result::Result<SourcePage, SourceFailure> {
    let account = source
        .x_account
        .as_ref()
        .ok_or_else(|| SourceFailure::unavailable("x_account_missing"))?;
    let user = resolve(client, source, &account.handle, now)?;
    if account.user_id.as_ref() != Some(&user.id) {
        return Err(SourceFailure::unavailable("x_stable_identity_mismatch"));
    }
    let mut url = base(source)?
        .join(&format!("users/{}/tweets", user.id))
        .map_err(|_| SourceFailure::unavailable("invalid_x_api_url"))?;
    url.query_pairs_mut()
        .append_pair("max_results", &window.limit.to_string())
        .append_pair(
            "start_time",
            &window
                .start_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        )
        .append_pair(
            "end_time",
            &window
                .end_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        );
    post_fields(&mut url);
    if let Some(cursor) = &window.next_token {
        url.query_pairs_mut()
            .append_pair("pagination_token", cursor);
    }
    // Deliberately no exclude=replies/retweets: these are the actor's own actions.
    let token = client.token(source)?;
    let response = client.get(request(source, url, token), now)?;
    let parsed: Timeline = serde_json::from_slice(&response.body)
        .map_err(|_| SourceFailure::unavailable("x_timeline_response_invalid"))?;
    if parsed.data.is_none() && parsed.meta.result_count != Some(0) {
        return Err(SourceFailure::unavailable("x_timeline_missing_data"));
    }
    let data = parsed.data.unwrap_or_default();
    if data.len() > window.limit {
        return Err(SourceFailure::unavailable("x_item_budget_exceeded"));
    }
    if data.iter().any(|p| p.author_id != user.id) {
        return Err(SourceFailure::unavailable(
            "x_incoming_reaction_is_not_actor_speech",
        ));
    }
    let mut gaps: Vec<String> = GAPS.iter().map(|s| (*s).into()).collect();
    if !parsed.errors.is_empty() {
        gaps.push("x_partial_response_context_or_posts_missing".into());
    }
    if data.is_empty() {
        gaps.push("empty_timeline_does_not_verify_recent_account_activity".into());
    }
    let items = data
        .iter()
        .map(|p| normalize(source, p, &parsed.includes, now))
        .collect::<Result<Vec<_>>>()
        .map_err(|_| SourceFailure::unavailable("x_post_validation_failed"))?;
    Ok(SourcePage {
        items,
        next_token: parsed.meta.next_token,
        etag: None,
        last_modified: None,
        complete: parsed.errors.is_empty(),
        gaps,
    })
}

fn post_fields(url: &mut Url) {
    // The v2 wire API retains tweet-named fields and expansion paths.
    url.query_pairs_mut()
        .append_pair("tweet.fields", FIELDS)
        .append_pair(
            "expansions",
            "author_id,referenced_tweets.id,edit_history_tweet_ids",
        );
}

pub(super) fn lookup<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    input: &str,
    now: DateTime<Utc>,
) -> std::result::Result<SourcePage, SourceFailure> {
    let id = x_post_id(input).map_err(|_| SourceFailure::unavailable("invalid_submitted_x_url"))?;
    let mut url = base(source)?
        .join(&format!("tweets/{id}"))
        .map_err(|_| SourceFailure::unavailable("invalid_x_api_url"))?;
    post_fields(&mut url);
    let token = client.token(source)?;
    let response = client.get(request(source, url, token), now)?;
    let parsed: Lookup = serde_json::from_slice(&response.body)
        .map_err(|_| SourceFailure::unavailable("x_lookup_response_invalid"))?;
    let post = parsed
        .data
        .ok_or_else(|| SourceFailure::unavailable("post_deleted_private_or_inaccessible"))?;
    if post.id != id && !post.edit_history_post_ids.contains(&id) {
        return Err(SourceFailure::unavailable("x_lookup_identity_mismatch"));
    }
    let item = normalize(source, &post, &parsed.includes, now)
        .map_err(|_| SourceFailure::unavailable("x_post_validation_failed"))?;
    Ok(SourcePage {
        items: vec![item],
        next_token: None,
        etag: None,
        last_modified: None,
        complete: parsed.errors.is_empty(),
        gaps: if parsed.errors.is_empty() {
            vec![]
        } else {
            vec!["x_context_unavailable".into()]
        },
    })
}

fn normalize(
    source: &SourceConfig,
    post: &Post,
    includes: &Includes,
    now: DateTime<Utc>,
) -> Result<SourceItem> {
    let references = post.referenced_posts.as_deref().unwrap_or_default();
    require(
        valid_x_id(&post.id)
            && valid_x_id(&post.author_id)
            && references.len() <= 3
            && post.edit_history_post_ids.len() <= 6
            && references.iter().all(|r| valid_x_id(&r.id))
            && post.edit_history_post_ids.iter().all(|id| valid_x_id(id)),
        "invalid X post identity or references",
    )?;
    let reference = |kind: &str| {
        references
            .iter()
            .find(|r| r.kind == kind)
            .map(|r| r.id.clone())
    };
    let reply_to_id = reference("replied_to");
    let quoted_id = reference("quoted");
    let reposted_id = reference("retweeted").or_else(|| reference("reposted"));
    require(
        references.iter().all(|r| {
            matches!(
                r.kind.as_str(),
                "replied_to" | "quoted" | "retweeted" | "reposted"
            )
        }),
        "unknown X reference kind",
    )?;
    let action = if post.referenced_posts.is_none() {
        SocialAction::Unknown
    } else if reposted_id.is_some() {
        SocialAction::Repost
    } else if quoted_id.is_some() {
        SocialAction::Quote
    } else if reply_to_id.is_some() {
        SocialAction::Reply
    } else {
        SocialAction::Original
    };
    let original = reposted_id
        .as_ref()
        .or(quoted_id.as_ref())
        .or(reply_to_id.as_ref());
    let context = original.and_then(|id| includes.posts.iter().take(100).find(|p| &p.id == id));
    let missing_context = post.referenced_posts.is_none()
        || references.iter().any(|reference| {
            !includes
                .posts
                .iter()
                .take(100)
                .any(|p| p.id == reference.id)
        });
    let mut edit_history_ids = post.edit_history_post_ids.clone();
    if edit_history_ids.is_empty() {
        edit_history_ids.push(post.id.clone());
    }
    require(
        edit_history_ids.contains(&post.id),
        "edit history omits requested post",
    )?;
    let superseded = edit_history_ids.last() != Some(&post.id);
    let mut context_posts = vec![];
    for reference in references {
        if let Some(context) = includes
            .posts
            .iter()
            .take(100)
            .find(|p| p.id == reference.id)
        {
            require(
                valid_x_id(&context.author_id) && context.text.len() <= 65536,
                "invalid X context",
            )?;
            context_posts.push(ReferencedPost {
                post_id: context.id.clone(),
                author_id: context.author_id.clone(),
                text: Some(context.text.clone()),
                published_at: context.created_at,
            });
        }
    }
    let social = SocialObservation {
        action,
        actor_user_id: post.author_id.clone(),
        original_author_id: if original.is_some() {
            context.map(|p| p.author_id.clone())
        } else if post.referenced_posts.is_some() {
            Some(post.author_id.clone())
        } else {
            None
        },
        conversation_id: post.conversation_id.clone(),
        reply_to_id,
        quoted_id,
        reposted_id,
        edit_history_ids,
        missing_context,
        context: context_posts,
    };
    item(
        source,
        ItemDraft {
            external_id: post.id.clone(),
            url: format!("https://x.com/i/status/{}", post.id),
            publisher: format!("X user {}", post.author_id),
            feed_title: None,
            title: None,
            summary: None,
            text: Some(post.text.clone()),
            published_at: Some(post.created_at),
            language: post.lang.clone(),
            status: if superseded {
                ContentStatus::Superseded
            } else {
                ContentStatus::Available
            },
            social: Some(social),
            related_urls: post
                .entities
                .urls
                .iter()
                .filter_map(|u| u.unwound_url.clone().or(u.expanded_url.clone()))
                .take(16)
                .collect(),
            gaps: {
                let mut gaps = vec![];
                if missing_context {
                    gaps.push("referenced_post_context_missing".into());
                }
                if post.referenced_posts.is_none() {
                    gaps.push("x_action_metadata_unavailable".into());
                }
                if post.edit_history_post_ids.is_empty() {
                    gaps.push("x_edit_lineage_unavailable".into());
                }
                gaps
            },
        },
        now,
    )
}

fn approved_author<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    now: DateTime<Utc>,
) -> std::result::Result<XUser, SourceFailure> {
    let account = source
        .x_account
        .as_ref()
        .ok_or_else(|| SourceFailure::unavailable("x_account_missing"))?;
    let user = resolve(client, source, &account.handle, now)?;
    if account.user_id.as_ref() != Some(&user.id) {
        return Err(SourceFailure::unavailable("x_stable_identity_mismatch"));
    }
    Ok(user)
}

fn search_window(
    source: &SourceConfig,
    window: &CollectionRequest,
    now: DateTime<Utc>,
) -> std::result::Result<String, SourceFailure> {
    if source.kind != SourceKind::XRecentSearch
        || !(10..=100).contains(&window.limit)
        || window.start_at >= window.end_at
        || window.end_at > now - Duration::seconds(10)
        || window.start_at.timestamp_subsec_nanos() != 0
        || window.end_at.timestamp_subsec_nanos() != 0
        || window.start_at < now - Duration::days(7)
        || window
            .next_token
            .as_ref()
            .is_some_and(|t| t.is_empty() || t.len() > 2048)
    {
        return Err(SourceFailure::unavailable(
            "x_recent_history_or_request_bounds_unsupported",
        ));
    }
    let account = source
        .x_account
        .as_ref()
        .ok_or_else(|| SourceFailure::unavailable("x_account_missing"))?;
    source
        .x_rule
        .as_ref()
        .ok_or_else(|| SourceFailure::unavailable("x_versioned_rule_required"))?
        .query(&account.handle)
        .map_err(|_| SourceFailure::unavailable("x_invalid_query_rule"))
}

fn interval(url: &mut Url, query: &str, window: &CollectionRequest) {
    url.query_pairs_mut()
        .append_pair("query", query)
        .append_pair(
            "start_time",
            &window
                .start_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        )
        .append_pair(
            "end_time",
            &window
                .end_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        );
    if let Some(token) = &window.next_token {
        url.query_pairs_mut().append_pair("next_token", token);
    }
}

pub(super) fn recent_search<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    window: &CollectionRequest,
    now: DateTime<Utc>,
) -> std::result::Result<SourcePage, SourceFailure> {
    let query = search_window(source, window, now)?;
    let author = approved_author(client, source, now)?;
    let mut url = base(source)?
        .join("tweets/search/recent")
        .map_err(|_| SourceFailure::unavailable("invalid_x_api_url"))?;
    interval(&mut url, &query, window);
    // Expand the author only; routine context/edit/media expansion would spend the support budget.
    url.query_pairs_mut()
        .append_pair("max_results", &window.limit.to_string())
        .append_pair("tweet.fields", FIELDS)
        .append_pair("expansions", "author_id");
    let token = client.token(source)?;
    let response = client.get(request(source, url, token), now)?;
    let parsed: Timeline = serde_json::from_slice(&response.body)
        .map_err(|_| SourceFailure::unavailable("x_search_response_invalid"))?;
    if parsed.data.is_none() && parsed.meta.result_count != Some(0) {
        return Err(SourceFailure::unavailable("x_search_missing_data"));
    }
    let data = parsed.data.unwrap_or_default();
    if data.len() > window.limit
        || data.iter().any(|p| {
            p.author_id != author.id
                || p.created_at < window.start_at
                || p.created_at >= window.end_at
        })
    {
        return Err(SourceFailure::unavailable(
            "x_search_actor_interval_or_resource_mismatch",
        ));
    }
    let rule = source.x_rule.as_ref().expect("validated rule");
    if rule.exclude_reposts
        && data.iter().any(|p| {
            p.referenced_posts.as_ref().is_none_or(|refs| {
                refs.iter()
                    .any(|r| matches!(r.kind.as_str(), "retweeted" | "reposted"))
            })
        })
    {
        return Err(SourceFailure::unavailable(
            "x_repost_exclusion_not_verified",
        ));
    }
    let items = data
        .iter()
        .map(|p| normalize(source, p, &parsed.includes, now))
        .collect::<Result<Vec<_>>>()
        .map_err(|_| SourceFailure::unavailable("x_post_validation_failed"))?;
    let mut gaps = vec![
        "RECENT_SEARCH_not_realtime".into(),
        "query_selected_coverage_not_full_timeline".into(),
        "deletions_and_unreposts_not_delivered".into(),
        "context_and_media_not_routinely_fetched".into(),
    ];
    if !parsed.errors.is_empty() {
        gaps.push("x_partial_response_context_or_posts_missing".into());
    }
    Ok(SourcePage {
        items,
        next_token: parsed.meta.next_token,
        etag: None,
        last_modified: None,
        complete: parsed.errors.is_empty(),
        gaps,
    })
}

#[derive(Deserialize)]
struct CountsResponse {
    data: Option<Vec<XCountBucket>>,
    meta: CountsMeta,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
struct CountsMeta {
    #[serde(alias = "total_tweet_count")]
    total_post_count: u64,
    next_token: Option<String>,
}

pub(super) fn counts<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    window: &CollectionRequest,
    now: DateTime<Utc>,
) -> std::result::Result<XCountsMeasurement, SourceFailure> {
    let query = search_window(source, window, now)?;
    let author = approved_author(client, source, now)?;
    let mut url = base(source)?
        .join("tweets/counts/recent")
        .map_err(|_| SourceFailure::unavailable("invalid_x_api_url"))?;
    interval(&mut url, &query, window);
    url.query_pairs_mut().append_pair("granularity", "day");
    let token = client.token(source)?;
    let response = client.get(request(source, url, token), now)?;
    let parsed: CountsResponse = serde_json::from_slice(&response.body)
        .map_err(|_| SourceFailure::unavailable("x_counts_response_invalid"))?;
    let mut buckets = parsed
        .data
        .ok_or_else(|| SourceFailure::unavailable("x_counts_missing_data_unknown_volume"))?;
    if buckets.len() > 8 {
        return Err(SourceFailure::unavailable("x_counts_bucket_bound_exceeded"));
    }
    buckets.sort_by_key(|b| b.start);
    let mut cursor = window.start_at;
    let mut total = Some(0_u64);
    let mut contiguous = true;
    for b in &buckets {
        if b.start != cursor || b.end <= b.start || b.end > window.end_at {
            contiguous = false;
        }
        cursor = b.end;
        total = total.and_then(|n| n.checked_add(b.post_count));
    }
    let complete = parsed.errors.is_empty()
        && parsed.meta.next_token.is_none()
        && contiguous
        && cursor == window.end_at
        && total == Some(parsed.meta.total_post_count);
    let account = source.x_account.as_ref().expect("validated account");
    let rule_hash = source
        .x_rule
        .as_ref()
        .expect("validated rule")
        .hash(&account.handle, &author.id)
        .map_err(|_| SourceFailure::unavailable("x_rule_hash_invalid"))?;
    let mut gaps = if complete {
        vec!["counts_are_estimates_not_future_volume_or_completeness_guarantees".into()]
    } else {
        vec!["x_counts_incomplete_volume_unknown".into()]
    };
    if !parsed.errors.is_empty() {
        gaps.push("x_counts_provider_partial_error".into());
    }
    Ok(XCountsMeasurement {
        rule_hash,
        query,
        start_at: window.start_at,
        end_at: window.end_at,
        measured_at: now,
        complete,
        total_posts: parsed.meta.total_post_count,
        buckets,
        next_token: parsed.meta.next_token,
        gaps,
    })
}

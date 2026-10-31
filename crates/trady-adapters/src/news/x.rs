use super::*;
use serde::{Deserialize, Serialize};

const FIELDS: &str = "id,text,created_at,conversation_id,entities,lang";
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
    #[serde(default, alias = "referenced_tweets")]
    referenced_posts: Vec<Reference>,
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

fn base(source: &SourceConfig) -> std::result::Result<Url, SourceFailure> {
    let url =
        Url::parse(&source.url).map_err(|_| SourceFailure::unavailable("invalid_x_api_url"))?;
    if !matches!(source.kind, SourceKind::XTimeline | SourceKind::XLookup)
        || url.as_str() != "https://api.x.com/2/"
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
    // Current X v2 names; deserialization also accepts legacy tweet-named payloads.
    url.query_pairs_mut()
        .append_pair("post.fields", FIELDS)
        .append_pair(
            "expansions",
            "author_id,referenced_posts,edit_history_post_ids",
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
    require(
        valid_x_id(&post.id)
            && valid_x_id(&post.author_id)
            && post.referenced_posts.len() <= 3
            && post.edit_history_post_ids.len() <= 6
            && post.referenced_posts.iter().all(|r| valid_x_id(&r.id))
            && post.edit_history_post_ids.iter().all(|id| valid_x_id(id)),
        "invalid X post identity or references",
    )?;
    let reference = |kind: &str| {
        post.referenced_posts
            .iter()
            .find(|r| r.kind == kind)
            .map(|r| r.id.clone())
    };
    let reply_to_id = reference("replied_to");
    let quoted_id = reference("quoted");
    let reposted_id = reference("retweeted").or_else(|| reference("reposted"));
    require(
        post.referenced_posts.iter().all(|r| {
            matches!(
                r.kind.as_str(),
                "replied_to" | "quoted" | "retweeted" | "reposted"
            )
        }),
        "unknown X reference kind",
    )?;
    let action = if reposted_id.is_some() {
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
    let missing_context = post.referenced_posts.iter().any(|reference| {
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
    for reference in &post.referenced_posts {
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
        } else {
            Some(post.author_id.clone())
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
            gaps: if missing_context {
                vec!["referenced_post_context_missing".into()]
            } else {
                vec![]
            },
        },
        now,
    )
}

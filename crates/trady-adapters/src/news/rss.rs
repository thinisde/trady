use super::*;
use quick_xml::{Reader, events::Event};

#[derive(Default)]
struct Entry {
    id: String,
    url: String,
    url_base: Option<Url>,
    title: String,
    description: String,
    published: String,
    publisher: String,
}

pub(super) fn fetch<T: HttpTransport>(
    client: &mut SourceClient<T>,
    source: &SourceConfig,
    window: &CollectionRequest,
    now: DateTime<Utc>,
) -> std::result::Result<SourcePage, SourceFailure> {
    let mut url =
        Url::parse(&source.url).map_err(|_| SourceFailure::unavailable("invalid_feed_url"))?;
    if source.kind == SourceKind::GoogleNewsRss {
        if url.host_str() != Some("news.google.com")
            || !matches!(url.path(), "/rss" | "/rss/search")
        {
            return Err(SourceFailure::unavailable("invalid_google_news_feed"));
        }
        if let Some(query) = &source.query {
            url.set_path("/rss/search");
            let pairs: Vec<_> = url
                .query_pairs()
                .filter(|(k, _)| k != "q")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            url.set_query(None);
            url.query_pairs_mut()
                .extend_pairs(pairs)
                .append_pair("q", query);
        }
    }
    let response = client.get(
        HttpRequest {
            url,
            allowed_hosts: source.allowed_hosts.clone(),
            bearer: None,
            etag: window.etag.clone(),
            last_modified: window.last_modified.clone(),
            redirects: 3,
        },
        now,
    )?;
    if response.status == 304 {
        if window.etag.is_none() && window.last_modified.is_none() {
            return Err(SourceFailure::unavailable(
                "unsolicited_not_modified_response",
            ));
        }
        return Ok(SourcePage {
            items: vec![],
            next_token: None,
            etag: response.etag,
            last_modified: response.last_modified,
            complete: true,
            gaps: vec!["rss_snapshot_not_full_history".into()],
        });
    }
    parse(
        source,
        &response.body,
        window.limit,
        now,
        response.etag,
        response.last_modified,
        response.final_url,
    )
}

pub(super) fn parse(
    source: &SourceConfig,
    body: &[u8],
    limit: usize,
    now: DateTime<Utc>,
    etag: Option<String>,
    last_modified: Option<String>,
    base_url: Option<Url>,
) -> std::result::Result<SourcePage, SourceFailure> {
    let fail = || SourceFailure::unavailable("rss_parser_unhealthy");
    if body.len() > MAX_BODY_BYTES {
        return Err(SourceFailure::unavailable("response_too_large"));
    }
    let text = std::str::from_utf8(body).map_err(|_| fail())?;
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<String> = vec![];
    let document_base = base_url
        .or_else(|| Url::parse(&source.url).ok())
        .ok_or_else(fail)?;
    let mut bases: Vec<Url> = vec![];
    let mut current: Option<Entry> = None;
    let mut drafts = vec![];
    let mut feed_title = String::new();
    let mut saw_root = false;
    let mut root_closed = false;
    let mut capped = false;
    loop {
        match reader.read_event().map_err(|_| fail())? {
            Event::DocType(_) => return Err(SourceFailure::unavailable("xml_doctype_forbidden")),
            Event::Start(tag) => {
                let base = effective_base(&tag, bases.last().unwrap_or(&document_base))?;
                let name =
                    String::from_utf8(tag.local_name().as_ref().to_vec()).map_err(|_| fail())?;
                if stack.is_empty() {
                    if saw_root || !matches!(name.as_str(), "rss" | "feed" | "RDF") {
                        return Err(fail());
                    }
                    saw_root = true;
                }
                if stack.len() >= 32 {
                    return Err(SourceFailure::unavailable("xml_depth_exceeded"));
                }
                if matches!(name.as_str(), "item" | "entry") {
                    if current.is_some() {
                        return Err(fail());
                    }
                    current = Some(Entry {
                        url_base: Some(base.clone()),
                        ..Default::default()
                    });
                }
                if name == "link"
                    && let Some(entry) = &mut current
                    && let Some(href) = alternate_link(&tag)?
                {
                    entry.url_base = Some(base.clone());
                    entry.url = base.join(&href).map_err(|_| fail())?.into();
                }
                if name == "link"
                    && let Some(entry) = &mut current
                {
                    entry.url_base = Some(base.clone());
                }
                stack.push(name);
                bases.push(base);
            }
            Event::Empty(tag) => {
                let base = effective_base(&tag, bases.last().unwrap_or(&document_base))?;
                if tag.local_name().as_ref() == b"link"
                    && let Some(entry) = &mut current
                    && let Some(href) = alternate_link(&tag)?
                {
                    entry.url_base = Some(base.clone());
                    entry.url = base.join(&href).map_err(|_| fail())?.into();
                }
            }
            Event::Text(value) => append(
                &stack,
                &mut current,
                &mut feed_title,
                &value.decode().map_err(|_| fail())?,
            )?,
            Event::CData(value) => append(
                &stack,
                &mut current,
                &mut feed_title,
                &value.decode().map_err(|_| fail())?,
            )?,
            Event::GeneralRef(value) => {
                let entity = std::str::from_utf8(value.as_ref()).map_err(|_| fail())?;
                let raw = format!("&{entity};");
                let value = quick_xml::escape::unescape(&raw).map_err(|_| fail())?;
                append(&stack, &mut current, &mut feed_title, &value)?;
            }
            Event::End(tag) => {
                let name = std::str::from_utf8(tag.local_name().as_ref())
                    .map_err(|_| fail())?
                    .to_string();
                if stack.pop().as_deref() != Some(name.as_str()) {
                    return Err(fail());
                }
                bases.pop();
                if matches!(name.as_str(), "item" | "entry") {
                    let entry = current.take().ok_or_else(fail)?;
                    if drafts.len() == limit {
                        capped = true;
                        continue;
                    }
                    if entry.title.trim().is_empty() || entry.url.trim().is_empty() {
                        return Err(fail());
                    }
                    let published_at = if entry.published.trim().is_empty() {
                        None
                    } else {
                        Some(
                            DateTime::parse_from_rfc2822(entry.published.trim())
                                .or_else(|_| DateTime::parse_from_rfc3339(entry.published.trim()))
                                .map_err(|_| fail())?
                                .to_utc(),
                        )
                    };
                    let google = source.kind == SourceKind::GoogleNewsRss;
                    let mut gaps = vec!["primary_source_not_verified".into()];
                    if google {
                        gaps.push("google_redirect_not_resolved_metadata_only".into());
                    }
                    if published_at.is_none() {
                        gaps.push("publication_time_missing".into());
                    }
                    let url = entry
                        .url_base
                        .as_ref()
                        .unwrap_or(&document_base)
                        .join(entry.url.trim())
                        .map_err(|_| fail())?;
                    let url = canonical_url(url.as_str()).map_err(|_| fail())?;
                    drafts.push(ItemDraft {
                        external_id: if entry.id.trim().is_empty() {
                            url.clone()
                        } else {
                            entry.id.trim().into()
                        },
                        url,
                        publisher: entry.publisher.trim().into(),
                        feed_title: None,
                        title: Some(plain(&entry.title)),
                        summary: if entry.description.trim().is_empty() {
                            None
                        } else {
                            Some(plain(&entry.description))
                        },
                        text: None,
                        published_at,
                        language: None,
                        status: if google {
                            ContentStatus::MetadataOnly
                        } else {
                            ContentStatus::Snippet
                        },
                        social: None,
                        related_urls: vec![],
                        gaps,
                    });
                }
                if stack.is_empty() {
                    root_closed = true;
                }
            }
            Event::Eof => break,
            _ => (),
        }
    }
    if !saw_root
        || !root_closed
        || !stack.is_empty()
        || current.is_some()
        || feed_title.trim().is_empty()
    {
        return Err(fail());
    }
    if drafts.is_empty() && !capped {
        return Err(SourceFailure::unavailable("rss_empty_feed_requires_review"));
    }
    let items = drafts
        .into_iter()
        .map(|mut draft| {
            draft.feed_title = Some(feed_title.trim().into());
            if draft.publisher.is_empty() {
                draft.publisher = feed_title.trim().into();
            }
            item(source, draft, now).map_err(|_| fail())
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut gaps = vec!["rss_snapshot_not_full_history".into()];
    if capped {
        gaps.push("feed_item_budget_exhausted_no_history_cursor".into());
    }
    Ok(SourcePage {
        items,
        next_token: None,
        etag,
        last_modified,
        complete: !capped,
        gaps,
    })
}

fn effective_base(
    tag: &quick_xml::events::BytesStart<'_>,
    parent: &Url,
) -> std::result::Result<Url, SourceFailure> {
    for attribute in tag.attributes() {
        let attribute =
            attribute.map_err(|_| SourceFailure::unavailable("rss_attribute_invalid"))?;
        if attribute.key.as_ref() == b"xml:base" {
            let value = attribute
                .unescape_value()
                .map_err(|_| SourceFailure::unavailable("rss_attribute_invalid"))?;
            return parent
                .join(&value)
                .map_err(|_| SourceFailure::unavailable("rss_base_invalid"));
        }
    }
    Ok(parent.clone())
}

fn alternate_link(
    tag: &quick_xml::events::BytesStart<'_>,
) -> std::result::Result<Option<String>, SourceFailure> {
    let mut href = None;
    let mut alternate = true;
    for attribute in tag.attributes() {
        let attribute =
            attribute.map_err(|_| SourceFailure::unavailable("rss_attribute_invalid"))?;
        if attribute.key.as_ref() == b"href" {
            href = Some(
                attribute
                    .unescape_value()
                    .map_err(|_| SourceFailure::unavailable("rss_attribute_invalid"))?
                    .into_owned(),
            );
        }
        if attribute.key.as_ref() == b"rel" {
            alternate = attribute.value.as_ref() == b"alternate";
        }
    }
    Ok(if alternate { href } else { None })
}

fn append(
    stack: &[String],
    current: &mut Option<Entry>,
    feed_title: &mut String,
    value: &str,
) -> std::result::Result<(), SourceFailure> {
    let Some(name) = stack.last().map(String::as_str) else {
        if value.trim().is_empty() {
            return Ok(());
        }
        return Err(SourceFailure::unavailable("rss_text_outside_root"));
    };
    let parent = stack.get(stack.len().saturating_sub(2)).map(String::as_str);
    let enclosing_title = stack.iter().rposition(|s| s == "title");
    let title_parent = enclosing_title
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| stack.get(i))
        .map(String::as_str);
    let destination = if let Some(entry) = current {
        if stack.iter().any(|s| {
            matches!(
                s.as_str(),
                "description" | "summary" | "content" | "encoded"
            )
        }) {
            Some(&mut entry.description)
        } else if enclosing_title.is_some() {
            if title_parent == Some("source") {
                Some(&mut entry.publisher)
            } else {
                Some(&mut entry.title)
            }
        } else {
            match name {
                "title" if parent == Some("source") => Some(&mut entry.publisher),
                "title" => Some(&mut entry.title),
                "link" => Some(&mut entry.url),
                "guid" | "id" => Some(&mut entry.id),
                "pubDate" | "published" | "date" => Some(&mut entry.published),
                "description" | "summary" | "content" | "encoded" => Some(&mut entry.description),
                "source" | "creator" | "name" => Some(&mut entry.publisher),
                _ => None,
            }
        }
    } else if enclosing_title.is_some() && matches!(title_parent, Some("feed" | "channel")) {
        Some(feed_title)
    } else {
        None
    };
    if let Some(target) = destination {
        if target.len() + value.len() > 32768 {
            return Err(SourceFailure::unavailable("rss_field_too_large"));
        }
        target.push_str(value);
    }
    Ok(())
}

fn plain(input: &str) -> String {
    let mut output = String::new();
    let mut inside = false;
    for c in input.chars() {
        match c {
            '<' => {
                inside = true;
                output.push(' ');
            }
            '>' => inside = false,
            _ if !inside => output.push(c),
            _ => (),
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use std::{fs::File, io::Read};
use trady_adapters::news::{MAX_BODY_BYTES, MeteredHttp, SafeHttp, SourceClient, parse_feed};
use trady_application::research::*;
use trady_application::x_monitoring::measure_x_counts;
use trady_domain::{Error, Result, require, research::*};
use trady_storage::research::PgResearchRepository;
use trady_storage::x_monitoring::{PgXCostRepository, XReconciliation};

pub const HELP: &str = "\n  trady news status [--config FILE]\n  trady news collect --once [--source ID] [--config FILE] [--force]\n  trady news read [--source ID] [--limit 1..100] [--for-model] [--config FILE]\n  trady news x URL --source ID [--config FILE]\n  trady news resolve-x HANDLE --source ID [--config FILE]\n  trady news counts-x --source ID --config FILE\n  trady news budget-x --config FILE\n  trady news reconcile-x FILE --config FILE\n  trady news export-x [--source ID] [--as-of UTC] [--limit 1..100] [--config FILE]\n  trady news parse-feed FILE --source ID [--config FILE]\n\nResearch database commands use TRADY_ANALYST_DSN. X/API credentials are environment references in config/research.json.\n";

#[derive(Default)]
struct Options {
    config: Option<String>,
    source: Option<String>,
    limit: Option<usize>,
    once: bool,
    force: bool,
    for_model: bool,
    positional: Option<String>,
    as_of: Option<DateTime<Utc>>,
}

fn options(args: &[String]) -> Result<Options> {
    let mut result = Options::default();
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        if flag.starts_with("--") {
            require(seen.insert(flag), "duplicate news option")?;
        }
        match flag {
            "--config" | "--source" | "--limit" | "--as-of" => {
                i += 1;
                let value = args
                    .get(i)
                    .filter(|s| !s.starts_with("--"))
                    .ok_or_else(|| Error("missing news option value".into()))?;
                match flag {
                    "--config" => result.config = Some(value.clone()),
                    "--source" => result.source = Some(value.clone()),
                    "--as-of" => {
                        result.as_of = Some(
                            DateTime::parse_from_rfc3339(value)
                                .map_err(|_| Error("invalid export cutoff".into()))?
                                .to_utc(),
                        )
                    }
                    _ => {
                        result.limit = Some(
                            value
                                .parse()
                                .map_err(|_| Error("invalid news limit".into()))?,
                        )
                    }
                }
            }
            "--once" => result.once = true,
            "--force" => result.force = true,
            "--for-model" => result.for_model = true,
            _ if !flag.starts_with('-') && result.positional.is_none() => {
                result.positional = Some(flag.into())
            }
            _ => return Err(Error("unsupported news option".into())),
        }
        i += 1;
    }
    require(
        result.limit.is_none_or(|n| (1..=100).contains(&n)),
        "news limit must be 1 through 100",
    )?;
    Ok(result)
}

fn read_bounded(path: &str, limit: usize) -> Result<Vec<u8>> {
    let mut body = vec![];
    File::open(path)
        .map_err(|_| Error("cannot open research input".into()))?
        .take((limit + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| Error("cannot read research input".into()))?;
    require(body.len() <= limit, "research input exceeds size bound")?;
    Ok(body)
}

fn config(options: &Options) -> Result<ResearchConfig> {
    let body = if let Some(path) = &options.config {
        read_bounded(path, 262144)?
    } else {
        include_bytes!("../../../config/research.json").to_vec()
    };
    let config: ResearchConfig = serde_json::from_slice(&body)
        .map_err(|_| Error("invalid research configuration".into()))?;
    config.validate()?;
    if let Some(id) = &options.source {
        require(
            config.sources.iter().any(|s| &s.id == id),
            "unknown research source ID",
        )?;
    }
    Ok(config)
}

fn repository() -> Result<PgResearchRepository> {
    PgResearchRepository::connect(&super::secret("TRADY_ANALYST_DSN")?)
}

fn client(
    config: &ResearchConfig,
) -> Result<SourceClient<MeteredHttp<SafeHttp, PgXCostRepository>>> {
    let ledger = PgXCostRepository::connect(&super::secret("TRADY_ANALYST_DSN")?)?;
    Ok(SourceClient::with_transport(
        MeteredHttp::new(SafeHttp, ledger, config.x_budget.clone()),
        |name| trady_adapters::credentials::optional(name).map(|s| s.map(|s| s.into_inner())),
    ))
}

fn selected<'a>(config: &'a ResearchConfig, options: &Options) -> Result<&'a SourceConfig> {
    config
        .sources
        .iter()
        .find(|s| Some(&s.id) == options.source.as_ref())
        .ok_or_else(|| Error("news operation requires --source ID".into()))
}

fn sync_registry(config: &ResearchConfig, repository: &mut impl ResearchRepository) -> Result<()> {
    for source in &config.sources {
        let old = repository.state(&source.id)?;
        let next = prepare_state(source, old.clone())?;
        if old.registry_hash != next.registry_hash {
            repository.commit_page(&source.id, old.revision, &next, &[], Utc::now())?;
        }
    }
    Ok(())
}

pub fn run(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error(format!("missing news command\n{HELP}")))?;
    let options = options(&args[1..])?;
    require(
        !options.force || command == "collect",
        "--force applies only to news collect",
    )?;
    require(
        !options.once || command == "collect",
        "--once applies only to news collect",
    )?;
    require(
        !options.for_model || command == "read",
        "--for-model applies only to news read",
    )?;
    require(
        options.limit.is_none() || matches!(command, "read" | "export-x"),
        "--limit applies only to news read or export-x",
    )?;
    require(
        options.positional.is_none()
            || matches!(command, "x" | "resolve-x" | "parse-feed" | "reconcile-x"),
        "unexpected news argument",
    )?;
    require(
        options.as_of.is_none() || command == "export-x",
        "--as-of applies only to export-x",
    )?;
    let config = config(&options)?;
    let now = Utc::now();
    match command {
        "status" => {
            let sources: Vec<_> = config.sources.iter().filter(|s| options.source.as_ref().is_none_or(|id| id == &s.id)).map(|source| json!({"source_id":source.id,"kind":source.kind,
                "account":source.x_account,
                "rule":source.x_rule,
                "enabled":source.enabled,"activation_gap":source.activation_gap(now),"interval_seconds":source.interval_seconds,
                "daily_request_limit":source.daily_request_limit,"rights":source.rights,
                "delivery":match source.kind { SourceKind::XTimeline => "TIMELINE_POLLING", SourceKind::XRecentSearch => "INCREMENTAL_RECENT_SEARCH", _ => "ON_DEMAND" },
                "capability_gaps":if matches!(source.kind,SourceKind::XTimeline) {
                    vec!["no_stream_or_webhooks","no_delete_or_unrepost_delivery","provider_limited_history","no_subsecond_latency_guarantee"]
                } else if source.kind == SourceKind::XRecentSearch {
                    vec!["query_selected_coverage_not_full_timeline","no_stream_or_webhooks","complete_counts_and_budget_required","context_not_routinely_fetched"]
                } else { vec!["primary_verification_required"] }})).collect();
            Ok(
                json!({"sources":sources,"x_budget":config.x_budget,"entry_authorized":false,"provider_access_verified":false,
                "coverage_gaps":["Truth_Social_not_configured","live_entitlements_and_content_rights_require_operator_review"]}),
            )
        }
        "collect" => {
            require(options.once, "news collection requires --once")?;
            let mut repository = repository()?;
            let mut client = client(&config)?;
            let mut reports = vec![];
            for source in config
                .sources
                .iter()
                .filter(|s| options.source.as_ref().is_none_or(|id| id == &s.id))
            {
                if source.kind == SourceKind::XLookup {
                    continue;
                }
                reports.push(collect_source(
                    &config,
                    source,
                    &mut client,
                    &mut repository,
                    now,
                    options.force,
                )?);
            }
            require(
                !reports.is_empty(),
                "selected source has no scheduled collector",
            )?;
            let degraded = reports
                .iter()
                .any(|r| r.status != CollectionStatus::Complete || !r.gaps.is_empty());
            Ok(json!({"reports":reports,"research_degraded":degraded,"entry_authorized":false}))
        }
        "read" => {
            let mut repository = repository()?;
            sync_registry(&config, &mut repository)?;
            let limit = options.limit.unwrap_or(20);
            let mut items =
                repository.read_items(options.source.as_deref(), 100, now, options.for_model)?;
            items.retain(|i| config.sources.iter().any(|s| s.id == i.source_id));
            items.truncate(limit);
            Ok(
                json!({"items":items,"untrusted_source_data":true,"evidence_verified":false,"entry_authorized":false}),
            )
        }
        "budget-x" | "reconcile-x" => {
            let policy = config
                .x_budget
                .as_ref()
                .ok_or_else(|| Error("X billing policy required".into()))?;
            let mut ledger = PgXCostRepository::connect(&super::secret("TRADY_ANALYST_DSN")?)?;
            if command == "reconcile-x" {
                let path = options
                    .positional
                    .as_deref()
                    .ok_or_else(|| Error("reconcile-x requires a provider evidence file".into()))?;
                let evidence: XReconciliation = serde_json::from_slice(&read_bounded(path, 8192)?)
                    .map_err(|_| Error("invalid X reconciliation evidence".into()))?;
                ledger.reconcile(policy, &evidence)?;
            }
            ledger.status(policy)
        }
        "export-x" => {
            let mut repository = repository()?;
            sync_registry(&config, &mut repository)?;
            let as_of = options.as_of.unwrap_or(now);
            let observations = repository.export_x_evidence(
                options.source.as_deref(),
                as_of,
                now,
                options.limit.unwrap_or(100),
            )?;
            Ok(
                json!({"observations":observations,"as_of":as_of,"exported_at":now,"x_network_requests":0,
                "training_permitted":false,"untrusted_source_data":true,"entry_authorized":false,
                "limitations":["unverified_issuer_links","feature_ready_time_not_established","no_semantic_event_clustering","current_permitted_versions_only"]}),
            )
        }
        "counts-x" => {
            let source = selected(&config, &options)?;
            require(
                source.kind == SourceKind::XRecentSearch,
                "counts-x requires X_RECENT_SEARCH",
            )?;
            if let Some(gap) = source.activation_gap(now) {
                return Ok(json!({"status":"DISABLED","gaps":[gap],"entry_authorized":false}));
            }
            let mut repository = repository()?;
            let report =
                measure_x_counts(&config, source, &mut client(&config)?, &mut repository, now)?;
            Ok(
                json!({"report":report,"identity_context_and_recovery_costs_additional":true,
                "activation_authorized":false,"entry_authorized":false}),
            )
        }
        "parse-feed" => {
            let source = selected(&config, &options)?;
            let path = options
                .positional
                .as_deref()
                .ok_or_else(|| Error("parse-feed requires a file".into()))?;
            let page = parse_feed(
                source,
                &read_bounded(path, MAX_BODY_BYTES)?,
                config.page_size,
                now,
            )
            .map_err(|e| Error(e.reason))?;
            Ok(
                json!({"items":page.items,"complete":page.complete,"gaps":page.gaps,"untrusted_source_data":true,
                "entry_authorized":false,"stored":false}),
            )
        }
        "x" | "resolve-x" => {
            let source = selected(&config, &options)?;
            let input = options
                .positional
                .as_deref()
                .ok_or_else(|| Error("X operation requires a URL or handle".into()))?;
            require(source.enabled, "X source is disabled in the registry")?;
            require(
                matches!(
                    source.kind,
                    SourceKind::XLookup | SourceKind::XTimeline | SourceKind::XRecentSearch
                ),
                "selected source is not X",
            )?;
            require(
                command != "x" || source.kind == SourceKind::XLookup,
                "post lookup requires an X_LOOKUP source",
            )?;
            if command == "x" {
                trady_adapters::news::x_post_id(input)?;
            }
            let mut repository = repository()?;
            let old = repository.state(&source.id)?;
            let state = prepare_state(source, old)?;
            if state.retry_at.is_some_and(|at| at > now)
                || state.last_attempt_at.is_some_and(|at| {
                    now - at < Duration::seconds(i64::from(source.interval_seconds))
                })
            {
                return Ok(
                    json!({"status":"DEFERRED","retry_at":state.retry_at,"reason":"source_not_due","entry_authorized":false}),
                );
            }
            let (mut state, allowed) =
                reserve_source_requests(source, &mut repository, state, now, 1)?;
            if !allowed {
                return Ok(
                    json!({"status":"RATE_LIMITED","retry_at":state.retry_at,"gaps":state.gaps,"entry_authorized":false}),
                );
            }
            let mut client = client(&config)?;
            let result = if command == "resolve-x" {
                client.resolve_x(source,input.trim_start_matches('@'),now).map(|user| json!({"user":user,"identity_approved":false,
                    "next_step":"Verify ownership and recent activity, then set the stable user ID and review timestamp in the registry."}))
            } else {
                match client.lookup_x(source, input, now) {
                    Ok(page) => {
                        state.last_status = Some(if page.complete {
                            CollectionStatus::Complete
                        } else {
                            CollectionStatus::Partial
                        });
                        state.gaps = page.gaps;
                        repository.commit_page(
                            &source.id,
                            state.revision,
                            &state,
                            &page.items,
                            now,
                        )?;
                        let mut stored = vec![];
                        for item in &page.items {
                            if let Some(item) = repository.read_item(&item.item_id, now, false)? {
                                stored.push(item);
                            }
                        }
                        return Ok(
                            json!({"status":state.last_status,"items":stored,"gaps":state.gaps,"untrusted_source_data":true,"entry_authorized":false}),
                        );
                    }
                    Err(failure) => Err(failure),
                }
            };
            match result {
                Ok(value) => {
                    state.last_status = Some(CollectionStatus::Complete);
                    state.gaps = vec![];
                    repository.commit_page(&source.id, state.revision, &state, &[], now)?;
                    Ok(value)
                }
                Err(failure) => {
                    if command == "x"
                        && matches!(
                            failure.reason.as_str(),
                            "content_unavailable"
                                | "post_deleted_private_or_inaccessible"
                                | "provider_access_denied"
                        )
                    {
                        repository
                            .mark_x_unavailable(&trady_adapters::news::x_post_id(input)?, now)?;
                    }
                    state.last_status = Some(failure.status);
                    state.gaps = vec![failure.reason];
                    state.retry_at = failure.retry_at.or(Some(
                        now + Duration::seconds(i64::from(source.interval_seconds)),
                    ));
                    repository.commit_page(&source.id, state.revision, &state, &[], now)?;
                    Ok(
                        json!({"status":state.last_status,"gaps":state.gaps,"retry_at":state.retry_at,"items":[],"entry_authorized":false}),
                    )
                }
            }
        }
        _ => Err(Error(format!("unsupported news command\n{HELP}"))),
    }
}

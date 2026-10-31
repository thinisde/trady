//! Reviewed research information tools and bounded collector orchestration.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::time::Instant;
use trady_application::{harness::*, research::*, research_jobs::*};
use trady_domain::{
    Error, Result, require,
    research::{CollectionStatus, SourceKind},
};

use crate::{
    news::{HttpTransport, SourceClient},
    opencode::runtime_tools::RuntimeTools,
};

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
}
pub fn research_tool_specs(sentiment_enabled: bool) -> Vec<ToolSpec> {
    let id = json!({"type":"string","pattern":"^[A-Za-z0-9_-]{1,96}$"});
    let result = object(
        json!({"result":{"type":"object"},"entry_authorized":{"const":false}}),
        &["result", "entry_authorized"],
    );
    let mut rows = vec![
        (
            "news.search",
            "Read current permitted stored observations from an approved source. Empty results do not establish no news.",
            object(
                json!({"source_id":id,"limit":{"type":"integer","minimum":1,"maximum":3}}),
                &["source_id", "limit"],
            ),
        ),
        (
            "news.read_item",
            "Read one persisted item with current rights and expiry checks. No URL accepted.",
            object(
                json!({"item_id":{"type":"string","pattern":"^[a-f0-9]{64}$"}}),
                &["item_id"],
            ),
        ),
        (
            "sources.health",
            "Read scoped registered source health and coverage gaps.",
            object(json!({}), &[]),
        ),
        (
            "sources.lookup",
            "Queue a bounded registered-source lookup by source ID and external post ID. No arbitrary URL or network authority.",
            object(
                json!({"source_id":id,"external_id":{"type":"string","pattern":"^[0-9]{1,19}$"}}),
                &["source_id", "external_id"],
            ),
        ),
        (
            "profiles.get",
            "Read current verified packet/profile revision references for an approved instrument. Missing profile facts stay unavailable.",
            object(json!({"entity_id":id}), &["entity_id"]),
        ),
        (
            "evidence.compare",
            "Read persisted claim support, contradictions and concrete syndication clusters. This comparison creates no independent evidence.",
            object(json!({"packet_id":id}), &["packet_id"]),
        ),
        (
            "research.request",
            "Queue scoped collection/research; never adds instruments to the trading universe.",
            object(
                json!({"instrument_id":{"type":["string","null"],"pattern":"^[A-Za-z0-9_-]{1,96}$"}}),
                &["instrument_id"],
            ),
        ),
        (
            "research.get_artifact",
            "Read a current immutable verifier packet by its persisted ID.",
            object(json!({"artifact_id":id}), &["artifact_id"]),
        ),
        (
            "research.job_status",
            "Read a queued job/result in this owner/account/policy scope.",
            object(json!({"job_id":id}), &["job_id"]),
        ),
    ];
    for name in [
        "financial.snapshot",
        "fundamentals.snapshot",
        "filings.search",
        "macro.snapshot",
        "prices.features",
    ] {
        rows.push((name,"Read bounded stored financial context under current source rights and account scope. Historical data never authorize orders or executable quotes.",
            object(json!({"source_id":{"type":"string","pattern":"^[A-Za-z0-9_-]{1,96}$"},"limit":{"type":"integer","minimum":1,"maximum":3}}),&["source_id","limit"])));
    }
    if sentiment_enabled {
        rows.extend([
        ("sentiment.assess","Queue actual self-hosted FinBERT scoring of a verified persisted packet under the approved model revision.",object(json!({"packet_id":id,"model_revision":{"type":"string","pattern":"^[a-fA-F0-9]{40}$"}}),&["packet_id","model_revision"])),
        ("sentiment.read","Read actual persisted FinBERT outcomes, readiness blockers and dependency references.",object(json!({"assessment_id":id}),&["assessment_id"])),
    ]);
    }
    rows.into_iter()
        .map(|(name, description, arguments_schema)| ToolSpec {
            name: name.into(),
            version: "1.0.0".into(),
            description: description.into(),
            arguments_schema,
            result_schema: result.clone(),
        })
        .collect()
}

pub struct ResearchTools<R> {
    pub base: RuntimeTools,
    pub repository: R,
    pub sentiment_enabled: bool,
}
impl<R: ResearchInformation> ToolExecutor for ResearchTools<R> {
    fn specs(&self) -> Vec<ToolSpec> {
        let mut specs = self.base.specs();
        specs.extend(research_tool_specs(self.sentiment_enabled));
        specs
    }
    fn revision(&self) -> String {
        content_hash(&format!(
            "research-tools-v1:{}:{}",
            self.base.revision(),
            serde_json::to_string(&research_tool_specs(self.sentiment_enabled)).unwrap_or_default()
        ))
    }
    fn execute(
        &mut self,
        name: &str,
        args: &Value,
        session: &Session,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<ToolOutput> {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Ok(ToolOutput::error(
                "CANCELLED",
                "tool_deadline_or_cancellation",
            ));
        }
        if self.base.specs().iter().any(|s| s.name == name) {
            return self.base.execute(name, args, session, deadline, cancel);
        }
        let Some(spec) = research_tool_specs(self.sentiment_enabled)
            .into_iter()
            .find(|s| s.name == name)
        else {
            return Ok(ToolOutput::error("DENIED", "tool_not_registered"));
        };
        if !valid_tool_input(&spec.arguments_schema, args) {
            return Ok(ToolOutput::error(
                "DENIED",
                "invalid_research_tool_arguments",
            ));
        }
        let request_key = format!(
            "tool_{}",
            digest(&(
                session
                    .runs
                    .last()
                    .map(|r| r.id.as_str())
                    .unwrap_or(&session.id),
                name,
                args
            ))?
        );
        let data =
            match self
                .repository
                .information(&session.scope, name, args, &request_key, Utc::now())
            {
                Ok(data) => data,
                Err(_) => {
                    return Ok(ToolOutput::error(
                        "UNAVAILABLE",
                        "scoped_research_dependency_unavailable",
                    ));
                }
            };
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Ok(ToolOutput::error(
                "CANCELLED",
                "tool_deadline_or_cancellation",
            ));
        }
        let mut result = ToolOutput::ok(json!({"result":data,"entry_authorized":false}));
        if matches!(
            name,
            "financial.snapshot"
                | "fundamentals.snapshot"
                | "filings.search"
                | "macro.snapshot"
                | "prices.features"
        ) && let Some(status) = result
            .data
            .as_ref()
            .and_then(|v| v["result"]["status"].as_str())
        {
            result.status = status.to_owned();
        }
        Ok(result)
    }
}

/// Collects only configured/approved sources. Existing transport billing gates,
/// source watermarks, bounded pages and request reservations remain authoritative.
pub fn collect_registered(
    config: &ResearchConfig,
    policy: &JobPolicy,
    client: &mut impl NewsSource,
    repository: &mut impl ResearchRepository,
    now: DateTime<Utc>,
) -> Result<Value> {
    config.validate()?;
    policy.validate()?;
    require(
        digest(config)? == policy.registry_hash,
        "research registry changed since job approval",
    )?;
    let mut reports = vec![];
    let mut bounded_config = config.clone();
    bounded_config.max_pages = bounded_config.max_pages.min(2);
    for source in config
        .sources
        .iter()
        .filter(|s| policy.source_ids.contains(&s.id) && s.kind != SourceKind::XLookup)
    {
        reports.push(
            serde_json::to_value(collect_source(
                &bounded_config,
                source,
                client,
                repository,
                now,
                false,
            )?)
            .map_err(|_| Error("collection result encoding failed".into()))?,
        );
    }
    Ok(
        json!({"collection_reports":reports,"verification_status":"OBSERVATIONS_ONLY","entry_authorized":false,
        "limitations":["collected content requires trusted issuer and claim verification","unavailable sources are not evidence of no news"]}),
    )
}

pub fn lookup_registered<T: HttpTransport>(
    config: &ResearchConfig,
    policy: &JobPolicy,
    source_id: &str,
    external_id: &str,
    client: &mut SourceClient<T>,
    repository: &mut impl ResearchRepository,
    now: DateTime<Utc>,
) -> Result<Value> {
    config.validate()?;
    policy.validate()?;
    require(
        digest(config)? == policy.registry_hash
            && policy.source_ids.contains(source_id)
            && valid_x_id(external_id),
        "lookup outside approved source scope",
    )?;
    let source = config
        .sources
        .iter()
        .find(|s| s.id == source_id && s.kind == SourceKind::XLookup)
        .ok_or_else(|| Error("registered X lookup source unavailable".into()))?;
    let old = repository.state(source_id)?;
    let state = prepare_state(source, old)?;
    require(
        source.activation_gap(now).is_none() && state.retry_at.is_none_or(|at| at <= now),
        "source disabled, unverified or cooling down",
    )?;
    let (mut state, allowed) = reserve_source_requests(source, repository, state, now, 1)?;
    require(allowed, "lookup source request budget exhausted")?;
    // The LLM provides an ID. Rust constructs the sole supported canonical URL.
    let url = format!("https://x.com/i/status/{external_id}");
    match client.lookup_x(source, &url, now) {
        Ok(page) => {
            state.last_status = Some(if page.complete {
                CollectionStatus::Complete
            } else {
                CollectionStatus::Partial
            });
            state.gaps = page.gaps;
            let ids: Vec<_> = page.items.iter().map(|i| i.item_id.clone()).collect();
            repository.commit_page(source_id, state.revision, &state, &page.items, now)?;
            Ok(
                json!({"status":state.last_status,"item_ids":ids,"gaps":state.gaps,"verification_status":"UNVERIFIED_OBSERVATIONS","entry_authorized":false}),
            )
        }
        Err(failure) => {
            state.last_status = Some(failure.status);
            state.gaps = vec![failure.reason];
            state.retry_at = failure.retry_at.or(Some(
                now + chrono::Duration::seconds(i64::from(source.interval_seconds)),
            ));
            repository.commit_page(source_id, state.revision, &state, &[], now)?;
            Err(Error(
                "registered source lookup unavailable; failure persisted".into(),
            ))
        }
    }
}

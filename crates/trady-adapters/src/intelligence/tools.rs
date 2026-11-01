//! Analyst tools expose permitted summaries. Tax stays in the local owner CLI.

use chrono::Utc;
use serde_json::{Value, json};
use std::time::Instant;
use trady_application::{harness::*, intelligence::*};
use trady_domain::{Error, Mode, Result, intelligence::*, require};

pub struct IntelligenceTools<B, R> {
    pub base: B,
    pub repository: Option<R>,
    pub config: Option<IntelligenceConfig>,
    pub mode: Mode,
}

pub fn tool_specs(config: &IntelligenceConfig) -> Vec<ToolSpec> {
    FeatureKind::ALL.into_iter().filter(|f| *f != FeatureKind::Tax && config.enabled.contains(f))
        .map(|f| ToolSpec {
            name: f.tool().into(), version: FEATURE_VERSION.into(),
            description: "Read a current rights-checked research assessment for an approved entity. Missing data is unavailable. Uncalibrated context never establishes expected return, independent news confirmation or trading authority.".into(),
            arguments_schema: json!({"type":"object","additionalProperties":false,
                "properties":{"entity_id":{"type":"string","pattern":"^[A-Za-z0-9_-]{1,96}$"}},"required":["entity_id"]}),
            result_schema: json!({"type":"object","additionalProperties":false,"properties":{
                "result":{"type":"object"},"entry_authorized":{"const":false}},"required":["result","entry_authorized"]}),
        }).collect()
}

impl<B: ToolExecutor, R: IntelligenceRepository> ToolExecutor for IntelligenceTools<B, R> {
    fn requires_ephemeral_session(&self) -> bool {
        self.base.requires_ephemeral_session()
            || self
                .config
                .as_ref()
                .is_some_and(|c| !tool_specs(c).is_empty())
    }
    fn validate_context(&mut self, session: &Session) -> Result<()> {
        self.base.validate_context(session)?;
        let (Some(config), Some(repository)) = (&self.config, &mut self.repository) else {
            return Ok(());
        };
        let now = Utc::now();
        let invalidations = repository.invalidations(&config.scope)?;
        for call in session.runs.iter().flat_map(|r| &r.calls) {
            if FeatureKind::from_tool(&call.tool).is_none() {
                continue;
            }
            let Some(value) = call
                .result
                .as_ref()
                .and_then(|r| r.get("data"))
                .and_then(|d| d.get("result"))
            else {
                continue;
            };
            let record: AssessmentRecord = serde_json::from_value(value.clone())
                .map_err(|_| Error("invalid intelligence context record".into()))?;
            if record.result.is_none() {
                continue;
            }
            require(
                record.scope == config.scope
                    && record.registry_hash.as_deref() == Some(&config.revision()?)
                    && record.expires_at.is_some_and(|end| end > now),
                "intelligence context expired or configuration changed",
            )?;
            let id = record
                .observation_id
                .as_deref()
                .ok_or_else(|| Error("intelligence context has no input".into()))?;
            let stored = repository
                .find(&config.scope, id)?
                .ok_or_else(|| Error("intelligence context erased".into()))?;
            require(
                stored.retain_until > now
                    && !invalidations.iter().any(|i| {
                        i.matches(&stored.observation) && (i.erase_content || i.at <= now)
                    }),
                "intelligence context invalidated",
            )?;
            let checked = assess_ephemeral(
                config,
                &stored.observation,
                self.mode,
                Audience::ExternalLlm,
                record.decision_at,
                now,
            )?;
            require(
                checked.result.is_some(),
                "intelligence context rights unavailable",
            )?;
        }
        Ok(())
    }
    fn specs(&self) -> Vec<ToolSpec> {
        let mut specs = self.base.specs();
        if let Some(config) = &self.config {
            specs.extend(tool_specs(config));
        }
        specs
    }
    fn revision(&self) -> String {
        match &self.config {
            None => self.base.revision(),
            Some(config) => content_hash(&format!(
                "{}:{}:{}",
                FEATURE_VERSION,
                self.base.revision(),
                config
                    .revision()
                    .unwrap_or_else(|_| "INVALID_CONFIG".into())
            )),
        }
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
        let Some(feature) = FeatureKind::from_tool(name) else {
            return self.base.execute(name, args, session, deadline, cancel);
        };
        if feature == FeatureKind::Tax {
            return Ok(ToolOutput::error(
                "DENIED",
                "tax_ephemeral_owner_comparison_required",
            ));
        }
        let (Some(config), Some(repository)) = (&self.config, &mut self.repository) else {
            return Ok(ToolOutput::error("UNAVAILABLE", "intelligence_disabled"));
        };
        let Some(spec) = tool_specs(config).into_iter().find(|s| s.name == name) else {
            return Ok(ToolOutput::error("UNAVAILABLE", "feature_disabled"));
        };
        if !trady_application::research_jobs::valid_tool_input(&spec.arguments_schema, args) {
            return Ok(ToolOutput::error(
                "DENIED",
                "invalid_intelligence_arguments",
            ));
        }
        let scope = IntelligenceScope {
            owner_id: session.scope.owner_id.clone(),
            account_id: session.scope.account_id.clone(),
        };
        if scope != config.scope {
            return Ok(ToolOutput::error("DENIED", "intelligence_scope_mismatch"));
        }
        let now = Utc::now();
        let assessment = read(
            repository,
            config,
            ReadRequest {
                scope: &scope,
                feature,
                entity: args["entity_id"].as_str().unwrap_or_default(),
                mode: self.mode,
                audience: Audience::ExternalLlm,
                at: now,
                now,
            },
        )?;
        let result = json!({"result":assessment,"entry_authorized":false});
        if serde_json::to_vec(&result)
            .map_err(|_| Error("cannot encode intelligence tool result".into()))?
            .len()
            > 65_536
        {
            return Ok(ToolOutput::error(
                "UNAVAILABLE",
                "intelligence_result_exceeds_byte_limit",
            ));
        }
        Ok(ToolOutput::ok(result))
    }
}

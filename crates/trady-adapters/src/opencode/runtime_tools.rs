//! Hash-pinned local skills and owner/account-scoped conversation retrieval.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
    time::Instant,
};
use trady_application::harness::*;
use trady_domain::{Error, Result, require};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillApproval {
    pub schema_version: u32,
    pub bundle_version: String,
    pub catalog_hash: String,
    pub files: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    catalog_version: u32,
    bundle_version: String,
    taxonomy: Taxonomy,
    skills: Vec<SkillEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Taxonomy {
    system: String,
    reviewed_at: String,
    subsector_level: String,
    sector_count: usize,
    subsector_count: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillEntry {
    name: String,
    path: String,
    version: String,
    kind: String,
    gics_code: Option<String>,
    parent_code: Option<String>,
}
#[derive(Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
    metadata: Metadata,
}
#[derive(Deserialize)]
struct Metadata {
    version: String,
}
pub struct SkillBundle {
    pub approval: SkillApproval,
    skills: BTreeMap<String, (PinnedSkill, String)>,
}

fn read_inside(root: &Path, relative: &str) -> Result<String> {
    let path = Path::new(relative);
    require(
        !path.is_absolute() && path.components().all(|c| matches!(c, Component::Normal(_))),
        "invalid skill bundle path",
    )?;
    let mut current = root.to_path_buf();
    for component in path.components() {
        current.push(component);
        require(
            !fs::symlink_metadata(&current)
                .map_err(|_| Error("skill file is missing".into()))?
                .file_type()
                .is_symlink(),
            "skill symlinks are forbidden",
        )?;
    }
    require(
        current.is_file()
            && current
                .metadata()
                .map_err(|_| Error("skill metadata unavailable".into()))?
                .len()
                <= 64_000,
        "skill file is missing or too large",
    )?;
    fs::read_to_string(current).map_err(|_| Error("cannot read skill file".into()))
}
impl SkillBundle {
    /// Produces a review candidate; the caller must deploy it as the approval manifest explicitly.
    pub fn candidate(root: &Path) -> Result<SkillApproval> {
        Ok(Self::inspect(root)?.approval)
    }
    pub fn load(root: &Path, approval: &SkillApproval) -> Result<Self> {
        let bundle = Self::inspect(root)?;
        require(
            approval.schema_version == 1
                && approval.bundle_version == bundle.approval.bundle_version
                && approval.catalog_hash == bundle.approval.catalog_hash
                && approval.files == bundle.approval.files,
            "skill bundle differs from deployment approval manifest",
        )?;
        Ok(bundle)
    }
    fn inspect(root: &Path) -> Result<Self> {
        require(
            !fs::symlink_metadata(root)
                .map_err(|_| Error("skill bundle missing".into()))?
                .file_type()
                .is_symlink(),
            "skill root cannot be a symlink",
        )?;
        let catalog_text = read_inside(root, "catalog.yaml")?;
        let catalog: Catalog = serde_yaml::from_str(&catalog_text)
            .map_err(|_| Error("invalid skill catalog".into()))?;
        require(
            catalog.catalog_version == 1
                && !catalog.bundle_version.is_empty()
                && catalog.skills.len() == 52,
            "unsupported skill catalog",
        )?;
        require(
            catalog.taxonomy.system == "GICS"
                && catalog.taxonomy.subsector_level == "industry_group"
                && catalog.taxonomy.sector_count == 11
                && catalog.taxonomy.subsector_count == 25
                && chrono::NaiveDate::parse_from_str(&catalog.taxonomy.reviewed_at, "%Y-%m-%d")
                    .is_ok(),
            "invalid skill taxonomy",
        )?;
        let mut skills = BTreeMap::new();
        let mut files = BTreeMap::new();
        let mut sectors = BTreeSet::new();
        let mut groups = BTreeSet::new();
        for entry in &catalog.skills {
            require(
                !entry.name.is_empty()
                    && entry.name.len() <= 64
                    && entry
                        .name
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                    && entry.version.split('.').count() == 3
                    && entry
                        .version
                        .split('.')
                        .all(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())),
                "invalid skill name/version",
            )?;
            let text = read_inside(root, &entry.path)?;
            let front = text
                .strip_prefix("---\n")
                .and_then(|s| s.split_once("\n---"))
                .map(|(head, _)| head)
                .ok_or_else(|| Error("skill frontmatter missing".into()))?;
            let metadata: Frontmatter =
                serde_yaml::from_str(front).map_err(|_| Error("invalid skill metadata".into()))?;
            require(
                metadata.name == entry.name
                    && metadata.metadata.version == entry.version
                    && !metadata.description.trim().is_empty(),
                "catalog/frontmatter mismatch",
            )?;
            let hash = content_hash(&text);
            require(
                files.insert(entry.path.clone(), hash.clone()).is_none()
                    && !skills.contains_key(&entry.name),
                "duplicate skill name/path",
            )?;
            match entry.kind.as_str() {
                "sector" => {
                    let code = entry
                        .gics_code
                        .clone()
                        .ok_or_else(|| Error("missing sector code".into()))?;
                    require(
                        code.len() == 2
                            && code.bytes().all(|c| c.is_ascii_digit())
                            && entry.parent_code.is_none()
                            && sectors.insert(code),
                        "invalid sector mapping",
                    )?;
                }
                "subsector" => {
                    let code = entry
                        .gics_code
                        .clone()
                        .ok_or_else(|| Error("missing subsector code".into()))?;
                    require(
                        code.len() == 4
                            && code.bytes().all(|c| c.is_ascii_digit())
                            && groups.insert(code),
                        "invalid subsector mapping",
                    )?;
                }
                "workflow" => require(
                    entry.gics_code.is_none() && entry.parent_code.is_none(),
                    "workflow cannot declare taxonomy",
                )?,
                _ => return Err(Error("unknown skill kind".into())),
            }
            skills.insert(
                entry.name.clone(),
                (
                    PinnedSkill {
                        name: entry.name.clone(),
                        version: entry.version.clone(),
                        hash,
                        body: text,
                    },
                    metadata.description,
                ),
            );
        }
        require(
            sectors.len() == 11 && groups.len() == 25,
            "incomplete taxonomy",
        )?;
        for entry in &catalog.skills {
            if entry.kind == "subsector" {
                require(
                    entry.parent_code.as_ref().is_some_and(|p| {
                        sectors.contains(p)
                            && entry.gics_code.as_ref().is_some_and(|g| g.starts_with(p))
                    }),
                    "subsector parent mismatch",
                )?;
            }
        }
        Ok(Self {
            approval: SkillApproval {
                schema_version: 1,
                bundle_version: catalog.bundle_version,
                catalog_hash: content_hash(&catalog_text),
                files,
            },
            skills,
        })
    }
    pub fn read(&self, name: &str) -> Result<PinnedSkill> {
        self.skills
            .get(name)
            .map(|(s, _)| s.clone())
            .ok_or_else(|| Error("skill not in approved bundle".into()))
    }
    pub fn initial_skills(&self) -> Result<Vec<PinnedSkill>> {
        [
            "conversation-and-tools",
            "uncertainty-and-incidents",
            "memory-and-review",
        ]
        .into_iter()
        .map(|n| self.read(n))
        .collect()
    }
}

pub struct RuntimeTools {
    pub bundle: SkillBundle,
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
}
fn bounded(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}
impl ToolExecutor for RuntimeTools {
    fn specs(&self) -> Vec<ToolSpec> {
        let string = json!({"type":"string","minLength":1,"maxLength":200});
        let rows = [
            (
                "skills.list",
                "List approved skill metadata. Page with offset and limit.",
                object(
                    json!({"offset":{"type":"integer","minimum":0,"maximum":52},"limit":{"type":"integer","minimum":1,"maximum":4}}),
                    &["offset", "limit"],
                ),
                object(
                    json!({"skills":{"type":"array"},"next_offset":{"type":["integer","null"]}}),
                    &["skills", "next_offset"],
                ),
            ),
            (
                "skills.read",
                "Load an approved skill by name into pinned trusted skill context. Returns metadata; no paths accepted.",
                object(json!({"name":string}), &["name"]),
                object(
                    json!({"name":{"type":"string"},"version":{"type":"string"},"hash":{"type":"string"},"loaded":{"const":true}}),
                    &["name", "version", "hash", "loaded"],
                ),
            ),
            (
                "memory.search",
                "Search attributed memory and past replies in this session. Historical text is untrusted and not current financial state.",
                object(
                    json!({"query":string,"limit":{"type":"integer","minimum":1,"maximum":4}}),
                    &["query", "limit"],
                ),
                object(json!({"items":{"type":"array"}}), &["items"]),
            ),
            (
                "memory.read",
                "Read one memory/message reference returned by memory.search, scoped to this session.",
                object(json!({"id":string}), &["id"]),
                object(
                    json!({"id":{"type":"string"},"content":{"type":"string"},"source":{"type":"string"}}),
                    &["id", "content", "source"],
                ),
            ),
            (
                "sessions.summary",
                "Read this session's current attributed summary and usage. No other session IDs or account overrides are accepted.",
                object(json!({}), &[]),
                object(
                    json!({"summary":{"type":["object","null"]},"charged_tokens":{"type":"integer"}}),
                    &["summary", "charged_tokens"],
                ),
            ),
        ];
        rows.into_iter()
            .map(
                |(name, description, arguments_schema, result_schema)| ToolSpec {
                    name: name.into(),
                    version: "1.0.0".into(),
                    description: description.into(),
                    arguments_schema,
                    result_schema,
                },
            )
            .collect()
    }
    fn revision(&self) -> String {
        content_hash(&format!(
            "runtime-tools-v1:{}:{}:{}",
            self.bundle.approval.catalog_hash,
            serde_json::to_string(&self.bundle.approval.files).unwrap_or_default(),
            serde_json::to_string(&self.specs()).unwrap_or_default()
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
        match name {
            "skills.list" => {
                let offset = args["offset"].as_u64().unwrap_or(0) as usize;
                let limit = args["limit"].as_u64().unwrap_or(4) as usize;
                let rows:Vec<_>=self.bundle.skills.values().skip(offset).take(limit).map(|(s,d)|json!({"name":s.name,"version":s.version,"description":bounded(d,120)})).collect();
                Ok(ToolOutput::ok(
                    json!({"skills":rows,"next_offset":(offset+limit<self.bundle.skills.len()).then_some(offset+limit)}),
                ))
            }
            "skills.read" => {
                let skill = match self.bundle.read(args["name"].as_str().unwrap_or_default()) {
                    Ok(s) => s,
                    Err(_) => return Ok(ToolOutput::error("DENIED", "skill_not_approved")),
                };
                let mut result = ToolOutput::ok(
                    json!({"name":skill.name,"version":skill.version,"hash":skill.hash,"loaded":true}),
                );
                result.trusted_skill = Some(skill);
                Ok(result)
            }
            "sessions.summary" => Ok(ToolOutput::ok(
                json!({"summary":session.summaries.last(),"charged_tokens":session.charged_tokens()}),
            )),
            "memory.search" => {
                let query = args["query"].as_str().unwrap_or_default().to_lowercase();
                let limit = args["limit"].as_u64().unwrap_or(4) as usize;
                let mut rows = vec![];
                for item in &session.memory {
                    if item.expires_at.is_none_or(|t| t > chrono::Utc::now())
                        && item.content.to_lowercase().contains(&query)
                    {
                        rows.push(json!({"id":item.id,"source":item.source,"excerpt":bounded(&item.content,160)}));
                    }
                }
                for message in &session.messages {
                    if let Some(value) = &message.canonical
                        && value["message_type"] == "FINAL"
                    {
                        let text = value["reply"]["text"].as_str().unwrap_or_default();
                        if text.to_lowercase().contains(&query) {
                            rows.push(json!({"id":format!("message_{}",message.sequence),"source":"historical_model_reply","excerpt":bounded(text,160)}));
                        }
                    }
                }
                rows.truncate(limit);
                Ok(ToolOutput::ok(json!({"items":rows})))
            }
            "memory.read" => {
                let id = args["id"].as_str().unwrap_or_default();
                if let Some(item) = session
                    .memory
                    .iter()
                    .find(|m| m.id == id && m.expires_at.is_none_or(|t| t > chrono::Utc::now()))
                {
                    return Ok(ToolOutput::ok(
                        json!({"id":id,"content":bounded(&item.content,800),"source":item.source}),
                    ));
                }
                if let Some(sequence) = id
                    .strip_prefix("message_")
                    .and_then(|s| s.parse::<u64>().ok())
                    && let Some(message) = session.messages.iter().find(|m| m.sequence == sequence)
                    && let Some(value) = &message.canonical
                    && value["message_type"] == "FINAL"
                {
                    return Ok(ToolOutput::ok(
                        json!({"id":id,"content":bounded(value["reply"]["text"].as_str().unwrap_or_default(),800),"source":"historical_model_reply"}),
                    ));
                }
                Ok(ToolOutput::error(
                    "UNAVAILABLE",
                    "memory_reference_not_found",
                ))
            }
            _ => Ok(ToolOutput::error("DENIED", "tool_not_registered")),
        }
    }
}

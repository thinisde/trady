//! Authenticated market/research reads from immutable, separately reviewed imports.
//! There is no broker mutation, provider purchase, model approval or LLM trust here.

mod calendar;
mod http;
mod store;
mod validate;

pub use calendar::{CalendarDocument, CalendarSession, ClosedDate};
pub use http::{HttpServer, HttpServiceResponse};
pub use store::ImportStore;

use crate::autonomous::{AnalystPacket, CostsPacket, PredictionRequest, SessionPacket};
use crate::screening::UniverseDocument;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf};
use trady_application::{
    autonomous::EvidenceRecord,
    model_lab::{ApprovedModel, ForecastObservation, read_document},
    optimization::{FormulaAuthority, digest, valid_hash, valid_id},
};
use trady_domain::{
    AccountId, Currency, Error, InstrumentId, Quote, Result, require, research::SourceRights,
};
use trady_policy::Policy;

pub const MAX_IMPORTS: usize = 20_000;
pub const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportKind {
    Evidence,
    Calendar,
    Session,
    Quote,
    Costs,
    Universe,
    Research,
    Forecast,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePolicy {
    pub provider: String,
    pub allowed_hosts: Vec<String>,
    /// Reviewed common ownership/original reporting group. Different URLs do not prove independence.
    pub independence_group: String,
    pub may_be_primary: bool,
    pub rights: SourceRights,
    pub rights_valid_until: DateTime<Utc>,
    pub kinds: Vec<ImportKind>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewerPolicy {
    pub independence_group: String,
    pub may_verify_primary: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceAccount {
    pub owner_id: String,
    pub account_id: AccountId,
    pub currency: Currency,
    pub calendar_id: String,
    pub policy: Policy,
    pub formula_authority: FormulaAuthority,
    pub approved_skills: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDeployment {
    /// Operator-owned fixed path, never taken from a forecast or request.
    pub path: PathBuf,
    pub bundle_sha256: String,
    pub reviewer_id: String,
    pub evidence_import_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputsServiceConfig {
    pub config_version: u32,
    pub store_directory: PathBuf,
    /// Bind to loopback; deploy behind an approved TLS reverse proxy.
    pub listen: SocketAddr,
    pub workers: u8,
    pub request_timeout_ms: u32,
    pub maximum_body_bytes: usize,
    pub maximum_response_bytes: usize,
    pub sources: BTreeMap<String, SourcePolicy>,
    pub reviewers: BTreeMap<String, ReviewerPolicy>,
    pub accounts: Vec<ServiceAccount>,
    pub model: Option<ModelDeployment>,
}
impl InputsServiceConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1
                && self.listen.ip().is_loopback()
                && (1..=16).contains(&self.workers)
                && (100..=10_000).contains(&self.request_timeout_ms)
                && (1024..=262_144).contains(&self.maximum_body_bytes)
                && (1024..=MAX_IMPORT_BYTES).contains(&self.maximum_response_bytes)
                && !self.store_directory.as_os_str().is_empty()
                && self.sources.len() <= 128
                && self.reviewers.len() <= 32
                && self.accounts.len() <= 16,
            "invalid bounded input service configuration",
        )?;
        require(
            !self.sources.is_empty(),
            "approved input source registry is empty",
        )?;
        require(
            !self.reviewers.is_empty(),
            "approved input reviewer registry is empty",
        )?;
        for (id, s) in &self.sources {
            require(
                valid_id(id)
                    && !s.provider.is_empty()
                    && s.provider.len() <= 256
                    && valid_id(&s.independence_group)
                    && !s.allowed_hosts.is_empty()
                    && s.allowed_hosts.len() <= 16
                    && s.allowed_hosts.iter().all(|h| {
                        !h.is_empty()
                            && h.len() <= 253
                            && h.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
                    })
                    && s.rights.store_content
                    && s.rights.retention_hours > 0
                    && s.rights.retention_hours <= 24 * 366 * 10
                    && !s.rights.terms_reference.trim().is_empty()
                    && s.rights.terms_reference.len() <= 2048
                    && !s.kinds.is_empty()
                    && s.kinds.len() <= 8,
                "source has missing identity, rights or allowed kinds",
            )?;
        }
        for (id, r) in &self.reviewers {
            require(
                valid_id(id) && valid_id(&r.independence_group),
                "invalid reviewer registry",
            )?;
        }
        let mut accounts = std::collections::BTreeSet::new();
        for a in &self.accounts {
            require(
                valid_id(&a.owner_id)
                    && valid_id(&a.calendar_id)
                    && accounts.insert(&a.account_id)
                    && a.approved_skills.len() <= 64
                    && a.approved_skills
                        .iter()
                        .all(|(n, h)| valid_id(n) && valid_hash(h)),
                "invalid or duplicate service account",
            )?;
            a.policy.validate()?;
            a.formula_authority.validate(&a.policy)?;
            require(
                self.accounts
                    .first()
                    .is_none_or(|first| first.currency == a.currency),
                "costs route requires one account currency per service deployment",
            )?;
        }
        if let Some(m) = &self.model {
            require(
                valid_hash(&m.bundle_sha256)
                    && self.reviewers.contains_key(&m.reviewer_id)
                    && (3..=16).contains(&m.evidence_import_ids.len())
                    && m.evidence_import_ids.iter().all(|id| valid_id(id)),
                "model deployment lacks pinned approval evidence",
            )?;
        }
        Ok(())
    }
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut config: Self = read_document(path)?;
        let directory = path.parent().unwrap_or(std::path::Path::new("."));
        if config.store_directory.is_relative() {
            config.store_directory = directory.join(&config.store_directory);
        }
        if let Some(m) = &mut config.model
            && m.path.is_relative()
        {
            m.path = directory.join(&m.path);
        }
        config.validate()?;
        Ok(config)
    }
    pub fn account(&self, id: &AccountId) -> Result<&ServiceAccount> {
        self.accounts
            .iter()
            .find(|a| &a.account_id == id)
            .ok_or_else(|| Error("input account unavailable".into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceArtifact {
    pub record: EvidenceRecord,
    /// The finalized source/skill/calculation artifact whose hash is in record.revision_hash.
    pub artifact: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastImport {
    pub account_id: AccountId,
    pub observation: ForecastObservation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "document",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum ImportPayload {
    Evidence(EvidenceArtifact),
    Calendar(CalendarDocument),
    Session(SessionPacket),
    Quote(Quote),
    Costs(CostsPacket),
    Universe(UniverseDocument),
    Research(AnalystPacket),
    Forecast(Box<ForecastImport>),
}
impl ImportPayload {
    pub fn kind(&self) -> ImportKind {
        match self {
            Self::Evidence(_) => ImportKind::Evidence,
            Self::Calendar(_) => ImportKind::Calendar,
            Self::Session(_) => ImportKind::Session,
            Self::Quote(_) => ImportKind::Quote,
            Self::Costs(_) => ImportKind::Costs,
            Self::Universe(_) => ImportKind::Universe,
            Self::Research(_) => ImportKind::Research,
            Self::Forecast(_) => ImportKind::Forecast,
        }
    }
    pub fn key(&self) -> String {
        match self {
            Self::Evidence(e) => e.record.id.clone(),
            Self::Calendar(c) => c.calendar_id.clone(),
            Self::Session(s) => s.account_id.to_string(),
            Self::Quote(q) => q.instrument_id.to_string(),
            Self::Costs(c) => c.instrument_id.to_string(),
            Self::Universe(u) => u.index_id.clone(),
            Self::Research(r) => r.account_id.to_string(),
            Self::Forecast(f) => format!(
                "{}:{}:{}",
                f.account_id, f.observation.instrument_id, f.observation.market_snapshot_id
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceImport {
    pub schema_version: u32,
    pub id: String,
    pub source_id: String,
    pub provider: String,
    pub canonical_url: String,
    /// Original reporting/event lineage, also preserved by copies/reposts.
    pub origin_lineage_id: String,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub imported_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub retain_until: DateTime<Utc>,
    pub raw_document: String,
    pub raw_document_sha256: String,
    pub dependencies: Vec<String>,
    pub payload: ImportPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceReview {
    pub schema_version: u32,
    pub import_id: String,
    pub import_sha256: String,
    pub raw_document_sha256: String,
    pub reviewer_id: String,
    pub reviewed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub source_identity_checked: bool,
    pub rights_checked: bool,
    pub original_availability_checked: bool,
    pub mapping_checked: bool,
    pub original_lineage_checked: bool,
    pub model_self_attestation: bool,
    pub review_note: String,
}

pub struct InputsService {
    pub store: ImportStore,
    model: Option<ApprovedModel>,
}
impl InputsService {
    pub fn open(config: InputsServiceConfig) -> Result<Self> {
        config.validate()?;
        let model = if let Some(m) = &config.model {
            let bundle: ApprovedModel = read_document(&m.path)?;
            require(
                digest(&bundle)? == m.bundle_sha256 && bundle.approval.reviewer_id == m.reviewer_id,
                "model bundle differs from operator-owned deployment pin",
            )?;
            // Fitting and importing do not turn test artifacts into approved models.
            bundle.validate(Utc::now())?;
            Some(bundle)
        } else {
            None
        };
        Ok(Self {
            store: ImportStore::open(config)?,
            model,
        })
    }
    pub fn config(&self) -> &InputsServiceConfig {
        self.store.config()
    }
    pub fn quote(&self, instrument: &InstrumentId, now: DateTime<Utc>) -> Result<Quote> {
        match self
            .store
            .latest(ImportKind::Quote, instrument.as_str(), now)?
            .payload
        {
            ImportPayload::Quote(q) => {
                validate::quote(&q, now)?;
                Ok(q)
            }
            _ => Err(Error("quote unavailable".into())),
        }
    }
    pub fn costs(&self, instrument: &InstrumentId, now: DateTime<Utc>) -> Result<CostsPacket> {
        let account = self
            .config()
            .accounts
            .first()
            .ok_or_else(|| Error("cost account currency unavailable".into()))?;
        match self
            .store
            .latest(ImportKind::Costs, instrument.as_str(), now)?
            .payload
        {
            ImportPayload::Costs(c) => {
                validate::costs(&c, &account.currency, now)?;
                Ok(c)
            }
            _ => Err(Error("costs unavailable".into())),
        }
    }
    pub fn calendar(&self, id: &str, now: DateTime<Utc>) -> Result<CalendarDocument> {
        match self.store.latest(ImportKind::Calendar, id, now)?.payload {
            ImportPayload::Calendar(c) => {
                c.validate(now)?;
                Ok(c)
            }
            _ => Err(Error("calendar unavailable".into())),
        }
    }
    pub fn session(&self, account: &AccountId, now: DateTime<Utc>) -> Result<SessionPacket> {
        let binding = self.config().account(account)?;
        let calendar = self.calendar(&binding.calendar_id, now)?;
        let Some(active) = calendar.active_session(now)? else {
            return Ok(SessionPacket {
                account_id: account.clone(),
                session: None,
            });
        };
        let packet = match self
            .store
            .latest(ImportKind::Session, account.as_str(), now)?
            .payload
        {
            ImportPayload::Session(s) => s,
            _ => return Err(Error("session accounting unavailable".into())),
        };
        validate::session(&packet, binding, &active, now)?;
        Ok(packet)
    }
    pub fn research(&self, account: &AccountId, now: DateTime<Utc>) -> Result<AnalystPacket> {
        let import = self
            .store
            .latest(ImportKind::Research, account.as_str(), now)?;
        let ImportPayload::Research(packet) = &import.payload else {
            return Err(Error("research unavailable".into()));
        };
        self.validate_research(&import, packet, now)?;
        Ok(packet.clone())
    }
    pub fn universe(&self, now: DateTime<Utc>) -> Result<UniverseDocument> {
        match self
            .store
            .latest(ImportKind::Universe, "SP500", now)?
            .payload
        {
            ImportPayload::Universe(u) => {
                validate::universe(&u, now)?;
                Ok(u)
            }
            _ => Err(Error("universe unavailable".into())),
        }
    }
    pub fn forecast_observation(
        &self,
        account: &AccountId,
        instrument: &InstrumentId,
        snapshot: &str,
        now: DateTime<Utc>,
    ) -> Result<ForecastObservation> {
        self.config().account(account)?;
        let key = format!("{account}:{instrument}:{snapshot}");
        let import = self.store.latest(ImportKind::Forecast, &key, now)?;
        let ImportPayload::Forecast(f) = &import.payload else {
            return Err(Error("forecast features unavailable".into()));
        };
        self.validate_forecast(&import, f, now)?;
        Ok(f.observation.clone())
    }
    pub fn predictions(
        &self,
        request: &PredictionRequest,
        now: DateTime<Utc>,
    ) -> Result<trady_application::optimization::ScenarioPredictionInput> {
        let account = self.config().account(&request.account_id)?;
        require(
            request.schema_version == 1
                && request.owner_id == account.owner_id
                && request.input.account.account_id == account.account_id
                && request.input.account.spendable_cash.currency == account.currency,
            "prediction account binding invalid",
        )?;
        let session = self
            .session(&request.account_id, now)?
            .session
            .ok_or_else(|| Error("no active session".into()))?;
        let calendar = self.calendar(&account.calendar_id, now)?;
        require(
            calendar.deadline(now, request.calculation.limits.max_holding_sessions)?
                == request.input.holding_deadline_at,
            "prediction deadline differs from verified exchange calendar",
        )?;
        let quote = self.quote(&request.input.quote.instrument_id, now)?;
        require(
            quote == request.input.quote,
            "prediction quote differs from immutable current observation",
        )?;
        let costs = self.costs(&quote.instrument_id, now)?;
        require(
            costs.costs.fixed == request.input.exit_costs.fixed
                && costs.costs.rate == request.input.exit_costs.rate
                && costs.costs.uncertainty == request.input.exit_costs.uncertainty
                && costs.costs.fx == request.input.exit_costs.fx,
            "prediction costs differ from current observation",
        )?;
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| Error("approved four-outcome model unavailable".into()))?;
        self.validate_model_evidence(model, now)?;
        let observation = self.forecast_observation(
            &request.account_id,
            &quote.instrument_id,
            &request.input.market_snapshot_id,
            now,
        )?;
        trady_application::model_lab::infer_scenario(
            model,
            &account.formula_authority,
            &account.policy,
            &request.input,
            &request.calculation,
            &observation,
            &session.opening_equity,
            now,
        )
    }
}

#[cfg(test)]
mod tests;

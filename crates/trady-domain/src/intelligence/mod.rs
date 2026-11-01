//! Optional, uncalibrated research. These records never authorize an order.

pub mod accounting;
pub mod attention;
pub mod coordination;
pub mod cross_asset;
pub mod event;
pub mod insider;
pub mod microstructure;
pub mod options;
pub mod short_interest;
pub mod tax;

use crate::{Currency, Result, require};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const FEATURE_VERSION: &str = "trady-intelligence-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FeatureKind {
    Options,
    Coordination,
    Event,
    ShortInterest,
    Insider,
    CrossAsset,
    Attention,
    Accounting,
    Microstructure,
    Tax,
}
impl FeatureKind {
    pub const ALL: [Self; 10] = [
        Self::Options,
        Self::Coordination,
        Self::Event,
        Self::ShortInterest,
        Self::Insider,
        Self::CrossAsset,
        Self::Attention,
        Self::Accounting,
        Self::Microstructure,
        Self::Tax,
    ];

    pub fn tool(self) -> &'static str {
        match self {
            Self::Options => "options.expectations",
            Self::Coordination => "market.coordination_check",
            Self::Event => "event.surprise_check",
            Self::ShortInterest => "short.crowding",
            Self::Insider => "issuer.insider_activity",
            Self::CrossAsset => "macro.cross_asset_context",
            Self::Attention => "attention.snapshot",
            Self::Accounting => "issuer.earnings_quality",
            Self::Microstructure => "market.microstructure",
            Self::Tax => "account.tax_scenarios",
        }
    }
    pub fn from_tool(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.tool() == name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "feature",
    content = "input",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum FeatureInput {
    Options(options::OptionsInput),
    Coordination(coordination::CoordinationInput),
    Event(Box<event::EventInput>),
    ShortInterest(short_interest::ShortInterestInput),
    Insider(insider::InsiderInput),
    CrossAsset(cross_asset::CrossAssetInput),
    Attention(attention::AttentionInput),
    Accounting(accounting::AccountingInput),
    Microstructure(microstructure::MicrostructureInput),
    Tax(Box<tax::TaxInput>),
}
impl FeatureInput {
    /// External identifiers are resolved by the owner-reviewed registry.
    pub fn subject(&self) -> Option<&str> {
        match self {
            Self::Options(x) => Some(&x.instrument_id),
            Self::Coordination(x) => Some(&x.issuer_id),
            Self::Event(x) => x.issuer_id.as_deref(),
            Self::ShortInterest(x) => Some(&x.issuer_id),
            Self::Insider(x) => Some(&x.issuer_cik),
            Self::CrossAsset(x) => Some(&x.region),
            Self::Attention(x) => Some(&x.issuer_id),
            Self::Accounting(x) => Some(&x.issuer_id),
            Self::Microstructure(x) => Some(&x.instrument_id),
            Self::Tax(x) => Some(x.instrument_id.as_str()),
        }
    }
    pub fn kind(&self) -> FeatureKind {
        match self {
            Self::Options(_) => FeatureKind::Options,
            Self::Coordination(_) => FeatureKind::Coordination,
            Self::Event(_) => FeatureKind::Event,
            Self::ShortInterest(_) => FeatureKind::ShortInterest,
            Self::Insider(_) => FeatureKind::Insider,
            Self::CrossAsset(_) => FeatureKind::CrossAsset,
            Self::Attention(_) => FeatureKind::Attention,
            Self::Accounting(_) => FeatureKind::Accounting,
            Self::Microstructure(_) => FeatureKind::Microstructure,
            Self::Tax(_) => FeatureKind::Tax,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "feature",
    content = "assessment",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum FeatureAssessment {
    Options(options::OptionsAssessment),
    Coordination(coordination::CoordinationAssessment),
    Event(event::EventAssessment),
    ShortInterest(short_interest::ShortInterestAssessment),
    Insider(insider::InsiderAssessment),
    CrossAsset(cross_asset::CrossAssetAssessment),
    Attention(attention::AttentionAssessment),
    Accounting(accounting::AccountingAssessment),
    Microstructure(microstructure::MicrostructureAssessment),
    Tax(Box<tax::TaxAssessment>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntelligenceScope {
    pub owner_id: String,
    pub account_id: String,
}
pub fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
pub fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl IntelligenceScope {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_key(&self.owner_id) && valid_key(&self.account_id),
            "invalid intelligence owner/account scope",
        )
    }
}

/// The registry, rather than this untrusted input, grants use of evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidence {
    pub source_id: String,
    pub provider: String,
    pub rights_revision: String,
    pub entity_id: String,
    pub currency: Option<Currency>,
    pub unit: String,
    pub venue: Option<String>,
    pub adjustment_basis: String,
    pub observed_at: DateTime<Utc>,
    pub published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub source_reference: String,
    pub content_hash: String,
    pub lineage_ids: Vec<String>,
    pub complete: bool,
    pub gaps: Vec<String>,
}
impl SourceEvidence {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_key(&self.source_id)
                && valid_key(&self.entity_id)
                && !self.provider.is_empty()
                && self.provider.len() <= 96
                && !self.rights_revision.is_empty()
                && self.rights_revision.len() <= 128
                && !self.unit.is_empty()
                && self.unit.len() <= 96
                && !self.adjustment_basis.is_empty()
                && self.adjustment_basis.len() <= 128,
            "missing source identity, rights, units or adjustment basis",
        )?;
        require(
            self.observed_at <= self.published_at
                && self.published_at <= self.available_at
                && self.available_at <= self.received_at
                && self.received_at < self.expires_at,
            "invalid observation/publication/availability/receipt chronology",
        )?;
        require(
            !self.source_reference.is_empty()
                && self.source_reference.len() <= 2048
                && valid_hash(&self.content_hash)
                && !self.lineage_ids.is_empty()
                && self.lineage_ids.len() <= 256
                && self.lineage_ids.iter().all(|x| valid_key(x))
                && self.gaps.len() <= 32
                && self.gaps.iter().all(|s| s.len() <= 256),
            "invalid source reference, hash, lineage or coverage",
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureObservation {
    pub schema_version: u32,
    pub observation_id: String,
    pub scope: IntelligenceScope,
    pub entity_id: String,
    pub payload_hash: String,
    pub evidence: Vec<SourceEvidence>,
    pub supersedes: Option<String>,
    pub payload: FeatureInput,
}
impl FeatureObservation {
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        require(
            self.schema_version == SCHEMA_VERSION
                && valid_key(&self.observation_id)
                && valid_key(&self.entity_id)
                && valid_hash(&self.payload_hash)
                && !self.evidence.is_empty()
                && self.evidence.len() <= 64
                && self
                    .supersedes
                    .as_deref()
                    .is_none_or(|id| valid_key(id) && id != self.observation_id),
            "invalid feature observation envelope",
        )?;
        for e in &self.evidence {
            e.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssessmentStatus {
    Available,
    Partial,
    Unavailable,
    Abstained,
    Invalidated,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Audience {
    Owner,
    ExternalLlm,
    LocalInference,
    ModelTraining,
    Export,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentRecord {
    pub schema_version: u32,
    pub feature_version: String,
    pub assessment_id: String,
    pub feature: FeatureKind,
    pub scope: IntelligenceScope,
    pub entity_id: String,
    pub decision_at: DateTime<Utc>,
    pub computed_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub input_hash: Option<String>,
    pub registry_hash: Option<String>,
    pub observation_id: Option<String>,
    pub status: AssessmentStatus,
    pub reasons: Vec<String>,
    pub evidence: Vec<SourceEvidence>,
    pub result: Option<FeatureAssessment>,
    /// Always RESEARCH_ONLY_UNCALIBRATED. No conversion to return forecasts exists.
    pub calibration: String,
    pub entry_authorized: bool,
}

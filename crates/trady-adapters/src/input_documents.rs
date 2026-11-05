//! Immutable wire documents shared by the research input service and offline model tools.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use trady_application::{
    autonomous::{Dossier, MarketSession},
    optimization::BoundCalculation,
};
use trady_domain::{
    AccountId, Currency, InstrumentId, perpetual::FeatureValue, screening::ScreeningObservations,
};
use trady_policy::{
    ConfidenceInputs, EntryCosts, EntryRange, InstrumentRules,
    limits::{FormulaEvidence, LimitsInput},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchDraft {
    pub proposal_id: String,
    pub instrument_id: InstrumentId,
    pub currency: Currency,
    pub valid_until: DateTime<Utc>,
    /// Verified exchange-calendar deadline, never an inferred count of weekdays.
    pub deadline_at: DateTime<Utc>,
    pub entry_range: EntryRange,
    pub confidence_inputs: ConfidenceInputs,
    pub formula_evidence: FormulaEvidence,
    pub entry_costs: EntryCosts,
    pub exit_costs: EntryCosts,
    pub instrument_rules: InstrumentRules,
    pub dossier: Dossier,
    pub feature_schema_version: String,
    pub features: Vec<FeatureValue>,
    pub perpetual_review_reason: String,
    #[serde(default)]
    pub sentiment_reference: Option<trady_application::research_jobs::SentimentReadinessRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalystPacket {
    pub schema_version: u32,
    pub owner_id: String,
    pub account_id: AccountId,
    pub policy_version: u32,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub retain_until: DateTime<Utc>,
    /// Account/instrument support must contain reviewed evidence, beyond metadata presence.
    pub screening_observations: ScreeningObservations,
    pub proposals: Vec<ResearchDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPacket {
    pub account_id: AccountId,
    pub session: Option<MarketSession>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostsPacket {
    pub instrument_id: InstrumentId,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub costs: EntryCosts,
}

/// Frozen numerical scenario sent only to the deployment-approved analysis service.
/// Its response must retain the exact calculation hash, market and horizon bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictionRequest {
    pub schema_version: u32,
    pub account_id: AccountId,
    pub owner_id: String,
    pub input: LimitsInput,
    pub calculation: BoundCalculation,
}

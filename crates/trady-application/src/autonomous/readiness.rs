//! Runtime evidence resolution. A model score or syntactically valid proposal is insufficient.

use super::TradingConfig;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::*;
use trady_policy::{ConfidenceInputs, EntryCosts, EntryRange, InstrumentRules, div, fresh, sub};

pub trait TradingResearchStore:
    crate::perpetual::PerpetualRepository + crate::screening::ScreeningRepository
{
    fn store_calculation(
        &mut self,
        input: &trady_policy::limits::LimitsInput,
        calculation: &crate::optimization::BoundCalculation,
        retain_until: DateTime<Utc>,
    ) -> Result<()>;
    fn store_dossier(&mut self, dossier: &Dossier, retain_until: DateTime<Utc>) -> Result<()>;
    fn store_context(
        &mut self,
        context: &trady_domain::perpetual::AssessmentContext,
        retain_until: DateTime<Utc>,
    ) -> Result<()>;
}

/// Records come from the configured, trusted analyst store, never from model-declared facts.
/// Every record is scoped, immutable, current and bound to its dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRecord {
    pub id: String,
    pub account_id: AccountId,
    pub instrument_id: InstrumentId,
    pub policy_version: u32,
    pub kind: String,
    pub revision_hash: String,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub dependencies: Vec<String>,
    pub source_lineage: Option<String>,
    pub primary_verified: bool,
    pub material_conflict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillEvidence {
    pub name: String,
    pub content_hash: String,
    pub artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dossier {
    pub id: String,
    pub finalized: bool,
    pub sector_skill: String,
    pub subsector_skill: String,
    pub records: Vec<EvidenceRecord>,
    pub skills: Vec<SkillEvidence>,
    pub blockers: Vec<String>,
}

/// Created by native inference and automatic screening adapters in the analyst boundary.
/// Neither receipt contributes confidence points or chooses trading limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvisorReceipt {
    pub assessment_id: String,
    pub model_revision_id: String,
    pub scenario_hash: String,
    pub account_snapshot_id: String,
    pub assessed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub assessed_bid: Decimal,
    pub assessed_ask: Decimal,
    pub quote_drift_tolerance: Decimal,
    pub usable: bool,
    pub calibration_ok: bool,
    pub drift_ok: bool,
    pub review_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreeningReceipt {
    pub universe_snapshot_id: String,
    pub screening_id: String,
    pub finalist: InstrumentId,
    pub observed_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryCandidate {
    pub proposal_id: String,
    pub account_id: AccountId,
    pub account_snapshot_id: String,
    pub policy_version: u32,
    pub instrument_id: InstrumentId,
    pub currency: Currency,
    pub created_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub entry_range: EntryRange,
    pub stop_price: Decimal,
    pub target_net_fraction: Decimal,
    pub max_holding_sessions: u8,
    pub deadline_at: DateTime<Utc>,
    pub exceptional_verified: bool,
    pub confidence_inputs: ConfidenceInputs,
    pub entry_costs: EntryCosts,
    pub exit_costs: EntryCosts,
    pub instrument_rules: InstrumentRules,
    pub dossier: Dossier,
    pub screening: ScreeningReceipt,
    pub advisor: AdvisorReceipt,
    /// Immutable persisted source/claim/FinBERT dependencies, rechecked atomically
    /// when the executor activates a new entry or a holding-plan revision.
    #[serde(default)]
    pub sentiment_reference: Option<crate::research_jobs::SentimentReadinessRef>,
    /// Resolved and persisted by the application before final scenario assessment.
    #[serde(default)]
    pub formula: Option<crate::optimization::FormulaProposal>,
}

impl EntryCandidate {
    /// Refresh FX-dependent economics before final dispatch. Exact rate equality
    /// is deliberately stricter than a model's allowed FX drift; a changed cost
    /// or conversion requires a new frozen calculation and native assessment.
    pub fn validate_refreshed_exit_costs(
        &self,
        current: &EntryCosts,
        now: DateTime<Utc>,
    ) -> Result<()> {
        require(
            current.fixed == self.exit_costs.fixed
                && current.rate == self.exit_costs.rate
                && current.uncertainty == self.exit_costs.uncertainty,
            "exit fees changed; reassess the final scenario",
        )?;
        match (&self.exit_costs.fx, &current.fx) {
            (Some(expected), Some(actual)) => require(
                actual.from == expected.from
                    && actual.to == expected.to
                    && actual.rate == expected.rate
                    && actual.rate > Decimal::ZERO
                    && fresh(actual.observed_at, now, 60),
                "current FX differs from the assessed scenario or is stale",
            ),
            (None, None) => Ok(()),
            _ => Err(Error(
                "current FX coverage differs from the assessed scenario".into(),
            )),
        }
    }

    pub fn liquidation_costs(&self) -> trady_policy::economics::ExitCosts {
        trady_policy::economics::ExitCosts {
            fixed: self.exit_costs.fixed.clone(),
            rate: self.exit_costs.rate,
            uncertainty: self.exit_costs.uncertainty.clone(),
            fx: self.exit_costs.fx.clone(),
        }
    }
    /// All limit/economics changes create a different binding, including fee and FX changes.
    pub fn scenario_hash(&self) -> Result<String> {
        let scenario = serde_json::json!({"proposal_id":self.proposal_id,"account_id":self.account_id,"account_snapshot_id":self.account_snapshot_id,"policy_version":self.policy_version,"instrument_id":self.instrument_id,"currency":self.currency,"entry_range":self.entry_range,"stop_price":self.stop_price,"target_net_fraction":self.target_net_fraction,"max_holding_sessions":self.max_holding_sessions,"entry_costs":self.entry_costs,"exit_costs":self.exit_costs,"instrument_rules":self.instrument_rules,"dossier_id":self.dossier.id,"screening_id":self.screening.screening_id,"formula":self.formula});
        let scenario = serde_json::json!({"terms":scenario,"deadline_at":self.deadline_at});
        let text = serde_json::to_string(&scenario)
            .map_err(|_| Error("scenario encoding failed".into()))?;
        Ok(crate::harness::content_hash(&text))
    }

    pub fn validate(
        &self,
        config: &TradingConfig,
        snapshot: &AccountSnapshot,
        quote: &Quote,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.authoritative_limits(config, snapshot, quote, now)?;
        require(
            !self.proposal_id.is_empty()
                && self.account_id == config.account_id
                && self.account_snapshot_id == snapshot.snapshot_id
                && self.policy_version == config.policy.version
                && snapshot.account_id == config.account_id,
            "proposal scope/snapshot mismatch",
        )?;
        require(
            self.created_at <= now
                && self.valid_until > now
                && self.valid_until - self.created_at <= Duration::minutes(5),
            "expired or invalid proposal lifetime",
        )?;
        require(
            quote.instrument_id == self.instrument_id
                && quote.currency == self.currency
                && self.entry_range.currency == self.currency,
            "proposal quote/instrument mismatch",
        )?;
        require(
            (1..=config.policy.max_holding_sessions).contains(&self.max_holding_sessions)
                && self.stop_price > Decimal::ZERO
                && self.deadline_at > now
                && self.deadline_at - self.created_at <= Duration::days(21),
            "invalid holding horizon or stop",
        )?;
        require(
            self.instrument_rules.price_tick > Decimal::ZERO
                && self.stop_price % self.instrument_rules.price_tick == Decimal::ZERO,
            "stop is not aligned to instrument tick",
        )?;
        self.validate_dossier(config, now)?;
        require(
            !self.screening.universe_snapshot_id.is_empty()
                && !self.screening.screening_id.is_empty()
                && self.screening.finalist == self.instrument_id
                && fresh(self.screening.observed_at, now, 300)
                && self.screening.valid_until > now,
            "automatic screening missing, stale or for another finalist",
        )?;
        let advisor = &self.advisor;
        require(
            advisor.usable
                && advisor.calibration_ok
                && advisor.drift_ok
                && !advisor.assessment_id.is_empty()
                && !advisor.model_revision_id.is_empty()
                && !advisor.review_reason.trim().is_empty()
                && advisor.account_snapshot_id == snapshot.snapshot_id
                && advisor.scenario_hash == self.scenario_hash()?
                && fresh(advisor.assessed_at, now, 300)
                && advisor.valid_until > now,
            "usable scenario-bound Perpetual review required",
        )?;
        require(
            advisor.assessed_bid > Decimal::ZERO
                && advisor.assessed_ask >= advisor.assessed_bid
                && advisor.quote_drift_tolerance >= Decimal::ZERO
                && advisor.quote_drift_tolerance <= Decimal::new(1, 3),
            "invalid model quote tolerance",
        )?;
        for (actual, assessed) in [
            (quote.bid, advisor.assessed_bid),
            (quote.ask, advisor.assessed_ask),
        ] {
            require(
                div(sub(actual, assessed)?.abs(), assessed)? <= advisor.quote_drift_tolerance,
                "quote changed beyond evaluated Perpetual tolerance; reassess",
            )?;
        }
        let spread = div(sub(quote.ask, quote.bid)?, quote.ask)?;
        require(
            self.confidence_inputs
                .spread_fraction
                .is_some_and(|s| s >= spread),
            "confidence spread understates observed spread",
        )?;
        require(
            trady_policy::confidence(&self.confidence_inputs).total()
                >= config.policy.confidence_entry_min,
            "recomputed entry confidence below floor",
        )
    }

    pub fn authoritative_limits<'a>(
        &'a self,
        config: &TradingConfig,
        snapshot: &AccountSnapshot,
        quote: &Quote,
        now: DateTime<Utc>,
    ) -> Result<&'a trady_policy::limits::CalculatedLimits> {
        let authority = config.limits_authority.as_ref().ok_or_else(|| {
            Error("approved weekly formula authority is required for new entries".into())
        })?;
        let proposal = self.formula.as_ref().ok_or_else(|| {
            Error("final FORMULA_V1 calculation is required for new entries".into())
        })?;
        let input = &proposal.input;
        if let Some(reference) = &self.sentiment_reference {
            let base = trady_policy::confidence(&self.confidence_inputs).total();
            let adjusted = base.saturating_sub(
                if reference.maximum_negative_probability >= Decimal::new(6, 1) {
                    10
                } else {
                    0
                },
            );
            require(
                reference.maximum_negative_probability < Decimal::new(8, 1)
                    && input.evidence.confidence_score <= adjusted,
                "formula entry bypasses persisted adverse sentiment",
            )?;
        }
        require(
            input.account == *snapshot
                && input.quote.instrument_id == self.instrument_id
                && input.quote.currency == self.currency
                && input.entry_range == self.entry_range
                && input.entry_costs == self.entry_costs
                && input.rules == self.instrument_rules
                && input.valid_until <= self.valid_until
                && input.holding_deadline_at == self.deadline_at
                && input.exit_costs == self.liquidation_costs()
                && input.evidence.base_confidence_score
                    == trady_policy::confidence(&self.confidence_inputs).total()
                && input.evidence.base_confidence_score
                    == trady_policy::confidence(&self.confidence_inputs).total()
                && input.evidence.confidence_score <= input.evidence.base_confidence_score
                && input.evidence.exceptional_verified == self.exceptional_verified,
            "formula inputs differ from the final proposal, account, costs or confidence",
        )?;
        let limits = proposal.validate(authority, &config.policy, quote, now)?;
        require(
            self.stop_price == limits.stop_trigger
                && self.target_net_fraction == limits.target_net_fraction
                && self.max_holding_sessions == limits.max_holding_sessions
                && self.deadline_at == limits.holding_deadline_at,
            "proposal hints cannot override approved formula stop, target or horizon",
        )?;
        Ok(limits)
    }

    pub(super) fn validate_dossier(
        &self,
        config: &TradingConfig,
        now: DateTime<Utc>,
    ) -> Result<()> {
        require(
            config.mode != Mode::Live || self.sentiment_reference.is_some(),
            "live research requires a persisted sentiment readiness reference",
        )?;
        if let Some(reference) = &self.sentiment_reference {
            require(
                reference.scope.owner_id == config.owner_id
                    && reference.scope.account_id == config.account_id.as_str()
                    && reference.scope.policy_version == config.policy.version
                    && reference.instrument_id == self.instrument_id.as_str()
                    && reference.expires_at > now
                    && fresh(reference.scored_at, now, 900)
                    && reference.expires_at <= reference.scored_at + Duration::minutes(15)
                    && reference.maximum_negative_probability >= Decimal::ZERO
                    && reference.maximum_negative_probability <= Decimal::ONE
                    && [&reference.assessment_id, &reference.packet_id]
                        .iter()
                        .all(|v| trady_domain::perpetual::valid_reference(v))
                    && [
                        &reference.assessment_hash,
                        &reference.packet_hash,
                        &reference.dependency_hash,
                    ]
                    .iter()
                    .all(|v| crate::optimization::valid_hash(v))
                    && reference.model_revision.len() == 40
                    && reference
                        .model_revision
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit()),
                "sentiment readiness reference is stale or differently scoped",
            )?;
            if let Some(predictions) = self.formula.as_ref().and_then(|f| f.predictions.as_ref()) {
                require(
                    predictions
                        .predictions
                        .iter()
                        .filter(|p| {
                            p.metadata.source == trady_domain::prediction::PredictionSource::FinBert
                        })
                        .all(|p| p.metadata.model_version == reference.model_revision),
                    "sentiment forecast differs from persisted classifier revision",
                )?;
            }
        }
        let dossier = &self.dossier;
        require(
            !dossier.id.is_empty()
                && dossier.finalized
                && dossier.blockers.is_empty()
                && dossier.records.len() <= 128,
            "research is incomplete or blocked",
        )?;
        let mut records = BTreeMap::new();
        for record in &dossier.records {
            require(
                !record.id.is_empty() && records.insert(record.id.as_str(), record).is_none(),
                "duplicate or empty evidence identity",
            )?;
            require(
                record.account_id == config.account_id
                    && record.instrument_id == self.instrument_id
                    && record.policy_version == config.policy.version
                    && record.observed_at <= now
                    && record.valid_until > now
                    && record.revision_hash.len() == 64
                    && record.revision_hash.bytes().all(|b| b.is_ascii_hexdigit())
                    && !record.material_conflict,
                "stale, conflicting or incompatible evidence",
            )?;
        }
        for record in &dossier.records {
            let mut deps = BTreeSet::new();
            require(
                record.dependencies.iter().all(|id| {
                    id != &record.id && deps.insert(id) && records.contains_key(id.as_str())
                }),
                "unresolved or duplicate evidence dependency",
            )?;
        }
        let mut resolved = BTreeSet::new();
        for _ in 0..records.len() {
            let before = resolved.len();
            for record in &dossier.records {
                if record
                    .dependencies
                    .iter()
                    .all(|id| resolved.contains(id.as_str()))
                {
                    resolved.insert(record.id.as_str());
                }
            }
            if resolved.len() == before {
                break;
            }
        }
        require(
            resolved.len() == records.len(),
            "cyclic evidence dependencies",
        )?;
        // Required final artifacts must actually exist; model references cannot manufacture them.
        for kind in [
            "universe",
            "screening",
            "company_profile",
            "relationships",
            "watchdog",
            "economics",
            "confidence",
        ] {
            require(
                dossier.records.iter().any(|r| r.kind == kind),
                "required finalized research artifact missing",
            )?;
        }
        require(
            records
                .get(self.screening.universe_snapshot_id.as_str())
                .is_some_and(|r| r.kind == "universe")
                && records
                    .get(self.screening.screening_id.as_str())
                    .is_some_and(|r| r.kind == "screening"),
            "focus does not resolve automatic universe/screening artifacts",
        )?;
        let primary = dossier
            .records
            .iter()
            .any(|r| r.kind == "source" && r.primary_verified);
        let primary_lineages: BTreeSet<_> = dossier
            .records
            .iter()
            .filter(|r| r.kind == "source" && r.primary_verified)
            .filter_map(|r| r.source_lineage.as_ref())
            .collect();
        let lineages: BTreeSet<_> = dossier
            .records
            .iter()
            .filter(|r| r.kind == "source" && !r.primary_verified)
            .filter_map(|r| r.source_lineage.as_ref())
            .filter(|s| !s.is_empty() && !primary_lineages.contains(s))
            .collect();
        require(
            primary
                && self.confidence_inputs.primary_verified
                && usize::from(self.confidence_inputs.independent_lineages) <= lineages.len()
                && !lineages.is_empty(),
            "verified primary facts and independent source comparison required",
        )?;
        require(
            !self.exceptional_verified
                || dossier
                    .records
                    .iter()
                    .any(|r| r.kind == "exceptional_target"),
            "exceptional target justification is missing",
        )?;
        let required = [
            "conversation-and-tools",
            "search-planning",
            "market-research",
            "news-analysis",
            "source-comparison",
            "entry-readiness",
            "sector-selection",
            dossier.sector_skill.as_str(),
            dossier.subsector_skill.as_str(),
            "company-profile",
            "relationship-analysis",
            "holding-watchdog",
            "perpetual-review",
        ];
        require(
            dossier.sector_skill.starts_with("sector-")
                && dossier.subsector_skill.starts_with("subsector-")
                && dossier.skills.len() == required.len(),
            "incorrect mandatory skill routing",
        )?;
        let mut skills = BTreeSet::new();
        for finding in &dossier.skills {
            require(
                skills.insert(finding.name.as_str())
                    && required.contains(&finding.name.as_str())
                    && config.approved_skills.get(&finding.name) == Some(&finding.content_hash)
                    && !finding.artifact_ids.is_empty()
                    && finding
                        .artifact_ids
                        .iter()
                        .all(|id| records.contains_key(id.as_str())),
                "skill is unapproved, duplicated or has unresolved artifacts",
            )?;
        }
        Ok(())
    }

    /// Conservative proceeds in account currency after liquidation fees and uncertainty.
    pub fn net_proceeds(
        &self,
        quantity: Decimal,
        price: Decimal,
        now: DateTime<Utc>,
    ) -> Result<Decimal> {
        let basis = trady_policy::economics::CostBasis {
            instrument_id: self.instrument_id.clone(),
            quantity,
            instrument_currency: self.currency.clone(),
            acquisition_cost: Money {
                amount: Decimal::ONE,
                currency: self.entry_costs.fixed.currency.clone(),
            },
        };
        Ok(trady_policy::economics::liquidation_proceeds(
            &basis,
            price,
            &self.liquidation_costs(),
            now,
        )?
        .amount)
    }
}

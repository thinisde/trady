//! Research-only analyst boundary. The model can rank verified survivors but
//! cannot create eligibility, prices, quantities, or broker mutations.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use trady_domain::selection::{
    FallbackAttempt, ModelFingerprint, RankedCandidate, SelectionProposal, SelectionRun,
};
use trady_domain::{Currency, InstrumentId, Result, require};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalystRequest {
    pub run: SelectionRun,
    pub eligible_instrument_ids: Vec<InstrumentId>,
    pub candidate_evidence_ids: Vec<String>,
    pub fallback_attempts: Vec<FallbackAttempt>,
    pub as_of: DateTime<Utc>,
}

impl AnalystRequest {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.as_of <= now
                && self.eligible_instrument_ids.len() <= 256
                && self
                    .eligible_instrument_ids
                    .iter()
                    .all(|id| id.as_str().ends_with("_EQ"))
                && self.candidate_evidence_ids.len() <= 256
                && self.candidate_evidence_ids.iter().all(|id| valid_id(id)),
            "invalid analyst request",
        )?;
        self.run.model.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalystRank {
    pub instrument_id: InstrumentId,
    pub score: Decimal,
    pub sentiment: Option<Decimal>,
    pub confidence: Option<Decimal>,
    pub evidence_ids: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalystResponse {
    pub run_id: String,
    pub session_id: String,
    pub model: ModelFingerprint,
    pub ranks: Vec<AnalystRank>,
    pub selected_instrument_id: InstrumentId,
    pub proposal: SelectionProposal,
    pub uncertainties: Vec<String>,
    pub contradictions: Vec<String>,
    pub completed_at: DateTime<Utc>,
}

impl AnalystResponse {
    /// Bind cryptographic fields that the application, rather than the model,
    /// can calculate. The model still has to return the exact run/session and
    /// pinned provider identity; hashes are replaced with the locally derived
    /// values before the response is persisted.
    pub fn bind_application_fingerprint(&mut self, expected: &ModelFingerprint) -> Result<()> {
        require(
            self.model.run_id == expected.run_id
                && self.model.session_id == expected.session_id
                && self.model.provider == expected.provider
                && self.model.model == expected.model,
            "analyst response identity differs from the pinned application run",
        )?;
        self.model = expected.clone();
        Ok(())
    }

    pub fn validate(&self, request: &AnalystRequest, now: DateTime<Utc>) -> Result<()> {
        self.model.validate()?;
        require(
            self.run_id == request.run.run_id
                && self.session_id == request.run.session_id
                && self.completed_at <= now
                && self.ranks.len() <= 256
                && self.ranks.iter().all(|rank| {
                    rank.instrument_id.as_str().ends_with("_EQ")
                        && request
                            .eligible_instrument_ids
                            .contains(&rank.instrument_id)
                        && rank.score >= Decimal::ZERO
                        && rank.score <= Decimal::ONE
                        && rank
                            .evidence_ids
                            .iter()
                            .all(|id| request.candidate_evidence_ids.contains(id))
                        && !rank.rationale.trim().is_empty()
                })
                && self
                    .ranks
                    .iter()
                    .map(|rank| &rank.instrument_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.ranks.len()
                && self
                    .ranks
                    .iter()
                    .any(|rank| rank.instrument_id == self.selected_instrument_id)
                && self.uncertainties.len() <= 32
                && self.contradictions.len() <= 32,
            "analyst response is untrusted, out of scope or incomplete",
        )?;
        require(
            !self.selected_instrument_id.as_str().is_empty(),
            "analyst selected instrument is empty",
        )?;
        Ok(())
    }

    pub fn into_selection_run(
        self,
        mut request: SelectionRun,
        fallback_attempts: Vec<FallbackAttempt>,
    ) -> Result<SelectionRun> {
        let currency = Currency::new("EUR")?;
        let mut ranks: Vec<RankedCandidate> = self
            .ranks
            .into_iter()
            .map(|rank| RankedCandidate {
                instrument_id: rank.instrument_id,
                currency: currency.clone(),
                eligibility_snapshot_id: request.universe_snapshot_id.clone(),
                eligible: true,
                ranking_score: rank.score,
                sentiment_score: rank.sentiment,
                confidence: rank.confidence,
                evidence_ids: rank.evidence_ids,
                exclusion_reasons: vec![],
                catalyst: Some(rank.rationale),
                contradictions: self.contradictions.clone(),
                strategy_snapshot_ids: vec![],
            })
            .collect();
        // Keep deterministic exclusions in the immutable run. The model only
        // receives survivors, but the audit record must still explain every
        // approved-universe rejection.
        ranks.extend(
            request
                .candidates
                .iter()
                .filter(|candidate| !candidate.eligible)
                .cloned(),
        );
        let ranks = trady_domain::selection::rank_candidates(ranks)?;
        request.model = self.model;
        request.candidates = ranks;
        request.fallback_attempts = fallback_attempts;
        request.successful(
            self.selected_instrument_id,
            self.proposal,
            self.completed_at,
        )
    }
}

pub trait EurResearchAnalyst {
    fn select(&mut self, request: &AnalystRequest) -> Result<AnalystResponse>;
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use trady_domain::selection::ModelFingerprint;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }
    fn fingerprint() -> ModelFingerprint {
        ModelFingerprint {
            run_id: "selection_1".into(),
            session_id: "session_1".into(),
            provider: "opencode-go".into(),
            model: "deepseek-v4.1-flash".into(),
            prompt_hash: "a".repeat(64),
            tool_manifest_hash: "b".repeat(64),
            model_revision: "c".repeat(64),
        }
    }

    #[test]
    fn model_cannot_rank_an_ineligible_or_universe_external_instrument() {
        let request = AnalystRequest {
            run: SelectionRun {
                schema_version: 1,
                run_id: "selection_1".into(),
                account_id: "paper_eur".into(),
                session_id: "session_1".into(),
                universe_snapshot_id: "universe_1".into(),
                started_at: now() - Duration::minutes(1),
                completed_at: now(),
                model: fingerprint(),
                fallback_attempts: vec![],
                candidates: vec![],
                selected_instrument_id: None,
                proposal: None,
                evidence_snapshot_ids: vec![],
                failure: None,
            },
            eligible_instrument_ids: vec![InstrumentId::new("AIR_PA_EQ").expect("id")],
            candidate_evidence_ids: vec!["evidence_1".into()],
            fallback_attempts: vec![],
            as_of: now(),
        };
        let response = AnalystResponse {
            run_id: "selection_1".into(),
            session_id: "session_1".into(),
            model: fingerprint(),
            ranks: vec![AnalystRank {
                instrument_id: InstrumentId::new("AAPL_US_EQ").expect("id"),
                score: Decimal::ONE,
                sentiment: None,
                confidence: None,
                evidence_ids: vec!["evidence_1".into()],
                rationale: "foreign".into(),
            }],
            selected_instrument_id: InstrumentId::new("AAPL_US_EQ").expect("id"),
            proposal: SelectionProposal {
                entry_minimum: Decimal::ONE,
                entry_maximum: Decimal::new(2, 0),
                stop_price: Decimal::new(9, 1),
                target_price: Decimal::new(3, 0),
                maximum_hold_until: now() + Duration::hours(1),
                rationale: "foreign".into(),
            },
            uncertainties: vec![],
            contradictions: vec![],
            completed_at: now(),
        };
        assert!(response.validate(&request, now()).is_err());
    }
}

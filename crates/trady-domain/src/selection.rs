//! Versioned research selection contracts.
//!
//! A successful selection run always contains one selected eligible EUR
//! instrument. Provider failures are represented explicitly and can never be
//! converted into an invented ticker or an empty successful result.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{Currency, Error, InstrumentId, Result, require};

pub const SELECTION_SCHEMA_VERSION: u32 = 1;
pub const MAX_RANKED_CANDIDATES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFingerprint {
    pub run_id: String,
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub prompt_hash: String,
    pub tool_manifest_hash: String,
    pub model_revision: String,
}

impl ModelFingerprint {
    pub fn validate(&self) -> Result<()> {
        require(
            valid_id(&self.run_id)
                && valid_id(&self.session_id)
                && valid_text(&self.provider, 96)
                && self.model == "deepseek-v4.1-flash"
                && is_hash(&self.prompt_hash)
                && is_hash(&self.tool_manifest_hash)
                && is_hash(&self.model_revision),
            "invalid or unpinned analyst model fingerprint",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankedCandidate {
    pub instrument_id: InstrumentId,
    pub currency: Currency,
    pub eligibility_snapshot_id: String,
    pub eligible: bool,
    pub ranking_score: Decimal,
    pub sentiment_score: Option<Decimal>,
    pub confidence: Option<Decimal>,
    pub evidence_ids: Vec<String>,
    pub exclusion_reasons: Vec<String>,
    pub catalyst: Option<String>,
    pub contradictions: Vec<String>,
    pub strategy_snapshot_ids: Vec<String>,
}

impl RankedCandidate {
    pub fn validate(&self) -> Result<()> {
        require(
            self.instrument_id.as_str().ends_with("_EQ")
                && self.currency.as_str() == "EUR"
                && valid_id(&self.eligibility_snapshot_id)
                && self.ranking_score >= Decimal::ZERO
                && self.ranking_score <= Decimal::ONE
                && self.evidence_ids.len() <= 64
                && self.evidence_ids.iter().all(|id| valid_id(id))
                && self.strategy_snapshot_ids.len() <= 16
                && self.strategy_snapshot_ids.iter().all(|id| valid_id(id))
                && self.exclusion_reasons.len() <= 32
                && self.contradictions.len() <= 32,
            "invalid ranked candidate",
        )?;
        if let Some(value) = self.sentiment_score {
            require(
                (-Decimal::ONE..=Decimal::ONE).contains(&value),
                "candidate sentiment is outside the bounded range",
            )?;
        }
        if let Some(value) = self.confidence {
            require(
                (Decimal::ZERO..=Decimal::ONE).contains(&value),
                "candidate confidence is outside the bounded range",
            )?;
        }
        if self.eligible {
            require(
                self.exclusion_reasons.is_empty(),
                "eligible candidate carries exclusion reasons",
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionProposal {
    pub entry_minimum: Decimal,
    pub entry_maximum: Decimal,
    pub stop_price: Decimal,
    pub target_price: Decimal,
    pub maximum_hold_until: DateTime<Utc>,
    pub rationale: String,
}

impl SelectionProposal {
    pub fn validate(&self, now: DateTime<Utc>, session_flatten_at: DateTime<Utc>) -> Result<()> {
        require(
            self.entry_minimum > Decimal::ZERO
                && self.entry_minimum <= self.entry_maximum
                && self.stop_price > Decimal::ZERO
                && self.stop_price < self.entry_minimum
                && self.target_price > self.entry_maximum
                && self.maximum_hold_until > now
                && self.maximum_hold_until <= session_flatten_at
                && valid_text(&self.rationale, 2_000),
            "invalid bounded selection proposal",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SelectionFallbackStage {
    ReviewedUniverse,
    BrokerCatalog,
    AlphaVantage,
    YfinanceNews,
    PublisherRss,
    XMonitoring,
    UserSubmittedUrl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FallbackAttempt {
    pub stage: SelectionFallbackStage,
    pub status: SelectionAttemptStatus,
    pub provider_reference: String,
    pub candidate_count: u32,
    pub completed_at: DateTime<Utc>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SelectionAttemptStatus {
    Complete,
    Partial,
    Unavailable,
    Stale,
    RateLimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum SelectionFailure {
    OperationalInputsUnavailable {
        stage: SelectionFallbackStage,
        code: String,
        message: String,
    },
    NoEligibleInstrument {
        message: String,
    },
}

impl SelectionFailure {
    fn validate(&self) -> Result<()> {
        match self {
            Self::OperationalInputsUnavailable { code, message, .. } => require(
                valid_id(code) && valid_text(message, 500),
                "invalid operational selection failure",
            ),
            Self::NoEligibleInstrument { message } => {
                require(valid_text(message, 500), "invalid selection failure")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionRun {
    pub schema_version: u32,
    pub run_id: String,
    pub account_id: String,
    pub session_id: String,
    pub universe_snapshot_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub model: ModelFingerprint,
    pub fallback_attempts: Vec<FallbackAttempt>,
    pub candidates: Vec<RankedCandidate>,
    pub selected_instrument_id: Option<InstrumentId>,
    pub proposal: Option<SelectionProposal>,
    pub evidence_snapshot_ids: Vec<String>,
    pub failure: Option<SelectionFailure>,
}

impl SelectionRun {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.schema_version == SELECTION_SCHEMA_VERSION
                && valid_id(&self.run_id)
                && valid_id(&self.account_id)
                && valid_id(&self.session_id)
                && valid_id(&self.universe_snapshot_id)
                && self.started_at <= self.completed_at
                && self.completed_at <= now
                && self.candidates.len() <= MAX_RANKED_CANDIDATES
                && self.evidence_snapshot_ids.len() <= 128
                && self.evidence_snapshot_ids.iter().all(|id| valid_id(id))
                && self.fallback_attempts.len() <= 16,
            "invalid selection run envelope",
        )?;
        self.model.validate()?;
        let mut ids = BTreeSet::new();
        for candidate in &self.candidates {
            candidate.validate()?;
            require(
                ids.insert(&candidate.instrument_id),
                "duplicate ranked instrument",
            )?;
        }
        for attempt in &self.fallback_attempts {
            require(
                valid_id(&attempt.provider_reference)
                    && attempt.candidate_count <= MAX_RANKED_CANDIDATES as u32
                    && attempt.completed_at <= now
                    && attempt
                        .error_code
                        .as_ref()
                        .is_none_or(|code| valid_id(code)),
                "invalid fallback attempt",
            )?;
        }
        match (&self.selected_instrument_id, &self.proposal, &self.failure) {
            (Some(selected), Some(proposal), None) => {
                let candidate = self
                    .candidates
                    .iter()
                    .find(|candidate| &candidate.instrument_id == selected)
                    .ok_or_else(|| Error("selected instrument is not ranked".into()))?;
                require(candidate.eligible, "selected candidate is ineligible")?;
                require(!self.candidates.is_empty(), "successful selection is empty")?;
                let mut sorted = self.candidates.clone();
                sorted.sort_by(|a, b| {
                    b.ranking_score
                        .cmp(&a.ranking_score)
                        .then_with(|| a.instrument_id.cmp(&b.instrument_id))
                });
                require(
                    sorted
                        .first()
                        .is_some_and(|first| first.instrument_id == *selected),
                    "selected instrument is not the highest-ranked eligible candidate",
                )?;
                // The proposal is checked against a session-specific deadline by
                // the application gate. The envelope still rejects impossible
                // ordering and time reversal here.
                require(
                    proposal.entry_minimum > Decimal::ZERO
                        && proposal.entry_minimum <= proposal.entry_maximum
                        && proposal.stop_price > Decimal::ZERO
                        && proposal.stop_price < proposal.entry_minimum
                        && proposal.target_price > proposal.entry_maximum
                        && proposal.maximum_hold_until > self.started_at,
                    "invalid selection proposal bounds",
                )?;
            }
            (None, None, Some(failure)) => failure.validate()?,
            _ => {
                return Err(Error(
                    "selection run must be selected or failed exactly once".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn successful(
        mut self,
        selected: InstrumentId,
        proposal: SelectionProposal,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        self.selected_instrument_id = Some(selected);
        self.proposal = Some(proposal);
        self.failure = None;
        self.completed_at = now;
        self.validate(now)?;
        Ok(self)
    }

    pub fn operational_failure(
        mut self,
        failure: SelectionFailure,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        self.selected_instrument_id = None;
        self.proposal = None;
        self.failure = Some(failure);
        self.completed_at = now;
        self.validate(now)?;
        Ok(self)
    }
}

/// Deterministically order only eligible candidates. This function is the
/// boundary the LLM ranker consumes; ineligible rows remain in the immutable
/// run for audit but cannot be selected.
pub fn rank_candidates(mut candidates: Vec<RankedCandidate>) -> Result<Vec<RankedCandidate>> {
    require(
        candidates.len() <= MAX_RANKED_CANDIDATES,
        "too many candidate rows",
    )?;
    for candidate in &candidates {
        candidate.validate()?;
    }
    candidates.sort_by(|a, b| {
        b.eligible
            .cmp(&a.eligible)
            .then_with(|| b.ranking_score.cmp(&a.ranking_score))
            .then_with(|| a.instrument_id.cmp(&b.instrument_id))
    });
    Ok(candidates)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains(['\0', '\r', '\n'])
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }

    fn candidate(id: &str, score: i64, eligible: bool) -> RankedCandidate {
        RankedCandidate {
            instrument_id: InstrumentId::new(id).expect("id"),
            currency: Currency::new("EUR").expect("currency"),
            eligibility_snapshot_id: "elig_1".into(),
            eligible,
            ranking_score: Decimal::new(score, 2),
            sentiment_score: Some(Decimal::ZERO),
            confidence: Some(Decimal::new(80, 2)),
            evidence_ids: vec!["evidence_1".into()],
            exclusion_reasons: if eligible {
                vec![]
            } else {
                vec!["stale".into()]
            },
            catalyst: Some("earnings".into()),
            contradictions: vec![],
            strategy_snapshot_ids: vec!["signal_1".into()],
        }
    }

    fn run() -> SelectionRun {
        SelectionRun {
            schema_version: 1,
            run_id: "selection_1".into(),
            account_id: "paper_eur".into(),
            session_id: "xpar_2026_11_03".into(),
            universe_snapshot_id: "eur_universe_1".into(),
            started_at: now() - Duration::minutes(1),
            completed_at: now(),
            model: ModelFingerprint {
                run_id: "selection_1".into(),
                session_id: "xpar_2026_11_03".into(),
                provider: "opencode-go".into(),
                model: "deepseek-v4.1-flash".into(),
                prompt_hash: "a".repeat(64),
                tool_manifest_hash: "b".repeat(64),
                model_revision: "c".repeat(64),
            },
            fallback_attempts: vec![],
            candidates: vec![candidate("AIR_PA_EQ", 90, true)],
            selected_instrument_id: None,
            proposal: None,
            evidence_snapshot_ids: vec!["evidence_1".into()],
            failure: None,
        }
    }

    #[test]
    fn successful_selection_requires_one_eligible_top_rank() {
        let selected = run()
            .successful(
                InstrumentId::new("AIR_PA_EQ").expect("id"),
                SelectionProposal {
                    entry_minimum: Decimal::new(100, 0),
                    entry_maximum: Decimal::new(101, 0),
                    stop_price: Decimal::new(99, 0),
                    target_price: Decimal::new(103, 0),
                    maximum_hold_until: now() + Duration::hours(1),
                    rationale: "bounded proposal".into(),
                },
                now(),
            )
            .expect("selection");
        assert_eq!(
            selected.selected_instrument_id.unwrap().as_str(),
            "AIR_PA_EQ"
        );
    }

    #[test]
    fn an_empty_or_ineligible_run_is_a_typed_failure() {
        let mut failed = run();
        failed.candidates = vec![candidate("AIR_PA_EQ", 90, false)];
        assert!(
            failed
                .operational_failure(
                    SelectionFailure::OperationalInputsUnavailable {
                        stage: SelectionFallbackStage::YfinanceNews,
                        code: "provider_unavailable".into(),
                        message: "provider unavailable".into(),
                    },
                    now(),
                )
                .is_ok()
        );
        let mut invalid = run();
        invalid.candidates.clear();
        invalid.selected_instrument_id = Some(InstrumentId::new("AIR_PA_EQ").expect("id"));
        invalid.proposal = Some(SelectionProposal {
            entry_minimum: Decimal::ONE,
            entry_maximum: Decimal::TWO,
            stop_price: Decimal::new(9, 1),
            target_price: Decimal::new(3, 0),
            maximum_hold_until: now() + Duration::hours(1),
            rationale: "bad".into(),
        });
        assert!(invalid.validate(now()).is_err());
    }
}

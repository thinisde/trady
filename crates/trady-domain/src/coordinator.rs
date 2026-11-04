//! Durable same-day coordinator state machine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{Error, InstrumentId, Result, require};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoordinatorState {
    #[default]
    Idle,
    Researching,
    CandidatesRanked,
    CandidateValidated,
    SignalReady,
    RiskApproved,
    EntryPending,
    Open,
    ExitManaging,
    Closed,
    Rejected,
    Expired,
    EmergencyExit,
    Recovery,
}

impl CoordinatorState {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Closed | Self::Rejected | Self::Expired)
    }

    pub fn risk_reducing(self) -> bool {
        matches!(
            self,
            Self::ExitManaging | Self::EmergencyExit | Self::Recovery
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoordinatorEventKind {
    StartResearch,
    CandidatesRanked,
    CandidateValidated,
    SignalProduced,
    RiskApproved,
    EntrySubmitted,
    EntryFilled,
    ExitStarted,
    ExitClosed,
    Reject,
    Expire,
    Emergency,
    Recover,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatorEvent {
    pub event_id: String,
    pub kind: CoordinatorEventKind,
    pub from: CoordinatorState,
    pub to: CoordinatorState,
    pub account_id: String,
    pub instrument_id: Option<InstrumentId>,
    pub policy_revision: String,
    pub evidence_snapshot_ids: Vec<String>,
    pub occurred_at: DateTime<Utc>,
    pub reason: Option<String>,
}

impl CoordinatorEvent {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_id(&self.event_id)
                && valid_id(&self.account_id)
                && valid_id(&self.policy_revision)
                && self
                    .instrument_id
                    .as_ref()
                    .is_none_or(|id| valid_id(id.as_str()))
                && self.evidence_snapshot_ids.len() <= 128
                && self.evidence_snapshot_ids.iter().all(|id| valid_id(id))
                && self.occurred_at <= now
                && self
                    .reason
                    .as_ref()
                    .is_none_or(|reason| valid_text(reason, 1_000)),
            "invalid coordinator event",
        )?;
        require(
            allowed_transition(self.from, self.to, self.kind),
            "coordinator transition is not allowed",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatorSnapshot {
    pub revision: u64,
    pub state: CoordinatorState,
    pub event_id: String,
    pub account_id: String,
    pub instrument_id: Option<InstrumentId>,
    pub policy_revision: String,
    pub evidence_snapshot_ids: Vec<String>,
    pub session_id: String,
    pub updated_at: DateTime<Utc>,
    pub flatten_deadline: DateTime<Utc>,
}

impl CoordinatorSnapshot {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_id(&self.event_id)
                && valid_id(&self.account_id)
                && valid_id(&self.policy_revision)
                && valid_id(&self.session_id)
                && self
                    .instrument_id
                    .as_ref()
                    .is_none_or(|id| valid_id(id.as_str()))
                && self.evidence_snapshot_ids.len() <= 128
                && self.evidence_snapshot_ids.iter().all(|id| valid_id(id))
                && self.updated_at <= now
                && self.flatten_deadline > self.updated_at,
            "invalid coordinator snapshot",
        )
    }
}

#[derive(Debug, Clone)]
pub struct Coordinator {
    snapshot: CoordinatorSnapshot,
    event_ids: BTreeSet<String>,
}

impl Coordinator {
    pub fn new(
        account_id: String,
        session_id: String,
        policy_revision: String,
        flatten_deadline: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        let snapshot = CoordinatorSnapshot {
            revision: 0,
            state: CoordinatorState::Idle,
            event_id: format!("event_{}_idle", session_id),
            account_id,
            instrument_id: None,
            policy_revision,
            evidence_snapshot_ids: vec![],
            session_id,
            updated_at: now,
            flatten_deadline,
        };
        snapshot.validate(now)?;
        Ok(Self {
            event_ids: BTreeSet::from([snapshot.event_id.clone()]),
            snapshot,
        })
    }

    pub fn snapshot(&self) -> &CoordinatorSnapshot {
        &self.snapshot
    }

    pub fn apply(&mut self, event: CoordinatorEvent) -> Result<()> {
        event.validate(event.occurred_at)?;
        require(
            event.from == self.snapshot.state
                && event.account_id == self.snapshot.account_id
                && event.policy_revision == self.snapshot.policy_revision
                && event.occurred_at >= self.snapshot.updated_at
                && !self.event_ids.contains(&event.event_id),
            "coordinator event is stale, differently scoped or duplicated",
        )?;
        if let Some(instrument_id) = &self.snapshot.instrument_id {
            require(
                event
                    .instrument_id
                    .as_ref()
                    .is_none_or(|id| id == instrument_id),
                "coordinator instrument identity changed",
            )?;
        }
        let instrument_id = event
            .instrument_id
            .clone()
            .or_else(|| self.snapshot.instrument_id.clone());
        if matches!(
            event.to,
            CoordinatorState::CandidateValidated
                | CoordinatorState::SignalReady
                | CoordinatorState::RiskApproved
                | CoordinatorState::EntryPending
                | CoordinatorState::Open
                | CoordinatorState::ExitManaging
        ) {
            require(
                instrument_id.is_some(),
                "active coordinator state lacks instrument identity",
            )?;
        }
        self.snapshot.revision = self
            .snapshot
            .revision
            .checked_add(1)
            .ok_or_else(|| Error("coordinator revision overflow".into()))?;
        self.snapshot.state = event.to;
        self.snapshot.event_id = event.event_id.clone();
        self.snapshot.instrument_id = instrument_id;
        self.snapshot.evidence_snapshot_ids = event.evidence_snapshot_ids.clone();
        self.snapshot.updated_at = event.occurred_at;
        self.event_ids.insert(event.event_id);
        Ok(())
    }

    pub fn reset_to_idle(&mut self, event_id: String, now: DateTime<Utc>) -> Result<()> {
        let event = CoordinatorEvent {
            event_id,
            kind: CoordinatorEventKind::Reset,
            from: self.snapshot.state,
            to: CoordinatorState::Idle,
            account_id: self.snapshot.account_id.clone(),
            instrument_id: None,
            policy_revision: self.snapshot.policy_revision.clone(),
            evidence_snapshot_ids: vec![],
            occurred_at: now,
            reason: None,
        };
        self.apply(event)
    }
}

pub fn allowed_transition(
    from: CoordinatorState,
    to: CoordinatorState,
    kind: CoordinatorEventKind,
) -> bool {
    if matches!(
        to,
        CoordinatorState::Rejected
            | CoordinatorState::Expired
            | CoordinatorState::EmergencyExit
            | CoordinatorState::Recovery
    ) {
        return matches!(
            kind,
            CoordinatorEventKind::Reject
                | CoordinatorEventKind::Expire
                | CoordinatorEventKind::Emergency
                | CoordinatorEventKind::Recover
        ) && !matches!(from, CoordinatorState::Closed);
    }
    matches!(
        (from, to, kind),
        (
            CoordinatorState::Idle,
            CoordinatorState::Researching,
            CoordinatorEventKind::StartResearch,
        ) | (
            CoordinatorState::Researching,
            CoordinatorState::CandidatesRanked,
            CoordinatorEventKind::CandidatesRanked,
        ) | (
            CoordinatorState::CandidatesRanked,
            CoordinatorState::CandidateValidated,
            CoordinatorEventKind::CandidateValidated,
        ) | (
            CoordinatorState::CandidateValidated,
            CoordinatorState::SignalReady,
            CoordinatorEventKind::SignalProduced,
        ) | (
            CoordinatorState::SignalReady,
            CoordinatorState::RiskApproved,
            CoordinatorEventKind::RiskApproved,
        ) | (
            CoordinatorState::RiskApproved,
            CoordinatorState::EntryPending,
            CoordinatorEventKind::EntrySubmitted,
        ) | (
            CoordinatorState::EntryPending,
            CoordinatorState::Open,
            CoordinatorEventKind::EntryFilled,
        ) | (
            CoordinatorState::Open,
            CoordinatorState::ExitManaging,
            CoordinatorEventKind::ExitStarted,
        ) | (
            CoordinatorState::ExitManaging,
            CoordinatorState::Closed,
            CoordinatorEventKind::ExitClosed,
        ) | (
            CoordinatorState::Closed,
            CoordinatorState::Idle,
            CoordinatorEventKind::Reset
        ) | (
            CoordinatorState::Rejected,
            CoordinatorState::Idle,
            CoordinatorEventKind::Reset
        ) | (
            CoordinatorState::Expired,
            CoordinatorState::Idle,
            CoordinatorEventKind::Reset
        ) | (
            CoordinatorState::EmergencyExit,
            CoordinatorState::Idle,
            CoordinatorEventKind::Reset
        ) | (
            CoordinatorState::Recovery,
            CoordinatorState::Idle,
            CoordinatorEventKind::Reset
        )
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .expect("time")
            .to_utc()
    }

    fn event(
        coordinator: &Coordinator,
        id: &str,
        kind: CoordinatorEventKind,
        to: CoordinatorState,
        now: DateTime<Utc>,
        instrument_id: Option<InstrumentId>,
    ) -> CoordinatorEvent {
        CoordinatorEvent {
            event_id: id.into(),
            kind,
            from: coordinator.snapshot().state,
            to,
            account_id: coordinator.snapshot().account_id.clone(),
            instrument_id,
            policy_revision: coordinator.snapshot().policy_revision.clone(),
            evidence_snapshot_ids: vec![],
            occurred_at: now,
            reason: None,
        }
    }

    #[test]
    fn legal_path_is_durable_and_illegal_skip_is_rejected() {
        let now = now();
        let mut coordinator = Coordinator::new(
            "paper_eur".into(),
            "xpar_2026_11_03".into(),
            "policy_eur_v1".into(),
            now + Duration::hours(8),
            now,
        )
        .expect("coordinator");
        assert!(
            coordinator
                .apply(event(
                    &coordinator,
                    "event_research",
                    CoordinatorEventKind::StartResearch,
                    CoordinatorState::Researching,
                    now,
                    None,
                ))
                .is_ok()
        );
        let bad = event(
            &coordinator,
            "event_bad",
            CoordinatorEventKind::EntrySubmitted,
            CoordinatorState::EntryPending,
            now,
            None,
        );
        assert!(coordinator.apply(bad).is_err());
    }

    #[test]
    fn emergency_exit_is_available_from_an_active_state_and_cannot_be_reversed_without_reset() {
        let now = now();
        let mut coordinator = Coordinator::new(
            "paper_eur".into(),
            "xpar_2026_11_03".into(),
            "policy_eur_v1".into(),
            now + Duration::hours(8),
            now,
        )
        .expect("coordinator");
        coordinator
            .apply(event(
                &coordinator,
                "event_research",
                CoordinatorEventKind::StartResearch,
                CoordinatorState::Researching,
                now,
                None,
            ))
            .expect("research");
        coordinator
            .apply(event(
                &coordinator,
                "event_emergency",
                CoordinatorEventKind::Emergency,
                CoordinatorState::EmergencyExit,
                now + Duration::seconds(1),
                None,
            ))
            .expect("emergency");
        assert!(
            coordinator
                .apply(event(
                    &coordinator,
                    "event_open",
                    CoordinatorEventKind::EntryFilled,
                    CoordinatorState::Open,
                    now + Duration::seconds(2),
                    Some(InstrumentId::new("AIR_PA_EQ").expect("id")),
                ))
                .is_err()
        );
    }
}

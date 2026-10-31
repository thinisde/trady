use super::*;
use chrono::{Duration, NaiveDate};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarSession {
    pub session_id: String,
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedDate {
    pub date: NaiveDate,
    pub reason: String,
}
/// Complete date coverage includes provider-confirmed holidays, weekends and
/// special closures. UTC opening/closing instants preserve DST and half days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarDocument {
    pub calendar_id: String,
    pub listing_mic: String,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub coverage_start: NaiveDate,
    pub coverage_end: NaiveDate,
    pub sessions: Vec<CalendarSession>,
    pub closures: Vec<ClosedDate>,
}
impl CalendarDocument {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            valid_id(&self.calendar_id)
                && self.listing_mic.len() == 4
                && self
                    .listing_mic
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                && self.observed_at <= self.available_at
                && self.available_at <= now
                && now < self.valid_until
                && self.coverage_start <= self.coverage_end
                && (self.coverage_end - self.coverage_start).num_days() <= 400
                && self.sessions.len() <= 401
                && self.closures.len() <= 401,
            "invalid or expired exchange calendar",
        )?;
        let mut dates = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut prior = None;
        for s in &self.sessions {
            require(
                valid_id(&s.session_id)
                    && ids.insert(&s.session_id)
                    && s.opens_at < s.closes_at
                    && s.closes_at - s.opens_at < Duration::hours(24)
                    && s.opens_at.date_naive() == s.closes_at.date_naive()
                    && dates.insert(s.opens_at.date_naive())
                    && prior.is_none_or(|t| t < s.opens_at),
                "duplicate, overlapping or unordered calendar session",
            )?;
            prior = Some(s.closes_at);
        }
        for c in &self.closures {
            require(
                dates.insert(c.date) && !c.reason.trim().is_empty() && c.reason.len() <= 256,
                "duplicate or unexplained exchange closure",
            )?;
        }
        let expected = (self.coverage_end - self.coverage_start).num_days() + 1;
        require(
            dates.len() as i64 == expected
                && dates
                    .iter()
                    .all(|d| *d >= self.coverage_start && *d <= self.coverage_end),
            "calendar does not explicitly cover every date",
        )
    }
    pub fn active_session(&self, now: DateTime<Utc>) -> Result<Option<CalendarSession>> {
        self.validate(now)?;
        require(
            now.date_naive() >= self.coverage_start && now.date_naive() <= self.coverage_end,
            "current date lies outside verified calendar coverage",
        )?;
        Ok(self
            .sessions
            .iter()
            .find(|s| s.opens_at <= now && now < s.closes_at)
            .cloned())
    }
    /// Entry session counts as session one; missing dates are not inferred weekdays.
    pub fn deadline(&self, decision: DateTime<Utc>, count: u8) -> Result<DateTime<Utc>> {
        require(
            (1..=5).contains(&count) && self.active_session(decision)?.is_some(),
            "deadline requires active exchange session",
        )?;
        self.sessions
            .iter()
            .filter(|s| s.closes_at > decision)
            .nth(usize::from(count) - 1)
            .map(|s| s.closes_at)
            .ok_or_else(|| Error("verified calendar has insufficient future sessions".into()))
    }
}

//! Point-in-time stock research screening. A shortlist confers no entry authority.

use crate::{Currency, Error, InstrumentId, Result, require, research::SourceRights};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_UNIVERSE_ROWS: usize = 2_048;
pub const MAX_CANDIDATES: usize = 5;
pub const MAX_PEER_CONTROLS: usize = 2;
pub const SCREENING_METHOD: &str = "sp500-research-screen-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveInterval {
    pub from: DateTime<Utc>,
    /// Exclusive. Announced additions/removals take effect at the supplied instant.
    pub until: Option<DateTime<Utc>>,
}
impl EffectiveInterval {
    pub fn valid(&self) -> bool {
        self.until.is_none_or(|until| self.from < until)
    }
    pub fn contains(&self, at: DateTime<Utc>) -> bool {
        self.valid() && self.from <= at && self.until.is_none_or(|until| at < until)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Classification {
    pub sector_code: String,
    pub industry_group_code: String,
    pub effective: EffectiveInterval,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniverseMember {
    pub issuer_id: String,
    /// ISIN identifies the share class; ticker alone is never an identity join.
    pub share_class_isin: String,
    pub symbol: String,
    /// ISO 10383 market identifier, supplied by the approved data provider.
    pub listing_mic: String,
    pub currency: Currency,
    pub membership: EffectiveInterval,
    pub classification: Classification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniverseProvenance {
    pub source_id: String,
    pub source_url: String,
    pub provider: String,
    pub content_sha256: String,
    pub parser_version: String,
    pub fetched_at: DateTime<Utc>,
    pub rights: SourceRights,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniverseSnapshot {
    pub snapshot_id: String,
    pub classification_snapshot_id: String,
    pub index_id: String,
    pub taxonomy_edition: String,
    /// Effective provider snapshot time, separate from when it became available.
    pub as_of: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub provenance: UniverseProvenance,
    pub members: Vec<UniverseMember>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupportStatus {
    Unknown,
    Unsupported,
    Verified,
}

/// A trusted broker verifier supplies these records. Metadata presence alone is
/// insufficient evidence of current owner/account instrument support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerStockSupport {
    pub instrument_id: InstrumentId,
    pub share_class_isin: String,
    pub listing_mic: String,
    pub currency: Currency,
    pub instrument_kind: String,
    pub status: SupportStatus,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub snapshot_id: String,
    pub evidence_id: String,
    /// In instrument currency, not the owner's base currency.
    pub minimum_order_notional: Decimal,
    pub fractional_orders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingObservation {
    pub snapshot_id: String,
    pub session_id: String,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
    /// Trusted event/calendar check for this instrument and proposed horizon.
    pub event_window_clear: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketObservation {
    pub instrument_id: InstrumentId,
    pub snapshot_id: String,
    pub source_id: String,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub currency: Currency,
    pub bid: Decimal,
    pub ask: Decimal,
    /// All notionals are in instrument currency and use a common provider basis.
    pub average_daily_notional: Decimal,
    pub proposed_order_notional: Decimal,
    /// Includes spread, fees, FX and other known round-trip execution costs.
    pub estimated_round_trip_cost_bps: Decimal,
    pub timing: Option<TimingObservation>,
}

/// Five rubric components: catalyst, breadth, economic transmission,
/// tradability, counterevidence/coverage. Unknown is distinct from observed zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorityObservation {
    pub instrument_id: InstrumentId,
    pub calculation_id: String,
    pub rubric_version: String,
    pub evidence_ids: Vec<String>,
    pub observed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub components: [Option<u8>; 5],
    pub material_exposure: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreeningObservations {
    pub broker_support: Vec<BrokerStockSupport>,
    pub markets: Vec<MarketObservation>,
    pub priorities: Vec<PriorityObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreeningPolicy {
    pub max_membership_age_seconds: i64,
    pub max_broker_age_seconds: i64,
    pub max_quote_age_seconds: i64,
    pub max_priority_age_seconds: i64,
    pub max_timing_age_seconds: i64,
    pub minimum_seconds_to_close: i64,
    pub max_spread_bps: Decimal,
    pub max_round_trip_cost_bps: Decimal,
    pub minimum_daily_notional: Decimal,
    pub max_order_participation_bps: Decimal,
    pub candidate_limit: usize,
    pub peer_limit: usize,
}
impl ScreeningPolicy {
    pub fn validate(&self) -> Result<()> {
        require(
            (1..=604_800).contains(&self.max_membership_age_seconds)
                && (1..=86_400).contains(&self.max_broker_age_seconds)
                && (1..=300).contains(&self.max_quote_age_seconds)
                && (1..=86_400).contains(&self.max_priority_age_seconds)
                && (1..=3_600).contains(&self.max_timing_age_seconds)
                && (0..=86_400).contains(&self.minimum_seconds_to_close)
                && self.max_spread_bps > Decimal::ZERO
                && self.max_spread_bps <= Decimal::from(10_000)
                && self.max_round_trip_cost_bps > Decimal::ZERO
                && self.max_round_trip_cost_bps <= Decimal::from(10_000)
                && self.minimum_daily_notional > Decimal::ZERO
                && self.max_order_participation_bps > Decimal::ZERO
                && self.max_order_participation_bps <= Decimal::from(10_000)
                && (1..=MAX_CANDIDATES).contains(&self.candidate_limit)
                && self.peer_limit <= MAX_PEER_CONTROLS,
            "invalid screening policy bounds",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RejectionReason {
    InvalidUniverse,
    UnpermittedSourceStorage,
    StaleOrFutureUniverse,
    EmptyUniverse,
    InvalidIdentity,
    MembershipNotEffective,
    InvalidClassification,
    ClassificationNotEffective,
    StaleOrFutureClassification,
    DuplicateShareClass,
    MissingBrokerSupport,
    AmbiguousBrokerSupport,
    BrokerStockNotVerified,
    StaleOrFutureBrokerSupport,
    OwnerNotPermitted,
    MissingMarketData,
    AmbiguousMarketData,
    StaleOrFutureMarketData,
    InvalidQuote,
    SpreadTooWide,
    InsufficientLiquidity,
    InvalidOrExcessiveCosts,
    UnaffordableOrder,
    MissingTiming,
    StaleOrFutureTiming,
    OutsideSession,
    EventTimingBlocked,
    MissingPriority,
    AmbiguousPriority,
    StaleOrFuturePriority,
    InvalidOrIncompletePriority,
    NoMaterialExposure,
    ShortlistLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RejectedMember {
    pub issuer_id: String,
    pub share_class_isin: String,
    pub symbol: String,
    pub instrument_id: Option<InstrumentId>,
    pub reasons: Vec<RejectionReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenedCandidate {
    pub instrument_id: InstrumentId,
    pub issuer_id: String,
    pub share_class_isin: String,
    pub sector_code: String,
    pub industry_group_code: String,
    pub priority_score: u8,
    pub priority_components: [u8; 5],
    pub spread_bps: Decimal,
    pub average_daily_notional: Decimal,
    pub broker_snapshot_id: String,
    pub market_snapshot_id: String,
    pub timing_snapshot_id: String,
    pub calculation_id: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScreeningPurpose {
    ResearchOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreeningResult {
    pub method: String,
    pub purpose: ScreeningPurpose,
    pub as_of: DateTime<Utc>,
    pub universe_snapshot_id: String,
    pub classification_snapshot_id: String,
    pub candidates: Vec<ScreenedCandidate>,
    pub peer_controls: Vec<ScreenedCandidate>,
    /// A nomination for further company research, never an executable entry.
    pub finalist_instrument_id: Option<InstrumentId>,
    pub rejections: Vec<RejectedMember>,
    pub blockers: Vec<RejectionReason>,
}

pub fn valid_reference(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

pub fn valid_isin(value: &str) -> bool {
    if value.len() != 12
        || !value[..2].bytes().all(|b| b.is_ascii_uppercase())
        || !value.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        || !value.as_bytes()[11].is_ascii_digit()
    {
        return false;
    }
    let mut digits = Vec::with_capacity(24);
    for b in value.bytes() {
        if b.is_ascii_digit() {
            digits.push(b - b'0');
        } else {
            let n = b - b'A' + 10;
            digits.extend([n / 10, n % 10]);
        }
    }
    digits.iter().rev().enumerate().map(|(i, digit)| {
        let n = if i % 2 == 1 { digit * 2 } else { *digit };
        u16::from(n / 10 + n % 10)
    }).sum::<u16>() % 10 == 0
}

/// Validate the effective 11-sector / 25-industry-group taxonomy, not a draft
/// consultation. The edition itself remains provider-supplied and persisted.
pub fn valid_gics_pair(sector: &str, group: &str) -> bool {
    matches!((sector, group),
        ("10", "1010") | ("15", "1510") | ("20", "2010" | "2020" | "2030")
        | ("25", "2510" | "2520" | "2530" | "2550")
        | ("30", "3010" | "3020" | "3030") | ("35", "3510" | "3520")
        | ("40", "4010" | "4020" | "4030") | ("45", "4510" | "4520" | "4530")
        | ("50", "5010" | "5020") | ("55", "5510") | ("60", "6010" | "6020"))
}

fn fresh(at: DateTime<Utc>, expires: DateTime<Utc>, now: DateTime<Utc>, age: i64) -> bool {
    at <= now && now < expires && now.signed_duration_since(at) <= Duration::seconds(age)
}

fn checked_bps(numerator: Decimal, denominator: Decimal) -> Option<Decimal> {
    numerator.checked_div(denominator)?.checked_mul(Decimal::from(10_000))
}

/// All inputs must come from the trusted application ports. Missing data is a
/// rejection, not an observed zero. Only finite bounded collections are accepted.
pub fn screen_universe(
    universe: &UniverseSnapshot,
    owner_allowlist: &BTreeSet<InstrumentId>,
    observations: &ScreeningObservations,
    policy: &ScreeningPolicy,
    now: DateTime<Utc>,
) -> Result<ScreeningResult> {
    policy.validate()?;
    require(
        universe.members.len() <= MAX_UNIVERSE_ROWS
            && owner_allowlist.len() <= MAX_UNIVERSE_ROWS
            && observations.broker_support.len() <= MAX_UNIVERSE_ROWS * 2
            && observations.markets.len() <= MAX_UNIVERSE_ROWS * 2
            && observations.priorities.len() <= MAX_UNIVERSE_ROWS * 2,
        "screening input bound exceeded",
    )?;
    let mut result = ScreeningResult {
        method: SCREENING_METHOD.into(), purpose: ScreeningPurpose::ResearchOnly,
        as_of: now, universe_snapshot_id: universe.snapshot_id.clone(),
        classification_snapshot_id: universe.classification_snapshot_id.clone(),
        candidates: vec![], peer_controls: vec![], finalist_instrument_id: None,
        rejections: vec![], blockers: vec![],
    };
    let lineage = &universe.provenance;
    if universe.index_id != "SP500" || !valid_reference(&universe.snapshot_id)
        || !valid_reference(&universe.classification_snapshot_id)
        || !valid_reference(&universe.taxonomy_edition)
        || !valid_reference(&lineage.source_id) || !valid_reference(&lineage.parser_version)
        || lineage.provider.trim().is_empty() || lineage.provider.len() > 256
        || lineage.source_url.len() > 2048 || !lineage.source_url.starts_with("https://")
        || lineage.content_sha256.len() != 64
        || !lineage.content_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || lineage.rights.terms_reference.trim().is_empty()
        || lineage.rights.terms_reference.len() > 512
        || !(1..=8760).contains(&lineage.rights.retention_hours)
    { result.blockers.push(RejectionReason::InvalidUniverse); }
    if !lineage.rights.store_content {
        result.blockers.push(RejectionReason::UnpermittedSourceStorage);
    }
    if !fresh(universe.as_of, universe.expires_at, now, policy.max_membership_age_seconds)
        || universe.available_at < universe.as_of || universe.available_at > now
        || lineage.fetched_at < universe.available_at || lineage.fetched_at > now
        || now.signed_duration_since(lineage.fetched_at) > Duration::hours(i64::from(lineage.rights.retention_hours))
    { result.blockers.push(RejectionReason::StaleOrFutureUniverse); }
    if universe.members.is_empty() { result.blockers.push(RejectionReason::EmptyUniverse); }

    let mut active_classes: BTreeMap<&str, usize> = BTreeMap::new();
    for member in &universe.members {
        if member.membership.contains(now) {
            *active_classes.entry(&member.share_class_isin).or_default() += 1;
        }
    }
    let mut accepted = Vec::new();
    for member in &universe.members {
        let mut reasons = result.blockers.clone();
        if !valid_reference(&member.issuer_id) || !valid_isin(&member.share_class_isin)
            || member.symbol.is_empty() || member.symbol.len() > 32
            || !member.symbol.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
            || member.listing_mic.len() != 4
            || !member.listing_mic.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            || !member.membership.valid()
        { reasons.push(RejectionReason::InvalidIdentity); }
        if !member.membership.contains(now) { reasons.push(RejectionReason::MembershipNotEffective); }
        if active_classes.get(member.share_class_isin.as_str()).is_some_and(|n| *n > 1) {
            reasons.push(RejectionReason::DuplicateShareClass);
        }
        let classification = &member.classification;
        if !valid_gics_pair(&classification.sector_code, &classification.industry_group_code)
            || !classification.effective.valid()
        { reasons.push(RejectionReason::InvalidClassification); }
        if !classification.effective.contains(now) { reasons.push(RejectionReason::ClassificationNotEffective); }
        if classification.observed_at > universe.as_of
            || now.signed_duration_since(classification.observed_at) > Duration::seconds(policy.max_membership_age_seconds)
        { reasons.push(RejectionReason::StaleOrFutureClassification); }

        let matching: Vec<_> = observations.broker_support.iter().filter(|support| {
            support.share_class_isin == member.share_class_isin
                && support.listing_mic == member.listing_mic && support.currency == member.currency
        }).collect();
        let mut candidate = None;
        let instrument_id = matching.first().map(|s| s.instrument_id.clone());
        if matching.len() != 1 {
            reasons.push(if matching.is_empty() { RejectionReason::MissingBrokerSupport } else { RejectionReason::AmbiguousBrokerSupport });
        } else {
            let support = matching[0];
            // A broker ID must not alias a second share class or listing.
            if observations.broker_support.iter().filter(|s| s.instrument_id == support.instrument_id).count() != 1 {
                reasons.push(RejectionReason::AmbiguousBrokerSupport);
            }
            if support.status != SupportStatus::Verified || support.instrument_kind != "STOCK"
                || !valid_reference(&support.evidence_id) || !valid_reference(&support.snapshot_id)
            { reasons.push(RejectionReason::BrokerStockNotVerified); }
            if !fresh(support.observed_at, support.expires_at, now, policy.max_broker_age_seconds) {
                reasons.push(RejectionReason::StaleOrFutureBrokerSupport);
            }
            if !owner_allowlist.contains(&support.instrument_id) { reasons.push(RejectionReason::OwnerNotPermitted); }
            candidate = evaluate_member(member, support, observations, policy, now, &mut reasons);
        }
        if reasons.is_empty() {
            accepted.push(candidate.ok_or_else(|| Error("screening candidate missing after validation".into()))?);
        } else {
            result.rejections.push(RejectedMember {
                issuer_id: member.issuer_id.clone(), share_class_isin: member.share_class_isin.clone(),
                symbol: member.symbol.clone(), instrument_id, reasons,
            });
        }
    }
    accepted.sort_by(|a, b| b.priority_score.cmp(&a.priority_score)
        .then_with(|| b.priority_components[4].cmp(&a.priority_components[4]))
        .then_with(|| a.spread_bps.cmp(&b.spread_bps))
        .then_with(|| b.average_daily_notional.cmp(&a.average_daily_notional))
        .then_with(|| a.instrument_id.cmp(&b.instrument_id)));
    let primary_group = accepted.first().map(|c| c.industry_group_code.clone());
    for candidate in accepted {
        if result.candidates.len() < policy.candidate_limit {
            result.candidates.push(candidate);
        } else if result.peer_controls.len() < policy.peer_limit
            && primary_group.as_deref() == Some(&candidate.industry_group_code)
        {
            result.peer_controls.push(candidate);
        } else {
            result.rejections.push(RejectedMember {
                issuer_id: candidate.issuer_id, share_class_isin: candidate.share_class_isin,
                symbol: universe.members.iter().find(|m| m.share_class_isin == candidate.share_class_isin)
                    .map(|m| m.symbol.clone()).unwrap_or_default(),
                instrument_id: Some(candidate.instrument_id), reasons: vec![RejectionReason::ShortlistLimit],
            });
        }
    }
    result.finalist_instrument_id = result.candidates.first().map(|c| c.instrument_id.clone());
    result.rejections.sort_by(|a, b| a.share_class_isin.cmp(&b.share_class_isin).then_with(|| a.symbol.cmp(&b.symbol)));
    Ok(result)
}

fn evaluate_member(
    member: &UniverseMember, support: &BrokerStockSupport, observations: &ScreeningObservations,
    policy: &ScreeningPolicy, now: DateTime<Utc>, reasons: &mut Vec<RejectionReason>,
) -> Option<ScreenedCandidate> {
    let markets: Vec<_> = observations.markets.iter().filter(|m| m.instrument_id == support.instrument_id).collect();
    let priorities: Vec<_> = observations.priorities.iter().filter(|p| p.instrument_id == support.instrument_id).collect();
    if markets.len() != 1 { reasons.push(if markets.is_empty() { RejectionReason::MissingMarketData } else { RejectionReason::AmbiguousMarketData }); }
    if priorities.len() != 1 { reasons.push(if priorities.is_empty() { RejectionReason::MissingPriority } else { RejectionReason::AmbiguousPriority }); }
    let market = markets.first()?;
    let priority = priorities.first()?;
    if !fresh(market.observed_at, market.expires_at, now, policy.max_quote_age_seconds) {
        reasons.push(RejectionReason::StaleOrFutureMarketData);
    }
    let midpoint = market.bid.checked_add(market.ask).and_then(|sum| sum.checked_div(Decimal::from(2)));
    let spread = market.ask.checked_sub(market.bid).and_then(|gap| checked_bps(gap, midpoint?));
    if market.bid <= Decimal::ZERO || market.ask < market.bid || market.currency != member.currency
        || !valid_reference(&market.snapshot_id) || !valid_reference(&market.source_id) || spread.is_none()
    { reasons.push(RejectionReason::InvalidQuote); }
    let spread = spread.unwrap_or(Decimal::MAX);
    if spread > policy.max_spread_bps { reasons.push(RejectionReason::SpreadTooWide); }
    let participation = checked_bps(market.proposed_order_notional, market.average_daily_notional);
    if market.average_daily_notional < policy.minimum_daily_notional
        || participation.is_none_or(|v| v > policy.max_order_participation_bps)
    { reasons.push(RejectionReason::InsufficientLiquidity); }
    if market.estimated_round_trip_cost_bps < spread
        || market.estimated_round_trip_cost_bps > policy.max_round_trip_cost_bps
    { reasons.push(RejectionReason::InvalidOrExcessiveCosts); }
    if support.minimum_order_notional <= Decimal::ZERO || market.proposed_order_notional <= Decimal::ZERO
        || market.proposed_order_notional < support.minimum_order_notional
        || (!support.fractional_orders && market.proposed_order_notional < market.ask)
    { reasons.push(RejectionReason::UnaffordableOrder); }
    if let Some(timing) = &market.timing {
        if !fresh(timing.observed_at, timing.expires_at, now, policy.max_timing_age_seconds)
            || !valid_reference(&timing.snapshot_id) || !valid_reference(&timing.session_id)
        { reasons.push(RejectionReason::StaleOrFutureTiming); }
        if timing.opens_at >= timing.closes_at || now < timing.opens_at
            || timing.closes_at.signed_duration_since(now) <= Duration::seconds(policy.minimum_seconds_to_close)
        { reasons.push(RejectionReason::OutsideSession); }
        if !timing.event_window_clear { reasons.push(RejectionReason::EventTimingBlocked); }
    } else { reasons.push(RejectionReason::MissingTiming); }
    if !fresh(priority.observed_at, priority.expires_at, now, policy.max_priority_age_seconds) {
        reasons.push(RejectionReason::StaleOrFuturePriority);
    }
    if priority.components.iter().any(|c| c.is_none_or(|value| value > 4))
        || !valid_reference(&priority.calculation_id) || !valid_reference(&priority.rubric_version)
        || priority.evidence_ids.is_empty() || priority.evidence_ids.len() > 64
        || !priority.evidence_ids.iter().all(|id| valid_reference(id))
    { reasons.push(RejectionReason::InvalidOrIncompletePriority); }
    if !priority.material_exposure { reasons.push(RejectionReason::NoMaterialExposure); }
    if !reasons.is_empty() { return None; }
    let components = priority.components.map(|value| value.unwrap_or(0));
    Some(ScreenedCandidate {
        instrument_id: support.instrument_id.clone(), issuer_id: member.issuer_id.clone(),
        share_class_isin: member.share_class_isin.clone(), sector_code: member.classification.sector_code.clone(),
        industry_group_code: member.classification.industry_group_code.clone(),
        priority_score: components.iter().sum(), priority_components: components, spread_bps: spread,
        average_daily_notional: market.average_daily_notional, broker_snapshot_id: support.snapshot_id.clone(),
        market_snapshot_id: market.snapshot_id.clone(), timing_snapshot_id: market.timing.as_ref()?.snapshot_id.clone(),
        calculation_id: priority.calculation_id.clone(), evidence_ids: priority.evidence_ids.clone(),
    })
}

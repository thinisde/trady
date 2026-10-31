use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeSet;
use trady_domain::{Currency, InstrumentId, research::SourceRights, screening::*};

struct Fixture {
    now: DateTime<Utc>,
    universe: UniverseSnapshot,
    owner: BTreeSet<InstrumentId>,
    observations: ScreeningObservations,
    policy: ScreeningPolicy,
}
type GateCase = (fn(&mut Fixture), RejectionReason);
impl Fixture {
    fn new(count: u32) -> Self {
        let now: DateTime<Utc> = "2026-10-30T14:00:00Z".parse().unwrap();
        let mut fixture = Self {
            now,
            universe: UniverseSnapshot {
                snapshot_id: "universe_1".into(),
                classification_snapshot_id: "gics_snapshot_1".into(),
                index_id: "SP500".into(),
                complete: true,
                taxonomy_edition: "GICS_2026".into(),
                as_of: now - Duration::seconds(30),
                available_at: now - Duration::seconds(20),
                expires_at: now + Duration::hours(1),
                provenance: UniverseProvenance {
                    source_id: "approved_fixture".into(),
                    source_url: "https://example.com/universe.json".into(),
                    provider: "Fixture provider".into(),
                    content_sha256: "a".repeat(64),
                    parser_version: "fixture_v1".into(),
                    fetched_at: now,
                    rights: SourceRights {
                        store_content: true,
                        llm_processing: false,
                        retention_hours: 24,
                        terms_reference: "fixture_only".into(),
                    },
                },
                members: vec![],
            },
            owner: BTreeSet::new(),
            observations: ScreeningObservations::default(),
            policy: ScreeningPolicy {
                max_membership_age_seconds: 86400,
                max_broker_age_seconds: 600,
                max_quote_age_seconds: 60,
                max_priority_age_seconds: 3600,
                max_timing_age_seconds: 300,
                minimum_seconds_to_close: 1800,
                max_spread_bps: Decimal::from(20),
                max_round_trip_cost_bps: Decimal::from(50),
                minimum_daily_notional: Decimal::from(100_000),
                max_order_participation_bps: Decimal::from(10),
                candidate_limit: 5,
                peer_limit: 2,
            },
        };
        for number in 1..=count {
            fixture.add(number);
        }
        fixture
    }
    fn add(&mut self, number: u32) {
        let id = InstrumentId::new(format!("STOCK_{number:03}")).unwrap();
        let isin = (0..=9)
            .map(|check| format!("US{number:09}{check}"))
            .find(|value| valid_isin(value))
            .unwrap();
        let currency = Currency::new("USD").unwrap();
        self.owner.insert(id.clone());
        self.universe.members.push(UniverseMember {
            issuer_id: format!("issuer_{}", number / 2),
            share_class_isin: isin.clone(),
            symbol: format!("S{number}"),
            listing_mic: "XNAS".into(),
            currency: currency.clone(),
            membership: EffectiveInterval {
                from: self.now - Duration::days(30),
                until: None,
            },
            classification: Classification {
                sector_code: "45".into(),
                industry_group_code: "4510".into(),
                effective: EffectiveInterval {
                    from: self.now - Duration::days(30),
                    until: None,
                },
                observed_at: self.now - Duration::seconds(40),
            },
        });
        self.observations.broker_support.push(BrokerStockSupport {
            instrument_id: id.clone(),
            share_class_isin: isin,
            listing_mic: "XNAS".into(),
            currency: currency.clone(),
            instrument_kind: "STOCK".into(),
            status: SupportStatus::Verified,
            observed_at: self.now - Duration::seconds(1),
            expires_at: self.now + Duration::minutes(5),
            snapshot_id: "broker_1".into(),
            evidence_id: "support_1".into(),
            minimum_order_notional: Decimal::ONE,
            fractional_orders: true,
        });
        self.observations.markets.push(MarketObservation {
            instrument_id: id.clone(),
            snapshot_id: "market_1".into(),
            source_id: "market_provider".into(),
            observed_at: self.now - Duration::seconds(1),
            expires_at: self.now + Duration::seconds(30),
            currency,
            bid: "99.99".parse().unwrap(),
            ask: "100.01".parse().unwrap(),
            average_daily_notional: Decimal::from(1_000_000),
            proposed_order_notional: Decimal::from(100),
            estimated_round_trip_cost_bps: Decimal::from(10),
            timing: Some(TimingObservation {
                snapshot_id: "calendar_1".into(),
                session_id: "XNAS_2026-10-30".into(),
                observed_at: self.now - Duration::seconds(1),
                expires_at: self.now + Duration::minutes(1),
                opens_at: self.now - Duration::minutes(30),
                closes_at: self.now + Duration::hours(6),
                event_window_clear: true,
            }),
        });
        self.observations.priorities.push(PriorityObservation {
            instrument_id: id,
            calculation_id: "priority_1".into(),
            rubric_version: "priority_v1".into(),
            evidence_ids: vec!["evidence_1".into()],
            observed_at: self.now - Duration::seconds(1),
            expires_at: self.now + Duration::minutes(10),
            components: [Some(3); 5],
            material_exposure: true,
        });
    }
    fn screen(&self) -> ScreeningResult {
        screen_universe(
            &self.universe,
            &self.owner,
            &self.observations,
            &self.policy,
            self.now,
        )
        .unwrap()
    }
    fn rejects(&self, reason: RejectionReason) {
        let result = self.screen();
        assert!(
            result.candidates.is_empty(),
            "unexpected candidate for {reason:?}"
        );
        assert!(
            result
                .rejections
                .iter()
                .any(|row| row.reasons.contains(&reason)),
            "missing {reason:?}: {:?}",
            result.rejections
        );
    }
}

#[test]
fn screening_effective_membership_has_inclusive_start_and_exclusive_end() {
    let mut f = Fixture::new(1);
    f.universe.members[0].membership.from = f.now;
    assert_eq!(f.screen().candidates.len(), 1);
    f.universe.members[0].membership.from = f.now + Duration::seconds(1);
    f.rejects(RejectionReason::MembershipNotEffective);
    f.universe.members[0].membership.from = f.now - Duration::days(30);
    f.universe.members[0].membership.until = Some(f.now);
    f.rejects(RejectionReason::MembershipNotEffective);
}

#[test]
fn screening_rejects_future_and_stale_universe_and_classification() {
    let cases: &[GateCase] = &[
        (
            |f| f.universe.as_of = f.now + Duration::seconds(1),
            RejectionReason::StaleOrFutureUniverse,
        ),
        (
            |f| f.universe.as_of = f.now - Duration::days(2),
            RejectionReason::StaleOrFutureUniverse,
        ),
        (
            |f| f.universe.available_at = f.now + Duration::seconds(1),
            RejectionReason::StaleOrFutureUniverse,
        ),
        (
            |f| f.universe.expires_at = f.now,
            RejectionReason::StaleOrFutureUniverse,
        ),
        (
            |f| f.universe.provenance.fetched_at = f.now + Duration::seconds(1),
            RejectionReason::StaleOrFutureUniverse,
        ),
        (
            |f| f.universe.members[0].classification.observed_at = f.now,
            RejectionReason::StaleOrFutureClassification,
        ),
        (
            |f| f.universe.members[0].classification.observed_at = f.now - Duration::days(2),
            RejectionReason::StaleOrFutureClassification,
        ),
        (
            |f| f.universe.members[0].classification.effective.from = f.now + Duration::seconds(1),
            RejectionReason::ClassificationNotEffective,
        ),
        (
            |f| f.universe.members[0].classification.industry_group_code = "1010".into(),
            RejectionReason::InvalidClassification,
        ),
    ];
    for (change, reason) in cases {
        let mut f = Fixture::new(1);
        change(&mut f);
        f.rejects(*reason);
    }
}

#[test]
fn screening_rejects_active_duplicate_classes_but_keeps_distinct_issuer_classes() {
    let mut f = Fixture::new(2);
    f.universe.members[1].issuer_id = f.universe.members[0].issuer_id.clone();
    assert_eq!(f.screen().candidates.len(), 2);
    f.universe.members.push(f.universe.members[0].clone());
    let result = f.screen();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(
        result
            .rejections
            .iter()
            .filter(|r| r.reasons.contains(&RejectionReason::DuplicateShareClass))
            .count(),
        2
    );
}

#[test]
fn screening_historical_classification_or_membership_rows_do_not_override_current_rows() {
    let mut f = Fixture::new(1);
    let mut historical = f.universe.members[0].clone();
    historical.membership.until = Some(f.now);
    f.universe.members.push(historical);
    let result = f.screen();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.rejections.len(), 1);
    assert!(
        result.rejections[0]
            .reasons
            .contains(&RejectionReason::MembershipNotEffective)
    );
}

#[test]
fn screening_intersects_owner_allowlist_and_exact_verified_broker_stock_identity() {
    let cases: &[GateCase] = &[
        (|f| f.owner.clear(), RejectionReason::OwnerNotPermitted),
        (
            |f| f.observations.broker_support.clear(),
            RejectionReason::MissingBrokerSupport,
        ),
        (
            |f| f.observations.broker_support[0].status = SupportStatus::Unknown,
            RejectionReason::BrokerStockNotVerified,
        ),
        (
            |f| f.observations.broker_support[0].status = SupportStatus::Unsupported,
            RejectionReason::BrokerStockNotVerified,
        ),
        (
            |f| f.observations.broker_support[0].instrument_kind = "ETF".into(),
            RejectionReason::BrokerStockNotVerified,
        ),
        (
            |f| f.observations.broker_support[0].evidence_id.clear(),
            RejectionReason::BrokerStockNotVerified,
        ),
        (
            |f| f.observations.broker_support[0].expires_at = f.now,
            RejectionReason::StaleOrFutureBrokerSupport,
        ),
        (
            |f| f.observations.broker_support[0].observed_at = f.now + Duration::seconds(1),
            RejectionReason::StaleOrFutureBrokerSupport,
        ),
        (
            |f| f.observations.broker_support[0].listing_mic = "XNYS".into(),
            RejectionReason::MissingBrokerSupport,
        ),
        (
            |f| f.observations.broker_support[0].currency = Currency::new("EUR").unwrap(),
            RejectionReason::MissingBrokerSupport,
        ),
        (
            |f| {
                f.observations
                    .broker_support
                    .push(f.observations.broker_support[0].clone())
            },
            RejectionReason::AmbiguousBrokerSupport,
        ),
    ];
    for (change, reason) in cases {
        let mut f = Fixture::new(1);
        change(&mut f);
        f.rejects(*reason);
    }
    let mut f = Fixture::new(2);
    f.observations.broker_support[1].instrument_id =
        f.observations.broker_support[0].instrument_id.clone();
    f.rejects(RejectionReason::AmbiguousBrokerSupport);
}

#[test]
fn screening_quote_liquidity_cost_affordability_and_timing_gates_fail_closed() {
    let cases: &[GateCase] = &[
        (
            |f| f.observations.markets.clear(),
            RejectionReason::MissingMarketData,
        ),
        (
            |f| {
                f.observations
                    .markets
                    .push(f.observations.markets[0].clone())
            },
            RejectionReason::AmbiguousMarketData,
        ),
        (
            |f| f.observations.markets[0].observed_at = f.now - Duration::seconds(61),
            RejectionReason::StaleOrFutureMarketData,
        ),
        (
            |f| f.observations.markets[0].observed_at = f.now + Duration::seconds(1),
            RejectionReason::StaleOrFutureMarketData,
        ),
        (
            |f| f.observations.markets[0].expires_at = f.now,
            RejectionReason::StaleOrFutureMarketData,
        ),
        (
            |f| f.observations.markets[0].bid = Decimal::ZERO,
            RejectionReason::InvalidQuote,
        ),
        (
            |f| f.observations.markets[0].ask = Decimal::ONE,
            RejectionReason::InvalidQuote,
        ),
        (
            |f| f.observations.markets[0].bid = Decimal::MAX,
            RejectionReason::InvalidQuote,
        ),
        (
            |f| f.observations.markets[0].ask = Decimal::from(102),
            RejectionReason::SpreadTooWide,
        ),
        (
            |f| f.observations.markets[0].average_daily_notional = Decimal::from(10),
            RejectionReason::InsufficientLiquidity,
        ),
        (
            |f| f.observations.markets[0].proposed_order_notional = Decimal::from(2_000),
            RejectionReason::InsufficientLiquidity,
        ),
        (
            |f| f.observations.markets[0].estimated_round_trip_cost_bps = Decimal::ZERO,
            RejectionReason::InvalidOrExcessiveCosts,
        ),
        (
            |f| f.observations.markets[0].estimated_round_trip_cost_bps = Decimal::from(51),
            RejectionReason::InvalidOrExcessiveCosts,
        ),
        (
            |f| f.observations.markets[0].proposed_order_notional = Decimal::ZERO,
            RejectionReason::UnaffordableOrder,
        ),
        (
            |f| f.observations.broker_support[0].minimum_order_notional = Decimal::from(101),
            RejectionReason::UnaffordableOrder,
        ),
        (
            |f| f.observations.broker_support[0].fractional_orders = false,
            RejectionReason::UnaffordableOrder,
        ),
        (
            |f| f.observations.markets[0].timing = None,
            RejectionReason::MissingTiming,
        ),
        (
            |f| {
                f.observations.markets[0]
                    .timing
                    .as_mut()
                    .unwrap()
                    .observed_at = f.now + Duration::seconds(1)
            },
            RejectionReason::StaleOrFutureTiming,
        ),
        (
            |f| {
                f.observations.markets[0]
                    .timing
                    .as_mut()
                    .unwrap()
                    .expires_at = f.now
            },
            RejectionReason::StaleOrFutureTiming,
        ),
        (
            |f| {
                f.observations.markets[0].timing.as_mut().unwrap().opens_at =
                    f.now + Duration::seconds(1)
            },
            RejectionReason::OutsideSession,
        ),
        (
            |f| {
                f.observations.markets[0].timing.as_mut().unwrap().closes_at =
                    f.now + Duration::minutes(30)
            },
            RejectionReason::OutsideSession,
        ),
        (
            |f| {
                f.observations.markets[0]
                    .timing
                    .as_mut()
                    .unwrap()
                    .event_window_clear = false
            },
            RejectionReason::EventTimingBlocked,
        ),
    ];
    for (change, reason) in cases {
        let mut f = Fixture::new(1);
        change(&mut f);
        f.rejects(*reason);
    }
}

#[test]
fn screening_priority_requires_known_bounded_components_and_current_evidence() {
    let cases: &[GateCase] = &[
        (
            |f| f.observations.priorities.clear(),
            RejectionReason::MissingPriority,
        ),
        (
            |f| {
                f.observations
                    .priorities
                    .push(f.observations.priorities[0].clone())
            },
            RejectionReason::AmbiguousPriority,
        ),
        (
            |f| f.observations.priorities[0].components[0] = None,
            RejectionReason::InvalidOrIncompletePriority,
        ),
        (
            |f| f.observations.priorities[0].components[0] = Some(5),
            RejectionReason::InvalidOrIncompletePriority,
        ),
        (
            |f| f.observations.priorities[0].evidence_ids.clear(),
            RejectionReason::InvalidOrIncompletePriority,
        ),
        (
            |f| f.observations.priorities[0].observed_at = f.now + Duration::seconds(1),
            RejectionReason::StaleOrFuturePriority,
        ),
        (
            |f| f.observations.priorities[0].observed_at = f.now - Duration::hours(2),
            RejectionReason::StaleOrFuturePriority,
        ),
        (
            |f| f.observations.priorities[0].material_exposure = false,
            RejectionReason::NoMaterialExposure,
        ),
    ];
    for (change, reason) in cases {
        let mut f = Fixture::new(1);
        change(&mut f);
        f.rejects(*reason);
    }
    let mut f = Fixture::new(1);
    f.observations.priorities[0].components = [Some(0); 5];
    assert_eq!(f.screen().candidates[0].priority_score, 0);
}

#[test]
fn screening_tie_ranking_is_stable_with_five_candidates_and_two_peer_controls() {
    let mut f = Fixture::new(10);
    let first = f.screen();
    assert_eq!(first.candidates.len(), 5);
    assert_eq!(first.peer_controls.len(), 2);
    assert_eq!(first.rejections.len(), 3);
    assert_eq!(
        first.finalist_instrument_id.as_ref().unwrap().as_str(),
        "STOCK_001"
    );
    assert_eq!(first.peer_controls[0].instrument_id.as_str(), "STOCK_006");
    assert_eq!(first.purpose, ScreeningPurpose::ResearchOnly);
    f.universe.members.reverse();
    f.observations.broker_support.reverse();
    f.observations.markets.reverse();
    f.observations.priorities.reverse();
    assert_eq!(first, f.screen());
}

#[test]
fn screening_prefers_score_then_coverage_then_execution_and_matches_peer_group() {
    let mut f = Fixture::new(9);
    f.policy.candidate_limit = 1;
    f.observations.priorities[1].components[0] = Some(4);
    f.universe.members[2].classification.industry_group_code = "4520".into();
    let result = f.screen();
    assert_eq!(result.candidates[0].instrument_id.as_str(), "STOCK_002");
    assert!(
        result
            .peer_controls
            .iter()
            .all(|c| c.industry_group_code == "4510")
    );
    assert_eq!(result.peer_controls[1].instrument_id.as_str(), "STOCK_004");
}

#[test]
fn screening_empty_and_partial_universes_and_policy_limits_are_explicit() {
    let f = Fixture::new(0);
    assert_eq!(f.screen().blockers, vec![RejectionReason::EmptyUniverse]);
    assert!(f.screen().finalist_instrument_id.is_none());
    let mut f = Fixture::new(1);
    f.universe.complete = false;
    f.rejects(RejectionReason::InvalidUniverse);
    f.universe.complete = true;
    f.universe.provenance.rights.store_content = false;
    f.rejects(RejectionReason::UnpermittedSourceStorage);
    f.policy.candidate_limit = 6;
    assert!(screen_universe(&f.universe, &f.owner, &f.observations, &f.policy, f.now).is_err());
    f.policy.candidate_limit = 5;
    f.policy.peer_limit = 3;
    assert!(f.policy.validate().is_err());
    f.policy.peer_limit = 0;
    let template = f.universe.members[0].clone();
    f.universe.members.resize(MAX_UNIVERSE_ROWS + 1, template);
    assert!(screen_universe(&f.universe, &f.owner, &f.observations, &f.policy, f.now).is_err());
}

#[test]
fn screening_isin_validation_rejects_bad_checksum_and_unicode_without_panicking() {
    assert!(valid_isin("US0378331005"));
    assert!(!valid_isin("US0378331006"));
    assert!(!valid_isin("éA000000000"));
    assert!(!valid_isin("us0378331005"));
    assert!(valid_gics_pair("60", "6020"));
    assert!(!valid_gics_pair("45", "6020"));
}

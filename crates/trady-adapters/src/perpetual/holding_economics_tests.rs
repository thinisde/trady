//! TEST ONLY: fitted native inference and final reviewed holding activation.
//! Synthetic evidence/approval fixtures establish no financial performance.
use super::*;
use std::collections::BTreeSet;
use trady_application::autonomous::*;
use trady_domain::{AccountId, Capabilities, Mode, Money, Quote};
use trady_policy::{
    EntryCosts, FxObservation, Policy, div,
    economics::{CostBasis, ExitCosts, target_price},
    holding::{HoldingContext, HoldingPlan, HoldingState, SessionWindow, TrailingSettings},
};

struct Fixture {
    config: TradingConfig,
    plan: ActivePlan,
    research: EntryCandidate,
    basis: CostBasis,
    quote: Quote,
    costs: ExitCosts,
    holding_context: HoldingContext,
    assessment_context: AssessmentContext,
}
fn fixture(with_fx: bool) -> Fixture {
    let at = now();
    let eur = Currency::new("EUR").unwrap();
    let currency = Currency::new(if with_fx { "USD" } else { "EUR" }).unwrap();
    let account = AccountId::new("TEST_ONLY_ACCOUNT").unwrap();
    let instrument = InstrumentId::new("US_TEST").unwrap();
    let skills = [
        "conversation-and-tools",
        "search-planning",
        "market-research",
        "news-analysis",
        "source-comparison",
        "entry-readiness",
        "sector-selection",
        "sector-test",
        "subsector-test",
        "company-profile",
        "relationship-analysis",
        "holding-watchdog",
        "perpetual-review",
    ];
    let config = TradingConfig {
        config_version: 1,
        account_id: account.clone(),
        mode: Mode::Paper,
        base_currency: eur.clone(),
        owner_id: "TEST_ONLY_OWNER".into(),
        policy: Policy::default(),
        capabilities: Capabilities::default(),
        approved_skills: skills
            .iter()
            .map(|s| (s.to_string(), "a".repeat(64)))
            .collect(),
        live_release: None,
        limits_authority: None,
    };
    let cash = |amount| Money {
        amount,
        currency: eur.clone(),
    };
    let fx = with_fx.then(|| FxObservation {
        from: currency.clone(),
        to: eur.clone(),
        rate: decimal("1.02"),
        observed_at: at,
    });
    let entry_costs = EntryCosts {
        fixed: cash(Decimal::ZERO),
        rate: Decimal::ZERO,
        uncertainty: cash(Decimal::ZERO),
        fx: fx.clone(),
    };
    let costs = ExitCosts {
        fixed: entry_costs.fixed.clone(),
        rate: entry_costs.rate,
        uncertainty: entry_costs.uncertainty.clone(),
        fx,
    };
    let records = [
        ("universe", "universe", None, false),
        ("screening_1", "screening", None, false),
        ("profile_1", "company_profile", None, false),
        ("relationships", "relationships", None, false),
        ("watchdog", "watchdog", None, false),
        ("economics", "economics", None, false),
        ("confidence", "confidence", None, false),
        ("primary", "source", Some("TEST_ONLY_ISSUER"), true),
        (
            "secondary_one",
            "source",
            Some("TEST_ONLY_OWNER_ONE"),
            false,
        ),
        (
            "secondary_two",
            "source",
            Some("TEST_ONLY_OWNER_TWO"),
            false,
        ),
    ]
    .into_iter()
    .map(|(id, kind, lineage, primary)| EvidenceRecord {
        id: id.into(),
        account_id: account.clone(),
        instrument_id: instrument.clone(),
        policy_version: 1,
        kind: kind.into(),
        revision_hash: "b".repeat(64),
        observed_at: at,
        valid_until: at + Duration::minutes(5),
        dependencies: vec![],
        source_lineage: lineage.map(String::from),
        primary_verified: primary,
        material_conflict: false,
    })
    .collect::<Vec<_>>();
    let mut research:EntryCandidate=serde_json::from_value(serde_json::json!({
        "proposal_id":"TEST_ONLY_RESEARCH", "account_id":account,"account_snapshot_id":"TEST_ONLY_ACCOUNT_SNAPSHOT",
        "policy_version":1,"instrument_id":instrument,"currency":currency,"created_at":at,"valid_until":at+Duration::minutes(5),
        "entry_range":{"minimum":"98","maximum":"103","currency":currency},"stop_price":"99","target_net_fraction":"0.02",
        "max_holding_sessions":3,"deadline_at":at+Duration::hours(1),"exceptional_verified":false,
        "confidence_inputs":{"primary_verified":true,"catalyst":"DIRECT","independent_lineages":2,
            "positive_15m_return":true,"above_vwap":true,"spread_fraction":"0.0025","round_trip_cost_fraction":"0.005",
            "catalyst_age_hours":1,"sources_current":true,"continuing_relevance":true},
        "entry_costs":entry_costs,"exit_costs":entry_costs,
        "instrument_rules":{"price_tick":"0.01","quantity_step":"0.01","minimum_quantity":"0.01","currency_unit":"0.01"},
        "dossier":{"id":"TEST_ONLY_FRESH_DOSSIER","finalized":true,"sector_skill":"sector-test","subsector_skill":"subsector-test",
            "records":records,"skills":[],"blockers":[]},
        "screening":{"universe_snapshot_id":"universe","screening_id":"screening_1","finalist":instrument,"observed_at":at,
            "valid_until":at+Duration::minutes(5)},
        "advisor":{"assessment_id":"TEST_ONLY_RECEIPT","model_revision_id":"TEST_ONLY_MODEL","scenario_hash":"a".repeat(64),
            "account_snapshot_id":"TEST_ONLY_ACCOUNT_SNAPSHOT","assessed_at":at,"valid_until":at+Duration::seconds(60),
            "assessed_bid":"100","assessed_ask":"100","quote_drift_tolerance":"0.001","usable":false,"calibration_ok":false,
            "drift_ok":false,"review_reason":"TEST ONLY: holding review uses the actual persisted native record"},
        "formula":null
    })).unwrap();
    research.dossier.skills = skills
        .iter()
        .map(|s| SkillEvidence {
            name: s.to_string(),
            content_hash: config.approved_skills[*s].clone(),
            artifact_ids: vec!["profile_1".into()],
        })
        .collect();
    let mut original = research.clone();
    original.proposal_id = "TEST_ONLY_POSITION".into();
    original.created_at = at - Duration::minutes(10);
    original.valid_until = at - Duration::minutes(5);
    original.target_net_fraction = decimal("0.01");
    original.confidence_inputs.positive_15m_return = false;
    original.confidence_inputs.above_vwap = false;
    assert_eq!(
        trady_policy::confidence(&original.confidence_inputs).total(),
        75
    );
    assert_eq!(
        trady_policy::confidence(&research.confidence_inputs).total(),
        85
    );
    let quantity = if with_fx { Decimal::ONE } else { decimal("2") };
    let basis = CostBasis {
        instrument_id: instrument.clone(),
        quantity,
        instrument_currency: currency.clone(),
        acquisition_cost: cash(if with_fx {
            decimal("100")
        } else {
            decimal("205.01")
        }),
    };
    let plan = ActivePlan {
        candidate: original,
        acquired_quantity: quantity,
        entry_session: "TEST_ONLY_SESSION".into(),
        entry_session_close: at + Duration::hours(2),
        entered_at: at - Duration::minutes(10),
        first_order: 0,
        holding_state: HoldingState::new(
            instrument.clone(),
            decimal("0.01"),
            HoldingPlan {
                version: 1,
                stop_price: decimal("99"),
                target_net_fraction: decimal("0.01"),
                deadline: at + Duration::hours(1),
                trailing: TrailingSettings::default(),
            },
            &config.policy,
        )
        .unwrap(),
        holding_sessions: BTreeSet::from(["TEST_ONLY_SESSION".into()]),
        exit_latch: None,
        market_exit_required: false,
        data_unusable_since: None,
    };
    let bid = if with_fx {
        decimal("99.01")
    } else {
        decimal("102.5")
    };
    let quote = Quote {
        instrument_id: instrument,
        currency,
        bid,
        ask: bid,
        observed_at: at,
        received_at: at,
        tradable: true,
    };
    let holding_context = HoldingContext {
        now: at,
        session: Some(SessionWindow {
            opens_at: at - Duration::hours(1),
            closes_at: at + Duration::hours(2),
        }),
        holding_session_number: Some(1),
        overnight_protection_verified: true,
        research_available: false,
        order_outcome_known: true,
        owner_close_authenticated: false,
    };
    let mut assessment_context = context(1.0);
    let fx = costs.fx.as_ref().map_or(Decimal::ONE, |f| f.rate);
    let binding = &mut assessment_context.scenario.binding;
    binding.arguments.position_id = Some(plan.candidate.proposal_id.clone());
    binding.arguments.scenario_calculation_id = "TEST_ONLY_HOLDING_CALCULATION".into();
    binding.currency = basis.instrument_currency.clone();
    binding.entry_price = div(
        div(basis.acquisition_cost.amount, basis.quantity).unwrap(),
        fx,
    )
    .unwrap();
    binding.stop_price = plan.holding_state.plan().stop_price;
    binding.target_price =
        target_price(&basis, decimal("0.02"), &costs, decimal("0.01"), at).unwrap();
    binding.estimated_cost_fraction =
        holding_advisory_cost_fraction(&basis, bid, &costs, at).unwrap();
    assessment_context.scenario.market.executable_price = bid;
    assessment_context.scenario.market.fx_rate = fx;
    Fixture {
        config,
        plan,
        research,
        basis,
        quote,
        costs,
        holding_context,
        assessment_context,
    }
}
fn reviewed_update(f: &Fixture) -> HoldingUpdate {
    let (bytes, _, _) = trained_fixture();
    let mut m = manifest(artifact_sha256(&bytes));
    m.model_revision_id = "TEST_ONLY_HOLDING_MODEL".into();
    m.support.supports_positions = true;
    m.support.minimum_stop_fraction = decimal("-0.02");
    let advisor = NativePerpetualAdvisor::from_approved_artifacts(
        vec![ApprovedArtifact { manifest: m, bytes }],
        now(),
    )
    .unwrap();
    let mut repo = Repository {
        context: Some(f.assessment_context.clone()),
        ..Repository::default()
    };
    let a = application::assess(
        &mut repo,
        &advisor,
        &f.assessment_context.scenario.binding.arguments,
        now(),
    )
    .unwrap();
    assert_eq!(a.status, AssessmentStatus::Ok);
    assert!(a.target_hit_probability.is_some());
    let review=PerpetualReview {assessment_id:a.assessment_id.clone(),influence:AssessmentInfluence::Used,
        reason:"TEST ONLY: reviewed fitted native advice, fresh independent research, and explicit holding terms".into()};
    application::record_review(&mut repo, &review, now()).unwrap();
    let mut proposed = f.plan.holding_state.plan().clone();
    proposed.version += 1;
    proposed.target_net_fraction = decimal("0.02");
    HoldingUpdate {
        id: "TEST_ONLY_HOLDING_UPDATE".into(),
        position_id: f.plan.candidate.proposal_id.clone(),
        expected_plan_version: 1,
        proposed,
        created_at: now(),
        valid_until: a.valid_until,
        research: f.research.clone(),
        assessment: repo.records.remove(&a.assessment_id).unwrap(),
        review,
    }
}
fn activate(f: &Fixture, update: &HoldingUpdate) -> Result<trady_policy::holding::PlanActivation> {
    activate_reviewed_holding_update(
        &f.config,
        &f.plan,
        update,
        &f.basis,
        &f.quote,
        &f.costs,
        &f.holding_context,
    )
}

// Synthetic references exercise only the restrictive policy overlay here. Real
// activation still requires the journal's exact persisted dependency check.
fn with_sentiment(f: &Fixture, update: &mut HoldingUpdate, negative: Decimal) {
    update.research.sentiment_reference=Some(serde_json::from_value(serde_json::json!({
        "assessment_id":"TEST_ONLY_SENTIMENT_ASSESSMENT", "assessment_hash":"c".repeat(64),
        "packet_id":"TEST_ONLY_VERIFIED_PACKET", "packet_hash":"d".repeat(64),
        "scope":{"owner_id":f.config.owner_id,"account_id":f.config.account_id.as_str(),"policy_version":f.config.policy.version},
        "instrument_id":f.plan.candidate.instrument_id.as_str(),"model_revision":"e".repeat(40),
        "maximum_negative_probability":negative,"scored_at":now(),"expires_at":now()+Duration::minutes(5),
        "dependency_hash":"f".repeat(64)
    })).unwrap());
}

#[test]
fn fitted_native_review_and_fresh_score_85_raise_an_original_score_75_target() {
    let f = fixture(false);
    let update = reviewed_update(&f);
    let result = activate(&f, &update).unwrap();
    assert!(result.activated, "{:?}", result.rejection);
    assert_eq!(
        result.evaluation.state.plan().target_net_fraction,
        decimal("0.02")
    );
    assert_eq!(result.evaluation.state.plan().version, 2);
    assert_eq!(
        f.plan.holding_state.plan().target_net_fraction,
        decimal("0.01")
    );
    let mut low_confidence = update.clone();
    low_confidence.research.confidence_inputs = f.plan.candidate.confidence_inputs.clone();
    let rejected = activate(&f, &low_confidence).unwrap();
    assert!(!rejected.activated);
    assert!(rejected.rejection.unwrap().contains("ceiling"));
    // Both nominal fractions round to the same price. Explicit fresh-confidence
    // validation must still reject the excessive nominal request.
    let mut nominal_overshoot = update;
    nominal_overshoot.proposed.target_net_fraction = decimal("0.020001");
    assert_eq!(
        target_price(
            &f.basis,
            nominal_overshoot.proposed.target_net_fraction,
            &f.costs,
            decimal("0.01"),
            now()
        )
        .unwrap(),
        nominal_overshoot
            .assessment
            .context
            .scenario
            .binding
            .target_price
    );
    let rejected = activate(&f, &nominal_overshoot).unwrap();
    assert!(!rejected.activated);
    assert!(rejected.rejection.unwrap().contains("ceiling"));
}

#[test]
fn profitable_fx_stop_requires_explicit_signed_native_support_before_activation() {
    let f = fixture(true);
    assert!(
        f.assessment_context.scenario.binding.entry_price
            < f.assessment_context.scenario.binding.stop_price
    );
    f.assessment_context.validate(now()).unwrap();
    let (bytes, _, _) = trained_fixture();
    let mut legacy = manifest(artifact_sha256(&bytes));
    legacy.support.supports_positions = true;
    legacy.validate(now()).unwrap();
    assert!(
        !legacy.supports(&f.assessment_context),
        "positive ranges must not use absolute distance"
    );
    let old_advisor = NativePerpetualAdvisor::from_approved_artifacts(
        vec![ApprovedArtifact {
            manifest: legacy.clone(),
            bytes,
        }],
        now(),
    )
    .unwrap();
    assert_eq!(
        old_advisor
            .assess("TEST_ONLY_UNSUPPORTED", &f.assessment_context, now())
            .unwrap()
            .assessment
            .status,
        AssessmentStatus::Unavailable
    );
    legacy.support.minimum_stop_fraction = decimal("-0.02");
    legacy.support.supports_positions = false;
    assert!(
        legacy.validate(now()).is_err(),
        "signed holding range requires explicit position support"
    );
    legacy.support.supports_positions = true;
    legacy.validate(now()).unwrap();
    assert!(legacy.supports(&f.assessment_context));
    let mut unsupported = f.assessment_context.clone();
    unsupported.scenario.binding.stop_price = decimal("99.9");
    legacy.support.minimum_stop_fraction = decimal("-0.01");
    assert!(
        !legacy.supports(&unsupported),
        "profitable stop exceeds declared trained range"
    );
    let mut as_entry = f.assessment_context.clone();
    as_entry.scenario.binding.arguments.position_id = None;
    assert!(!legacy.supports(&as_entry));
    let result = activate(&f, &reviewed_update(&f)).unwrap();
    assert!(result.activated, "{:?}", result.rejection);
    assert_eq!(
        result.evaluation.state.plan().target_net_fraction,
        decimal("0.02")
    );
}

#[test]
fn persisted_adverse_sentiment_blocks_target_raises_and_never_adds_confidence() {
    let f = fixture(false);
    let update = reviewed_update(&f);
    for negative in [decimal("0"), decimal("0.59"), decimal("0.599999")] {
        let mut checked = update.clone();
        with_sentiment(&f, &mut checked, negative);
        assert!(
            activate(&f, &checked).unwrap().activated,
            "non-adverse {negative}"
        );
        checked.research.confidence_inputs = f.plan.candidate.confidence_inputs.clone();
        assert!(
            !activate(&f, &checked).unwrap().activated,
            "positive sentiment cannot raise the base score"
        );
    }
    for negative in [decimal("0.60"), decimal("0.80"), Decimal::ONE] {
        let mut checked = update.clone();
        with_sentiment(&f, &mut checked, negative);
        let rejected = activate(&f, &checked).unwrap();
        assert!(!rejected.activated, "adverse {negative}");
        assert_eq!(
            rejected.evaluation.state.plan(),
            f.plan.holding_state.plan()
        );
        // At base score 100 the post-penalty ceiling still allows 2%. The
        // independent target-increase permission must veto the raise anyway.
        checked.research.confidence_inputs.positive_15m_return = true;
        checked.research.confidence_inputs.above_vwap = true;
        checked.research.confidence_inputs.relative_volume = Some(decimal("2"));
        checked.research.confidence_inputs.spread_fraction = Some(decimal("0.0001"));
        checked.research.confidence_inputs.round_trip_cost_fraction = Some(decimal("0.001"));
        assert_eq!(
            trady_policy::confidence(&checked.research.confidence_inputs).total(),
            100
        );
        let rejected = activate(&f, &checked).unwrap();
        assert!(
            !rejected.activated,
            "adverse permission must veto even with score 100"
        );
        assert!(rejected.rejection.unwrap().contains("permissions"));
    }
    let mut f = fixture(false);
    let mut active = f.plan.holding_state.plan().clone();
    active.target_net_fraction = decimal("0.02");
    f.plan.holding_state = HoldingState::new(
        f.basis.instrument_id.clone(),
        decimal("0.01"),
        active,
        &f.config.policy,
    )
    .unwrap();
    let mut checked = reviewed_update(&f);
    with_sentiment(&f, &mut checked, decimal("0.60"));
    // No increase is requested; 85 minus ten must independently enforce the
    // 75-point ceiling rather than relying only on the increase-permission veto.
    let rejected = activate(&f, &checked).unwrap();
    assert!(!rejected.activated);
    assert!(rejected.rejection.unwrap().contains("ceiling"));
    f.config.mode = Mode::Live;
    checked.research.sentiment_reference = None;
    assert!(
        activate(&f, &checked).is_err(),
        "live research requires a persisted sentiment reference"
    );
}

#[test]
fn reviewed_activation_rejects_changed_economics_native_identity_drift_and_missing_research() {
    let f = fixture(false);
    let update = reviewed_update(&f);
    for fault in [
        "quantity",
        "proportional_quantity_cost",
        "cost",
        "fees",
        "currency",
        "native_model",
        "missing_model",
        "retired_model",
        "drift",
        "quote_drift",
        "missing_research",
        "unfinalized_research",
        "stale_research",
    ] {
        let mut update = update.clone();
        let mut basis = f.basis.clone();
        let mut costs = f.costs.clone();
        let mut quote = f.quote.clone();
        match fault {
            "quantity" => basis.quantity += Decimal::ONE,
            "proportional_quantity_cost" => {
                basis.quantity *= decimal("2");
                basis.acquisition_cost.amount *= decimal("2");
            }
            "cost" => basis.acquisition_cost.amount += Decimal::ONE,
            "fees" => costs.fixed.amount = decimal("0.01"),
            "currency" => basis.acquisition_cost.currency = Currency::new("USD").unwrap(),
            "native_model" => {
                update.assessment.model.as_mut().unwrap().model_revision_id =
                    "TEST_ONLY_CHANGED_MODEL".into()
            }
            "missing_model" => update.assessment.model = None,
            "retired_model" => {
                update.assessment.model.as_mut().unwrap().approval = ModelApproval::Retired
            }
            "drift" => update.assessment.context.features.features[0].value = 100.0,
            "quote_drift" => {
                quote.bid += Decimal::ONE;
                quote.ask = quote.bid;
            }
            "missing_research" => update.research.dossier.records.clear(),
            "unfinalized_research" => update.research.dossier.finalized = false,
            "stale_research" => update.research.valid_until = now(),
            _ => unreachable!(),
        }
        let result = activate_reviewed_holding_update(
            &f.config,
            &f.plan,
            &update,
            &basis,
            &quote,
            &costs,
            &f.holding_context,
        );
        assert!(
            match result {
                Err(_) => true,
                Ok(r) => !r.activated,
            },
            "accepted {fault}"
        );
    }
    let f = fixture(true);
    let update = reviewed_update(&f);
    for fault in ["rate", "direction", "stale"] {
        let mut costs = f.costs.clone();
        let fx = costs.fx.as_mut().unwrap();
        match fault {
            "rate" => fx.rate = decimal("1.0201"),
            "direction" => std::mem::swap(&mut fx.from, &mut fx.to),
            "stale" => fx.observed_at = now() - Duration::seconds(61),
            _ => unreachable!(),
        }
        let result = activate_reviewed_holding_update(
            &f.config,
            &f.plan,
            &update,
            &f.basis,
            &f.quote,
            &costs,
            &f.holding_context,
        );
        assert!(
            match result {
                Err(_) => true,
                Ok(r) => !r.activated,
            },
            "accepted FX {fault}"
        );
    }
}

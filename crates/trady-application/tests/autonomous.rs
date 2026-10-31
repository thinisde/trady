//! Behavioral executor acceptance tests. All authority and observations are local fixtures;
//! no predictor, live broker, credentials, wall clock, or database is involved.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};
use trady_application::{
    Clock,
    autonomous::*,
    broker::*,
    optimization::{
        ApprovedFormula, FormulaAuthority, FormulaProposal, PredictionAuthority,
        ScenarioPredictionInput, calculate_authoritative_limits, digest,
    },
};
use trady_domain::*;
use trady_policy::{
    Catalyst, ConfidenceInputs, EntryCosts, EntryRange, InstrumentRules, Policy,
    limits::{FormulaEnvelope, FormulaEvidence, FormulaFamily, FormulaParameters, LimitsInput},
};

fn d(value: &str) -> Decimal {
    Decimal::from_str_exact(value).unwrap()
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
        .unwrap()
        .to_utc()
}

fn currency() -> Currency {
    Currency::new("EUR").unwrap()
}

fn money(amount: Decimal) -> Money {
    Money {
        amount,
        currency: currency(),
    }
}

fn instrument() -> InstrumentId {
    InstrumentId::new("AAPL_US_EQ").unwrap()
}

fn prediction_authority() -> PredictionAuthority {
    use trady_domain::prediction::PredictionSource::*;
    PredictionAuthority {
        fusion: trady_policy::prediction::FusionConfig {
            version: 1,
            revision_id: "fixture_fusion".into(),
            approved_models: [LlmResearch, FinBert, Perpetual, Quantitative]
                .into_iter()
                .map(|source| (source, format!("fixture_{source:?}")))
                .collect(),
            outcome_calibration_report_id: "fixture_outcome_calibration".into(),
            minimum_calibration_samples: 100,
            maximum_age_seconds: 300,
            maximum_uncertainty: d("0.01"),
        },
        utility: trady_policy::prediction::UtilityConfig {
            version: 1,
            lambda_risk: Decimal::ONE,
            lambda_uncertainty: Decimal::ONE,
            lambda_complexity: Decimal::ZERO,
            complexity: 1,
            cash_hurdle: Decimal::ZERO,
            approved_estimation_report_id: "fixture_payoff_report".into(),
            cost_model_hash: "c".repeat(64),
            comparison_deadline_at: now() + Duration::days(5),
        },
    }
}

fn predictions(
    calculation: &trady_application::optimization::BoundCalculation,
) -> ScenarioPredictionInput {
    use trady_domain::prediction::*;
    let authority = prediction_authority();
    let hash = calculation
        .calculation_id
        .strip_prefix("limits_")
        .unwrap()
        .to_string();
    let horizon = ForecastHorizon {
        decision_at: calculation.limits.calculated_at,
        deadline_at: calculation.limits.holding_deadline_at,
        exchange_sessions: calculation.limits.max_holding_sessions,
    };
    let forecasts = [
        PredictionSource::LlmResearch,
        PredictionSource::FinBert,
        PredictionSource::Perpetual,
        PredictionSource::Quantitative,
    ]
    .into_iter()
    .map(|source| {
        let (target, payload) = match source {
            PredictionSource::LlmResearch => (
                PredictionTarget::ResearchEvidence,
                PredictionPayload::Research {
                    subjective_confidence: Some(100),
                    material_conflict: false,
                },
            ),
            PredictionSource::FinBert => (
                PredictionTarget::FinancialTextSentiment,
                PredictionPayload::Sentiment {
                    positive: d("0.8"),
                    negative: d("0.1"),
                    neutral: d("0.1"),
                },
            ),
            PredictionSource::Perpetual => (
                PredictionTarget::ScenarioOutcomes,
                PredictionPayload::ScenarioOutcomes {
                    probabilities: OutcomeProbabilities {
                        target_first: d("0.8"),
                        stop_first: d("0.1"),
                        expiry: d("0.05"),
                        non_fill: d("0.05"),
                    },
                },
            ),
            PredictionSource::Quantitative => (
                PredictionTarget::MarketIndicators,
                PredictionPayload::Quantitative {
                    indicators: QuantitativeIndicators {
                        momentum_return: Some(d("0.01")),
                        atr: Some(d("0.5")),
                        realized_volatility: Some(d("0.02")),
                        relative_volume: Some(d("1.5")),
                        vwap_distance_fraction: Some(d("0.001")),
                        relative_strength: Some(d("0.01")),
                        spread_fraction: Some(d("0.0001")),
                    },
                },
            ),
        };
        Prediction {
            metadata: PredictionMetadata {
                schema_version: 1,
                prediction_id: format!("fixture_prediction_{source:?}"),
                source,
                model_version: authority.fusion.approved_models[&source].clone(),
                instrument_id: instrument(),
                market_snapshot_id: calculation.limits.market_snapshot_id.clone(),
                scenario_hash: Some(hash.clone()),
                observed_at: now(),
                available_at: now(),
                generated_at: now(),
                valid_until: now() + Duration::minutes(5),
                horizon: horizon.clone(),
                target,
                target_definition: "Synthetic acceptance fixture only".into(),
                label_version: "fixture_labels".into(),
                uncertainty: Some(PredictionUncertainty {
                    unit: UncertaintyUnit::NetAccountReturn,
                    radius: d("0.001"),
                }),
                calibration: CalibrationRecord {
                    status: CalibrationStatus::Validated,
                    calibrator_version: Some("fixture_calibrator".into()),
                    report_id: Some(authority.fusion.outcome_calibration_report_id.clone()),
                    training_cutoff_at: Some(now() - Duration::days(30)),
                    validation_start_at: Some(now() - Duration::days(29)),
                    validation_end_at: Some(now() - Duration::days(1)),
                    sample_count: 100,
                    out_of_sample: true,
                    supported_regimes: vec!["fixture_regime".into()],
                },
                regime: "fixture_regime".into(),
                dependence_group_id: format!("fixture_group_{source:?}"),
                evidence_lineages: vec![format!("fixture_lineage_{source:?}")],
            },
            payload,
        }
    })
    .collect();
    ScenarioPredictionInput {
        context: trady_policy::prediction::FusionContext {
            instrument_id: instrument(),
            market_snapshot_id: calculation.limits.market_snapshot_id.clone(),
            scenario_hash: hash.clone(),
            horizon,
            regime: "fixture_regime".into(),
        },
        predictions: forecasts,
        outcomes: trady_policy::prediction::OutcomeEstimates {
            scenario_hash: hash,
            forecast_prediction_id: "fixture_prediction_Perpetual".into(),
            estimation_report_id: authority.utility.approved_estimation_report_id,
            cost_model_hash: authority.utility.cost_model_hash,
            available_at: now(),
            normalization: trady_policy::prediction::ReturnNormalization {
                account_id: AccountId::new("paper_autonomous_test").unwrap(),
                account_snapshot_id: calculation.limits.account_snapshot_id.clone(),
                opening_equity: money(d("1000")),
            },
            net_account_returns: [d("0.02"), d("-0.01"), Decimal::ZERO, Decimal::ZERO],
        },
    }
}

#[derive(Clone)]
struct TestClock(Rc<Cell<DateTime<Utc>>>);

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.get()
    }
}

#[derive(Default)]
struct Durable {
    state: TradingState,
    fence: Option<Fence>,
    generation: i64,
    saves: usize,
    renewals: usize,
    fail_save_at: Option<usize>,
    fail_renew_at: Option<usize>,
    committed: Vec<TradingState>,
}

struct Journal(Rc<RefCell<Durable>>);

impl Journal {
    fn check(&self, fence: &Fence) -> Result<()> {
        require(
            self.0.borrow().fence.as_ref() == Some(fence),
            "test fence lost",
        )
    }
}

impl ExecutionJournal for Journal {
    fn acquire(&mut self, config: &TradingConfig, worker: &WorkerId, _: i32) -> Result<Fence> {
        let mut durable = self.0.borrow_mut();
        require(
            durable.fence.is_none(),
            "another executor owns the test lease",
        )?;
        durable.generation += 1;
        let fence = Fence {
            account_id: config.account_id.clone(),
            worker_id: worker.clone(),
            generation: durable.generation,
        };
        durable.fence = Some(fence.clone());
        Ok(fence)
    }

    fn renew(&mut self, fence: &Fence, _: i32) -> Result<()> {
        self.check(fence)?;
        let mut durable = self.0.borrow_mut();
        durable.renewals += 1;
        if durable.fail_renew_at == Some(durable.renewals) {
            durable.fence = None;
            return Err(Error("test fence lost before broker mutation".into()));
        }
        Ok(())
    }

    fn load(&mut self, fence: &Fence) -> Result<TradingState> {
        self.check(fence)?;
        Ok(self.0.borrow().state.clone())
    }

    fn save(&mut self, fence: &Fence, expected_revision: u64, state: &TradingState) -> Result<()> {
        self.check(fence)?;
        let mut durable = self.0.borrow_mut();
        durable.saves += 1;
        if durable.fail_save_at == Some(durable.saves) {
            return Err(Error("injected fenced compare-and-swap failure".into()));
        }
        require(
            durable.state.revision == expected_revision,
            "test journal revision conflict",
        )?;
        assert_eq!(state.revision, expected_revision + 1);
        durable.state = state.clone();
        durable.committed.push(state.clone());
        Ok(())
    }

    fn release(&mut self, fence: &Fence) -> Result<()> {
        self.check(fence)?;
        self.0.borrow_mut().fence = None;
        Ok(())
    }
}

#[derive(Debug)]
enum Mutation {
    Submit(BrokerOrderRequest),
    Cancel(String),
}

enum Ack {
    Open,
    Reject,
    Unknown(Option<String>),
}

struct Broker {
    durable: Rc<RefCell<Durable>>,
    observation: ExecutionObservation,
    next_observations: VecDeque<ExecutionObservation>,
    mutations: Vec<Mutation>,
    acknowledgments: VecDeque<Ack>,
    observe_count: usize,
    cancel_failure: Option<BrokerFailure>,
}

impl Broker {
    fn submitted(&self) -> Vec<&BrokerOrderRequest> {
        self.mutations
            .iter()
            .filter_map(|m| match m {
                Mutation::Submit(request) => Some(request),
                Mutation::Cancel(_) => None,
            })
            .collect()
    }

    fn cancelled(&self) -> Vec<&str> {
        self.mutations
            .iter()
            .filter_map(|m| match m {
                Mutation::Cancel(id) => Some(id.as_str()),
                Mutation::Submit(_) => None,
            })
            .collect()
    }

    fn set_order(&mut self, id: &str, filled: Decimal, status: OrderStatus) {
        let row = self
            .observation
            .snapshot
            .orders
            .iter_mut()
            .find(|o| o.broker_order_id == id)
            .unwrap();
        row.filled_quantity = filled;
        row.status = status;
        self.revalue_position();
    }

    fn revalue_position(&mut self) {
        let orders = &self.observation.snapshot.orders;
        let bought: Decimal = orders
            .iter()
            .filter(|o| o.side == Side::Buy)
            .map(|o| o.filled_quantity)
            .sum();
        let sold: Decimal = orders
            .iter()
            .filter(|o| o.side == Side::Sell)
            .map(|o| o.filled_quantity)
            .sum();
        let held = bought - sold;
        assert!(held >= Decimal::ZERO, "fixture cannot oversell");
        self.observation.snapshot.positions = if held == Decimal::ZERO {
            vec![]
        } else {
            vec![Position {
                instrument_id: instrument(),
                quantity: held,
            }]
        };
        let reserved: Decimal = orders
            .iter()
            .filter(|o| o.side == Side::Sell && !o.status.terminal())
            .map(|o| o.quantity - o.filled_quantity)
            .sum();
        self.observation
            .available_quantities
            .insert(instrument(), (held - reserved).max(Decimal::ZERO));
        self.observation
            .position_costs
            .insert(instrument(), money(held * d("100.10")));
    }
}

impl BrokerExecutionPort for Broker {
    fn submit_order(&mut self, request: &BrokerOrderRequest) -> BrokerResult<BrokerOrder> {
        // These assertions run at the transport boundary, before every test send.
        // Checking only the state after cycle() would miss a write-after-send bug.
        let durable = self.durable.borrow();
        assert!(durable.fence.is_some(), "send requires a current fence");
        let tracked = durable
            .state
            .orders
            .iter()
            .find(|o| o.request.intent_id == request.intent_id)
            .expect("intent must be durably reserved before send");
        assert_eq!(&tracked.request, request);
        assert!(
            tracked.attempted,
            "attempted=true must be durable before send"
        );
        assert!(tracked.observation.is_none());
        assert_eq!(
            self.submitted()
                .iter()
                .filter(|r| r.intent_id == request.intent_id)
                .count(),
            0
        );
        if tracked.role == OrderRole::Entry {
            let plan = durable.state.plan.as_ref().unwrap();
            assert!(
                durable
                    .state
                    .consumed_sessions
                    .contains(&plan.entry_session)
            );
        }
        if tracked.role == OrderRole::Exit {
            assert!(durable.state.plan.as_ref().unwrap().exit_latch.is_some());
            assert!(
                durable
                    .state
                    .orders
                    .iter()
                    .filter(|o| matches!(o.role, OrderRole::Entry | OrderRole::Protection))
                    .all(|o| o.observation.as_ref().is_some_and(|r| r.status.terminal())),
                "an exit cannot race a live entry or protective order"
            );
            let held = self
                .observation
                .snapshot
                .positions
                .first()
                .unwrap()
                .quantity;
            assert!(
                request.quantity <= held,
                "exit must not oversell after a cancel/fill race"
            );
        }
        drop(durable);
        self.mutations.push(Mutation::Submit(request.clone()));
        let id = self.mutations.len().to_string();
        match self.acknowledgments.pop_front().unwrap_or(Ack::Open) {
            Ack::Reject => Err(BrokerFailure::new(BrokerFailureKind::Rejected)),
            Ack::Unknown(broker_order_id) => {
                let mut failure = BrokerFailure::new(BrokerFailureKind::Transport);
                failure.outcome_unknown = true;
                failure.broker_order_id = broker_order_id;
                Err(failure)
            }
            Ack::Open => {
                let row = BrokerOrder {
                    broker_order_id: id,
                    client_intent_id: Some(request.intent_id.clone()),
                    instrument_id: request.instrument_id.clone(),
                    side: request.side,
                    quantity: request.quantity,
                    filled_quantity: Decimal::ZERO,
                    status: OrderStatus::Open,
                };
                self.observation.snapshot.orders.push(row.clone());
                self.revalue_position();
                Ok(row)
            }
        }
    }

    fn cancel_order(
        &mut self,
        account: &AccountId,
        broker_order_id: &str,
    ) -> BrokerResult<CancellationReceipt> {
        let durable = self.durable.borrow();
        assert_eq!(account, &durable.fence.as_ref().unwrap().account_id);
        let tracked = durable
            .state
            .orders
            .iter()
            .find(|o| o.broker_id() == Some(broker_order_id))
            .unwrap();
        assert!(
            tracked.cancellation_requested,
            "cancel intent must be durable before send"
        );
        assert!(
            self.cancelled().iter().all(|id| *id != broker_order_id)
                || tracked.failure.as_deref() == Some("broker RateLimited"),
            "only a definite fixture no-send may be retried after reconciliation"
        );
        drop(durable);
        self.mutations
            .push(Mutation::Cancel(broker_order_id.into()));
        if let Some(error) = self.cancel_failure.take() {
            return Err(error);
        }
        // An acknowledgment deliberately does not make the order terminal or free stock.
        Ok(CancellationReceipt {
            broker_order_id: broker_order_id.into(),
        })
    }
}

impl ExecutionBroker for Broker {
    fn observe(&mut self, account: &AccountId, _: &[TrackedOrder]) -> Result<ExecutionObservation> {
        assert_eq!(account, &self.observation.snapshot.account_id);
        self.observe_count += 1;
        Ok(self
            .next_observations
            .pop_front()
            .unwrap_or_else(|| self.observation.clone()))
    }
}

struct Inputs {
    candidate: EntryCandidate,
    session: Option<MarketSession>,
    quote: Quote,
    quote_unavailable: bool,
    candidate_reads: usize,
}

impl TradingInputs for Inputs {
    fn session(&mut self, _: &AccountId, _: DateTime<Utc>) -> Result<Option<MarketSession>> {
        Ok(self.session.clone())
    }

    fn quote(&mut self, _: &InstrumentId) -> Result<Quote> {
        if self.quote_unavailable {
            Err(Error("test quote provider unavailable".into()))
        } else {
            Ok(self.quote.clone())
        }
    }

    fn candidate(
        &mut self,
        _: &AccountSnapshot,
        _: DateTime<Utc>,
    ) -> Result<Option<EntryCandidate>> {
        self.candidate_reads += 1;
        Ok(Some(self.candidate.clone()))
    }
}

const SKILLS: [&str; 13] = [
    "conversation-and-tools",
    "search-planning",
    "market-research",
    "news-analysis",
    "source-comparison",
    "entry-readiness",
    "sector-selection",
    "sector-technology",
    "subsector-software",
    "company-profile",
    "relationship-analysis",
    "holding-watchdog",
    "perpetual-review",
];

fn config() -> TradingConfig {
    let policy = Policy::default();
    let parameters = FormulaParameters {
        version: 1,
        family: FormulaFamily::Trend,
        entry_allowance_fraction: d("0.001"),
        stop_atr_multiple: d("2"),
        target_net_fraction: d("0.03"),
        max_holding_sessions: 3,
        atr_lookback: 14,
        bar_seconds: 60,
    };
    let authority = FormulaAuthority {
        prediction: Some(prediction_authority()),
        envelope: FormulaEnvelope {
            max_entry_allowance_fraction: d("0.005"),
            min_stop_atr_multiple: d("1"),
            max_stop_atr_multiple: d("4"),
            min_target_net_fraction: d("0.01"),
            max_target_net_fraction: d("0.03"),
        },
        max_account_risk_fraction: d("0.01"),
        active: ApprovedFormula {
            revision_id: "approved_formula_week_44".into(),
            formula_hash: digest(&parameters).unwrap(),
            policy_hash: digest(&policy).unwrap(),
            parameters,
            approved_at: now() - Duration::days(1),
            effective_from: now() - Duration::hours(1),
            valid_until: now() + Duration::days(7),
            proof_report_id: "proof_approved".into(),
            parity_report_id: "rust_lean_parity_approved".into(),
            evaluation_report_id: "evaluation_approved".into(),
            shadow_report_id: "shadow_approved".into(),
            owner_approval_id: "owner_release_approved".into(),
        },
    };
    authority.validate_active(&policy, now()).unwrap();
    let capability = Capability {
        support: Support::Verified,
        evidence_id: Some("deployment_capability_check".into()),
        valid_until: Some(now() + Duration::days(1)),
    };
    TradingConfig {
        config_version: 1,
        account_id: AccountId::new("paper_autonomous_test").unwrap(),
        mode: Mode::Paper,
        base_currency: currency(),
        owner_id: "test_owner".into(),
        policy,
        capabilities: Capabilities {
            cash_account: capability.clone(),
            instrument: capability.clone(),
            fractional_orders: capability.clone(),
            protective_orders: capability.clone(),
            quotes: capability,
        },
        approved_skills: SKILLS
            .iter()
            .map(|name| (name.to_string(), "a".repeat(64)))
            .collect(),
        live_release: None,
        limits_authority: Some(authority),
    }
}

fn candidate(config: &TradingConfig, account: &AccountSnapshot, quote: &Quote) -> EntryCandidate {
    let costs = EntryCosts {
        fixed: money(Decimal::ZERO),
        rate: Decimal::ZERO,
        uncertainty: money(Decimal::ZERO),
        fx: None,
    };
    let rules = InstrumentRules {
        price_tick: d("0.01"),
        quantity_step: d("0.01"),
        minimum_quantity: d("0.01"),
        currency_unit: d("0.01"),
    };
    let confidence_inputs = ConfidenceInputs {
        primary_verified: true,
        catalyst: Catalyst::Direct,
        independent_lineages: 2,
        positive_15m_return: true,
        above_vwap: true,
        relative_volume: Some(d("2")),
        spread_fraction: Some(d("0.0001")),
        round_trip_cost_fraction: Some(d("0.001")),
        catalyst_age_hours: Some(1),
        sources_current: true,
        continuing_relevance: true,
    };
    assert_eq!(trady_policy::confidence(&confidence_inputs).total(), 100);
    let range = EntryRange {
        minimum: d("99"),
        maximum: d("101"),
        currency: currency(),
    };
    let input = LimitsInput {
        account: account.clone(),
        market_snapshot_id: "market_snapshot_initial".into(),
        quote: quote.clone(),
        entry_range: range.clone(),
        entry_costs: costs.clone(),
        exit_costs: trady_policy::economics::ExitCosts {
            fixed: costs.fixed.clone(),
            rate: costs.rate,
            uncertainty: costs.uncertainty.clone(),
            fx: None,
        },
        rules: rules.clone(),
        evidence: FormulaEvidence {
            snapshot_id: "completed_volatility_bars".into(),
            family: FormulaFamily::Trend,
            signal_eligible: true,
            atr: d("0.50"),
            atr_lookback: 14,
            bar_seconds: 60,
            completed_bars: 30,
            last_completed_at: now() - Duration::seconds(45),
            available_at: now() - Duration::seconds(30),
            valid_until: now() + Duration::minutes(5),
            minimum_stop_distance: d("0.01"),
            noise_distance: d("0.10"),
            confidence_score: 100,
            base_confidence_score: 100,
            sentiment_entry_allowed: true,
            sentiment_target_ceiling: d("0.03"),
            exceptional_verified: false,
        },
        holding_deadline_at: now() + Duration::days(3),
        valid_until: now() + Duration::minutes(5),
    };
    let calculation = calculate_authoritative_limits(
        config.limits_authority.as_ref().unwrap(),
        &config.policy,
        &input,
        now(),
    )
    .unwrap();
    let records = [
        ("universe", "universe", None, false),
        ("screening", "screening", None, false),
        ("profile", "company_profile", None, false),
        ("relationships", "relationships", None, false),
        ("watchdog", "watchdog", None, false),
        ("economics", "economics", None, false),
        ("confidence", "confidence", None, false),
        ("issuer_release", "source", Some("issuer"), true),
        ("independent_one", "source", Some("independent_one"), false),
        ("independent_two", "source", Some("independent_two"), false),
    ]
    .into_iter()
    .map(|(id, kind, lineage, primary_verified)| EvidenceRecord {
        id: id.into(),
        account_id: config.account_id.clone(),
        instrument_id: instrument(),
        policy_version: config.policy.version,
        kind: kind.into(),
        revision_hash: "b".repeat(64),
        observed_at: now(),
        valid_until: now() + Duration::minutes(5),
        dependencies: vec![],
        source_lineage: lineage.map(String::from),
        primary_verified,
        material_conflict: false,
    })
    .collect();
    let mut candidate = EntryCandidate {
        proposal_id: "final_proposal".into(),
        account_id: config.account_id.clone(),
        account_snapshot_id: account.snapshot_id.clone(),
        policy_version: config.policy.version,
        instrument_id: instrument(),
        currency: currency(),
        created_at: now(),
        valid_until: now() + Duration::minutes(5),
        entry_range: range,
        stop_price: calculation.limits.stop_trigger,
        target_net_fraction: calculation.limits.target_net_fraction,
        max_holding_sessions: calculation.limits.max_holding_sessions,
        deadline_at: now() + Duration::days(3),
        exceptional_verified: false,
        confidence_inputs,
        entry_costs: costs.clone(),
        exit_costs: costs,
        instrument_rules: rules,
        dossier: Dossier {
            id: "final_dossier".into(),
            finalized: true,
            sector_skill: "sector-technology".into(),
            subsector_skill: "subsector-software".into(),
            records,
            skills: SKILLS
                .iter()
                .map(|name| SkillEvidence {
                    name: name.to_string(),
                    content_hash: config.approved_skills[*name].clone(),
                    artifact_ids: vec!["profile".into()],
                })
                .collect(),
            blockers: vec![],
        },
        screening: ScreeningReceipt {
            universe_snapshot_id: "universe".into(),
            screening_id: "screening".into(),
            finalist: instrument(),
            observed_at: now(),
            valid_until: now() + Duration::minutes(5),
        },
        advisor: AdvisorReceipt {
            assessment_id: "persisted_native_assessment".into(),
            model_revision_id: "approved_model_revision".into(),
            scenario_hash: String::new(),
            account_snapshot_id: account.snapshot_id.clone(),
            assessed_at: now(),
            valid_until: now() + Duration::minutes(5),
            assessed_bid: quote.bid,
            assessed_ask: quote.ask,
            quote_drift_tolerance: d("0.001"),
            usable: true,
            calibration_ok: true,
            drift_ok: true,
            review_reason: "Checked the final scenario with usable current advisory evidence."
                .into(),
        },
        formula: Some(FormulaProposal {
            predictions: Some(predictions(&calculation)),
            input,
            calculation,
        }),
        sentiment_reference: None,
    };
    candidate.advisor.scenario_hash = candidate.scenario_hash().unwrap();
    candidate.validate(config, account, quote, now()).unwrap();
    candidate
}

struct Harness {
    config: TradingConfig,
    clock: TestClock,
    journal: Journal,
    broker: Broker,
    inputs: Inputs,
    trader: AutonomousTrader,
}

impl Harness {
    fn new() -> Self {
        let config = config();
        let snapshot = AccountSnapshot {
            snapshot_id: "account_initial".into(),
            account_id: config.account_id.clone(),
            mode: Mode::Paper,
            spendable_cash: money(d("1000")),
            positions: vec![],
            orders: vec![],
            observed_at: now(),
        };
        let quote = Quote {
            instrument_id: instrument(),
            currency: currency(),
            bid: d("99.99"),
            ask: d("100"),
            observed_at: now(),
            received_at: now(),
            tradable: true,
        };
        let candidate = candidate(&config, &snapshot, &quote);
        let durable = Rc::new(RefCell::new(Durable::default()));
        let mut journal = Journal(durable.clone());
        let clock = TestClock(Rc::new(Cell::new(now())));
        let trader = AutonomousTrader::start(
            config.clone(),
            WorkerId::new("test_worker").unwrap(),
            &mut journal,
            &clock,
        )
        .unwrap();
        let broker = Broker {
            durable,
            observation: ExecutionObservation {
                snapshot,
                position_costs: BTreeMap::new(),
                available_quantities: BTreeMap::new(),
                total_equity: money(d("1000")),
            },
            next_observations: VecDeque::new(),
            mutations: vec![],
            acknowledgments: VecDeque::new(),
            observe_count: 0,
            cancel_failure: None,
        };
        let inputs = Inputs {
            candidate,
            session: Some(MarketSession {
                session_id: "XNYS_2026_10_30".into(),
                opens_at: now() - Duration::minutes(30),
                closes_at: now() + Duration::hours(6),
                observed_at: now(),
                opening_equity: money(d("1000")),
                deposits: money(Decimal::ZERO),
                withdrawals: money(Decimal::ZERO),
            }),
            quote,
            quote_unavailable: false,
            candidate_reads: 0,
        };
        Self {
            config,
            clock,
            journal,
            broker,
            inputs,
            trader,
        }
    }

    fn run(&mut self, controls: OwnerControls) -> Result<CycleReport> {
        self.trader.cycle(
            &mut self.journal,
            &mut self.broker,
            &mut self.inputs,
            &controls,
            &self.clock,
        )
    }

    fn cycle(&mut self) -> CycleReport {
        self.run(OwnerControls::default()).unwrap()
    }

    fn close(&mut self) -> CycleReport {
        self.run(OwnerControls {
            pause_entries: false,
            close_position: true,
        })
        .unwrap()
    }

    fn state(&self) -> TradingState {
        self.journal.0.borrow().state.clone()
    }

    fn quantity(&self) -> Decimal {
        self.inputs
            .candidate
            .formula
            .as_ref()
            .unwrap()
            .calculation
            .limits
            .quantity
    }

    fn entry(&mut self) {
        assert_eq!(self.cycle().action, "entry_submitted");
        assert_eq!(self.broker.submitted().len(), 1);
    }

    fn filled_entry(&mut self) {
        self.entry();
        self.broker
            .set_order("1", self.quantity(), OrderStatus::Filled);
    }

    fn protected(&mut self) {
        self.filled_entry();
        assert_eq!(self.cycle().action, "protection_submitted");
        assert_eq!(self.cycle().state, PositionState::OpenProtected);
    }

    fn restart(&mut self) {
        self.journal.release(&self.trader.fence).unwrap();
        self.trader = AutonomousTrader::start(
            self.config.clone(),
            WorkerId::new("test_successor").unwrap(),
            &mut self.journal,
            &self.clock,
        )
        .unwrap();
    }

    fn advance(&mut self, seconds: i64) {
        let next = self.clock.now() + Duration::seconds(seconds);
        self.clock.0.set(next);
        self.broker.observation.snapshot.observed_at = next;
        self.inputs.quote.observed_at = next;
        self.inputs.quote.received_at = next;
        if let Some(session) = &mut self.inputs.session {
            session.observed_at = next;
        }
    }
}

#[test]
fn approved_formula_reaches_submit_with_attempt_and_daily_slot_already_durable() {
    let mut h = Harness::new();
    h.entry();
    let request = h.broker.submitted()[0];
    let limits = &h
        .inputs
        .candidate
        .formula
        .as_ref()
        .unwrap()
        .calculation
        .limits;
    assert_eq!(request.quantity, limits.quantity);
    assert_eq!(
        request.kind,
        OrderKind::Limit {
            price: limits.buy_limit,
            time_in_force: TimeInForce::Day
        }
    );
    assert!(
        limits.buy_limit > h.inputs.quote.ask,
        "fixture distinguishes formula limit from raw ask"
    );
    assert_eq!(h.state().position_state, PositionState::EntryPending);
    assert_eq!(
        h.broker.observe_count, 2,
        "account must be observed again after research"
    );
    let durable = h.journal.0.borrow();
    assert!(
        durable
            .committed
            .iter()
            .any(|s| s.orders.last().is_some_and(|o| !o.attempted))
    );
    assert!(durable.committed.iter().any(|s| {
        s.orders
            .last()
            .is_some_and(|o| o.attempted && o.observation.is_none())
    }));
}

#[test]
fn dependency_cycles_and_close_requests_block_new_entries() {
    let mut h = Harness::new();
    h.inputs.candidate.dossier.records[2].dependencies = vec!["relationships".into()];
    h.inputs.candidate.dossier.records[3].dependencies = vec!["profile".into()];
    assert!(h.run(OwnerControls::default()).is_err());
    assert!(h.broker.mutations.is_empty());
    let mut h = Harness::new();
    assert_eq!(h.close().action, "entry_paused");
    assert!(h.broker.mutations.is_empty());
}

#[test]
fn refreshed_fx_cannot_reuse_advice_after_rate_fee_or_coverage_change() {
    let mut h = Harness::new();
    let candidate = &mut h.inputs.candidate;
    candidate.exit_costs.fx = Some(trady_policy::FxObservation {
        from: Currency::new("USD").unwrap(),
        to: currency(),
        rate: d("0.95"),
        observed_at: now() - Duration::seconds(1),
    });
    let mut current = candidate.exit_costs.clone();
    current.fx.as_mut().unwrap().observed_at = now();
    candidate
        .validate_refreshed_exit_costs(&current, now())
        .unwrap();
    current.fx.as_mut().unwrap().rate = d("0.9499");
    assert!(
        candidate
            .validate_refreshed_exit_costs(&current, now())
            .is_err()
    );
    current = candidate.exit_costs.clone();
    current.fx.as_mut().unwrap().observed_at = now() - Duration::seconds(61);
    assert!(
        candidate
            .validate_refreshed_exit_costs(&current, now())
            .is_err()
    );
    current = candidate.exit_costs.clone();
    current.fixed.amount += Decimal::ONE;
    assert!(
        candidate
            .validate_refreshed_exit_costs(&current, now())
            .is_err()
    );
    current.fx = None;
    assert!(
        candidate
            .validate_refreshed_exit_costs(&current, now())
            .is_err()
    );
}

#[test]
fn definitive_cancellation_no_send_can_retry_after_reconciliation() {
    let mut h = Harness::new();
    h.protected();
    h.broker.cancel_failure = Some(BrokerFailure::new(BrokerFailureKind::RateLimited));
    h.close();
    assert!(!h.state().orders[1].cancellation_requested);
    h.broker.cancel_failure = None;
    h.cycle();
    assert_eq!(h.broker.cancelled(), vec!["2", "2"]);
    assert_eq!(h.broker.submitted().len(), 2);
}

#[test]
fn target_limit_reversal_cancels_before_irreversible_market_liquidation() {
    let mut h = Harness::new();
    h.protected();
    let target = h
        .inputs
        .candidate
        .formula
        .as_ref()
        .unwrap()
        .calculation
        .limits
        .target_sell_limit;
    h.advance(1);
    h.inputs.quote.bid = target;
    h.inputs.quote.ask = target + d("0.01");
    h.cycle();
    h.broker
        .set_order("2", Decimal::ZERO, OrderStatus::Cancelled);
    h.cycle();
    let exit = h
        .state()
        .orders
        .last()
        .unwrap()
        .broker_id()
        .unwrap()
        .to_string();
    assert!(matches!(
        h.broker.submitted()[2].kind,
        OrderKind::Limit { .. }
    ));
    h.advance(1);
    h.inputs.quote.bid = d("100");
    h.inputs.quote.ask = d("100.01");
    assert_eq!(h.cycle().action, "cancel_target_before_market_exit");
    assert_eq!(h.broker.submitted().len(), 3);
    h.broker
        .set_order(&exit, Decimal::ZERO, OrderStatus::Cancelled);
    assert_eq!(h.cycle().action, "exit_submitted");
    assert!(matches!(
        h.broker.submitted()[3].kind,
        OrderKind::Market { .. }
    ));
    assert!(h.state().plan.unwrap().market_exit_required);
}

#[test]
fn missing_or_unapproved_skill_and_stale_scoped_evidence_never_reach_submit() {
    for case in [
        "skill_missing",
        "skill_hash",
        "skill_artifact",
        "watchdog_missing",
        "not_final",
        "blocked",
        "stale",
        "future",
        "account",
        "instrument",
        "policy",
        "conflict",
        "hash",
        "dependency",
        "primary",
        "lineage",
        "duplicate",
        "screening_finalist",
        "advisor_unusable",
        "advisor_binding",
        "advisor_calibration",
        "advisor_drift",
    ] {
        let mut h = Harness::new();
        let c = &mut h.inputs.candidate;
        match case {
            "skill_missing" => {
                c.dossier.skills.pop();
            }
            "skill_hash" => c.dossier.skills[0].content_hash = "c".repeat(64),
            "skill_artifact" => c.dossier.skills[0].artifact_ids = vec!["invented_artifact".into()],
            "watchdog_missing" => c.dossier.records.retain(|r| r.kind != "watchdog"),
            "not_final" => c.dossier.finalized = false,
            "blocked" => c.dossier.blockers.push("unresolved_fact".into()),
            "stale" => c.dossier.records[0].valid_until = now(),
            "future" => c.dossier.records[0].observed_at = now() + Duration::seconds(1),
            "account" => c.dossier.records[0].account_id = AccountId::new("paper_other").unwrap(),
            "instrument" => {
                c.dossier.records[0].instrument_id = InstrumentId::new("MSFT_US_EQ").unwrap()
            }
            "policy" => c.dossier.records[0].policy_version += 1,
            "conflict" => c.dossier.records[0].material_conflict = true,
            "hash" => c.dossier.records[0].revision_hash = "z".repeat(64),
            "dependency" => c.dossier.records[0].dependencies.push("unresolved".into()),
            "primary" => c
                .dossier
                .records
                .iter_mut()
                .for_each(|r| r.primary_verified = false),
            "lineage" => c
                .dossier
                .records
                .iter_mut()
                .filter(|r| r.kind == "source")
                .for_each(|r| r.source_lineage = Some("same_syndication".into())),
            "duplicate" => c.dossier.records.push(c.dossier.records[0].clone()),
            "screening_finalist" => c.screening.finalist = InstrumentId::new("MSFT_US_EQ").unwrap(),
            "advisor_unusable" => c.advisor.usable = false,
            "advisor_binding" => c.advisor.account_snapshot_id = "older_snapshot".into(),
            "advisor_calibration" => c.advisor.calibration_ok = false,
            "advisor_drift" => c.advisor.drift_ok = false,
            _ => unreachable!(),
        }
        assert!(
            h.run(OwnerControls::default()).is_err(),
            "case {case} must block"
        );
        assert!(
            h.broker.mutations.is_empty(),
            "case {case} reached transport"
        );
        assert!(
            h.state().consumed_sessions.is_empty(),
            "invalid research cannot consume a slot"
        );
    }
}

#[test]
fn proposal_hints_cannot_override_authoritative_stop_target_quantity_or_formula_approval() {
    for case in [
        "no_authority",
        "expired_authority",
        "no_calculation",
        "corrupt_formula",
        "stop_hint",
        "target_hint",
        "horizon_hint",
        "quantity",
        "changed_costs",
    ] {
        let mut h = Harness::new();
        match case {
            "no_authority" => h.trader.config.limits_authority = None,
            "expired_authority" => {
                h.trader
                    .config
                    .limits_authority
                    .as_mut()
                    .unwrap()
                    .active
                    .valid_until = now()
            }
            "no_calculation" => h.inputs.candidate.formula = None,
            "corrupt_formula" => {
                h.trader
                    .config
                    .limits_authority
                    .as_mut()
                    .unwrap()
                    .active
                    .formula_hash = "f".repeat(64)
            }
            "stop_hint" => h.inputs.candidate.stop_price += d("0.01"),
            "target_hint" => h.inputs.candidate.target_net_fraction = d("0.02"),
            "horizon_hint" => h.inputs.candidate.max_holding_sessions = 4,
            "quantity" => {
                h.inputs
                    .candidate
                    .formula
                    .as_mut()
                    .unwrap()
                    .calculation
                    .limits
                    .quantity += d("0.01")
            }
            "changed_costs" => h.inputs.candidate.exit_costs.fixed.amount = d("1"),
            _ => unreachable!(),
        }
        assert!(
            h.run(OwnerControls::default()).is_err(),
            "case {case} must block"
        );
        assert!(
            h.broker.mutations.is_empty(),
            "case {case} reached transport"
        );
    }
}

#[test]
fn fenced_compare_and_swap_failure_before_attempt_commit_prevents_send_and_restart_retry() {
    let mut h = Harness::new();
    h.journal.0.borrow_mut().fail_save_at = Some(3);
    assert!(h.run(OwnerControls::default()).is_err());
    assert!(h.broker.mutations.is_empty());
    assert!(!h.state().orders[0].attempted);
    assert_eq!(h.state().consumed_sessions.len(), 1);
    h.restart();
    assert_eq!(h.cycle().state, PositionState::RecoveryRequired);
    assert!(h.broker.mutations.is_empty());
}

#[test]
fn fence_loss_immediately_before_submission_prevents_send_and_stale_worker_cannot_resume() {
    let mut h = Harness::new();
    h.journal.0.borrow_mut().fail_renew_at = Some(2);
    assert!(h.run(OwnerControls::default()).is_err());
    assert!(h.state().orders[0].attempted);
    assert!(h.broker.mutations.is_empty());
    assert!(h.run(OwnerControls::default()).is_err());
    assert_eq!(
        h.broker.observe_count, 2,
        "stale cycle must stop before observation/mutations"
    );
}

#[test]
fn unknown_submission_is_not_retried_after_repeated_cycles_or_restart() {
    let mut h = Harness::new();
    h.broker.acknowledgments.push_back(Ack::Unknown(None));
    h.entry();
    let intent = h.state().orders[0].request.intent_id.clone();
    assert_eq!(h.state().position_state, PositionState::RecoveryRequired);
    for _ in 0..2 {
        assert_eq!(h.cycle().state, PositionState::RecoveryRequired);
    }
    h.restart();
    assert_eq!(h.cycle().state, PositionState::RecoveryRequired);
    assert_eq!(h.broker.submitted().len(), 1);
    assert_eq!(h.state().orders[0].request.intent_id, intent);
    assert_eq!(h.state().consumed_sessions.len(), 1);
}

#[test]
fn definitive_entry_rejection_consumes_the_day_without_chasing_a_replacement() {
    let mut h = Harness::new();
    h.broker.acknowledgments.push_back(Ack::Reject);
    h.entry();
    assert_eq!(h.cycle().action, "position_closed");
    h.restart();
    assert_eq!(h.cycle().action, "no_entry");
    assert_eq!(h.broker.submitted().len(), 1);
    assert_eq!(h.inputs.candidate_reads, 1);
}

#[test]
fn final_account_cash_change_during_research_blocks_entry_before_consuming_a_slot() {
    let mut h = Harness::new();
    h.broker
        .next_observations
        .push_back(h.broker.observation.clone());
    let mut changed = h.broker.observation.clone();
    changed.snapshot.spendable_cash.amount -= d("1");
    h.broker.next_observations.push_back(changed);
    assert!(h.run(OwnerControls::default()).is_err());
    assert!(h.broker.mutations.is_empty());
    assert!(h.state().consumed_sessions.is_empty());
}

#[test]
fn final_account_scope_change_during_research_must_block_entry() {
    for case in ["account", "mode"] {
        let mut h = Harness::new();
        h.broker
            .next_observations
            .push_back(h.broker.observation.clone());
        let mut changed = h.broker.observation.clone();
        if case == "account" {
            changed.snapshot.account_id = AccountId::new("paper_other").unwrap();
        } else {
            changed.snapshot.mode = Mode::Live;
        }
        h.broker.next_observations.push_back(changed);
        let result = h.run(OwnerControls::default());
        assert!(
            result.is_err(),
            "final {case} mismatch must fail before dispatch: {result:?}"
        );
        assert!(h.broker.mutations.is_empty());
    }
}

#[test]
fn partial_entry_and_late_fill_each_receive_only_the_uncovered_protection() {
    let mut h = Harness::new();
    h.entry();
    h.broker
        .set_order("1", d("3"), OrderStatus::PartiallyFilled);
    assert_eq!(h.cycle().action, "protection_submitted");
    assert_eq!(h.broker.cancelled(), vec!["1"]);
    assert_eq!(h.broker.submitted()[1].quantity, d("3"));
    assert!(matches!(
        h.broker.submitted()[1].kind,
        OrderKind::Stop { .. }
    ));
    h.broker
        .set_order("1", d("4"), OrderStatus::PartiallyFilled);
    assert_eq!(h.cycle().action, "protection_submitted");
    assert_eq!(h.broker.submitted()[2].quantity, d("1"));
    assert_eq!(
        h.broker.cancelled(),
        vec!["1"],
        "cancel acknowledgment cannot cause repeated sends"
    );
    assert_eq!(h.cycle().state, PositionState::OpenProtected);
    let before = h.broker.submitted().len();
    assert_eq!(h.close().action, "cancel_entry_before_exit");
    assert_eq!(
        h.broker.submitted().len(),
        before,
        "late buys must become terminal before an exit"
    );
    assert!(h.state().plan.unwrap().exit_latch.is_some());
}

#[test]
fn protective_cancel_ack_and_partial_fill_do_not_authorize_an_early_or_oversized_exit() {
    let mut h = Harness::new();
    h.protected();
    let quantity = h.quantity();
    assert_eq!(h.close().action, "cancel_protection_before_exit");
    assert_eq!(h.broker.cancelled(), vec!["2"]);
    assert_eq!(h.broker.submitted().len(), 2);
    h.broker
        .set_order("2", d("1"), OrderStatus::PartiallyFilled);
    assert_eq!(h.cycle().action, "cancel_protection_before_exit");
    assert_eq!(h.broker.submitted().len(), 2);
    h.broker.set_order("2", d("1"), OrderStatus::Cancelled);
    assert_eq!(h.cycle().action, "exit_submitted");
    assert_eq!(h.broker.submitted()[2].quantity, quantity - d("1"));
    assert_eq!(h.broker.cancelled(), vec!["2"]);
}

#[test]
fn full_native_stop_fill_during_cancel_closes_without_a_software_sale() {
    let mut h = Harness::new();
    h.protected();
    h.close();
    h.broker.set_order("2", h.quantity(), OrderStatus::Filled);
    assert_eq!(h.cycle().action, "position_closed");
    assert!(h.state().plan.is_none());
    assert_eq!(h.broker.submitted().len(), 2);
    assert_eq!(
        h.cycle().action,
        "no_entry",
        "same-day exits cannot free a second entry slot"
    );
}

#[test]
fn exit_latch_survives_restart_price_rebound_and_missing_new_entry_formula_authority() {
    let mut h = Harness::new();
    h.protected();
    h.close();
    let original = h.state().plan.unwrap().exit_latch.unwrap();
    h.config.limits_authority = None;
    h.restart();
    h.inputs.quote.bid = d("102");
    h.inputs.quote.ask = d("102.01");
    assert_eq!(h.cycle().action, "cancel_protection_before_exit");
    assert_eq!(
        h.state().plan.unwrap().exit_latch.as_deref(),
        Some(original.as_str())
    );
    h.broker
        .set_order("2", Decimal::ZERO, OrderStatus::Cancelled);
    assert_eq!(h.cycle().action, "exit_submitted");
    assert_eq!(h.broker.submitted().len(), 3);
    assert_eq!(h.cycle().action, "exit_pending");
    assert_eq!(h.broker.submitted().len(), 3);
}

#[test]
fn partial_exit_reconciles_without_duplicate_sale_and_replaces_only_after_terminal_history() {
    let mut h = Harness::new();
    h.protected();
    h.close();
    h.broker
        .set_order("2", Decimal::ZERO, OrderStatus::Cancelled);
    assert_eq!(h.cycle().action, "exit_submitted");
    let exit_id = h
        .state()
        .orders
        .last()
        .unwrap()
        .broker_id()
        .unwrap()
        .to_string();
    h.broker
        .set_order(&exit_id, d("3"), OrderStatus::PartiallyFilled);
    for _ in 0..2 {
        assert_eq!(h.cycle().action, "exit_pending");
    }
    assert_eq!(h.broker.submitted().len(), 3);
    h.broker.set_order(&exit_id, d("3"), OrderStatus::Cancelled);
    assert_eq!(h.cycle().action, "exit_submitted");
    assert_eq!(h.broker.submitted()[3].quantity, h.quantity() - d("3"));
    assert_ne!(
        h.broker.submitted()[2].intent_id,
        h.broker.submitted()[3].intent_id
    );
}

#[test]
fn definitive_protection_rejection_latches_liquidation_instead_of_retrying_stop() {
    let mut h = Harness::new();
    h.filled_entry();
    h.broker.acknowledgments.push_back(Ack::Reject);
    h.cycle();
    assert_eq!(
        h.state().plan.unwrap().exit_latch.as_deref(),
        Some("protection_rejected")
    );
    assert_eq!(h.cycle().action, "exit_submitted");
    assert_eq!(h.broker.submitted().len(), 3);
    assert_eq!(h.state().orders.last().unwrap().role, OrderRole::Exit);
}

#[test]
fn delayed_protection_rejection_must_latch_liquidation_instead_of_retrying_stop() {
    let mut h = Harness::new();
    h.protected();
    h.broker
        .set_order("2", Decimal::ZERO, OrderStatus::Rejected);
    let report = h.cycle();
    assert!(
        h.state().plan.unwrap().exit_latch.is_some(),
        "asynchronous broker rejection must latch safe liquidation: {report:?}"
    );
    assert_eq!(h.state().orders.last().unwrap().role, OrderRole::Exit);
    assert_eq!(
        h.broker
            .submitted()
            .iter()
            .filter(|r| matches!(r.kind, OrderKind::Stop { .. }))
            .count(),
        1,
        "reconciling a rejected native stop cannot silently retry protection"
    );
}

#[test]
fn unknown_protection_outcome_blocks_replacement_and_liquidation_until_exact_reconciliation() {
    let mut h = Harness::new();
    h.filled_entry();
    h.broker.acknowledgments.push_back(Ack::Unknown(None));
    h.cycle();
    assert_eq!(h.state().position_state, PositionState::RecoveryRequired);
    h.restart();
    for _ in 0..2 {
        assert_eq!(h.close().state, PositionState::RecoveryRequired);
    }
    assert_eq!(h.broker.submitted().len(), 2);
    assert!(h.broker.cancelled().is_empty());
}

#[test]
fn missing_tracked_id_cannot_be_resolved_by_an_otherwise_identical_history_order() {
    let mut h = Harness::new();
    h.entry();
    let mut lookalike = h.broker.observation.snapshot.orders.remove(0);
    lookalike.broker_order_id = "999".into();
    lookalike.status = OrderStatus::Cancelled;
    h.broker.observation.snapshot.orders.push(lookalike);
    let report = h.cycle();
    assert_eq!(report.state, PositionState::RecoveryRequired);
    assert!(report.reason.unwrap().contains("missing"));
    assert_eq!(h.broker.submitted().len(), 1);
    assert!(h.broker.cancelled().is_empty());
}

#[test]
fn durable_terminal_fill_is_not_erased_when_broker_history_no_longer_returns_it() {
    let mut h = Harness::new();
    h.protected();
    h.broker
        .observation
        .snapshot
        .orders
        .retain(|o| o.broker_order_id != "1");
    // Keep actual holdings/costs: dropping old history is not a new account valuation.
    assert_eq!(h.cycle().state, PositionState::OpenProtected);
    assert_eq!(h.state().plan.unwrap().acquired_quantity, h.quantity());
    assert_eq!(h.broker.submitted().len(), 2);
}

#[test]
fn reviewed_exact_id_recovery_checks_broker_facts_without_replaying_the_unknown_intent() {
    let mut h = Harness::new();
    h.broker.acknowledgments.push_back(Ack::Unknown(None));
    h.entry();
    let request = h.state().orders[0].request.clone();
    let mut row = BrokerOrder {
        broker_order_id: "909".into(),
        client_intent_id: None,
        instrument_id: request.instrument_id.clone(),
        side: request.side,
        quantity: request.quantity + d("0.01"),
        filled_quantity: Decimal::ZERO,
        status: OrderStatus::Open,
    };
    h.broker.observation.snapshot.orders.push(row.clone());
    assert!(
        recover_order(
            &mut h.journal,
            &mut h.broker,
            &h.trader.fence,
            &request.intent_id,
            "909",
            &h.clock
        )
        .is_err()
    );
    assert!(h.state().orders[0].broker_id().is_none());
    row.quantity = request.quantity;
    h.broker.observation.snapshot.orders = vec![row];
    let report = recover_order(
        &mut h.journal,
        &mut h.broker,
        &h.trader.fence,
        &request.intent_id,
        "909",
        &h.clock,
    )
    .unwrap();
    assert_eq!(report.action, "exact_order_recovered");
    assert_eq!(h.state().orders[0].broker_id(), Some("909"));
    assert_eq!(h.cycle().action, "entry_pending");
    assert_eq!(h.broker.submitted().len(), 1);
    assert_eq!(h.state().consumed_sessions.len(), 1);
}

#[test]
fn known_id_from_unknown_ack_is_resolved_only_by_that_exact_broker_order() {
    let mut h = Harness::new();
    h.broker
        .acknowledgments
        .push_back(Ack::Unknown(Some("909".into())));
    h.entry();
    assert_eq!(h.cycle().state, PositionState::RecoveryRequired);
    let request = h.state().orders[0].request.clone();
    h.broker.observation.snapshot.orders.push(BrokerOrder {
        broker_order_id: "909".into(),
        client_intent_id: None,
        instrument_id: request.instrument_id,
        side: request.side,
        quantity: request.quantity,
        filled_quantity: Decimal::ZERO,
        status: OrderStatus::Open,
    });
    assert_eq!(h.cycle().action, "entry_pending");
    assert_eq!(h.broker.submitted().len(), 1);
}

#[test]
fn session_loss_pause_survives_rebound_deposit_and_restart_but_next_session_can_enter() {
    let mut h = Harness::new();
    h.broker.observation.total_equity = money(d("980"));
    assert_eq!(h.cycle().action, "no_entry");
    assert_eq!(h.inputs.candidate_reads, 0);
    assert_eq!(h.state().loss_paused_sessions.len(), 1);
    h.inputs.session.as_mut().unwrap().deposits = money(d("1000"));
    h.broker.observation.total_equity = money(d("2000"));
    h.restart();
    assert_eq!(h.cycle().action, "no_entry");
    assert_eq!(h.inputs.candidate_reads, 0);
    h.inputs.session.as_mut().unwrap().session_id = "next_verified_session".into();
    assert_eq!(h.cycle().action, "entry_submitted");
    assert_eq!(h.broker.submitted().len(), 1);
}

#[test]
fn session_loss_guard_uses_opening_equity_plus_gross_deposits_as_denominator() {
    let mut h = Harness::new();
    h.inputs.session.as_mut().unwrap().deposits = money(d("2000"));
    h.inputs.session.as_mut().unwrap().withdrawals = money(d("500"));
    h.broker.observation.total_equity = money(d("2470"));
    // P&L = 2470 - 1000 - 2000 + 500 = -30; denominator = 3000 => -1%.
    assert_eq!(h.cycle().action, "entry_submitted");
    assert!(h.state().loss_paused_sessions.is_empty());
}

#[test]
fn session_loss_latch_does_not_block_protection_cancel_or_position_exit() {
    let mut h = Harness::new();
    h.protected();
    h.broker.observation.total_equity = money(d("980"));
    assert_eq!(h.cycle().action, "cancel_protection_before_exit");
    assert_eq!(
        h.state().plan.unwrap().exit_latch.as_deref(),
        Some("session_loss")
    );
    h.broker
        .set_order("2", Decimal::ZERO, OrderStatus::Cancelled);
    assert_eq!(h.cycle().action, "exit_submitted");
    assert_eq!(h.broker.submitted().len(), 3);
}

#[test]
fn session_unavailable_blocks_new_entry_and_preserves_existing_native_protection() {
    let mut h = Harness::new();
    h.inputs.session = None;
    assert_eq!(h.cycle().action, "no_entry");
    assert!(h.broker.mutations.is_empty());
    h = Harness::new();
    h.protected();
    h.inputs.session = None;
    assert_eq!(h.cycle().state, PositionState::OpenProtected);
    assert!(h.broker.cancelled().is_empty());
}

#[test]
fn owner_close_without_verified_session_must_retain_native_stop_until_exit_can_execute() {
    let mut h = Harness::new();
    h.protected();
    h.inputs.session = None;
    h.close();
    assert!(
        h.state().plan.unwrap().exit_latch.is_some(),
        "owner request remains durable"
    );
    assert!(
        h.broker.cancelled().is_empty(),
        "do not strip native protection when no verified session can execute liquidation"
    );
    assert_eq!(h.broker.submitted().len(), 2);
}

#[test]
fn quote_outage_retains_native_protection_even_when_owner_exit_is_latched() {
    let mut h = Harness::new();
    h.protected();
    h.inputs.quote_unavailable = true;
    h.close();
    assert!(h.state().plan.unwrap().exit_latch.is_some());
    assert!(
        h.broker.cancelled().is_empty(),
        "without executable prices a software exit must not expose a protected holding"
    );
    assert_eq!(h.broker.submitted().len(), 2);
}

#[test]
fn stopping_and_restarting_a_protected_worker_retains_stop_and_never_duplicates_it() {
    let mut h = Harness::new();
    h.protected();
    let before = h.broker.mutations.len();
    let old_generation = h.trader.fence.generation;
    h.restart();
    assert!(h.trader.fence.generation > old_generation);
    assert_eq!(
        h.broker.mutations.len(),
        before,
        "lease release cannot cancel broker protection"
    );
    assert_eq!(h.cycle().state, PositionState::OpenProtected);
    assert_eq!(h.broker.mutations.len(), before);
}

#[test]
fn fence_loss_before_protective_cancel_stops_all_broker_mutations() {
    let mut h = Harness::new();
    h.protected();
    let before = h.broker.mutations.len();
    let next_mutation_renew = h.journal.0.borrow().renewals + 2;
    h.journal.0.borrow_mut().fail_renew_at = Some(next_mutation_renew);
    assert!(
        h.run(OwnerControls {
            pause_entries: false,
            close_position: true
        })
        .is_err()
    );
    assert_eq!(h.broker.mutations.len(), before);
    assert!(
        h.state().orders[1].cancellation_requested,
        "restart must conservatively reconcile a durably attempted cancellation"
    );
}

#[test]
fn uncertain_protection_cancel_is_not_replayed_and_never_authorizes_a_sale() {
    let mut h = Harness::new();
    h.protected();
    let mut error = BrokerFailure::new(BrokerFailureKind::Transport);
    error.outcome_unknown = true;
    error.broker_order_id = Some("2".into());
    h.broker.cancel_failure = Some(error);
    h.close();
    h.restart();
    for _ in 0..2 {
        assert_eq!(h.cycle().action, "cancel_protection_before_exit");
    }
    assert_eq!(h.broker.cancelled(), vec!["2"]);
    assert_eq!(h.broker.submitted().len(), 2);
}

#[test]
fn crash_after_broker_send_before_ack_commit_recovers_without_replaying_submission() {
    let mut h = Harness::new();
    h.journal.0.borrow_mut().fail_save_at = Some(4);
    assert!(h.run(OwnerControls::default()).is_err());
    assert_eq!(h.broker.submitted().len(), 1);
    assert!(h.state().orders[0].attempted);
    assert!(h.state().orders[0].broker_id().is_none());
    h.restart();
    assert_eq!(h.cycle().state, PositionState::RecoveryRequired);
    assert_eq!(
        h.broker.submitted().len(),
        1,
        "even a lookalike actual pending order cannot authorize retry or automatic rebinding"
    );
}

#[test]
fn expired_unfilled_entry_is_cancelled_once_and_remains_unresolved_across_restart() {
    let mut h = Harness::new();
    h.entry();
    h.advance(300);
    assert_eq!(h.cycle().action, "entry_pending");
    assert_eq!(h.broker.cancelled(), vec!["1"]);
    h.restart();
    assert_eq!(h.cycle().action, "entry_pending");
    assert_eq!(h.broker.cancelled(), vec!["1"]);
    assert_eq!(h.broker.submitted().len(), 1);
    h.broker
        .set_order("1", Decimal::ZERO, OrderStatus::Cancelled);
    assert_eq!(h.cycle().action, "position_closed");
    assert_eq!(h.cycle().action, "no_entry");
}

#[test]
fn unfilled_entry_must_cancel_at_session_entry_cutoff_before_proposal_expiry() {
    let mut h = Harness::new();
    h.inputs.session.as_mut().unwrap().closes_at = now() + Duration::minutes(31);
    h.entry();
    h.advance(61);
    assert!(h.clock.now() < h.inputs.candidate.valid_until);
    h.cycle();
    assert_eq!(
        h.broker.cancelled(),
        vec!["1"],
        "the 30-minute session cutoff bounds even an unexpired five-minute entry"
    );
    assert_eq!(h.broker.submitted().len(), 1);
}

#[test]
fn trailing_peak_and_floor_survive_restart_and_latch_exit_on_subsequent_reversal() {
    let mut h = Harness::new();
    h.protected();
    h.advance(1);
    h.inputs.quote.bid = d("102");
    h.inputs.quote.ask = d("102.01");
    assert_eq!(h.cycle().state, PositionState::OpenProtected);
    let trailing = h.state().plan.unwrap().holding_state.trailing().clone();
    assert!(trailing.floor_net_fraction.is_some());
    assert!(
        trailing.peak_net_fraction.unwrap() < d("0.03"),
        "peak is below the profit target"
    );
    h.restart();
    assert_eq!(h.state().plan.unwrap().holding_state.trailing(), &trailing);
    h.advance(1);
    h.inputs.quote.bid = d("101");
    h.inputs.quote.ask = d("101.01");
    assert_eq!(h.cycle().action, "cancel_protection_before_exit");
    let state = h.state();
    let plan = state.plan.unwrap();
    assert_eq!(
        plan.holding_state.exit_latch().unwrap().reason,
        trady_policy::holding::ExitReason::TrailingFloor
    );
    assert_eq!(
        plan.holding_state.trailing().floor_net_fraction,
        trailing.floor_net_fraction
    );
    assert_eq!(h.broker.submitted().len(), 2);
}

#[test]
fn profit_target_exit_must_use_formula_limit_after_terminal_protection_cancellation() {
    let mut h = Harness::new();
    h.protected();
    let target = h
        .inputs
        .candidate
        .formula
        .as_ref()
        .unwrap()
        .calculation
        .limits
        .target_sell_limit;
    h.advance(1);
    h.inputs.quote.bid = target;
    h.inputs.quote.ask = target + d("0.01");
    assert_eq!(h.cycle().action, "cancel_protection_before_exit");
    h.broker
        .set_order("2", Decimal::ZERO, OrderStatus::Cancelled);
    assert_eq!(h.cycle().action, "exit_submitted");
    assert_eq!(
        h.broker.submitted()[2].kind,
        OrderKind::Limit {
            price: target,
            time_in_force: TimeInForce::Day
        },
        "profit taking must honor the approved numerical target with a limit order"
    );
}

#[test]
fn completed_prior_plan_fills_are_excluded_from_the_next_sessions_holding_reconciliation() {
    let mut h = Harness::new();
    h.protected();
    let old_quantity = h.quantity();
    h.broker.set_order("2", old_quantity, OrderStatus::Filled);
    assert_eq!(h.cycle().action, "position_closed");
    h.inputs.session.as_mut().unwrap().session_id = "next_verified_session".into();
    h.broker.observation.snapshot.snapshot_id = "account_next".into();
    h.inputs.candidate = candidate(&h.config, &h.broker.observation.snapshot, &h.inputs.quote);
    assert_eq!(h.cycle().action, "entry_submitted");
    assert_eq!(h.state().plan.unwrap().first_order, 2);
    let new_id = h
        .state()
        .orders
        .last()
        .unwrap()
        .broker_id()
        .unwrap()
        .to_string();
    let new_quantity = h.quantity();
    h.broker
        .set_order(&new_id, new_quantity, OrderStatus::Filled);
    assert_eq!(h.cycle().action, "protection_submitted");
    assert_eq!(h.state().plan.unwrap().acquired_quantity, new_quantity);
    assert_eq!(h.state().consumed_sessions.len(), 2);
    assert_eq!(h.broker.submitted().len(), 4);
}

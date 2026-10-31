//! Configured analyst/market-data boundary, automatic universe screening and native advice.
//! The approved analyst service resolves source truth; model output is never that service.

mod market;
pub use market::MarketInputs;

use crate::{
    news::{HttpTransport, SafeHttp, http::validate_destination},
    perpetual::NativePerpetualAdvisor,
    screening::{ApprovedUniverseConfig, HttpUniverseSource},
};
use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeSet;
use trady_application::{
    autonomous::*,
    harness::{content_hash, new_id},
    optimization::{
        BoundCalculation, FormulaProposal, ScenarioPredictionInput, calculate_authoritative_limits,
    },
    perpetual,
    screening::{self, ScreeningDataPort, ScreeningRequest, UniverseSource},
};
use trady_domain::{
    perpetual::{
        AssessArguments, AssessmentContext, AssessmentInfluence, AssessmentStatus,
        CalibrationStatus, DriftStatus, FeatureSnapshot, FeatureValue,
        MarketObservation as MlMarket, PerpetualReview, ScenarioBinding, ScenarioSnapshot,
    },
    screening::{ScreeningObservations, UniverseSnapshot},
    *,
};
use trady_policy::{
    ConfidenceInputs, EntryCosts, EntryRange, InstrumentRules, fresh,
    limits::{FormulaEvidence, LimitsInput},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradingInputsConfig {
    pub config_version: u32,
    /// A reviewed service exposing session, quote, costs and finalized research endpoints.
    pub base_url: String,
    pub allowed_hosts: Vec<String>,
    pub screening: ScreeningRequest,
}
impl TradingInputsConfig {
    pub fn validate(&self, trading: &TradingConfig) -> Result<()> {
        let url =
            Url::parse(&self.base_url).map_err(|_| Error("invalid trading input URL".into()))?;
        require(
            self.config_version == 1
                && self.screening.owner_id == trading.owner_id
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path().ends_with('/'),
            "invalid trading input configuration",
        )?;
        validate_destination(&url, &self.allowed_hosts)
            .map_err(|_| Error("unapproved trading input destination".into()))?;
        self.screening.policy.validate()
    }
}

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

struct CachedUniverse<U> {
    source: U,
    snapshot: Option<UniverseSnapshot>,
    refresh_seconds: u32,
}
impl<U: UniverseSource> UniverseSource for CachedUniverse<U> {
    fn fetch_universe(&mut self, now: DateTime<Utc>) -> Result<UniverseSnapshot> {
        if let Some(snapshot) = &self.snapshot
            && now < snapshot.expires_at
            && now - snapshot.provenance.fetched_at
                < Duration::seconds(i64::from(self.refresh_seconds))
        {
            return Ok(snapshot.clone());
        }
        let snapshot = self.source.fetch_universe(now)?;
        self.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }
}
struct PacketData(ScreeningObservations);
impl ScreeningDataPort for PacketData {
    fn observations(
        &mut self,
        _: &UniverseSnapshot,
        _: DateTime<Utc>,
    ) -> Result<ScreeningObservations> {
        Ok(self.0.clone())
    }
}

pub struct AutomaticInputs<R, U = HttpUniverseSource, T = SafeHttp> {
    config: TradingInputsConfig,
    trading: TradingConfig,
    universe: CachedUniverse<U>,
    advisor: NativePerpetualAdvisor,
    pub repository: R,
    service: market::InputService<T>,
    clock: Box<dyn trady_application::Clock>,
}
impl<R: TradingResearchStore> AutomaticInputs<R> {
    pub fn new(
        config: TradingInputsConfig,
        trading: TradingConfig,
        universe: ApprovedUniverseConfig,
        advisor: NativePerpetualAdvisor,
        repository: R,
        token: Option<String>,
    ) -> Result<Self> {
        let refresh = universe.minimum_interval_seconds;
        let universe_url =
            Url::parse(&universe.url).map_err(|_| Error("invalid universe URL".into()))?;
        let service_url =
            Url::parse(&config.base_url).map_err(|_| Error("invalid input service URL".into()))?;
        let universe_token = (universe_url.origin() == service_url.origin())
            .then(|| token.clone())
            .flatten();
        let source = HttpUniverseSource::new(universe)?.with_bearer(universe_token)?;
        Self::with_ports(
            config, trading, source, refresh, advisor, repository, SafeHttp, token,
        )
    }
}
impl<R: TradingResearchStore, U: UniverseSource, T: HttpTransport> AutomaticInputs<R, U, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn with_ports(
        config: TradingInputsConfig,
        trading: TradingConfig,
        source: U,
        refresh_seconds: u32,
        advisor: NativePerpetualAdvisor,
        repository: R,
        transport: T,
        token: Option<String>,
    ) -> Result<Self> {
        config.validate(&trading)?;
        require(
            (60..=86400).contains(&refresh_seconds),
            "invalid universe refresh interval",
        )?;
        let service = market::InputService::new(config.clone(), transport, token)?;
        Ok(Self {
            config,
            trading,
            universe: CachedUniverse {
                source,
                snapshot: None,
                refresh_seconds,
            },
            advisor,
            repository,
            service,
            clock: Box::new(crate::SystemClock),
        })
    }
    pub fn with_clock(mut self, clock: impl trady_application::Clock + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }
    fn get<D: DeserializeOwned>(&mut self, path: &[&str]) -> Result<D> {
        self.request(path, None)
    }
    fn request<D: DeserializeOwned>(&mut self, path: &[&str], body: Option<Vec<u8>>) -> Result<D> {
        self.service.request(path, body, self.clock.as_ref())
    }
    #[allow(clippy::too_many_arguments)]
    fn native_evidence(
        &self,
        dossier: &mut Dossier,
        kind: &str,
        id: &str,
        content: &impl Serialize,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
        valid_until: DateTime<Utc>,
    ) -> Result<()> {
        let old_ids: BTreeSet<_> = dossier
            .records
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.id.clone())
            .collect();
        dossier.records.retain(|r| r.kind != kind);
        for record in &mut dossier.records {
            for dependency in &mut record.dependencies {
                if old_ids.contains(dependency) {
                    *dependency = id.into();
                }
            }
        }
        for skill in &mut dossier.skills {
            for artifact in &mut skill.artifact_ids {
                if old_ids.contains(artifact) {
                    *artifact = id.into();
                }
            }
        }
        let text = serde_json::to_string(content)
            .map_err(|_| Error("native artifact encoding failed".into()))?;
        dossier.records.push(EvidenceRecord {
            id: id.into(),
            account_id: self.trading.account_id.clone(),
            instrument_id: instrument.clone(),
            policy_version: self.trading.policy.version,
            kind: kind.into(),
            revision_hash: content_hash(&text),
            observed_at: now,
            valid_until,
            dependencies: vec![],
            source_lineage: None,
            primary_verified: false,
            material_conflict: false,
        });
        Ok(())
    }
}

impl<R: TradingResearchStore, U: UniverseSource, T: HttpTransport> TradingInputs
    for AutomaticInputs<R, U, T>
{
    fn session(&mut self, account: &AccountId, _: DateTime<Utc>) -> Result<Option<MarketSession>> {
        self.service.session(account, self.clock.as_ref())
    }
    fn quote(&mut self, instrument: &InstrumentId) -> Result<Quote> {
        self.service.quote(instrument, self.clock.as_ref())
    }
    fn current_exit_costs(
        &mut self,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<Option<EntryCosts>> {
        self.service.costs(instrument, now, self.clock.as_ref())
    }
    fn candidate(
        &mut self,
        snapshot: &AccountSnapshot,
        now: DateTime<Utc>,
    ) -> Result<Option<EntryCandidate>> {
        let authority = self
            .trading
            .limits_authority
            .clone()
            .ok_or_else(|| Error("approved formula authority unavailable".into()))?;
        authority.validate_active(&self.trading.policy, now)?;
        let account = self.trading.account_id.to_string();
        let packet: AnalystPacket = self.get(&["research", &account])?;
        let now = self.clock.now();
        require(
            packet.schema_version == 1
                && packet.owner_id == self.trading.owner_id
                && packet.account_id == self.trading.account_id
                && packet.policy_version == self.trading.policy.version
                && fresh(packet.observed_at, now, 300)
                && packet.valid_until > now
                && packet.retain_until > packet.valid_until
                && packet.proposals.len() <= 5,
            "stale, unscoped or oversized analyst packet",
        )?;
        let mut request = self.config.screening.clone();
        request.run_id = new_id("scan");
        let mut data = PacketData(packet.screening_observations);
        let screen = screening::run_screening(
            &request,
            &mut self.universe,
            &mut data,
            &mut self.repository,
            now,
        )?;
        let Some(result) = &screen.result else {
            return Ok(None);
        };
        let Some(finalist) = result.candidates.first() else {
            return Ok(None);
        };
        let Some(draft) = packet
            .proposals
            .into_iter()
            .find(|p| p.instrument_id == finalist.instrument_id)
        else {
            return Ok(None);
        };
        let universe = screen
            .universe
            .as_ref()
            .ok_or_else(|| Error("screening lost universe lineage".into()))?;
        let quote = self.quote(&draft.instrument_id)?;
        let now = self.clock.now();
        let valid_until = draft
            .valid_until
            .min(packet.valid_until)
            .min(now + Duration::minutes(5))
            .min(draft.formula_evidence.valid_until)
            .min(authority.active.valid_until);
        let confidence = trady_policy::confidence(&draft.confidence_inputs).total();
        require(
            draft.formula_evidence.base_confidence_score == confidence
                && draft.formula_evidence.confidence_score <= confidence
                && (!draft.formula_evidence.exceptional_verified || confidence >= 95),
            "formula confidence exceeds independently recomputed evidence",
        )?;
        let exit = trady_policy::economics::ExitCosts {
            fixed: draft.exit_costs.fixed.clone(),
            rate: draft.exit_costs.rate,
            uncertainty: draft.exit_costs.uncertainty.clone(),
            fx: draft.exit_costs.fx.clone(),
        };
        let input = LimitsInput {
            account: snapshot.clone(),
            market_snapshot_id: finalist.market_snapshot_id.clone(),
            quote: quote.clone(),
            entry_range: draft.entry_range.clone(),
            entry_costs: draft.entry_costs.clone(),
            exit_costs: exit,
            rules: draft.instrument_rules.clone(),
            evidence: draft.formula_evidence.clone(),
            holding_deadline_at: draft.deadline_at,
            valid_until,
        };
        let calculation =
            calculate_authoritative_limits(&authority, &self.trading.policy, &input, now)?;
        let retain_until = packet.retain_until.min(
            universe.provenance.fetched_at
                + Duration::hours(i64::from(universe.provenance.rights.retention_hours)),
        );
        self.repository
            .store_calculation(&input, &calculation, retain_until)?;
        let prediction_request = PredictionRequest {
            schema_version: 1,
            owner_id: self.trading.owner_id.clone(),
            account_id: self.trading.account_id.clone(),
            input: input.clone(),
            calculation: calculation.clone(),
        };
        let prediction_body = serde_json::to_vec(&prediction_request)
            .map_err(|_| Error("prediction request encoding failed".into()))?;
        let predictions: ScenarioPredictionInput =
            self.request(&["predictions", &account], Some(prediction_body))?;
        let formula = FormulaProposal {
            input,
            calculation: calculation.clone(),
            predictions: Some(predictions),
        };
        let quote = self.quote(&draft.instrument_id)?;
        let now = self.clock.now();
        formula.validate(&authority, &self.trading.policy, &quote, now)?;
        let mut candidate = EntryCandidate {
            proposal_id: draft.proposal_id,
            account_id: self.trading.account_id.clone(),
            account_snapshot_id: snapshot.snapshot_id.clone(),
            policy_version: self.trading.policy.version,
            instrument_id: draft.instrument_id.clone(),
            currency: draft.currency,
            created_at: now,
            valid_until: valid_until.min(calculation.limits.valid_until),
            entry_range: draft.entry_range,
            stop_price: calculation.limits.stop_trigger,
            target_net_fraction: calculation.limits.target_net_fraction,
            max_holding_sessions: calculation.limits.max_holding_sessions,
            deadline_at: draft.deadline_at,
            exceptional_verified: draft.formula_evidence.exceptional_verified,
            confidence_inputs: draft.confidence_inputs,
            entry_costs: draft.entry_costs,
            exit_costs: draft.exit_costs,
            instrument_rules: draft.instrument_rules,
            dossier: draft.dossier,
            screening: ScreeningReceipt {
                universe_snapshot_id: universe.snapshot_id.clone(),
                screening_id: screen.snapshot_id.clone(),
                finalist: finalist.instrument_id.clone(),
                observed_at: screen.as_of,
                valid_until: universe.expires_at.min(valid_until),
            },
            advisor: AdvisorReceipt {
                assessment_id: String::new(),
                model_revision_id: String::new(),
                scenario_hash: String::new(),
                account_snapshot_id: snapshot.snapshot_id.clone(),
                assessed_at: now,
                valid_until: now,
                assessed_bid: quote.bid,
                assessed_ask: quote.ask,
                quote_drift_tolerance: Decimal::ZERO,
                usable: false,
                calibration_ok: false,
                drift_ok: false,
                review_reason: draft.perpetual_review_reason.clone(),
            },
            formula: Some(formula),
            sentiment_reference: draft.sentiment_reference,
        };
        let universe_evidence = new_id("universe");
        candidate.screening.universe_snapshot_id = universe_evidence.clone();
        self.native_evidence(
            &mut candidate.dossier,
            "universe",
            &universe_evidence,
            universe,
            &candidate.instrument_id,
            now,
            candidate.valid_until,
        )?;
        self.native_evidence(
            &mut candidate.dossier,
            "screening",
            &screen.snapshot_id,
            &screen,
            &candidate.instrument_id,
            now,
            candidate.valid_until,
        )?;
        self.native_evidence(
            &mut candidate.dossier,
            "economics",
            &calculation.calculation_id,
            &calculation,
            &candidate.instrument_id,
            now,
            candidate.valid_until,
        )?;
        let confidence_id = new_id("confidence");
        self.native_evidence(
            &mut candidate.dossier,
            "confidence",
            &confidence_id,
            &candidate.confidence_inputs,
            &candidate.instrument_id,
            now,
            candidate.valid_until,
        )?;
        candidate.dossier.id = new_id("dossier");
        self.repository
            .store_dossier(&candidate.dossier, retain_until)?;
        let profile = candidate
            .dossier
            .records
            .iter()
            .find(|r| r.kind == "company_profile")
            .ok_or_else(|| Error("company profile is missing".into()))?;
        let args = AssessArguments {
            instrument_id: candidate.instrument_id.clone(),
            scenario_calculation_id: calculation.calculation_id.clone(),
            market_snapshot_id: finalist.market_snapshot_id.clone(),
            profile_review_id: profile.id.clone(),
            screening_artifact_id: screen.snapshot_id,
            position_id: None,
        };
        let horizon = u32::try_from((candidate.deadline_at - now).num_seconds())
            .map_err(|_| Error("invalid scenario deadline".into()))?;
        let fx = candidate
            .exit_costs
            .fx
            .as_ref()
            .map_or(Decimal::ONE, |fx| fx.rate);
        let context =
            AssessmentContext {
                scenario: ScenarioSnapshot {
                    binding: ScenarioBinding {
                        arguments: args.clone(),
                        currency: candidate.currency.clone(),
                        entry_price: calculation.limits.buy_limit,
                        stop_price: candidate.stop_price,
                        target_price: calculation.limits.target_sell_limit,
                        estimated_cost_fraction: candidate
                            .confidence_inputs
                            .round_trip_cost_fraction
                            .ok_or_else(|| Error("round-trip costs missing".into()))?,
                        horizon_seconds: horizon,
                        deadline_at: candidate.deadline_at,
                    },
                    calculated_at: now,
                    valid_until: candidate.valid_until,
                    market: MlMarket {
                        instrument_id: candidate.instrument_id.clone(),
                        snapshot_id: args.market_snapshot_id.clone(),
                        observed_at: quote.observed_at,
                        valid_until: candidate
                            .valid_until
                            .min(quote.observed_at + Duration::seconds(10)),
                        executable_price: quote.ask,
                        fx_rate: fx,
                    },
                },
                features: FeatureSnapshot {
                    snapshot_id: new_id("features"),
                    schema_version: draft.feature_schema_version,
                    as_of: now,
                    valid_until: candidate.valid_until,
                    features: draft.features,
                },
            };
        self.repository.store_context(&context, retain_until)?;
        let assessment = perpetual::assess(&mut self.repository, &self.advisor, &args, now)?;
        let checked_at = self.clock.now();
        if assessment.status != AssessmentStatus::Ok {
            return Ok(None);
        }
        let review = PerpetualReview {
            assessment_id: assessment.assessment_id.clone(),
            influence: AssessmentInfluence::Used,
            reason: draft.perpetual_review_reason,
        };
        perpetual::validate_review_for_scenario(
            &mut self.repository,
            &review,
            &context.scenario.binding,
            &context.scenario.market,
            checked_at,
        )?;
        let record = self
            .repository
            .load_assessment(&assessment.assessment_id)?
            .ok_or_else(|| Error("persisted assessment is unavailable".into()))?;
        let model = record
            .model
            .ok_or_else(|| Error("assessment model is unavailable".into()))?;
        candidate.advisor = AdvisorReceipt {
            assessment_id: assessment.assessment_id,
            model_revision_id: model.model_revision_id,
            scenario_hash: candidate.scenario_hash()?,
            account_snapshot_id: snapshot.snapshot_id.clone(),
            assessed_at: now,
            valid_until: assessment.valid_until,
            assessed_bid: quote.bid,
            assessed_ask: quote.ask,
            quote_drift_tolerance: model.quote_drift_tolerance_fraction.min(Decimal::new(1, 3)),
            usable: true,
            calibration_ok: assessment.calibration_status == CalibrationStatus::Validated,
            drift_ok: assessment.drift_status == DriftStatus::InRange,
            review_reason: review.reason,
        };
        candidate.validate(&self.trading, snapshot, &quote, checked_at)?;
        Ok(Some(candidate))
    }
}

//! Immutable owner/account-scoped screening and model lineage records.

#[path = "holding_research.rs"]
mod holding;
pub use holding::VerifiedHolding;

use crate::{connect_local, db, verify_role};
use chrono::{DateTime, Duration, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use trady_application::{
    autonomous::{Dossier, EvidenceRecord, TradingResearchStore},
    optimization::{BoundCalculation, digest},
    perpetual::{PerpetualRepository, validate_record},
    screening::{ScreeningRepository, ScreeningSnapshot},
};
use trady_domain::{perpetual::*, *};
use trady_policy::limits::LimitsInput;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCalculation {
    input: LimitsInput,
    calculation: BoundCalculation,
}
impl StoredCalculation {
    fn validate(&self, account: &AccountId) -> Result<()> {
        let bound = &self.calculation;
        let mut identity = bound.clone();
        identity.calculation_id.clear();
        require(
            self.input.account.account_id == *account
                && bound.schema_version == 1
                && bound.input_hash == digest(&self.input)?
                && bound.calculation_id == format!("limits_{}", digest(&identity)?)
                && bound.limits.account_snapshot_id == self.input.account.snapshot_id
                && bound.limits.market_snapshot_id == self.input.market_snapshot_id
                && bound.limits.holding_deadline_at == self.input.holding_deadline_at,
            "calculation content or account/market binding differs",
        )
    }
    fn validate_context(&self, context: &AssessmentContext, account: &AccountId) -> Result<()> {
        self.validate(account)?;
        let binding = &context.scenario.binding;
        let limits = &self.calculation.limits;
        require(
            binding.arguments.scenario_calculation_id == self.calculation.calculation_id
                && binding.arguments.instrument_id == self.input.quote.instrument_id
                && binding.arguments.market_snapshot_id == self.input.market_snapshot_id
                && binding.currency == self.input.quote.currency
                && binding.entry_price == limits.buy_limit
                && binding.stop_price == limits.stop_trigger
                && binding.target_price == limits.target_sell_limit
                && binding.deadline_at == limits.holding_deadline_at
                && context.scenario.calculated_at >= limits.calculated_at
                && context.scenario.valid_until <= limits.valid_until
                && context.scenario.market.observed_at >= self.input.quote.observed_at
                && context.scenario.market.fx_rate
                    == self
                        .input
                        .exit_costs
                        .fx
                        .as_ref()
                        .map_or(1.into(), |fx| fx.rate),
            "Perpetual context differs from persisted economics or market binding",
        )
    }
}

pub struct PgTradingResearch {
    client: Client,
    owner_id: String,
    account_id: AccountId,
}
impl PgTradingResearch {
    pub fn connect(dsn: &str, owner_id: String, account_id: AccountId) -> Result<Self> {
        require(valid_reference(&owner_id), "invalid trading research owner")?;
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, "trady_analyst")?;
        Ok(Self {
            client,
            owner_id,
            account_id,
        })
    }
    /// A separate connection keeps retention work independent of inference.
    /// Server-side bounds also keep orderly service shutdown bounded.
    pub fn connect_retention(dsn: &str, owner_id: String, account_id: AccountId) -> Result<Self> {
        let mut repository = Self::connect(dsn, owner_id, account_id)?;
        repository
            .client
            .batch_execute("SET statement_timeout='10s'; SET lock_timeout='5s'")
            .map_err(db)?;
        Ok(repository)
    }
    fn put<T: Serialize>(
        &mut self,
        kind: &str,
        key: &str,
        body: &T,
        retain_until: DateTime<Utc>,
    ) -> Result<()> {
        require(valid_reference(key), "invalid trading research record ID")?;
        let value = serde_json::to_value(body)
            .map_err(|_| Error("research record encoding failed".into()))?;
        self.client
            .query_one(
                "SELECT research.put_trading_record($1,$2,$3,$4,$5,$6)",
                &[
                    &self.owner_id,
                    &self.account_id.as_str(),
                    &kind,
                    &key,
                    &value,
                    &retain_until,
                ],
            )
            .map_err(db)?;
        Ok(())
    }
    fn get<T: DeserializeOwned>(&mut self, kind: &str, key: &str) -> Result<Option<T>> {
        let row = self.client.query_opt("SELECT document FROM research.trading_records WHERE owner_id=$1 AND account_id=$2 AND kind=$3 AND record_id=$4 AND retain_until>clock_timestamp()", &[&self.owner_id, &self.account_id.as_str(), &kind, &key]).map_err(db)?;
        row.map(|r| {
            serde_json::from_value(r.get(0))
                .map_err(|_| Error("invalid stored trading research record".into()))
        })
        .transpose()
    }
    pub fn purge_expired(&mut self) -> Result<()> {
        self.client
            .query_one(
                "SELECT research.purge_trading_records($1,$2)",
                &[&self.owner_id, &self.account_id.as_str()],
            )
            .map_err(db)?;
        Ok(())
    }
    fn verify_context_references(&mut self, context: &AssessmentContext) -> Result<()> {
        let now = Utc::now();
        context.validate(now)?;
        let args = &context.scenario.binding.arguments;
        if let Some(position_id) = &args.position_id {
            self.load_verified_holding(position_id)?
                .validate_context(context, now)?;
        } else {
            let calculation: StoredCalculation = self
                .get("calculation", &args.scenario_calculation_id)?
                .ok_or_else(|| Error("Perpetual context references missing calculation".into()))?;
            calculation.validate_context(context, &self.account_id)?;
        }
        let screening: ScreeningSnapshot = self
            .get("screening", &args.screening_artifact_id)?
            .ok_or_else(|| Error("Perpetual context references missing screening".into()))?;
        require(
            screening.snapshot_id == args.screening_artifact_id
                && screening.request.owner_id == self.owner_id
                && screening.failure.is_none()
                && screening.result.as_ref().is_some_and(|result| {
                    result.candidates.iter().any(|candidate| {
                        candidate.instrument_id == args.instrument_id
                            && candidate.market_snapshot_id == args.market_snapshot_id
                    })
                }),
            "Perpetual screening belongs to another instrument or market",
        )?;
        let profile: EvidenceRecord = self
            .get("evidence", &args.profile_review_id)?
            .ok_or_else(|| Error("Perpetual context references missing profile".into()))?;
        self.verify_evidence(&profile, &args.profile_review_id, &args.instrument_id, now)?;
        require(
            profile.kind == "company_profile",
            "Perpetual profile reference has the wrong kind",
        )?;
        for value in &context.features.features {
            for lineage in &value.lineage {
                let evidence: EvidenceRecord = self
                    .get("evidence", &lineage.evidence_id)?
                    .ok_or_else(|| Error("feature lineage references missing evidence".into()))?;
                self.verify_evidence(&evidence, &lineage.evidence_id, &args.instrument_id, now)?;
                require(
                    evidence.observed_at <= lineage.available_at,
                    "feature evidence was observed after declared availability",
                )?;
            }
        }
        Ok(())
    }
    fn verify_evidence(
        &self,
        evidence: &EvidenceRecord,
        id: &str,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<()> {
        require(
            evidence.id == id
                && evidence.account_id == self.account_id
                && evidence.instrument_id == *instrument
                && evidence.observed_at <= now
                && evidence.valid_until > now
                && !evidence.material_conflict,
            "Perpetual evidence is stale, conflicting or differently bound",
        )
    }
}
impl TradingResearchStore for PgTradingResearch {
    fn store_calculation(
        &mut self,
        input: &trady_policy::limits::LimitsInput,
        calculation: &trady_application::optimization::BoundCalculation,
        retain_until: DateTime<Utc>,
    ) -> Result<()> {
        let stored = StoredCalculation {
            input: input.clone(),
            calculation: calculation.clone(),
        };
        stored.validate(&self.account_id)?;
        self.put(
            "calculation",
            &calculation.calculation_id,
            &stored,
            retain_until,
        )
    }
    fn store_dossier(&mut self, dossier: &Dossier, retain_until: DateTime<Utc>) -> Result<()> {
        for record in &dossier.records {
            require(
                record.account_id == self.account_id,
                "research evidence belongs to another account",
            )?;
            self.put("evidence", &record.id, record, retain_until)?;
        }
        self.put("dossier", &dossier.id, dossier, retain_until)
    }
    fn store_context(
        &mut self,
        context: &AssessmentContext,
        retain_until: DateTime<Utc>,
    ) -> Result<()> {
        self.verify_context_references(context)?;
        let args = &context.scenario.binding.arguments;
        self.put(
            "context",
            &args.scenario_calculation_id,
            context,
            retain_until,
        )
    }
}
impl ScreeningRepository for PgTradingResearch {
    fn store_screening_snapshot(&mut self, snapshot: &ScreeningSnapshot) -> Result<()> {
        require(
            snapshot.request.owner_id == self.owner_id,
            "screening belongs to another owner",
        )?;
        let retain = snapshot
            .universe
            .as_ref()
            .map(|u| {
                u.provenance.fetched_at
                    + Duration::hours(i64::from(u.provenance.rights.retention_hours))
            })
            .unwrap_or(snapshot.as_of + Duration::hours(1));
        self.put("screening", &snapshot.snapshot_id, snapshot, retain)
    }
}
impl PerpetualRepository for PgTradingResearch {
    fn load_context(&mut self, args: &AssessArguments) -> Result<AssessmentContext> {
        let context: AssessmentContext = self
            .get("context", &args.scenario_calculation_id)?
            .ok_or_else(|| Error("Perpetual context is unavailable".into()))?;
        require(
            context.scenario.binding.arguments == *args,
            "Perpetual argument references do not resolve",
        )?;
        self.verify_context_references(&context)?;
        Ok(context)
    }
    fn save_assessment(&mut self, record: &AssessmentRecord) -> Result<()> {
        validate_record(record)?;
        let args = &record.context.scenario.binding.arguments;
        let original = self.load_context(args)?;
        require(
            original == record.context,
            "Perpetual assessment changed persisted context",
        )?;
        let row = self.client.query_one("SELECT retain_until FROM research.trading_records WHERE owner_id=$1 AND account_id=$2 AND kind='context' AND record_id=$3", &[&self.owner_id, &self.account_id.as_str(), &args.scenario_calculation_id]).map_err(db)?;
        self.put(
            "assessment",
            &record.assessment.assessment_id,
            record,
            row.get(0),
        )
    }
    fn load_assessment(&mut self, id: &str) -> Result<Option<AssessmentRecord>> {
        self.get("assessment", id)
    }
    fn save_review(
        &mut self,
        review: &PerpetualReview,
        scenario: &ScenarioBinding,
        checked_at: DateTime<Utc>,
    ) -> Result<()> {
        require(
            self.get::<AssessmentRecord>("assessment", &review.assessment_id)?
                .is_some(),
            "Perpetual review references missing assessment",
        )?;
        let body = serde_json::json!({"review":review,"scenario":scenario,"checked_at":checked_at});
        let id = format!(
            "review_{}",
            trady_application::harness::content_hash(
                &serde_json::to_string(&body)
                    .map_err(|_| Error("review encoding failed".into()))?
            )
        );
        let row = self.client.query_one("SELECT retain_until FROM research.trading_records WHERE owner_id=$1 AND account_id=$2 AND kind='assessment' AND record_id=$3 AND retain_until>clock_timestamp()", &[&self.owner_id, &self.account_id.as_str(), &review.assessment_id]).map_err(db)?;
        self.put("review", &id, &body, row.get(0))
    }
}

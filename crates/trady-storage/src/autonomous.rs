//! Fenced PostgreSQL journal for automatic execution, separate from paper fixture intents.

use crate::{connect_local, db, verify_role};
use postgres::Client;
use trady_application::autonomous::{
    ExecutionJournal, ExecutionObservation, TradingConfig, TradingState,
};
use trady_domain::*;

pub struct PgTradingJournal {
    client: Client,
    release_binding: Option<crate::weekly::ReleaseCandidate>,
}
impl PgTradingJournal {
    pub fn connect(dsn: &str) -> Result<Self> {
        let mut client = connect_local(dsn)?;
        verify_role(&mut client, "trady_executor")?;
        Ok(Self {
            client,
            release_binding: None,
        })
    }
    pub fn bind_live_release(&mut self, candidate: crate::weekly::ReleaseCandidate) -> Result<()> {
        candidate.validate()?;
        require(
            candidate.mode == Mode::Live,
            "runtime release binding must use live scope",
        )?;
        self.release_binding = Some(candidate);
        Ok(())
    }
}
fn encode<T: serde::Serialize>(value: &T) -> Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|_| Error("execution journal encoding failed".into()))
}
impl ExecutionJournal for PgTradingJournal {
    fn validate_entry_release(
        &mut self,
        fence: &Fence,
        config: &TradingConfig,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
        if config.mode != Mode::Live {
            return Ok(None);
        }
        require(
            fence.account_id == config.account_id,
            "live release fence account mismatch",
        )?;
        let expected = self
            .release_binding
            .as_ref()
            .ok_or_else(|| Error("live deployment lacks immutable release bindings".into()))?;
        let resolved: serde_json::Value = self
            .client
            .query_one(
                "SELECT release.current_active($1,'live')",
                &[&config.account_id.as_str()],
            )
            .map_err(db)?
            .get(0);
        let candidate: crate::weekly::ReleaseCandidate =
            serde_json::from_value(resolved["candidate"].clone())
                .map_err(|_| Error("invalid active release document".into()))?;
        let authority = config
            .limits_authority
            .as_ref()
            .ok_or_else(|| Error("live formula authority missing".into()))?;
        let release = config
            .live_release
            .as_ref()
            .ok_or_else(|| Error("live gate configuration missing".into()))?;
        let documents = &resolved["documents"];
        let evidence = &resolved["evidence"];
        let evidence_id = |kind: &str| -> Result<&str> {
            evidence[kind]
                .as_str()
                .ok_or_else(|| Error("active release gate missing".into()))
        };
        let verification = &documents[evidence_id("verification")?]["content"];
        let owner = &documents[evidence_id("owner")?]["content"];
        let owner_expiry: chrono::DateTime<chrono::Utc> =
            serde_json::from_value(owner["valid_until"].clone())
                .map_err(|_| Error("invalid release approval expiry".into()))?;
        require(
            candidate == *expected
                && candidate.account_id == config.account_id
                && candidate.owner_id == config.owner_id
                && candidate.policy_hash
                    == trady_application::optimization::digest(&config.policy)?
                && candidate.formula_hash == authority.active.formula_hash
                && candidate.revision_id == authority.active.revision_id
                && candidate.parameters == authority.active.parameters
                && candidate.envelope == authority.envelope
                && candidate.effective_from == authority.active.effective_from
                && candidate.valid_until == authority.active.valid_until
                && release.valid_until <= candidate.valid_until
                && candidate.effective_from <= now
                && now < candidate.valid_until
                && now < owner_expiry
                && authority.active.owner_approval_id == evidence_id("owner")?
                && verification["proof_report_id"].as_str()
                    == Some(authority.active.proof_report_id.as_str())
                && verification["parity_report_id"].as_str()
                    == Some(authority.active.parity_report_id.as_str())
                && verification["evaluation_report_id"].as_str()
                    == Some(authority.active.evaluation_report_id.as_str()),
            "deployment differs from current persisted ACTIVE release authority",
        )?;
        let shadow = format!("shadow_{}", candidate.subject_hash()?);
        require(
            authority.active.shadow_report_id == shadow
                && release.gates.get("owner_approval").map(String::as_str)
                    == Some(evidence_id("owner")?)
                && release.gates.get("shadow_acceptance") == Some(&shadow)
                && release.gates.get("model_validation").map(String::as_str)
                    == Some(evidence_id("verification")?),
            "live approval, model or shadow references do not resolve",
        )?;
        for gate in [
            "account_binding",
            "broker_capabilities",
            "protection_recovery",
            "paper_acceptance",
            "costs_and_market_data",
        ] {
            require(
                release.gates.get(gate).map(String::as_str) == Some(evidence_id("readiness")?),
                "live readiness reference does not resolve",
            )?;
        }
        Ok(Some(candidate.valid_until.min(owner_expiry)))
    }
    fn acquire(
        &mut self,
        config: &TradingConfig,
        worker: &WorkerId,
        ttl_seconds: i32,
    ) -> Result<Fence> {
        let row = self
            .client
            .query_one(
                "SELECT execution.acquire_trader($1,$2,$3,$4)",
                &[
                    &encode(config)?,
                    &worker.as_str(),
                    &ttl_seconds,
                    &encode(&TradingState::default())?,
                ],
            )
            .map_err(db)?;
        Ok(Fence {
            account_id: config.account_id.clone(),
            worker_id: worker.clone(),
            generation: row.get(0),
        })
    }
    fn renew(&mut self, fence: &Fence, ttl_seconds: i32) -> Result<()> {
        self.client
            .query_one(
                "SELECT execution.renew_trader($1,$2,$3,$4)",
                &[
                    &fence.account_id.as_str(),
                    &fence.worker_id.as_str(),
                    &fence.generation,
                    &ttl_seconds,
                ],
            )
            .map_err(db)?;
        Ok(())
    }
    fn load(&mut self, fence: &Fence) -> Result<TradingState> {
        let row = self
            .client
            .query_one(
                "SELECT execution.load_trader($1,$2,$3)",
                &[
                    &fence.account_id.as_str(),
                    &fence.worker_id.as_str(),
                    &fence.generation,
                ],
            )
            .map_err(db)?;
        serde_json::from_value(row.get(0))
            .map_err(|_| Error("invalid stored autonomous state".into()))
    }
    fn save(&mut self, fence: &Fence, expected_revision: u64, state: &TradingState) -> Result<()> {
        let revision = i64::try_from(expected_revision)
            .map_err(|_| Error("execution revision exhausted".into()))?;
        self.client
            .query_one(
                "SELECT execution.save_trader($1,$2,$3,$4,$5)",
                &[
                    &fence.account_id.as_str(),
                    &fence.worker_id.as_str(),
                    &fence.generation,
                    &revision,
                    &encode(state)?,
                ],
            )
            .map_err(db)?;
        Ok(())
    }
    fn record_holding_observation(
        &mut self,
        fence: &Fence,
        observation: &ExecutionObservation,
        exit_costs: &trady_policy::EntryCosts,
    ) -> Result<()> {
        observation.snapshot.validate()?;
        let body = serde_json::json!({"observation":observation,"exit_costs":exit_costs});
        self.client
            .query_one(
                "SELECT execution.record_holding_observation($1,$2,$3,$4)",
                &[
                    &fence.account_id.as_str(),
                    &fence.worker_id.as_str(),
                    &fence.generation,
                    &body,
                ],
            )
            .map_err(db)?;
        Ok(())
    }
    fn pending_holding_update(
        &mut self,
        fence: &Fence,
    ) -> Result<Option<trady_application::autonomous::HoldingUpdate>> {
        let row = self
            .client
            .query_one(
                "SELECT execution.pending_holding_update($1,$2,$3)",
                &[
                    &fence.account_id.as_str(),
                    &fence.worker_id.as_str(),
                    &fence.generation,
                ],
            )
            .map_err(db)?;
        row.get::<_, Option<serde_json::Value>>(0)
            .map(|value| {
                let update: trady_application::autonomous::HoldingUpdate =
                    serde_json::from_value(value)
                        .map_err(|_| Error("invalid persisted holding update".into()))?;
                if let Some(release) = &self.release_binding {
                    let model =
                        update.assessment.model.as_ref().ok_or_else(|| {
                            Error("holding advice model provenance missing".into())
                        })?;
                    require(
                        release.model_hashes.get(&model.model_revision_id)
                            == Some(&model.artifact_sha256),
                        "holding advice used a model outside the deployment release",
                    )?;
                }
                Ok(update)
            })
            .transpose()
    }
    fn synchronize_owner_controls(
        &mut self,
        fence: &Fence,
        config: &TradingConfig,
        state: &TradingState,
        defaults: &trady_application::autonomous::OwnerControls,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<trady_application::autonomous::OwnerControls> {
        use trady_application::{
            harness::Scope, optimization::digest, telegram::gateway::OwnerControlContext,
        };
        let snapshot = state
            .last_snapshot
            .as_ref()
            .ok_or_else(|| Error("owner controls require reconciled account state".into()))?;
        let position = snapshot.positions.first();
        let context = OwnerControlContext {
            scope: Scope {
                owner_id: config.owner_id.clone(),
                account_id: config.account_id.as_str().into(),
                policy_version: config.policy.version,
            },
            mode: config.mode,
            holding_id: position
                .map(|p| {
                    state.plan.as_ref().map_or_else(
                        || digest(&p.instrument_id).map(|hash| format!("unmanaged_{hash}")),
                        |plan| Ok(plan.candidate.proposal_id.clone()),
                    )
                })
                .transpose()?,
            plan_revision: state
                .plan
                .as_ref()
                .map(|p| digest(&(&p.candidate.proposal_id, p.holding_state.plan())))
                .transpose()?,
            holding_revision: position
                .map(|p| digest(&(p, state.plan.as_ref().map(|p| &p.candidate.proposal_id))))
                .transpose()?,
            instrument_id: position.map(|p| p.instrument_id.as_str().into()),
            quantity: position.map(|p| p.quantity.to_string()),
            observed_at: snapshot.observed_at,
        };
        context.validate(now)?;
        let row = self
            .client
            .query_one(
                "SELECT execution.synchronize_owner_controls($1,$2,$3,$4,$5,$6)",
                &[
                    &fence.account_id.as_str(),
                    &fence.worker_id.as_str(),
                    &fence.generation,
                    &encode(&context)?,
                    &context.binding_hash()?,
                    &encode(defaults)?,
                ],
            )
            .map_err(db)?;
        serde_json::from_value(row.get(0))
            .map_err(|_| Error("invalid persisted owner control latches".into()))
    }
    fn release(&mut self, fence: &Fence) -> Result<()> {
        self.client
            .query_one(
                "SELECT execution.release_trader($1,$2,$3)",
                &[
                    &fence.account_id.as_str(),
                    &fence.worker_id.as_str(),
                    &fence.generation,
                ],
            )
            .map_err(db)?;
        Ok(())
    }
}

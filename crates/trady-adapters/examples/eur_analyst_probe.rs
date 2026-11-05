//! Explicit online model-contract probe with synthetic inputs and no broker port.
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use trady_adapters::{
    credentials::CredentialStore,
    opencode::{eur_analyst::OpenCodeEurAnalyst, llm::OpenCodeGoConfig},
};
use trady_application::{
    analyst::AnalystRequest, harness::new_id, market_tools::EurAnalystReadPort,
};
use trady_domain::{
    Currency, InstrumentId, RankedCandidate, SelectionRun, selection::ModelFingerprint,
};

struct SyntheticTools(Value);
impl EurAnalystReadPort for SyntheticTools {
    fn execute(&mut self, _: &str, _: &Value) -> trady_domain::Result<Value> {
        Ok(self.0.clone())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let at = Utc::now();
    let id = InstrumentId::new("AIR_PA_EQ")?;
    let run_id = new_id("synthetic_analyst_probe");
    let session = new_id("synthetic_session");
    let run = SelectionRun {
        schema_version: 1,
        run_id: run_id.clone(),
        account_id: "replay_synthetic_analyst_probe".into(),
        session_id: session.clone(),
        universe_snapshot_id: "synthetic_universe_not_release_evidence".into(),
        started_at: at,
        completed_at: at,
        model: ModelFingerprint {
            run_id,
            session_id: session,
            provider: "opencode-go".into(),
            model: "deepseek-v4.1-flash".into(),
            prompt_hash: "a".repeat(64),
            tool_manifest_hash: "b".repeat(64),
            model_revision: "c".repeat(64),
        },
        fallback_attempts: vec![],
        candidates: vec![RankedCandidate {
            instrument_id: id.clone(),
            currency: Currency::new("EUR")?,
            eligibility_snapshot_id: "synthetic_universe_not_release_evidence".into(),
            eligible: true,
            ranking_score: rust_decimal::Decimal::ONE,
            sentiment_score: None,
            confidence: None,
            evidence_ids: vec!["synthetic_fixture_evidence".into()],
            exclusion_reasons: vec![],
            catalyst: Some("SYNTHETIC CONTRACT PROBE; no market-data claim".into()),
            contradictions: vec![],
            strategy_snapshot_ids: vec![],
        }],
        selected_instrument_id: None,
        proposal: None,
        evidence_snapshot_ids: vec!["synthetic_fixture_evidence".into()],
        failure: None,
    };
    let request = AnalystRequest {
        run,
        eligible_instrument_ids: vec![id.clone()],
        candidate_evidence_ids: vec!["synthetic_fixture_evidence".into()],
        fallback_attempts: vec![],
        as_of: at,
    };
    let fixture = json!({"classification":"SYNTHETIC_CONTRACT_PROBE","instrument_id":id,"evidence_id":"synthetic_fixture_evidence","entry_minimum":"100","entry_maximum":"100.10","stop_price":"99.50","target_price":"102","maximum_hold_until":at+Duration::hours(1),"as_of":at,"rationale":"Synthetic numerical contract fixture; no market, strategy or release acceptance evidence"});
    let key = CredentialStore::load()?
        .required("OPENCODE_API_KEY")?
        .expose()
        .to_owned();
    let mut analyst = OpenCodeEurAnalyst::new(OpenCodeGoConfig::default(), key)?
        .with_tools(SyntheticTools(fixture));
    match analyst.select_candidate(&request, vec![], at) {
        Ok(selection) => println!(
            "{}",
            json!({"status":"ONLINE_PINNED_ANALYST_CONTRACT_PASSED","classification":"SYNTHETIC_INPUT_REAL_MODEL_TRANSPORT","selection":selection,"market_inputs_real":false,"paper_day_passed":false,"broker_mutations":0,"entry_authorized":false})
        ),
        Err(_) => {
            println!(
                "{}",
                json!({"status":"ONLINE_PINNED_ANALYST_CONTRACT_UNAVAILABLE","classification":"SYNTHETIC_INPUT_REAL_MODEL_TRANSPORT","broker_mutations":0,"entry_authorized":false})
            );
            std::process::exit(1);
        }
    }
    Ok(())
}

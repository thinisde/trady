//! Explicit synthetic entry calculation and recording. No credentials or network clients.
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use trady_adapters::shadow::{ShadowBroker, ShadowSource};
use trady_application::{
    autonomous::{ExecutionBroker, ExecutionObservation},
    broker::{BrokerExecutionPort, BrokerFailureKind, BrokerOrderRequest, OrderKind, TimeInForce},
};
use trady_domain::{Error, IntentId, Result, Side, require};
use trady_policy::{
    Policy,
    limits::{FormulaEnvelope, FormulaFamily, FormulaParameters, LimitsInput, calculate_limits},
};

pub const HELP: &str = "\nSimulated entry (synthetic inputs; no broker API or credentials):\n  trady demo-entry --directory ABSOLUTE_DIRECTORY\nRecords an explicit SIMULATION_ONLY entry through the recording-only shadow broker.\nThis verifies calculation and local order recording; it grants no real entry approval.\n";

pub(super) fn run(args: &[String]) -> Result<Value> {
    require(
        args.len() == 2 && args[0] == "--directory",
        "usage: trady demo-entry --directory ABSOLUTE_DIRECTORY",
    )?;
    let root = Path::new(&args[1]);
    require(root.is_absolute(), "simulation directory must be absolute")?;
    let directory = root.join("simulation-only");
    // A fixed fixture clock makes the recorded request reproducible across restarts.
    // This is neither the current exchange session nor a live observation.
    let now = DateTime::parse_from_rfc3339("2026-10-30T14:00:00Z")
        .map_err(|_| Error("invalid bundled simulation clock".into()))?
        .to_utc();
    let money = |amount: &str| json!({"amount":amount,"currency":"EUR"});
    let input: LimitsInput = serde_json::from_value(json!({
        "account":{"snapshot_id":"simulation_account_1","account_id":"shadow_simulated_entry",
            "mode":"shadow","spendable_cash":money("5000"),"positions":[],"orders":[],"observed_at":now},
        "market_snapshot_id":"simulation_market_1",
        "quote":{"instrument_id":"SIMULATED_STOCK","currency":"EUR","bid":"99.99","ask":"100",
            "observed_at":now,"received_at":now,"tradable":true},
        "entry_range":{"minimum":"99","maximum":"101","currency":"EUR"},
        "entry_costs":{"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null},
        "exit_costs":{"fixed":money("0"),"rate":"0","uncertainty":money("0"),"fx":null},
        "rules":{"price_tick":"0.01","quantity_step":"0.01","minimum_quantity":"0.01","currency_unit":"0.01"},
        "evidence":{"snapshot_id":"simulation_volatility_1","family":"TREND","signal_eligible":true,
            "atr":"0.5","atr_lookback":14,"bar_seconds":60,"completed_bars":30,
            "last_completed_at":now-Duration::seconds(45),"available_at":now-Duration::seconds(30),
            "valid_until":now+Duration::minutes(5),"minimum_stop_distance":"0.01","noise_distance":"0.1",
            "base_confidence_score":100,"confidence_score":100,"sentiment_entry_allowed":true,
            "sentiment_target_ceiling":"0.03","exceptional_verified":false},
        "holding_deadline_at":now+Duration::days(3),"valid_until":now+Duration::minutes(5)
    }))
    .map_err(|_| Error("invalid bundled simulation inputs".into()))?;
    let policy = Policy::default();
    let envelope = FormulaEnvelope {
        max_entry_allowance_fraction: Decimal::new(5, 3),
        min_stop_atr_multiple: Decimal::ONE,
        max_stop_atr_multiple: Decimal::from(4),
        min_target_net_fraction: Decimal::new(1, 2),
        max_target_net_fraction: Decimal::new(3, 2),
    };
    let parameters = FormulaParameters {
        version: 1,
        family: FormulaFamily::Trend,
        entry_allowance_fraction: Decimal::new(1, 3),
        stop_atr_multiple: Decimal::from(2),
        target_net_fraction: Decimal::new(3, 2),
        max_holding_sessions: 3,
        atr_lookback: 14,
        bar_seconds: 60,
    };
    let account_risk_fraction = Decimal::new(1, 2);
    let calculation = calculate_limits(
        &policy,
        &envelope,
        &parameters,
        account_risk_fraction,
        &input,
        now,
    )?;
    let request = BrokerOrderRequest {
        intent_id: IntentId::new("simulation_entry_1")?,
        account_id: input.account.account_id.clone(),
        instrument_id: input.quote.instrument_id.clone(),
        instrument_currency: input.quote.currency.clone(),
        side: Side::Buy,
        quantity: calculation.quantity,
        kind: OrderKind::Limit {
            price: calculation.buy_limit,
            time_in_force: TimeInForce::Day,
        },
    };
    let source = ShadowSource::Fixture {
        description: "SIMULATION_ONLY: synthetic account, quote, costs, volatility and confidence; no deployment approval".into(),
    };
    let observation = ExecutionObservation {
        snapshot: input.account.clone(),
        position_costs: BTreeMap::new(),
        available_quantities: BTreeMap::new(),
        total_equity: input.account.spendable_cash.clone(),
    };
    let mut recording = ShadowBroker::new(&directory, observation, source.clone())?;
    let before = recording.observe(&request.account_id, &[])?;
    // The concrete receiver has no network client. Its submit port durably records
    // the request and returns Unsupported, never an order acknowledgement or fill.
    let suppressed = recording.submit_order(&request);
    require(
        matches!(suppressed, Err(ref error) if error.kind == BrokerFailureKind::Unsupported && !error.outcome_unknown),
        "simulation broker did not suppress the order",
    )?;
    let intent_hash = recording.record_proposal(&request)?;
    let after = recording.observe(&request.account_id, &[])?;
    require(
        before.snapshot == after.snapshot,
        "simulation must not create fills or change the account",
    )?;
    Ok(json!({
        "schema_version":1,"classification":"SIMULATION_ONLY","status":"simulated_entry_recorded",
        "executed_at":Utc::now(),"simulation_clock":now,"source":source,
        "entry_authorized":false,"simulated_entry_generated":true,"production_approved":false,
        "broker_order_api_calls":0,"order_mutations":0,"network_clients_constructed":0,
        "submission_result":"SUPPRESSED_BY_SHADOW_BROKER","simulated_account_unchanged":true,
        "policy":policy,"formula_envelope":envelope,"formula_parameters":parameters,
        "account_risk_fraction":account_risk_fraction,"input":input,"calculation":calculation,
        "request":request,"after_snapshot":after.snapshot,"intent_hash":intent_hash,
        "recording_directory":directory,
        "proposal_record":directory.join(format!("intent_{}.json",request.intent_id.as_str())),
        "note":"Synthetic inputs verify calculation and local order recording only. No fitted-model, source, formula-release or live-readiness approval is supplied."
    }))
}

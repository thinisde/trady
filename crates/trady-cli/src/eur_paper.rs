//! Forward paper execution on retained, live SMART books. No broker transport.
//! Modeled fills and stops never become Trading 212 capability evidence.
use super::{
    eur::read_json,
    eur_decision::ReviewedCosts,
    eur_shadow::{private_dir, private_json},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    thread,
    time::{Duration, Instant},
};
use trady_application::{eur_runtime::EurRuntimeConfig, harness::content_hash};
use trady_domain::{AccountSnapshot, Error, ExecutionStage, Result, SelectionRun, require};
use trady_policy::intraday::IntradayTradePlan;

fn decimal(value: &Value) -> Result<Decimal> {
    value
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error("paper decimal missing".into()))
}
fn time(value: &Value) -> Result<DateTime<Utc>> {
    serde_json::from_value(value.clone()).map_err(|_| Error("paper timestamp missing".into()))
}
fn validate_book(book: &Value, binding: &Value, at: DateTime<Utc>) -> Result<()> {
    let provider = DateTime::from_timestamp(
        book["observed_at_seconds"]
            .as_i64()
            .ok_or_else(|| Error("provider quote clock missing".into()))?,
        0,
    )
    .ok_or_else(|| Error("invalid provider quote clock".into()))?;
    let receipt = time(&book["received_at"])?;
    let bid = decimal(&book["bid"])?;
    let ask = decimal(&book["ask"])?;
    require(
        book["market_data_type"] == 1
            && book["isin"] == binding["isin"]
            && book["con_id"] == binding["con_id"]
            && book["source_venue"] == "SMART"
            && book["currency"] == "EUR"
            && provider <= receipt
            && receipt <= at
            && at >= provider
            && (at - provider).num_milliseconds() <= 10000
            && bid > Decimal::ZERO
            && ask >= bid
            && (ask - bid) / bid * Decimal::from(10000) <= Decimal::from(20)
            && decimal(&book["bid_size"])? > Decimal::ZERO
            && decimal(&book["ask_size"])? > Decimal::ZERO,
        "paper entry/exit requires an exact preceding live SMART book",
    )
}
fn cash(
    gross: Decimal,
    fixed: Decimal,
    rate: Decimal,
    reserve: Decimal,
    buy: bool,
) -> Result<Decimal> {
    let costs = fixed + gross * rate + reserve;
    require(
        gross > Decimal::ZERO
            && fixed >= Decimal::ZERO
            && rate >= Decimal::ZERO
            && reserve >= Decimal::ZERO,
        "invalid paper economics",
    )?;
    if buy {
        trady_policy::ceil_to(gross + costs, Decimal::new(1, 2))
    } else {
        trady_policy::floor_to(gross - costs, Decimal::new(1, 2))
    }
}

pub fn run(args: &[String]) -> Result<Value> {
    let options = super::eur::options(
        args,
        &["--config", "--stream", "--out", "--exit-after-seconds"],
        &["--config", "--stream", "--out"],
    )?;
    let config_path = Path::new(&options["--config"]);
    let config: EurRuntimeConfig = read_json(config_path)?;
    config.validate(Utc::now())?;
    require(
        config.execution_stage == ExecutionStage::Shadow && !config.live_enabled,
        "forward paper requires mutation-free SHADOW",
    )?;
    let seconds: u64 = options
        .get("--exit-after-seconds")
        .map(|s| s.parse())
        .transpose()
        .map_err(|_| Error("invalid paper duration".into()))?
        .unwrap_or(60);
    require(
        (1..=900).contains(&seconds),
        "paper acceptance exit must be bounded to 1..900 seconds",
    )?;
    let out = Path::new(&options["--out"]);
    private_dir(out)?;
    let inputs = out.join("research-inputs");
    super::eur_shadow::export_bundle(&options["--config"], &inputs)?;
    let context: Value = read_json(&inputs.join("capture-context.json"))?;
    let recorded = &context["result"];
    if recorded["selection"]["selected_instrument_id"].is_null() || recorded["risk_plan"].is_null()
    {
        let result = json!({"source":"REAL_INPUT_FORWARD_PAPER","status":"PAPER_DAY_BLOCKED","paper_day_passed":false,
            "broker_mutations":0,"live_enabled":false,"selection_failure":recorded["selection_failure"],
            "strategy_assessments":recorded["strategy_assessments"],"risk_blockers":recorded["risk_blockers"],
            "blockers":["DURABLE_REAL_INPUT_SELECTION_AND_RISK_PLAN_REQUIRED"]});
        private_json(&out.join("paper-day-summary.json"), &result)?;
        return Ok(result);
    }
    let selection: SelectionRun = serde_json::from_value(recorded["selection"].clone())
        .map_err(|_| Error("invalid durable paper selection".into()))?;
    let plan: IntradayTradePlan = serde_json::from_value(recorded["risk_plan"].clone())
        .map_err(|_| Error("invalid durable paper risk plan".into()))?;
    selection.validate(Utc::now())?;
    plan.validate(Utc::now(), &config.policy)?;
    require(
        recorded["analyst_invoked"] == true
            && selection.failure.is_none()
            && selection.selected_instrument_id.as_ref() == Some(&plan.instrument_id)
            && plan.account_id == config.account_id.as_str(),
        "paper selection, analyst and risk authority differ",
    )?;
    let costs: ReviewedCosts =
        serde_json::from_value(context["provider_configurations"]["risk"].clone())
            .map_err(|_| Error("paper cost model unavailable".into()))?;
    require(
        plan.cost_model
            .as_ref()
            .is_some_and(|model| model.entry == costs.entry && model.exit == costs.exit),
        "paper plan differs from its retained cost inputs",
    )?;
    require(
        costs.reviewed_at <= Utc::now()
            && costs.valid_until > Utc::now()
            && !costs.review_reference.is_empty()
            && costs.entry.fx.is_none()
            && costs.exit.fx.is_none(),
        "paper cost model expired or FX unresolved",
    )?;
    let mut account = None;
    for item in fs::read_dir(inputs.join("provider-snapshots"))
        .map_err(|_| Error("paper inputs unavailable".into()))?
    {
        let path = item
            .map_err(|_| Error("paper input unavailable".into()))?
            .path();
        if path.extension().is_some_and(|e| e == "json") {
            let value: Value = read_json(&path)?;
            if value["source_id"] == "account_snapshot" && value["status"] == "OK" {
                account = Some(
                    serde_json::from_value::<AccountSnapshot>(value["response"].clone())
                        .map_err(|_| Error("paper account input invalid".into()))?,
                );
            }
        }
    }
    let account = account.ok_or_else(|| Error("paper initial account unavailable".into()))?;
    require(
        account.account_id == config.account_id
            && account.positions.is_empty()
            && account.orders.is_empty(),
        "paper initial account not flat",
    )?;
    let id = plan.instrument_id.as_str();
    let identity = context["crosswalk"]["members"]
        .as_array()
        .and_then(|rows| rows.iter().find(|m| m["internal_instrument_id"] == id))
        .ok_or_else(|| Error("paper identity unavailable".into()))?;
    let mut binding = context["provider_configurations"]["ibkr"]["bindings"][id].clone();
    binding["isin"] = identity["isin"].clone();
    let stream = Path::new(&options["--stream"]);
    let book = || -> Result<Value> {
        let latest: Value = read_json(&stream.join("latest.json"))?;
        require(
            latest["provider"] == "IBKR_SMART_LIVE" && latest["broker_mutations"] == 0,
            "paper stream source changed",
        )?;
        let mut book = latest["instruments"][id].clone();
        let at = Utc::now();
        validate_book(&book, &binding, at)?;
        book["paper_recorded_at"] = json!(at);
        Ok(book)
    };
    let entry = book()?;
    let entry_at = time(&entry["paper_recorded_at"])?;
    let entry_price = decimal(&entry["ask"])?;
    require(
        entry_price <= plan.entry_price && entry_price > plan.stop_price,
        "fresh paper ask outside the approved risk entry",
    )?;
    let debit = cash(
        entry_price * plan.quantity,
        costs.entry.fixed.amount,
        costs.entry.rate,
        costs.entry.uncertainty.amount,
        true,
    )?;
    require(
        debit <= account.spendable_cash.amount && debit <= plan.acquisition_cost.amount,
        "paper purchase exceeds account or plan budget",
    )?;
    let protection_at = Utc::now();
    let started = Instant::now();
    let mut books = vec![entry.clone()];
    let mut journal_options = fs::OpenOptions::new();
    journal_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        journal_options.mode(0o600);
    }
    let mut events = journal_options
        .open(out.join("paper-events.jsonl"))
        .map_err(|_| Error("paper event journal unavailable".into()))?;
    let mut append = |event: Value| -> Result<()> {
        writeln!(events, "{event}")
            .and_then(|_| events.sync_all())
            .map_err(|_| Error("paper journal sync failed".into()))
    };
    append(
        json!({"at":entry_at,"kind":"PAPER_ENTRY_FILL","quote":entry,"quantity":plan.quantity,"debit_eur":debit}),
    )?;
    append(
        json!({"at":protection_at,"kind":"PAPER_STOP_ARMED","price":plan.stop_price,"quantity":plan.quantity,"broker_order":false}),
    )?;
    let (exit, reason) = loop {
        thread::sleep(Duration::from_secs(1));
        let current = book()?;
        let bid = decimal(&current["bid"])?;
        books.push(current.clone());
        append(
            json!({"at":current["paper_recorded_at"],"kind":"PAPER_MARKET_OBSERVATION","quote":current}),
        )?;
        let at = time(&current["paper_recorded_at"])?;
        let reason = if bid <= plan.stop_price {
            Some("STOP")
        } else if bid >= plan.target_price {
            Some("TARGET")
        } else if at >= plan.flatten_deadline {
            Some("DEADLINE")
        } else if started.elapsed() >= Duration::from_secs(seconds) {
            Some("BOUNDED_PAPER_ACCEPTANCE_EXIT")
        } else {
            None
        };
        if let Some(reason) = reason {
            break (current, reason);
        }
    };
    let exit_price = decimal(&exit["bid"])?;
    let credit = cash(
        exit_price * plan.quantity,
        costs.exit.fixed.amount,
        costs.exit.rate,
        costs.exit.uncertainty.amount,
        false,
    )?;
    require(credit >= Decimal::ZERO, "paper exit proceeds invalid")?;
    let mut ledger = json!({"schema_version":1,"source":"REAL_INPUT_FORWARD_PAPER","execution_model":"CROSS_DISPLAYED_SMART_BOOK_PLUS_REVIEWED_MODELED_COSTS",
        "simulation":true,"synthetic_fixture":false,"broker_mutations":0,"live_enabled":false,"reconciled_virtual_flat":true,"protection_model":"PAPER_BID_TRIGGER_ON_ONE_SECOND_OBSERVATIONS",
        "source_cycle_id":recorded["cycle_id"],"source_configuration_hash":recorded["configuration_hash"],"source_policy_hash":recorded["policy_hash"],
        "paper_engine_hash":content_hash(include_str!("eur_paper.rs")),"selection":selection,"risk_plan":plan,"cost_model":costs,
        "binding":binding,"entry_quote":entry,"exit_quote":exit,"books":books,"protection_recorded_at":protection_at,
        "exit_reason":reason,"bounded_exit_seconds":seconds,"initial_account_snapshot":account,"initial_cash_eur":account.spendable_cash.amount,
        "entry_debit_eur":debit,"exit_credit_eur":credit,"ending_cash_eur":account.spendable_cash.amount-debit+credit,"net_pnl_eur":credit-debit});
    validate_for_capture(&ledger, &context)?;
    let provenance = capture_stream_provenance(stream, &ledger)?;
    let provenance_hash = private_json(&out.join("forward-stream.json"), &provenance)?;
    ledger["stream_provenance_sha256"] = json!(provenance_hash);
    append(
        json!({"at":exit["paper_recorded_at"],"kind":"PAPER_EXIT_FILL_AND_FLAT","quote":exit,"credit_eur":credit}),
    )?;
    private_json(&out.join("paper-ledger.json"), &ledger)?;
    let capture = out.join("paper-capture");
    copy_capture(&inputs, &capture)?;
    private_json(&capture.join("forward-stream.json"), &provenance)?;
    let ledger_hash = private_json(&capture.join("paper-ledger.json"), &ledger)?;
    let mut manifest: Value = read_json(&inputs.join("manifest.json"))?;
    manifest["inputs"]["paper_order_ledger"] =
        json!({"path":"paper-ledger.json","sha256":ledger_hash});
    manifest["inputs"]["paper_quote_stream"] =
        json!({"path":"forward-stream.json","sha256":provenance_hash});
    private_json(&capture.join("manifest.json"), &manifest)?;
    let result = json!({"source":"REAL_INPUT_FORWARD_PAPER","status":"PAPER_STAGES_EXERCISED_PENDING_INDEPENDENT_ACCEPTANCE",
        "paper_day_passed":false,"paper_stages_exercised":true,"reconciled_virtual_flat":true,"net_pnl_eur":credit-debit,
        "capture_manifest":capture.join("manifest.json"),"broker_evidence":false,"broker_mutations":0,"live_enabled":false});
    private_json(&out.join("paper-day-summary.json"), &result)?;
    Ok(result)
}

fn capture_stream_provenance(stream: &Path, ledger: &Value) -> Result<Value> {
    let file = fs::File::open(stream.join("events.jsonl"))
        .map_err(|_| Error("paper raw callbacks unavailable".into()))?;
    let mut bytes = Vec::new();
    file.take(64_000_001)
        .read_to_end(&mut bytes)
        .map_err(|_| Error("paper raw callbacks unreadable".into()))?;
    require(bytes.len() <= 64_000_000, "paper raw stream exceeds bound")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Error("paper raw stream invalid".into()))?;
    let mut types = Vec::new();
    let mut callbacks = Vec::new();
    let books = ledger["books"]
        .as_array()
        .ok_or_else(|| Error("paper books missing".into()))?;
    for line in text.lines() {
        let event: Value =
            serde_json::from_str(line).map_err(|_| Error("paper raw callback invalid".into()))?;
        if event["kind"] == "market_data_type" {
            types.push(event);
        } else if event["kind"] == "bid_ask"
            && books.iter().any(|book| callback_matches(&event, book))
        {
            callbacks.push(event);
        }
    }
    let contracts: Value = read_json(&stream.join("contracts.json"))?;
    let provenance = json!({"provider":"IBKR_SMART_LIVE","broker_mutations":0,"contracts":contracts,"market_data_types":types,"callbacks":callbacks});
    validate_stream_provenance(ledger, &provenance)?;
    Ok(provenance)
}

fn callback_matches(event: &Value, book: &Value) -> bool {
    event["kind"] == "bid_ask"
        && event.as_object().is_some_and(|fields| {
            [
                "kind",
                "request_id",
                "observed_at_seconds",
                "received_at",
                "bid",
                "ask",
                "bid_size",
                "ask_size",
            ]
            .iter()
            .all(|key| fields.contains_key(*key))
                && fields
                    .iter()
                    .all(|(key, value)| book.get(key) == Some(value))
        })
}

pub(super) fn validate_stream_provenance(ledger: &Value, provenance: &Value) -> Result<()> {
    require(
        provenance["provider"] == "IBKR_SMART_LIVE" && provenance["broker_mutations"] == 0,
        "paper raw stream source changed",
    )?;
    let contracts = provenance["contracts"]["contracts"]
        .as_array()
        .ok_or_else(|| Error("paper raw contracts missing".into()))?;
    let contract = contracts
        .iter()
        .find(|c| c["con_id"] == ledger["binding"]["con_id"])
        .ok_or_else(|| Error("paper exact contract missing".into()))?;
    require(
        contract["identifiers"]["ISIN"] == ledger["binding"]["isin"]
            && contract["primary_exchange"] == ledger["binding"]["primary_exchange"]
            && contract["currency"] == "EUR"
            && contract["exchange"] == "SMART",
        "paper raw contract rebound",
    )?;
    let index = contracts.iter().position(|c| c == contract).unwrap() as u64;
    let types = provenance["market_data_types"]
        .as_array()
        .ok_or_else(|| Error("paper raw live type missing".into()))?;
    let callbacks = provenance["callbacks"]
        .as_array()
        .ok_or_else(|| Error("paper raw books missing".into()))?;
    for book in ledger["books"]
        .as_array()
        .ok_or_else(|| Error("paper books missing".into()))?
    {
        let event = callbacks
            .iter()
            .find(|event| callback_matches(event, book))
            .ok_or_else(|| Error("paper book absent from retained raw callbacks".into()))?;
        require(
            event["request_id"] == 2000 + index,
            "paper raw request contract differs",
        )?;
        let receipt = time(&event["received_at"])?;
        let live_type = types
            .iter()
            .filter(|event| {
                event["request_id"] == 1000 + index
                    && time(&event["received_at"]).is_ok_and(|at| at <= receipt)
            })
            .max_by_key(|event| time(&event["received_at"]).ok())
            .ok_or_else(|| Error("paper preceding live type missing".into()))?;
        require(
            live_type["data_type"] == 1,
            "paper raw market data was not live type1",
        )?;
    }
    Ok(())
}

fn copy_capture(source: &Path, target: &Path) -> Result<()> {
    private_dir(target)?;
    for entry in
        fs::read_dir(source).map_err(|_| Error("paper source capsule unavailable".into()))?
    {
        let entry = entry.map_err(|_| Error("paper capsule entry unavailable".into()))?;
        let kind = entry
            .file_type()
            .map_err(|_| Error("paper capsule kind unavailable".into()))?;
        require(!kind.is_symlink(), "paper capsule symlink rejected")?;
        if kind.is_dir() {
            copy_capture(&entry.path(), &target.join(entry.file_name()))?;
        } else if entry.file_name() != "manifest.json" {
            fs::copy(entry.path(), target.join(entry.file_name()))
                .map_err(|_| Error("paper input copy failed".into()))?;
        }
    }
    Ok(())
}

pub(super) fn validate_for_capture(ledger: &Value, context: &Value) -> Result<()> {
    let recorded = &context["result"];
    require(
        ledger["source"] == "REAL_INPUT_FORWARD_PAPER"
            && ledger["simulation"] == true
            && ledger["synthetic_fixture"] == false
            && ledger["broker_mutations"] == 0
            && ledger["live_enabled"] == false
            && ledger["reconciled_virtual_flat"] == true
            && ledger["protection_model"] == "PAPER_BID_TRIGGER_ON_ONE_SECOND_OBSERVATIONS"
            && ledger["source_cycle_id"] == recorded["cycle_id"]
            && ledger["source_configuration_hash"] == recorded["configuration_hash"]
            && ledger["source_policy_hash"] == recorded["policy_hash"]
            && ledger["selection"] == recorded["selection"]
            && ledger["risk_plan"] == recorded["risk_plan"]
            && recorded["analyst_invoked"] == true,
        "paper ledger rebinds research, risk or execution class",
    )?;
    let costs: ReviewedCosts =
        serde_json::from_value(context["provider_configurations"]["risk"].clone())
            .map_err(|_| Error("paper costs missing".into()))?;
    require(
        ledger["cost_model"]
            == serde_json::to_value(&costs)
                .map_err(|_| Error("paper cost encoding failed".into()))?,
        "paper cost model rebound",
    )?;
    let plan: IntradayTradePlan = serde_json::from_value(ledger["risk_plan"].clone())
        .map_err(|_| Error("paper plan invalid".into()))?;
    let config: EurRuntimeConfig = serde_json::from_value(context["runtime"].clone())
        .map_err(|_| Error("paper runtime missing".into()))?;
    let account: AccountSnapshot = serde_json::from_value(context["account_snapshot"].clone())
        .map_err(|_| Error("paper source account missing".into()))?;
    require(
        ledger["initial_account_snapshot"]
            == serde_json::to_value(&account)
                .map_err(|_| Error("paper account encoding failed".into()))?
            && account.positions.is_empty()
            && account.orders.is_empty()
            && account.account_id == config.account_id
            && decimal(&ledger["initial_cash_eur"])? == account.spendable_cash.amount,
        "paper initial account rebound",
    )?;
    let identity = context["crosswalk"]["members"]
        .as_array()
        .and_then(|members| {
            members
                .iter()
                .find(|m| m["internal_instrument_id"] == plan.instrument_id.as_str())
        })
        .ok_or_else(|| Error("paper source identity missing".into()))?;
    let mut expected_binding =
        context["provider_configurations"]["ibkr"]["bindings"][plan.instrument_id.as_str()].clone();
    expected_binding["isin"] = identity["isin"].clone();
    require(
        ledger["binding"] == expected_binding,
        "paper quote contract rebound",
    )?;
    let entry = &ledger["entry_quote"];
    let exit = &ledger["exit_quote"];
    let entry_at = time(&entry["paper_recorded_at"])?;
    let exit_at = time(&exit["paper_recorded_at"])?;
    plan.validate(entry_at, &config.policy)?;
    let selection: SelectionRun = serde_json::from_value(ledger["selection"].clone())
        .map_err(|_| Error("paper source selection invalid".into()))?;
    selection.validate(entry_at)?;
    require(
        selection.failure.is_none()
            && plan
                .cost_model
                .as_ref()
                .is_some_and(|model| model.entry == costs.entry && model.exit == costs.exit)
            && selection.selected_instrument_id.as_ref() == Some(&plan.instrument_id)
            && costs.reviewed_at <= entry_at
            && costs.valid_until > exit_at
            && costs.entry.fixed.currency.as_str() == "EUR"
            && costs.entry.uncertainty.currency.as_str() == "EUR"
            && costs.exit.fixed.currency.as_str() == "EUR"
            && costs.exit.uncertainty.currency.as_str() == "EUR"
            && costs.entry.fx.is_none()
            && costs.exit.fx.is_none(),
        "paper source selection or EUR costs invalid",
    )?;
    require(
        selection.account_id == plan.account_id
            && plan.account_id == config.account_id.as_str()
            && selection.universe_snapshot_id == config.approved_universe_snapshot_id
            && plan.policy_revision == config.policy.revision,
        "paper account, policy or universe changed",
    )?;
    validate_book(entry, &ledger["binding"], entry_at)?;
    validate_book(exit, &ledger["binding"], exit_at)?;
    require(
        entry_at >= plan.created_at
            && entry_at <= time(&ledger["protection_recorded_at"])?
            && time(&ledger["protection_recorded_at"])? <= exit_at
            && exit_at > entry_at
            && exit_at <= Utc::now()
            && decimal(&entry["ask"])? <= plan.entry_price
            && decimal(&entry["ask"])? > plan.stop_price,
        "paper lifecycle time or price invalid",
    )?;
    let books = ledger["books"]
        .as_array()
        .ok_or_else(|| Error("paper quote sequence missing".into()))?;
    require(
        books.len() >= 2 && books.first() == Some(entry) && books.last() == Some(exit),
        "paper quote sequence rebound",
    )?;
    let mut previous = entry_at;
    for (index, book) in books.iter().enumerate() {
        let at = time(&book["paper_recorded_at"])?;
        validate_book(book, &ledger["binding"], at)?;
        require(
            at >= previous && (at - previous).num_milliseconds() <= 10000,
            "paper quote sequence has a clock gap",
        )?;
        if index > 0 && index + 1 < books.len() {
            let bid = decimal(&book["bid"])?;
            require(
                bid > plan.stop_price
                    && bid < plan.target_price
                    && at < plan.flatten_deadline
                    && ledger["bounded_exit_seconds"]
                        .as_i64()
                        .is_some_and(|s| (at - entry_at).num_milliseconds() < s * 1000),
                "paper ledger skipped an earlier exit trigger",
            )?;
        }
        previous = at;
    }
    let bid = decimal(&exit["bid"])?;
    let reason_valid = match ledger["exit_reason"].as_str() {
        Some("STOP") => bid <= plan.stop_price,
        Some("TARGET") => bid >= plan.target_price,
        Some("DEADLINE") => exit_at >= plan.flatten_deadline,
        Some("BOUNDED_PAPER_ACCEPTANCE_EXIT") => {
            ledger["bounded_exit_seconds"].as_u64().is_some_and(|s| {
                (1..=900).contains(&s) && (exit_at - entry_at).num_milliseconds() >= s as i64 * 1000
            })
        }
        _ => false,
    };
    require(
        reason_valid
            && exit_at <= plan.flatten_deadline + chrono::Duration::seconds(10)
            && (time(&ledger["protection_recorded_at"])? - entry_at).num_milliseconds() <= 10000,
        "paper protection or exit reason invalid",
    )?;
    let debit = cash(
        decimal(&entry["ask"])? * plan.quantity,
        costs.entry.fixed.amount,
        costs.entry.rate,
        costs.entry.uncertainty.amount,
        true,
    )?;
    let credit = cash(
        decimal(&exit["bid"])? * plan.quantity,
        costs.exit.fixed.amount,
        costs.exit.rate,
        costs.exit.uncertainty.amount,
        false,
    )?;
    require(
        decimal(&ledger["entry_debit_eur"])? == debit
            && debit <= account.spendable_cash.amount
            && debit <= plan.acquisition_cost.amount
            && decimal(&ledger["exit_credit_eur"])? == credit
            && decimal(&ledger["ending_cash_eur"])?
                == decimal(&ledger["initial_cash_eur"])? - debit + credit
            && decimal(&ledger["net_pnl_eur"])? == credit - debit,
        "paper cash reconciliation differs",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ledger_fixture() -> (Value, Value) {
        let at = DateTime::from_timestamp(Utc::now().timestamp() - 5, 0).unwrap();
        let deadline = at + chrono::Duration::hours(1);
        let mut config: EurRuntimeConfig =
            serde_json::from_str(include_str!("../../../config/eur-shadow.example.json")).unwrap();
        config.account_id = trady_domain::AccountId::new("paper_fixture").unwrap();
        config.approved_universe_snapshot_id = "fixture_universe".into();
        let selection = json!({"schema_version":1,"run_id":"fixture_research","account_id":"paper_fixture","session_id":"fixture_session","universe_snapshot_id":"fixture_universe",
            "started_at":at-chrono::Duration::seconds(3),"completed_at":at-chrono::Duration::seconds(2),
            "model":{"run_id":"fixture_research","session_id":"fixture_session","provider":"opencode-go","model":"deepseek-v4.1-flash","prompt_hash":"a".repeat(64),"tool_manifest_hash":"b".repeat(64),"model_revision":"c".repeat(64)},
            "fallback_attempts":[],"candidates":[{"instrument_id":"AIR_PA_EQ","currency":"EUR","eligibility_snapshot_id":"fixture_universe","eligible":true,"ranking_score":"1","sentiment_score":null,"confidence":null,"evidence_ids":["fixture_quote"],"exclusion_reasons":[],"catalyst":null,"contradictions":[],"strategy_snapshot_ids":["fixture_signal"]}],
            "selected_instrument_id":"AIR_PA_EQ","proposal":{"entry_minimum":"100","entry_maximum":"100.10","stop_price":"99","target_price":"102","maximum_hold_until":deadline,"rationale":"Synthetic unit fixture only"},"evidence_snapshot_ids":["fixture_quote"],"failure":null});
        let mut plan = json!({"plan_id":"fixture_plan","policy_revision":config.policy.revision,"account_id":"paper_fixture","instrument_id":"AIR_PA_EQ","currency":"EUR",
            "entry_price":"100","quantity":"1","acquisition_cost":{"amount":"100.06","currency":"EUR"},"stop_price":"99","target_price":"102","target_net_fraction":"0.019","planned_loss_fraction":"0.011","reward_to_risk":"1.7",
            "created_at":at-chrono::Duration::seconds(1),"flatten_deadline":deadline,"evidence_snapshot_ids":["fixture_quote"],"stop_revision":0});
        let zero = json!({"amount":"0","currency":"EUR"});
        let reserve = json!({"amount":"0.01","currency":"EUR"});
        let costs = json!({"review_reference":"synthetic_unit_fixture_not_release","reviewed_at":at-chrono::Duration::seconds(5),"valid_until":deadline,
            "entry":{"fixed":zero,"rate":"0.0005","uncertainty":reserve,"fx":null},"exit":{"fixed":zero,"rate":"0.0005","uncertainty":reserve,"fx":null}});
        // Exact unrounded model proceeds for this one-share fixture: entry
        // 100 + 0.05 + 0.01; stop 99 - 0.0495 - 0.01; target 102 - 0.051 - 0.01.
        let acquisition = decimal(&json!("100.06")).unwrap();
        let loss = (acquisition - decimal(&json!("98.9405")).unwrap()) / acquisition;
        let target = (decimal(&json!("101.939")).unwrap() - acquisition) / acquisition;
        plan["cost_model"] = json!({"entry":costs["entry"],"exit":costs["exit"]});
        plan["planned_loss_fraction"] = json!(loss);
        plan["target_net_fraction"] = json!(target);
        plan["reward_to_risk"] = json!(target / loss);
        let account = json!({"snapshot_id":"fixture_account","account_id":"paper_fixture","mode":"paper","spendable_cash":{"amount":"1000","currency":"EUR"},"positions":[],"orders":[],"observed_at":at-chrono::Duration::seconds(3)});
        let binding = json!({"con_id":29612256,"primary_exchange":"SBF","isin":"NL0000235190"});
        let quote = |offset: i64, bid: &str, ask: &str| {
            json!({"kind":"bid_ask","isin":"NL0000235190","con_id":29612256,"currency":"EUR","source_venue":"SMART","market_data_type":1,
            "observed_at_seconds":(at+chrono::Duration::seconds(offset)).timestamp(),"received_at":at+chrono::Duration::seconds(offset)+chrono::Duration::milliseconds(100),
            "paper_recorded_at":at+chrono::Duration::seconds(offset)+chrono::Duration::milliseconds(200),"bid":bid,"ask":ask,"bid_size":"1","ask_size":"1"})
        };
        let entry = quote(0, "99.99", "100");
        let exit = quote(1, "100.10", "100.11");
        let context = json!({"runtime":config,"account_snapshot":account,"crosswalk":{"members":[{"internal_instrument_id":"AIR_PA_EQ","isin":"NL0000235190"}]},
            "provider_configurations":{"risk":costs,"ibkr":{"bindings":{"AIR_PA_EQ":{"con_id":29612256,"primary_exchange":"SBF"}}}},
            "result":{"cycle_id":"fixture_cycle","configuration_hash":"d".repeat(64),"policy_hash":"e".repeat(64),"analyst_invoked":true,"selection":selection,"risk_plan":plan}});
        let ledger = json!({"source":"REAL_INPUT_FORWARD_PAPER","simulation":true,"synthetic_fixture":false,"broker_mutations":0,"live_enabled":false,"reconciled_virtual_flat":true,"protection_model":"PAPER_BID_TRIGGER_ON_ONE_SECOND_OBSERVATIONS",
            "source_cycle_id":"fixture_cycle","source_configuration_hash":"d".repeat(64),"source_policy_hash":"e".repeat(64),"selection":selection,"risk_plan":plan,"cost_model":costs,"binding":binding,
            "entry_quote":entry,"exit_quote":exit,"books":[entry,exit],"protection_recorded_at":at+chrono::Duration::milliseconds(300),"bounded_exit_seconds":1,"exit_reason":"BOUNDED_PAPER_ACCEPTANCE_EXIT",
            "initial_account_snapshot":account,"initial_cash_eur":"1000","entry_debit_eur":"100.06","exit_credit_eur":"100.03","ending_cash_eur":"999.97","net_pnl_eur":"-0.03"});
        (ledger, context)
    }
    #[test]
    fn paper_books_require_exact_retained_live_contract_callbacks() {
        let (mut ledger, _) = ledger_fixture();
        let books = ledger["books"].as_array_mut().unwrap();
        let mut callbacks = Vec::new();
        for book in books {
            book["request_id"] = json!(2000);
            let event: serde_json::Map<String, Value> = [
                "kind",
                "request_id",
                "observed_at_seconds",
                "received_at",
                "bid",
                "ask",
                "bid_size",
                "ask_size",
            ]
            .into_iter()
            .map(|key| (key.into(), book[key].clone()))
            .collect();
            callbacks.push(Value::Object(event));
        }
        let before = time(&callbacks[0]["received_at"]).unwrap() - chrono::Duration::seconds(1);
        let provenance = json!({"provider":"IBKR_SMART_LIVE","broker_mutations":0,
            "contracts":{"contracts":[{"con_id":29612256,"identifiers":{"ISIN":"NL0000235190"},"primary_exchange":"SBF","exchange":"SMART","currency":"EUR"}]},
            "market_data_types":[{"request_id":1000,"data_type":1,"received_at":before}],"callbacks":callbacks});
        validate_stream_provenance(&ledger, &provenance).unwrap();
        let mut missing = provenance.clone();
        missing["callbacks"][1]["ask"] = json!("100.12");
        assert!(validate_stream_provenance(&ledger, &missing).is_err());
        let mut incomplete = provenance.clone();
        for event in incomplete["callbacks"].as_array_mut().unwrap() {
            event.as_object_mut().unwrap().remove("bid");
            event.as_object_mut().unwrap().remove("ask");
        }
        assert!(validate_stream_provenance(&ledger, &incomplete).is_err());
        let mut delayed = provenance.clone();
        delayed["market_data_types"][0]["data_type"] = json!(3);
        assert!(validate_stream_provenance(&ledger, &delayed).is_err());
        let mut rebound = provenance;
        rebound["contracts"]["contracts"][0]["identifiers"]["ISIN"] = json!("NL0010273215");
        assert!(validate_stream_provenance(&ledger, &rebound).is_err());
    }

    #[test]
    fn paper_ledger_rejects_rebound_cash_contract_and_invented_exit() {
        let (ledger, context) = ledger_fixture();
        validate_for_capture(&ledger, &context).unwrap();
        for case in [
            "cash",
            "contract",
            "exit_reason",
            "future_receipt",
            "protection",
            "synthetic",
            "earlier_trigger",
            "protection_model",
        ] {
            let mut bad = ledger.clone();
            match case {
                "cash" => bad["ending_cash_eur"] = json!("1001"),
                "contract" => bad["binding"]["con_id"] = json!(1),
                "exit_reason" => bad["exit_reason"] = json!("TARGET"),
                "future_receipt" => {
                    bad["entry_quote"]["received_at"] =
                        bad["exit_quote"]["paper_recorded_at"].clone()
                }
                "protection" => {
                    bad["protection_recorded_at"] = json!(Utc::now() + chrono::Duration::hours(2))
                }
                "earlier_trigger" => {
                    let mut book = ledger["entry_quote"].clone();
                    book["bid"] = json!("102.01");
                    book["ask"] = json!("102.02");
                    book["paper_recorded_at"] = json!(
                        time(&book["paper_recorded_at"]).unwrap()
                            + chrono::Duration::milliseconds(500)
                    );
                    bad["books"].as_array_mut().unwrap().insert(1, book);
                }
                "protection_model" => bad["protection_model"] = json!("NATIVE_BROKER_STOP"),
                _ => bad["synthetic_fixture"] = json!(true),
            }
            assert!(validate_for_capture(&bad, &context).is_err(), "{case}");
        }
    }
    #[test]
    fn conservative_eur_cash_rounding_charges_the_declared_model_only() {
        assert_eq!(
            cash(
                Decimal::new(100051, 4),
                Decimal::ZERO,
                Decimal::new(5, 4),
                Decimal::new(1, 2),
                true
            )
            .unwrap(),
            Decimal::new(1003, 2)
        );
        assert_eq!(
            cash(
                Decimal::new(9999, 3),
                Decimal::ZERO,
                Decimal::new(5, 4),
                Decimal::new(1, 2),
                false
            )
            .unwrap(),
            Decimal::new(998, 2)
        );
        assert!(
            cash(
                Decimal::ONE,
                Decimal::ZERO,
                -Decimal::ONE,
                Decimal::ZERO,
                true
            )
            .is_err()
        );
    }
}

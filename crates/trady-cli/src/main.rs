mod account;
mod api_health;
mod autonomous;
mod broker_read;
mod entry_demo;
mod financial;
mod finbert;
mod inputs_service;
mod intelligence;
mod model_lab;
mod news;
mod opencode;
mod release;
mod stock_report;
mod telegram;
mod weekly;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::json;
use std::{env, fs};
use trady_adapters::credentials::{self, CredentialStore};
use trady_adapters::trading212::{Credentials, Trading212Config, Trading212Reader};
use trady_adapters::{FakeBroker, FakeClock, MemoryExecutionRepository};
use trady_application::contracts::{Contract, Contracts};
use trady_application::{Clock, ExecutionRepository, FoundationConfig, reconcile_account};
use trady_domain::*;
use trady_storage::PgExecutionRepository;

const HELP: &str = "Trady\n\n  trady analyst|executor|gateway --once [--config FILE]\n  trady demo\n  trady demo-postgres\n  trady migrate\n  trady credentials init|status\n  trady t212 snapshot --config FILE\n  trady validate-json model|results|envelope|artifact|artifact-result FILE\n\nService probes validate configuration and service-specific database roles.\nThe autonomous command runs enabled demo/live execution or records real-data shadow proposals.\nLive execution requires independently reviewed release evidence and a separately managed analyst.\nBroker environment selects matching demo/live API credentials; shadow uses only the read-only broker interface.\nCredentials use environment overrides or config/credentials.local.json; TRADY_CREDENTIALS_FILE selects another private file.\nCredential status prints names and availability, never values.\n";

fn print_help() {
    println!(
        "{HELP}{}{}{}{}{}{}{}{}{}",
        opencode::HELP,
        news::HELP,
        telegram::HELP,
        finbert::HELP,
        autonomous::HELP,
        inputs_service::HELP,
        model_lab::HELP,
        weekly::HELP,
        release::HELP
    );
    print!("{}", api_health::HELP);
    print!("{}", stock_report::HELP);
    print!("{}", financial::HELP);
    print!("{}", intelligence::HELP);
    print!("{}", entry_demo::HELP);
    print!("{}", account::HELP);
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("trady: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
fn output(value: serde_json::Value) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&value)
            .map_err(|_| Error("output serialization failed".into()))?
    );
    Ok(())
}
fn secret(name: &str) -> Result<String> {
    credentials::required(name).map(|value| value.into_inner())
}
fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    match command {
        "credentials" if args.len() == 2 && args[1] == "init" => {
            let path = credentials::initialize()?;
            output(json!({"status":"created","path":path}))
        }
        "credentials" if args.len() == 2 && args[1] == "status" => {
            output(json!({"credentials":CredentialStore::load()?.status()?}))
        }
        "account" => output(account::run(&args[1..])?),
        "telegram" => output(telegram::run(&args[1..])?),
        "autonomous" => output(autonomous::run(&args[1..])?),
        "weekly" => output(weekly::run(&args[1..])?),
        "release" => output(release::run(&args[1..])?),
        "inputs-service" => output(inputs_service::run(&args[1..])?),
        "model-lab" => output(model_lab::run(&args[1..])?),
        "news" => output(news::run(&args[1..])?),
        "financial" => output(financial::run(&args[1..])?),
        "api-health" => output(api_health::run(&args[1..])?),
        "research-report" => output(stock_report::run(&args[1..])?),
        "intelligence" => output(intelligence::run(&args[1..])?),
        "sentiment" => output(finbert::run(&args[1..])?),
        "t212" if args.len() == 4 && args[1] == "snapshot" && args[2] == "--config" => {
            trading212_snapshot(&args[3])
        }
        "--help" | "help" | "-h" => {
            print_help();
            Ok(())
        }
        "chat" | "sessions" | "skill-manifest" => output(opencode::run(command, &args[1..])?),
        "analyst" | "executor" | "gateway" => service(command, &args[1..]),
        "migrate" if args.len() == 1 => {
            trady_storage::migrate(&secret("TRADY_MIGRATOR_DSN")?)?;
            output(json!({"migration":10,"status":"applied_or_verified"}))
        }
        "demo" if args.len() == 1 => demo_memory(),
        "demo-entry" => output(entry_demo::run(&args[1..])?),
        "demo-postgres" if args.len() == 1 => demo_postgres(),
        "validate-json" if args.len() == 3 => {
            let contract = match args[1].as_str() {
                "model" => Contract::ModelTurn,
                "results" => Contract::ToolResults,
                "envelope" => Contract::ProposalEnvelope,
                "artifact" => Contract::StoreArtifactArguments,
                "artifact-result" => Contract::StoreArtifactResult,
                _ => return Err(Error("unsupported contract name".into())),
            };
            let input = fs::read_to_string(&args[2])
                .map_err(|_| Error("cannot read JSON input file".into()))?;
            Contracts::new()?.validate(contract, &input)?;
            output(
                json!({"schema_valid":true,"entry_authorized":false,"note":"Evidence resolution and full readiness verification remain required."}),
            )
        }
        _ => Err(Error(format!(
            "unsupported arguments; use trady --help\n{HELP}"
        ))),
    }
}

fn trading212_snapshot(path: &str) -> Result<()> {
    let input = fs::read_to_string(path)
        .map_err(|_| Error("cannot read Trading 212 configuration".into()))?;
    let config: Trading212Config = serde_json::from_str(&input)
        .map_err(|_| Error("invalid or unsupported Trading 212 configuration fields".into()))?;
    config.validate().map_err(|e| Error(e.to_string()))?;
    let credentials = Credentials::from_store(config.environment, &CredentialStore::load()?)?;
    let mut reader =
        Trading212Reader::new(config.clone(), credentials).map_err(|e| Error(e.to_string()))?;
    let snapshot = broker_read::snapshot(
        &mut reader,
        &config.account_id,
        &std::sync::atomic::AtomicBool::new(false),
    )?;
    output(
        json!({"provider":"trading212","environment":config.environment,"entry_authorized":false,"snapshot":snapshot}),
    )
}
fn service(service: &str, args: &[String]) -> Result<()> {
    require(
        args.first().map(String::as_str) == Some("--once"),
        "foundation services require --once; long-running workers are not implemented",
    )?;
    let text = match args.len() {
        1 => include_str!("../../../config/paper.json").to_string(),
        3 if args[1] == "--config" => {
            fs::read_to_string(&args[2]).map_err(|_| Error("cannot read configuration".into()))?
        }
        _ => return Err(Error("unsupported service arguments".into())),
    };
    let config: FoundationConfig = serde_json::from_str(&text)
        .map_err(|_| Error("invalid or unsupported configuration fields".into()))?;
    config.validate()?;
    Contracts::new()?;
    let (variable, role) = match service {
        "analyst" => ("TRADY_ANALYST_DSN", "trady_analyst"),
        "executor" => ("TRADY_EXECUTOR_DSN", "trady_executor"),
        _ => ("TRADY_GATEWAY_DSN", "trady_gateway"),
    };
    let database = match credentials::optional(variable)? {
        Some(dsn) => {
            trady_storage::check_service_database(dsn.expose(), role)?;
            "restricted_role_verified"
        }
        None => "not_configured",
    };
    output(
        json!({"service":service,"mode":config.mode,"account_id":config.account_id,"policy_version":config.policy.version,"database":database,"status":"foundation_probe_complete","entry_authorized":false,"entry_blockers":config.entry_blockers(Utc::now())}),
    )
}

fn snapshot(account: &AccountId, id: &str, now: DateTime<Utc>) -> Result<AccountSnapshot> {
    Ok(AccountSnapshot {
        snapshot_id: id.into(),
        account_id: account.clone(),
        mode: Mode::Paper,
        spendable_cash: Money {
            amount: Decimal::new(50, 0),
            currency: Currency::new("EUR")?,
        },
        positions: vec![],
        orders: vec![],
        observed_at: now,
    })
}
fn demo_intent(account: &AccountId, id: &str, now: DateTime<Utc>) -> Result<OrderIntent> {
    Ok(OrderIntent {
        intent_id: IntentId::new(id)?,
        account_id: account.clone(),
        instrument_id: InstrumentId::new("fixture_stock")?,
        side: Side::Buy,
        quantity: Decimal::new(49, 1),
        limit_price: Decimal::TEN,
        session_id: format!("fixture_{}", now.format("%Y_%m_%d")),
        created_at: now,
        state: IntentState::Prepared,
        broker_order_id: None,
    })
}
fn begin_demo(
    repo: &mut impl ExecutionRepository,
    account: &AccountId,
    now: DateTime<Utc>,
    intent_key: &str,
) -> Result<(Fence, OrderIntent, bool)> {
    let fence = repo.acquire(
        account,
        &WorkerId::new(format!("demo_{}", std::process::id()))?,
        30,
    )?;
    let clock = FakeClock::new(now);
    let mut broker = FakeBroker::new(snapshot(account, "flat", now)?)?;
    reconcile_account(&mut broker, repo, &fence, &clock)?;
    let intent = demo_intent(account, intent_key, now)?;
    require(
        repo.reserve_paper_intent(&fence, &intent)?,
        "demo expected new fixture intent",
    )?;
    let replay_created = repo.reserve_paper_intent(&fence, &intent)?;
    Ok((fence, intent, replay_created))
}
fn finish_demo(
    repo: &mut impl ExecutionRepository,
    fence: &Fence,
    intent: &OrderIntent,
    now: DateTime<Utc>,
    replay_created: bool,
    storage: &str,
) -> Result<()> {
    let clock = FakeClock::new(now);
    let mut missing = snapshot(&fence.account_id, "unknown", now)?;
    let mut broker = FakeBroker::new(missing.clone())?;
    let unknown = reconcile_account(&mut broker, repo, fence, &clock)?;
    missing.snapshot_id = "filled".into();
    missing.spendable_cash.amount = Decimal::ONE;
    missing.positions.push(Position {
        instrument_id: intent.instrument_id.clone(),
        quantity: intent.quantity,
    });
    missing.orders.push(BrokerOrder {
        broker_order_id: "fake_accepted_order".into(),
        client_intent_id: Some(intent.intent_id.clone()),
        instrument_id: intent.instrument_id.clone(),
        side: Side::Buy,
        quantity: intent.quantity,
        filled_quantity: intent.quantity,
        status: OrderStatus::Filled,
    });
    broker.set_snapshot(missing)?;
    let filled = reconcile_account(&mut broker, repo, fence, &clock)?;
    output(
        json!({"mode":"paper","storage":storage,"account_id":fence.account_id,"duplicate_intent_created":replay_created,"missing_response_state":unknown.state,"reconciled_fill_state":filled.state,"remaining_unresolved_intents":repo.unresolved_intents(&fence.account_id)?.len(),"cash":filled.snapshot.spendable_cash,"positions":filled.snapshot.positions,"entry_authorized":false,"note":"Offline fixture only. Protection and broker submission are not implemented."}),
    )
}
fn demo_memory() -> Result<()> {
    let now = Utc::now();
    let clock = FakeClock::new(now);
    let mut repo = MemoryExecutionRepository::new(clock.clone());
    let account = AccountId::new("paper_memory_demo")?;
    repo.register_paper_account(&account)?;
    let (fence, intent, replay_created) =
        begin_demo(&mut repo, &account, clock.now(), "memory_demo_intent")?;
    finish_demo(&mut repo, &fence, &intent, now, replay_created, "memory")
}
fn demo_postgres() -> Result<()> {
    let dsn = secret("TRADY_EXECUTOR_DSN")?;
    let now = Utc::now();
    let suffix = now.timestamp_micros();
    let account = AccountId::new(format!("paper_demo_{suffix}"))?;
    let mut repo = PgExecutionRepository::connect(&dsn)?;
    repo.register_paper_account(&account, &Currency::new("EUR")?)?;
    let (fence, intent, replay_created) =
        begin_demo(&mut repo, &account, now, &format!("demo_intent_{suffix}"))?;
    drop(repo);
    let mut recovered = PgExecutionRepository::connect(&dsn)?;
    require(
        recovered.unresolved_intents(&account)?.len() == 1,
        "durable intent was not recovered after reconnect",
    )?;
    finish_demo(
        &mut recovered,
        &fence,
        &intent,
        now,
        replay_created,
        "postgres_after_reconnect",
    )
}
